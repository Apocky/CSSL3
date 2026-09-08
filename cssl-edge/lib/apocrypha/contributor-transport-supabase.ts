import { createClient, type SupabaseClient } from '@supabase/supabase-js';

import {
  ContributorTransportError,
  type ContributorNodeRecord,
  type ContributorTransportStore,
  type EnrollmentReplay,
  type LeaseReplay,
  type ResultReplay,
  type RevokeReplay,
} from './contributor-transport';

/**
 * Server-only persistence adapter for the contributor transport contract.
 *
 * PostgREST gives us HTTP calls, not a generic multi-statement transaction.
 * `ContributorTransportStore.transaction` therefore refuses to run until a
 * caller supplies a real transaction provider (for example an operation RPC
 * or a server-side database transaction).  A no-op wrapper would allow two
 * concurrent lease retries to diverge, so it is deliberately not provided.
 */

export const CONTRIBUTOR_TRANSPORT_TABLES = {
  node: 'apocrypha_contributor_node',
  enrollmentReplay: 'apocrypha_contributor_enrollment_replay',
  leaseReplay: 'apocrypha_contributor_lease_replay',
  resultReplay: 'apocrypha_contributor_result_replay',
  revokeReplay: 'apocrypha_contributor_revoke_replay',
} as const;

export const CONTRIBUTOR_TRANSPORT_SERVER_ENV = {
  url: 'APOCKY_HUB_SUPABASE_URL or NEXT_PUBLIC_SUPABASE_URL',
  serviceKey: 'SUPABASE_SERVICE_ROLE_KEY',
} as const;

export type ContributorTransportTransaction = <T>(operation: () => Promise<T>) => Promise<T>;

export interface SupabaseContributorTransportStoreOptions {
  readonly client: SupabaseClient;
  readonly transaction?: ContributorTransportTransaction;
}

export type ContributorTransportStoreAvailability =
  | { readonly ok: true; readonly store: SupabaseContributorTransportStore }
  | { readonly ok: false; readonly code: 'TRANSPORT_STORE_UNAVAILABLE'; readonly reason: string };

type JsonRecord = Record<string, unknown>;

function record(value: unknown): JsonRecord | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as JsonRecord
    : null;
}

function storeUnavailable(operation: string, error?: unknown): ContributorTransportError {
  const code = error && typeof error === 'object' && 'code' in error
    ? String((error as { code?: unknown }).code ?? 'unknown')
    : 'unknown';
  return new ContributorTransportError(
    'TRANSPORT_STORE_UNAVAILABLE',
    `contributor transport persistence unavailable during ${operation} (${code})`,
  );
}

function withStoreError<T>(operation: string, task: () => Promise<T>): Promise<T> {
  return task().catch((error: unknown) => {
    if (error instanceof ContributorTransportError) throw error;
    throw storeUnavailable(operation, error);
  });
}

function text(value: unknown, pattern: RegExp, maximum: number): string | null {
  return typeof value === 'string' && value.length > 0 && value.length <= maximum && pattern.test(value)
    ? value
    : null;
}

function integer(value: unknown, minimum: number, maximum: number): number | null {
  return Number.isSafeInteger(value) && Number(value) >= minimum && Number(value) <= maximum
    ? Number(value)
    : null;
}

function timestampMs(value: unknown): number | null {
  if (typeof value !== 'string' || value.length > 64) return null;
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : null;
}

function timestampIso(value: number): string {
  return new Date(value).toISOString();
}

function jsonObject(value: unknown, maximumBytes: number): JsonRecord | null {
  const source = record(value);
  if (!source || Buffer.byteLength(JSON.stringify(source), 'utf8') > maximumBytes) return null;
  return source;
}

function exactKeys(value: JsonRecord, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const keys = [...expected].sort();
  return actual.length === keys.length && actual.every((key, index) => key === keys[index]);
}

const IDENTIFIER = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;
const KEY_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/;
const HASH = /^[0-9a-f]{64}$/;
const PLATFORM = new Set(['windows-x64', 'macos-arm64', 'linux-x64', 'android', 'ios']);
const SIGNATURE = /^[A-Za-z0-9_-]{86}$/;

function capabilityList(value: unknown): readonly ['vector_dot'] | null {
  return Array.isArray(value)
    && value.length === 1
    && value[0] === 'vector_dot'
    ? ['vector_dot']
    : null;
}

function signedReceipt(
  value: unknown,
  schema: string,
  keys: readonly string[],
  maximumBytes = 64 * 1024,
): JsonRecord | null {
  const source = jsonObject(value, maximumBytes);
  if (!source || !exactKeys(source, keys) || source.schema_version !== schema || typeof source.signature_b64 !== 'string'
    || !SIGNATURE.test(source.signature_b64)) return null;
  return source;
}

function nodeRow(value: unknown): ContributorNodeRecord | null {
  const source = record(value);
  if (!source) return null;
  const nodeId = text(source.node_id, IDENTIFIER, 128);
  const nodeKeyId = text(source.node_key_id, KEY_ID, 128);
  const publicKey = text(source.node_public_key_spki_b64, /^[A-Za-z0-9+/]+={0,2}$/, 512);
  const platform = typeof source.platform === 'string' && PLATFORM.has(source.platform)
    ? source.platform as ContributorNodeRecord['platform']
    : null;
  const capabilities = capabilityList(source.capabilities);
  const status = source.status === 'active' || source.status === 'revoked' ? source.status : null;
  const revision = integer(source.revision, 1, 1_000_000_000);
  const enrolledAt = timestampMs(source.enrolled_at);
  const revokedAt = source.revoked_at === null ? null : timestampMs(source.revoked_at);
  const revokeReason = source.revoke_reason === null
    ? null
    : text(source.revoke_reason, /^[\x20-\x7e]+$/, 256);
  if (!nodeId || !nodeKeyId || !publicKey || !platform || !capabilities || !status
    || revision === null || enrolledAt === null || (status === 'active' && revokedAt !== null)
    || (status === 'revoked' && revokedAt === null) || (source.revoke_reason !== null && !revokeReason)) {
    return null;
  }
  return {
    node_id: nodeId,
    node_key_id: nodeKeyId,
    node_public_key_spki_b64: publicKey,
    platform,
    capabilities,
    status,
    revision,
    enrolled_at: enrolledAt,
    revoked_at: revokedAt,
    revoke_reason: revokeReason,
  };
}

function requestHash(value: unknown): string | null {
  return text(value, HASH, 64);
}

function enrollmentReplay(value: unknown): EnrollmentReplay | null {
  const source = record(value);
  const hash = requestHash(source?.request_hash);
  const receipt = signedReceipt(source?.receipt, 'apocrypha.contributor.enrollment-receipt.v1', [
    'schema_version', 'request_id', 'enrollment_id', 'node_id', 'node_key_id', 'controller_key_id',
    'status', 'revision', 'request_hash', 'issued_at', 'expires_at', 'signature_b64',
  ]);
  return hash && receipt ? { request_hash: hash, receipt: receipt as unknown as EnrollmentReplay['receipt'] } : null;
}

function revokeReplay(value: unknown): RevokeReplay | null {
  const source = record(value);
  const hash = requestHash(source?.request_hash);
  const receipt = signedReceipt(source?.receipt, 'apocrypha.contributor.revoke-receipt.v1', [
    'schema_version', 'request_id', 'node_id', 'controller_key_id', 'status', 'revision',
    'reason', 'revoked_at', 'signature_b64',
  ]);
  return hash && receipt ? { request_hash: hash, receipt: receipt as unknown as RevokeReplay['receipt'] } : null;
}

function leaseReplay(value: unknown): LeaseReplay | null {
  const source = record(value);
  const hash = requestHash(source?.request_hash);
  const dispatch = signedReceipt(source?.dispatch, 'apocrypha.contributor.lease-dispatch.v1', [
    'schema_version', 'dispatch_id', 'request_id', 'idempotency_key', 'node_id', 'lease', 'signature_b64',
  ], 128 * 1024);
  return hash && dispatch ? { request_hash: hash, dispatch: dispatch as unknown as LeaseReplay['dispatch'] } : null;
}

function resultReplay(value: unknown): ResultReplay | null {
  const source = record(value);
  const hash = requestHash(source?.submission_hash);
  const receipt = signedReceipt(source?.receipt, 'apocrypha.contributor.result-receipt.v1', [
    'schema_version', 'dispatch_id', 'request_id', 'idempotency_key', 'node_id', 'lease_id',
    'result_hash', 'status', 'accepted_at', 'signature_b64',
  ]);
  return hash && receipt ? { submission_hash: hash, receipt: receipt as unknown as ResultReplay['receipt'] } : null;
}

function requireParsed<T>(value: T | null, operation: string): T {
  if (value === null) throw storeUnavailable(`${operation}:invalid-row`);
  return value;
}

function persistedNode(node: ContributorNodeRecord): JsonRecord {
  return {
    node_id: node.node_id,
    node_key_id: node.node_key_id,
    node_public_key_spki_b64: node.node_public_key_spki_b64,
    platform: node.platform,
    capabilities: [...node.capabilities],
    status: node.status,
    revision: node.revision,
    enrolled_at: timestampIso(node.enrolled_at),
    revoked_at: node.revoked_at === null ? null : timestampIso(node.revoked_at),
    revoke_reason: node.revoke_reason,
  };
}

export class SupabaseContributorTransportStore implements ContributorTransportStore {
  private readonly client: SupabaseClient;
  private readonly transactionProvider: ContributorTransportTransaction | null;

  constructor(options: SupabaseContributorTransportStoreOptions) {
    if (!options.client || typeof options.client.from !== 'function') {
      throw new ContributorTransportError('TRANSPORT_STORE_UNAVAILABLE', 'Supabase client unavailable');
    }
    this.client = options.client;
    this.transactionProvider = options.transaction ?? null;
  }

  get transactional(): boolean {
    return this.transactionProvider !== null;
  }

  async transaction<T>(operation: () => Promise<T>): Promise<T> {
    if (!this.transactionProvider) {
      throw new ContributorTransportError(
        'TRANSPORT_STORE_UNAVAILABLE',
        'PostgREST does not provide a generic transaction; operation RPC or transaction provider required',
      );
    }
    return this.transactionProvider(operation);
  }

  async getNode(nodeId: string): Promise<ContributorNodeRecord | null> {
    return withStoreError('get-node', async () => {
      const { data, error } = await this.client
        .from(CONTRIBUTOR_TRANSPORT_TABLES.node)
        .select('node_id,node_key_id,node_public_key_spki_b64,platform,capabilities,status,revision,enrolled_at,revoked_at,revoke_reason')
        .eq('node_id', nodeId)
        .maybeSingle();
      if (error) throw error;
      if (data === null) return null;
      return requireParsed(nodeRow(data), 'get-node');
    });
  }

  async putNode(node: ContributorNodeRecord): Promise<void> {
    return withStoreError('put-node', async () => {
      const { error } = await this.client
        .from(CONTRIBUTOR_TRANSPORT_TABLES.node)
        .upsert(persistedNode(node), { onConflict: 'node_id' });
      if (error) throw error;
    });
  }

  async getEnrollmentReplay(requestId: string): Promise<EnrollmentReplay | null> {
    return withStoreError('get-enrollment-replay', async () => {
      const { data, error } = await this.client
        .from(CONTRIBUTOR_TRANSPORT_TABLES.enrollmentReplay)
        .select('request_hash,receipt')
        .eq('request_id', requestId)
        .maybeSingle();
      if (error) throw error;
      if (data === null) return null;
      return requireParsed(enrollmentReplay(data), 'get-enrollment-replay');
    });
  }

  async putEnrollmentReplay(requestId: string, replay: EnrollmentReplay): Promise<void> {
    return withStoreError('put-enrollment-replay', async () => {
      const { error } = await this.client
        .from(CONTRIBUTOR_TRANSPORT_TABLES.enrollmentReplay)
        .upsert({ request_id: requestId, request_hash: replay.request_hash, receipt: replay.receipt }, { onConflict: 'request_id' });
      if (error) throw error;
    });
  }

  async getRevokeReplay(requestId: string): Promise<RevokeReplay | null> {
    return withStoreError('get-revoke-replay', async () => {
      const { data, error } = await this.client
        .from(CONTRIBUTOR_TRANSPORT_TABLES.revokeReplay)
        .select('request_hash,receipt')
        .eq('request_id', requestId)
        .maybeSingle();
      if (error) throw error;
      if (data === null) return null;
      return requireParsed(revokeReplay(data), 'get-revoke-replay');
    });
  }

  async putRevokeReplay(requestId: string, replay: RevokeReplay): Promise<void> {
    return withStoreError('put-revoke-replay', async () => {
      const { error } = await this.client
        .from(CONTRIBUTOR_TRANSPORT_TABLES.revokeReplay)
        .upsert({ request_id: requestId, request_hash: replay.request_hash, receipt: replay.receipt }, { onConflict: 'request_id' });
      if (error) throw error;
    });
  }

  async getLeaseReplay(nodeId: string, idempotencyKey: string): Promise<LeaseReplay | null> {
    return withStoreError('get-lease-replay', async () => {
      const { data, error } = await this.client
        .from(CONTRIBUTOR_TRANSPORT_TABLES.leaseReplay)
        .select('request_hash,dispatch')
        .eq('node_id', nodeId)
        .eq('idempotency_key', idempotencyKey)
        .maybeSingle();
      if (error) throw error;
      if (data === null) return null;
      return requireParsed(leaseReplay(data), 'get-lease-replay');
    });
  }

  async getLeaseByDispatch(dispatchId: string): Promise<LeaseReplay | null> {
    return withStoreError('get-lease-dispatch', async () => {
      const { data, error } = await this.client
        .from(CONTRIBUTOR_TRANSPORT_TABLES.leaseReplay)
        .select('request_hash,dispatch')
        .eq('dispatch_id', dispatchId)
        .maybeSingle();
      if (error) throw error;
      if (data === null) return null;
      return requireParsed(leaseReplay(data), 'get-lease-dispatch');
    });
  }

  async putLeaseReplay(nodeId: string, idempotencyKey: string, replay: LeaseReplay): Promise<void> {
    return withStoreError('put-lease-replay', async () => {
      const dispatch = record(replay.dispatch);
      if (!dispatch || typeof dispatch.dispatch_id !== 'string') throw storeUnavailable('put-lease-replay:invalid-dispatch');
      const { error } = await this.client
        .from(CONTRIBUTOR_TRANSPORT_TABLES.leaseReplay)
        .upsert({
          node_id: nodeId,
          idempotency_key: idempotencyKey,
          dispatch_id: dispatch.dispatch_id,
          request_hash: replay.request_hash,
          dispatch: replay.dispatch,
        }, { onConflict: 'node_id,idempotency_key' });
      if (error) throw error;
    });
  }

  async getResultReplay(dispatchId: string): Promise<ResultReplay | null> {
    return withStoreError('get-result-replay', async () => {
      const { data, error } = await this.client
        .from(CONTRIBUTOR_TRANSPORT_TABLES.resultReplay)
        .select('submission_hash,receipt')
        .eq('dispatch_id', dispatchId)
        .maybeSingle();
      if (error) throw error;
      if (data === null) return null;
      return requireParsed(resultReplay(data), 'get-result-replay');
    });
  }

  async putResultReplay(dispatchId: string, replay: ResultReplay): Promise<void> {
    return withStoreError('put-result-replay', async () => {
      const { error } = await this.client
        .from(CONTRIBUTOR_TRANSPORT_TABLES.resultReplay)
        .upsert({
          dispatch_id: dispatchId,
          submission_hash: replay.submission_hash,
          receipt: replay.receipt,
        }, { onConflict: 'dispatch_id' });
      if (error) throw error;
    });
  }
}

/**
 * Resolve the server-only Supabase client. The anon key is intentionally not
 * accepted: this adapter is the controller persistence boundary and must not
 * silently depend on client/RLS behavior that differs across deployments.
 */
export function createSupabaseContributorTransportStore(
  options: { readonly transaction?: ContributorTransportTransaction } = {},
): ContributorTransportStoreAvailability {
  const url = process.env.APOCKY_HUB_SUPABASE_URL ?? process.env.NEXT_PUBLIC_SUPABASE_URL;
  const serviceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!url || !serviceKey) {
    return {
      ok: false,
      code: 'TRANSPORT_STORE_UNAVAILABLE',
      reason: 'server Supabase URL and service-role configuration are required',
    };
  }
  try {
    const client = createClient(url, serviceKey, { auth: { persistSession: false, autoRefreshToken: false } });
    return { ok: true, store: new SupabaseContributorTransportStore({ client, transaction: options.transaction }) };
  } catch {
    return { ok: false, code: 'TRANSPORT_STORE_UNAVAILABLE', reason: 'server Supabase client could not be initialized' };
  }
}

/** Test seam for routes and adapter tests; production callers must not use it. */
export function createSupabaseContributorTransportStoreForClient(
  client: SupabaseClient,
  transaction?: ContributorTransportTransaction,
): SupabaseContributorTransportStore {
  return new SupabaseContributorTransportStore({ client, transaction });
}
