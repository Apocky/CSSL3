import { randomUUID } from 'node:crypto';
import type { SupabaseClient } from '@supabase/supabase-js';
import { getMnemeClient } from '../mneme/store';
import { ACCOUNT_UUID, CONVERSATION_UUID } from '../mobile/account-grant';
import { BRIDGE_LEASE_MS, BridgeError, bridgeConfiguration, bridgeMac, bridgeSessionId, createBridgeRequest,
  decodeBase64, decryptBridge, encryptBridge, equalMac, exactObject, keyedUuid, retryableBridgeResult, validateBridgeRequest, validateBridgeResult,
  type BridgeConfiguration, type BridgeEnvelope, type BridgeInput, type BridgeRequest } from './crypto';
import type { WorkerAuthentication } from './worker-auth';
export { bridgeConfigured } from './crypto';

const PROMPT = 'Apocrypha encrypted bridge request';
const JOB_SCHEMA = 'apocky.bridge.job-row.v1';
const NODE_SCHEMA = 'apocky.bridge.node-state.v1';
const JOB_ID = /^[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const PENDING_MAX = 128;
interface Pending { job_id: string; subject: string; channel: 'owner' | 'account'; created_at: string; }
interface NodeState { schema_version: typeof NODE_SCHEMA; key_id: string; worker_id: string; revision: number; pending: Pending[]; nonces: WorkerAuthentication[]; signature: string; }
interface JobMetadata { schema_version: typeof JOB_SCHEMA; key_id: string; worker_id: string; revision: number; request: BridgeEnvelope; response: BridgeEnvelope | null; lease_id: string | null; lease_expires_at: string | null; }
export interface BridgeSessionRow { id: string; user_id: string; title: string; metadata: unknown; }
export interface BridgeJobRow { id: string; user_id: string; session_id: string; prompt: string; response: string; status: string; leased_by: string | null; leased_at: string | null; finished_at: string | null; metadata: unknown; }
export interface BridgePersistence {
  getSession(id: string): Promise<BridgeSessionRow | null>;
  insertSession(row: BridgeSessionRow): Promise<boolean>;
  casSession(previous: BridgeSessionRow, next: BridgeSessionRow, revision: number): Promise<boolean>;
  getJob(id: string): Promise<BridgeJobRow | null>;
  insertJob(row: BridgeJobRow): Promise<boolean>;
  casJob(previous: BridgeJobRow, next: BridgeJobRow, revision: number): Promise<boolean>;
}
const sessionColumns = 'id,user_id,title,metadata';
const jobColumns = 'id,user_id,session_id,prompt,response,status,leased_by,leased_at,finished_at,metadata';
export function supabaseBridgePersistence(client: SupabaseClient): BridgePersistence {
  const check = (error: unknown) => { if (error) throw new BridgeError('BRIDGE_STORAGE_UNAVAILABLE'); };
  return {
    async getSession(id) { const { data, error } = await client.from('chat_session').select(sessionColumns).eq('id', id).abortSignal(AbortSignal.timeout(8000)).maybeSingle(); check(error); return data as BridgeSessionRow | null; },
    async insertSession(row) { const { error } = await client.from('chat_session').insert(row).abortSignal(AbortSignal.timeout(8000)); if (error?.code === '23505') return false; check(error); return true; },
    async casSession(previous, next, revision) { const { data, error } = await client.from('chat_session').update({ metadata: next.metadata }).eq('id', previous.id).eq('user_id', previous.user_id).eq('metadata->>revision', String(revision)).select('id').abortSignal(AbortSignal.timeout(8000)); check(error); return Boolean(data?.length === 1); },
    async getJob(id) { const { data, error } = await client.from('chat_turn').select(jobColumns).eq('id', id).abortSignal(AbortSignal.timeout(8000)).maybeSingle(); check(error); return data as BridgeJobRow | null; },
    async insertJob(row) { const { error } = await client.from('chat_turn').insert(row).abortSignal(AbortSignal.timeout(8000)); if (error?.code === '23505') return false; check(error); return true; },
    async casJob(previous, next, revision) {
      const { data, error } = await client.from('chat_turn').update({ metadata: next.metadata, status: next.status, leased_by: next.leased_by, leased_at: next.leased_at, finished_at: next.finished_at })
        .eq('id', previous.id).eq('user_id', previous.user_id).eq('session_id', previous.session_id).eq('status', previous.status)
        .eq('metadata->>revision', String(revision)).select('id').abortSignal(AbortSignal.timeout(8000));
      check(error); return Boolean(data?.length === 1);
    },
  };
}
function stateText(state: Omit<NodeState, 'signature'>): string {
  return JSON.stringify({ schema_version: state.schema_version, key_id: state.key_id, worker_id: state.worker_id, revision: state.revision,
    pending: state.pending.map(item => ({ job_id: item.job_id, subject: item.subject, channel: item.channel, created_at: item.created_at })),
    nonces: state.nonces.map(item => ({ nonce: item.nonce, time: item.time })) });
}
function sleep(milliseconds: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) { reject(new BridgeError('BRIDGE_WAIT_ABORTED', 504)); return; }
    const stop = () => { clearTimeout(timer); signal?.removeEventListener('abort', stop); reject(new BridgeError('BRIDGE_WAIT_ABORTED', 504)); };
    const timer = setTimeout(() => { signal?.removeEventListener('abort', stop); resolve(); }, milliseconds);
    signal?.addEventListener('abort', stop, { once: true });
  });
}
export class BridgeQueue {
  constructor(readonly configuration: BridgeConfiguration, readonly persistence: BridgePersistence,
    readonly now: () => number = Date.now, readonly wait: (ms: number, signal?: AbortSignal) => Promise<void> = sleep) {}
  private signedState(value: Omit<NodeState, 'signature'>): NodeState { return { ...value, signature: bridgeMac(this.configuration, 'queue-state', stateText(value)) }; }
  private nodeId(): string { return keyedUuid(this.configuration, 'session-id', `apocky.bridge.node.v1\n${this.configuration.ownerSubject}\n${this.configuration.workerId}`); }
  private validateState(row: BridgeSessionRow): NodeState {
    const state = row.metadata;
    if (row.id !== this.nodeId() || row.user_id !== this.configuration.ownerSubject || row.title !== 'Apocrypha encrypted bridge node'
      || !exactObject(state, ['schema_version', 'key_id', 'worker_id', 'revision', 'pending', 'nonces', 'signature'])
      || state.schema_version !== NODE_SCHEMA || state.key_id !== this.configuration.keyId || state.worker_id !== this.configuration.workerId
      || typeof state.revision !== 'number' || !Number.isSafeInteger(state.revision) || state.revision < 0
      || !Array.isArray(state.pending) || state.pending.length > PENDING_MAX || !Array.isArray(state.nonces) || state.nonces.length > 512 || typeof state.signature !== 'string') throw new BridgeError('BRIDGE_NODE_INTEGRITY_FAILED');
    if (!state.pending.every(item => exactObject(item, ['job_id', 'subject', 'channel', 'created_at']) && typeof item.job_id === 'string' && JOB_ID.test(item.job_id)
      && typeof item.created_at === 'string' && Number.isFinite(Date.parse(item.created_at))
      && typeof item.subject === 'string' && ACCOUNT_UUID.test(item.subject) && (item.channel === 'account' || (item.channel === 'owner' && item.subject === this.configuration.ownerSubject)))
      || new Set(state.pending.map(item => item.job_id)).size !== state.pending.length
      || !state.nonces.every(item => exactObject(item, ['nonce', 'time']) && typeof item.nonce === 'string' && CONVERSATION_UUID.test(item.nonce) && typeof item.time === 'number' && Number.isSafeInteger(item.time))) throw new BridgeError('BRIDGE_NODE_INTEGRITY_FAILED');
    const result = state as unknown as NodeState;
    if (!equalMac(result.signature, bridgeMac(this.configuration, 'queue-state', stateText(result)))) throw new BridgeError('BRIDGE_NODE_INTEGRITY_FAILED');
    return result;
  }
  private async node(): Promise<BridgeSessionRow> {
    const id = this.nodeId();
    let row = await this.persistence.getSession(id);
    if (!row) {
      const initial: BridgeSessionRow = { id, user_id: this.configuration.ownerSubject, title: 'Apocrypha encrypted bridge node', metadata: this.signedState({ schema_version: NODE_SCHEMA, key_id: this.configuration.keyId, worker_id: this.configuration.workerId, revision: 0, pending: [], nonces: [] }) };
      if (await this.persistence.insertSession(initial)) row = initial;
      else row = await this.persistence.getSession(id);
    }
    if (!row) throw new BridgeError('BRIDGE_STORAGE_UNAVAILABLE');
    this.validateState(row); return row;
  }
  private async changeNode(change: (state: NodeState) => Omit<NodeState, 'signature'>): Promise<void> {
    for (let attempt = 0; attempt < 8; attempt += 1) {
      const row = await this.node(); const state = this.validateState(row);
      const next = this.signedState({ ...change(structuredClone(state)), revision: state.revision + 1 });
      if (await this.persistence.casSession(row, { ...row, metadata: next }, state.revision)) return;
    }
    throw new BridgeError('BRIDGE_STORAGE_BUSY', 503);
  }
  async consumeAuthentication(auth: WorkerAuthentication): Promise<void> {
    await this.changeNode(state => {
      const floor = Math.floor(this.now() / 1000) - 120;
      state.nonces = state.nonces.filter(item => item.time >= floor);
      if (state.nonces.some(item => item.nonce === auth.nonce)) throw new BridgeError('BRIDGE_WORKER_REPLAY', 409);
      if (state.nonces.filter(item => item.time >= floor + 60).length >= 240 || state.nonces.length >= 512) throw new BridgeError('BRIDGE_WORKER_RATE_LIMIT', 429);
      state.nonces.push(auth); return state;
    });
  }
  private async ensureUserSession(subject: string): Promise<void> {
    const id = bridgeSessionId(this.configuration, subject);
    const metadata = { schema_version: 'apocky.bridge.session.v1', key_id: this.configuration.keyId, worker_id: this.configuration.workerId, subject };
    let row = await this.persistence.getSession(id);
    if (!row) {
      const initial = { id, user_id: subject, title: 'Apocrypha encrypted bridge', metadata };
      if (await this.persistence.insertSession(initial)) row = initial; else row = await this.persistence.getSession(id);
    }
    if (!row || row.id !== id || row.user_id !== subject || row.title !== 'Apocrypha encrypted bridge'
      || !exactObject(row.metadata, Object.keys(metadata)) || Object.entries(metadata).some(([key, value]) => (row!.metadata as Record<string, unknown>)[key] !== value)) throw new BridgeError('BRIDGE_SESSION_INTEGRITY_FAILED');
  }
  private validateJob(row: BridgeJobRow, expected?: Omit<Pending, 'created_at'> & { created_at?: string }): { request: BridgeRequest; metadata: JobMetadata } {
    const meta = row.metadata;
    if (row.prompt !== PROMPT || row.response !== '' || !JOB_ID.test(row.id) || !ACCOUNT_UUID.test(row.user_id)
      || row.session_id !== bridgeSessionId(this.configuration, row.user_id) || !['queued', 'leased', 'done'].includes(row.status)
      || !exactObject(meta, ['schema_version', 'key_id', 'worker_id', 'revision', 'request', 'response', 'lease_id', 'lease_expires_at'])
      || meta.schema_version !== JOB_SCHEMA || meta.key_id !== this.configuration.keyId || meta.worker_id !== this.configuration.workerId
      || typeof meta.revision !== 'number' || !Number.isSafeInteger(meta.revision) || meta.revision < 0) throw new BridgeError('BRIDGE_JOB_INTEGRITY_FAILED');
    const request = validateBridgeRequest(this.configuration, decryptBridge(this.configuration, 'request', row.id, meta.request));
    if (request.subject !== row.user_id || (expected && (expected.job_id !== row.id || expected.subject !== row.user_id || expected.channel !== request.channel || (expected.created_at !== undefined && expected.created_at !== request.created_at)))) throw new BridgeError('BRIDGE_JOB_INTEGRITY_FAILED');
    if (row.status === 'queued' ? (meta.lease_id !== null || meta.lease_expires_at !== null || row.leased_by !== null || meta.response !== null)
      : (typeof meta.lease_id !== 'string' || !CONVERSATION_UUID.test(meta.lease_id) || typeof meta.lease_expires_at !== 'string' || !Number.isFinite(Date.parse(meta.lease_expires_at)) || row.leased_by !== this.configuration.workerId || (row.status === 'leased' && meta.response !== null))) throw new BridgeError('BRIDGE_JOB_INTEGRITY_FAILED');
    if (row.status === 'done') validateBridgeResult(row.id, decryptBridge(this.configuration, 'response', row.id, meta.response));
    return { request, metadata: meta as unknown as JobMetadata };
  }
  private async removePending(id: string, createdAt: string): Promise<void> { await this.changeNode(state => ({ ...state, pending: state.pending.filter(item => item.job_id !== id || item.created_at !== createdAt) })); }
  async admit(input: BridgeInput): Promise<string> {
    if (input.signal?.aborted) throw new BridgeError('BRIDGE_WAIT_ABORTED', 504);
    let request = createBridgeRequest(this.configuration, input, this.now());
    await this.ensureUserSession(request.subject);
    const fresh = (): BridgeJobRow => ({ id: request.job_id, user_id: request.subject, session_id: bridgeSessionId(this.configuration, request.subject), prompt: PROMPT, response: '', status: 'queued', leased_by: null, leased_at: null, finished_at: null,
      metadata: { schema_version: JOB_SCHEMA, key_id: this.configuration.keyId, worker_id: this.configuration.workerId, revision: 0, request: encryptBridge(this.configuration, 'request', request.job_id, request), response: null, lease_id: null, lease_expires_at: null } satisfies JobMetadata });
    let row = await this.persistence.getJob(request.job_id);
    if (!row) {
      const state = this.validateState(await this.node());
      if (state.pending.length >= PENDING_MAX || state.pending.filter(item => item.subject === request.subject).length >= 8) throw new BridgeError('BRIDGE_QUEUE_FULL', 429);
      const candidate = fresh(); if (await this.persistence.insertJob(candidate)) row = candidate; else row = await this.persistence.getJob(request.job_id);
    }
    if (!row) throw new BridgeError('BRIDGE_STORAGE_UNAVAILABLE');
    const existing = this.validateJob(row, { job_id: request.job_id, subject: request.subject, channel: request.channel });
    if (existing.request.method !== request.method || existing.request.target !== request.target || existing.request.body_base64 !== request.body_base64) throw new BridgeError('BRIDGE_JOB_INTEGRITY_FAILED');
    const retryable = row.status === 'done' && request.method === 'POST'
      && retryableBridgeResult(validateBridgeResult(row.id, decryptBridge(this.configuration, 'response', row.id, existing.metadata.response)));
    if (row.status === 'done' && !retryable) return row.id;
    let admittedAt = existing.request.created_at;
    // § retry: only exact durable operation identity; an active lease is never cancelled.
    if (retryable || (Date.parse(existing.request.expires_at) <= this.now() && (row.status === 'queued' || Date.parse(existing.metadata.lease_expires_at!) <= this.now()))) {
      request = createBridgeRequest(this.configuration, input, Math.max(this.now(), Date.parse(existing.request.created_at) + 1), request.nonce);
      admittedAt = request.created_at;
      const next = fresh(); (next.metadata as JobMetadata).revision = existing.metadata.revision + 1;
      if (!await this.persistence.casJob(row, next, existing.metadata.revision)) throw new BridgeError('BRIDGE_STORAGE_BUSY');
    }
    await this.changeNode(state => {
      const previous = state.pending.find(item => item.job_id === request.job_id);
      if (previous) { if (Date.parse(previous.created_at) < Date.parse(admittedAt)) previous.created_at = admittedAt; return state; }
      if (state.pending.length >= PENDING_MAX || state.pending.filter(item => item.subject === request.subject).length >= 8) throw new BridgeError('BRIDGE_QUEUE_FULL', 429);
      state.pending.push({ job_id: request.job_id, subject: request.subject, channel: request.channel, created_at: admittedAt }); return state;
    });
    return request.job_id;
  }
  async poll(): Promise<{ job_id: string; lease_id: string; lease_expires_at: string; request: BridgeEnvelope } | null> {
    const state = this.validateState(await this.node());
    for (const item of state.pending) {
      const row = await this.persistence.getJob(item.job_id);
      // § account deletion: RLS permits deleting the user's session, which cascades its jobs.
      // Missing or invalid rows lose admission; no client-controlled row reaches the worker.
      if (!row) { await this.removePending(item.job_id, item.created_at); continue; }
      let verified: ReturnType<BridgeQueue['validateJob']>;
      try { verified = this.validateJob(row, item); }
      catch (error) {
        if (!(error instanceof BridgeError)) throw error;
        await this.removePending(item.job_id, item.created_at); continue;
      }
      const { request, metadata } = verified;
      if (row.status === 'done') { await this.removePending(row.id, request.created_at); continue; }
      if (row.status === 'leased' && Date.parse(metadata.lease_expires_at!) > this.now()) continue;
      if (Date.parse(request.expires_at) <= this.now()) { await this.removePending(row.id, request.created_at); continue; }
      const leaseId = randomUUID(); const expires = new Date(this.now() + BRIDGE_LEASE_MS).toISOString();
      const next = { ...row, status: 'leased', leased_by: this.configuration.workerId, leased_at: new Date(this.now()).toISOString(),
        metadata: { ...metadata, revision: metadata.revision + 1, lease_id: leaseId, lease_expires_at: expires } };
      if (await this.persistence.casJob(row, next, metadata.revision)) return { job_id: row.id, lease_id: leaseId, lease_expires_at: expires, request: metadata.request };
    }
    return null;
  }
  async complete(jobId: string, leaseId: string, response: unknown): Promise<void> {
    if (!JOB_ID.test(jobId) || !CONVERSATION_UUID.test(leaseId)) throw new BridgeError('BRIDGE_COMPLETION_INVALID', 400);
    validateBridgeResult(jobId, decryptBridge(this.configuration, 'response', jobId, response));
    const row = await this.persistence.getJob(jobId);
    if (!row) throw new BridgeError('BRIDGE_COMPLETION_CONFLICT', 409);
    const { metadata, request } = this.validateJob(row);
    if (metadata.lease_id !== leaseId || row.leased_by !== this.configuration.workerId) throw new BridgeError('BRIDGE_COMPLETION_CONFLICT', 409);
    if (row.status === 'done') {
      // JSONB may reorder envelope keys; compare the authenticated field values.
      if (!metadata.response || Object.entries(metadata.response).some(([key, value]) => (response as Record<string, unknown>)[key] !== value)) throw new BridgeError('BRIDGE_COMPLETION_CONFLICT', 409);
      return;
    }
    if (row.status !== 'leased' || Date.parse(metadata.lease_expires_at!) <= this.now()) throw new BridgeError('BRIDGE_COMPLETION_CONFLICT', 409);
    const next = { ...row, status: 'done', finished_at: new Date(this.now()).toISOString(), metadata: { ...metadata, revision: metadata.revision + 1, response: response as BridgeEnvelope } };
    if (!await this.persistence.casJob(row, next, metadata.revision)) throw new BridgeError('BRIDGE_COMPLETION_CONFLICT', 409);
    await this.removePending(jobId, request.created_at);
  }
  async fetch(input: BridgeInput): Promise<Response> {
    const jobId = await this.admit(input); const deadline = this.now() + 330_000;
    while (true) {
      if (input.signal?.aborted) throw new BridgeError('BRIDGE_WAIT_ABORTED', 504);
      const row = await this.persistence.getJob(jobId);
      if (!row) throw new BridgeError('BRIDGE_JOB_INTEGRITY_FAILED');
      const { metadata } = this.validateJob(row, { job_id: jobId, subject: input.subject, channel: input.channel });
      if (row.status === 'done') {
        const result = validateBridgeResult(jobId, decryptBridge(this.configuration, 'response', jobId, metadata.response));
        return new Response([204, 205, 304].includes(result.status) ? null : new Uint8Array(decodeBase64(result.body_base64, 2 * 1024 * 1024)), { status: result.status, headers: result.headers });
      }
      if (this.now() >= deadline) throw new BridgeError('BRIDGE_RESULT_PENDING', 504);
      await this.wait(900, input.signal);
    }
  }
}
export function configuredBridgeQueue(): BridgeQueue {
  const configuration = bridgeConfiguration(); const client = getMnemeClient();
  if (!client) { configuration.key.fill(0); throw new BridgeError('BRIDGE_STORAGE_UNAVAILABLE'); }
  return new BridgeQueue(configuration, supabaseBridgePersistence(client));
}
export async function fetchBridge(input: BridgeInput): Promise<Response> {
  const queue = configuredBridgeQueue();
  try { return await queue.fetch(input); } finally { queue.configuration.key.fill(0); }
}
