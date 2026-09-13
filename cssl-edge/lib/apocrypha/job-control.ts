import { createHash, createHmac, timingSafeEqual } from 'node:crypto';

import { createClient, type SupabaseClient } from '@supabase/supabase-js';

export const APOCRYPHA_MODEL_ALIAS = process.env.APOCRYPHA_MODEL_ALIAS ?? 'qwen35-35b-a3b-q4';
export const APOCRYPHA_PROFILE_HASH = process.env.APOCRYPHA_PROFILE_HASH ?? '5d390055297aed74dbba092eb313dc8c4bf4e551ca4bf2c50fed16c8cb3a21a9';
export const APOCRYPHA_TOOL_REGISTRY_VERSION = process.env.APOCRYPHA_TOOL_REGISTRY_VERSION ?? 'apocrypha-readonly-v1';
export const APOCRYPHA_MEMORY_MANIFEST_HASH = process.env.APOCRYPHA_MEMORY_MANIFEST_HASH ?? '307a86ce2ec83a37ad30f86327195e47259167728cf32e4276af377f08988273';

export type ApocryphaJobKind =
  | 'apocky_chat'
  | 'interpretation'
  | 'followup'
  | 'summary'
  | 'astrology_natal'
  | 'astrology_synastry'
  | 'astrology_transit'
  | 'continuation';

export type ApocryphaJobStatus =
  | 'queued'
  | 'leased'
  | 'running'
  | 'cancel_requested'
  | 'succeeded'
  | 'failed'
  | 'cancelled';

export interface JobIdentity {
  tenantId: string;
  principalId: string;
}

export interface EnqueueJobInput {
  identity: JobIdentity;
  kind: ApocryphaJobKind;
  capability: 'apocky_owner_chat' | 'chaos_tarot_reading';
  request: Record<string, unknown>;
  idempotencyKey: string;
  idempotencyScope?: string;
  priority?: number;
  maxAttempts?: number;
}

export interface ApocryphaJobRow {
  id: string;
  tenant_id: string;
  owner_principal_id: string;
  kind: ApocryphaJobKind;
  capability: string;
  status: ApocryphaJobStatus;
  request: Record<string, unknown>;
  current_attempt_id: string | null;
  terminal_revision_id?: string | null;
  model_alias: string;
  profile_hash: string;
  tool_registry_version: string;
  memory_manifest_hash: string;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
  error_code: string | null;
  error_detail: string | null;
}

export interface OwnerChatToolCall {
  name: string;
  ok: boolean;
  elapsed_ms?: number;
  error?: string | null;
}

export interface OwnerChatMessage {
  id: string;
  role: 'user' | 'apocrypha';
  text: string;
  ts_iso: string;
  tool_trace: OwnerChatToolCall[];
  truncated?: boolean;
}

export interface OwnerChatConversationSummary {
  id: string;
  title: string;
  last_active_iso: string;
  message_count: number;
  message_count_is_lower_bound: boolean;
  state: 'active';
}

export interface OwnerChatConversationList {
  conversations: OwnerChatConversationSummary[];
  truncated: boolean;
  maximum_conversations: number;
}

export interface OwnerChatConversation {
  conversation: OwnerChatConversationSummary;
  messages: OwnerChatMessage[];
  history_window: {
    truncated: boolean;
    row_window_truncated: boolean;
    maximum_jobs: number;
    maximum_bytes: number;
  };
}

export interface OwnerChatIdempotentJob {
  job: ApocryphaJobRow;
  request: Record<string, unknown>;
}

let serviceClient: SupabaseClient | null | undefined;

function canonicalize(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, item]) => [key, canonicalize(item)]),
    );
  }
  return value;
}

export function requestHash(request: Record<string, unknown>): string {
  return createHash('sha256').update(JSON.stringify(canonicalize(request))).digest('hex');
}

export function getApocryphaServiceClient(): SupabaseClient {
  if (serviceClient) return serviceClient;
  const url = process.env.APOCKY_HUB_SUPABASE_URL ?? process.env.NEXT_PUBLIC_SUPABASE_URL;
  const key = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!url || !key) throw new Error('APOCRYPHA_CONTROL_PLANE_UNCONFIGURED');
  serviceClient = createClient(url, key, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
  return serviceClient;
}

export function resetApocryphaServiceClientForTests(): void {
  serviceClient = undefined;
}

export function bearerToken(authorization: string | string[] | undefined): string | null {
  const header = Array.isArray(authorization) ? authorization[0] : authorization;
  if (!header?.startsWith('Bearer ')) return null;
  const token = header.slice('Bearer '.length).trim();
  return token || null;
}

export function secretMatches(presented: string | null, expected: string | undefined): boolean {
  if (!presented || !expected) return false;
  const observed = createHash('sha256').update(presented).digest();
  const wanted = createHash('sha256').update(expected).digest();
  return timingSafeEqual(observed, wanted);
}

export function assertWorkerRequest(authorization: string | string[] | undefined): string {
  const header = Array.isArray(authorization) ? null : authorization;
  const token = /^Bearer (apn_[0-9a-f]{64})$/.exec(header ?? '')?.[1];
  if (!token) throw new Error('WORKER_UNAUTHORIZED');
  return token;
}

export function assertChaosBridgeRequest(authorization: string | string[] | undefined): void {
  const token = bearerToken(authorization);
  if (!secretMatches(token, process.env.CHAOS_TAROT_BRIDGE_TOKEN)) throw new Error('BRIDGE_UNAUTHORIZED');
}

function oneHeader(value: string | string[] | undefined): string {
  return Array.isArray(value) ? (value[0] ?? '') : (value ?? '');
}

export function assertChaosBridgeSignature(input: {
  authorization: string | string[] | undefined;
  method: string | undefined;
  url: string | undefined;
  body: unknown;
  headers: Record<string, string | string[] | undefined>;
}): string {
  assertChaosBridgeRequest(input.authorization);
  const secret = process.env.CHAOS_TAROT_BRIDGE_TOKEN as string;
  const timestamp = oneHeader(input.headers['x-apocrypha-timestamp']);
  const principal = oneHeader(input.headers['x-apocrypha-principal']);
  const origin = oneHeader(input.headers['x-apocrypha-origin']);
  const tenant = oneHeader(input.headers['x-apocrypha-tenant']);
  const contentHash = oneHeader(input.headers['x-apocrypha-content-sha256']);
  const signature = oneHeader(input.headers['x-apocrypha-signature']);
  if (origin !== 'chaos-tarot' || tenant !== 'chaos-tarot' || !principal.startsWith('ct_')) {
    throw new Error('BRIDGE_UNAUTHORIZED');
  }
  const observedAt = Number(timestamp);
  if (!Number.isFinite(observedAt) || Math.abs(Date.now() - observedAt) > 300_000) {
    throw new Error('BRIDGE_UNAUTHORIZED');
  }
  const bodyText = input.body === undefined || input.body === null
    ? ''
    : typeof input.body === 'string' ? input.body : JSON.stringify(input.body);
  const wantedContentHash = createHash('sha256').update(bodyText).digest('hex');
  if (!secretMatches(contentHash, wantedContentHash)) throw new Error('BRIDGE_UNAUTHORIZED');
  const path = input.url?.startsWith('/') ? input.url : '/';
  const stringToSign = `${timestamp}\n${(input.method ?? 'GET').toUpperCase()}\n${path}\n${principal}\n${contentHash}`;
  const wantedSignature = `v1=${createHmac('sha256', secret).update(stringToSign).digest('hex')}`;
  if (!secretMatches(signature, wantedSignature)) throw new Error('BRIDGE_UNAUTHORIZED');
  return principal;
}

export async function ensureOwnerIdentity(authUserId: string): Promise<JobIdentity> {
  const client = getApocryphaServiceClient();
  const { data, error } = await client.rpc('apocrypha_ensure_owner_principal', {
    p_tenant_slug: 'apocky-owner',
    p_tenant_display_name: 'Apocky owner',
    p_auth_user_id: authUserId,
  });
  if (error) throw new Error(`OWNER_IDENTITY_FAILED:${error.code ?? 'unknown'}`);
  const row = Array.isArray(data) ? data[0] : data;
  if (!row?.tenant_id || !row?.principal_id) throw new Error('OWNER_IDENTITY_EMPTY');
  return { tenantId: String(row.tenant_id), principalId: String(row.principal_id) };
}

export async function ensureExternalIdentity(subject: string): Promise<JobIdentity> {
  const client = getApocryphaServiceClient();
  const subjectHash = createHash('sha256').update(`chaos-tarot:${subject}`).digest('hex');
  const { data: existingTenant, error: tenantReadError } = await client
    .from('apocrypha_tenant')
    .select('id')
    .eq('slug', 'chaos-tarot')
    .maybeSingle();
  if (tenantReadError) throw new Error(`TENANT_READ_FAILED:${tenantReadError.code ?? 'unknown'}`);
  let tenantId = existingTenant?.id ? String(existingTenant.id) : null;
  if (!tenantId) {
    const { data: inserted, error: insertError } = await client
      .from('apocrypha_tenant')
      .insert({ slug: 'chaos-tarot', display_name: 'Chaos Tarot', status: 'active' })
      .select('id')
      .single();
    if (insertError && insertError.code !== '23505') {
      throw new Error(`TENANT_CREATE_FAILED:${insertError.code ?? 'unknown'}`);
    }
    if (inserted?.id) tenantId = String(inserted.id);
    if (!tenantId) {
      const { data: raced, error: racedError } = await client
        .from('apocrypha_tenant').select('id').eq('slug', 'chaos-tarot').single();
      if (racedError || !raced?.id) throw new Error('TENANT_RACE_RECOVERY_FAILED');
      tenantId = String(raced.id);
    }
  }

  const { data: existing, error: readError } = await client
    .from('apocrypha_principal')
    .select('id')
    .eq('tenant_id', tenantId)
    .eq('external_subject_hash', subjectHash)
    .maybeSingle();
  if (readError) throw new Error(`PRINCIPAL_READ_FAILED:${readError.code ?? 'unknown'}`);
  let principalId = existing?.id ? String(existing.id) : null;
  if (!principalId) {
    const { data: inserted, error: insertError } = await client
      .from('apocrypha_principal')
      .insert({
        tenant_id: tenantId,
        principal_kind: 'member',
        external_subject_hash: subjectHash,
        display_name: 'Chaos Tarot member',
        status: 'active',
      })
      .select('id')
      .single();
    if (insertError && insertError.code !== '23505') {
      throw new Error(`PRINCIPAL_CREATE_FAILED:${insertError.code ?? 'unknown'}`);
    }
    if (inserted?.id) principalId = String(inserted.id);
    if (!principalId) {
      const { data: raced, error: racedError } = await client
        .from('apocrypha_principal')
        .select('id')
        .eq('tenant_id', tenantId)
        .eq('external_subject_hash', subjectHash)
        .single();
      if (racedError || !raced?.id) throw new Error('PRINCIPAL_RACE_RECOVERY_FAILED');
      principalId = String(raced.id);
    }
  }
  return { tenantId, principalId };
}

export async function enqueueApocryphaJob(input: EnqueueJobInput): Promise<ApocryphaJobRow> {
  const client = getApocryphaServiceClient();
  const { data, error } = await client.rpc('apocrypha_enqueue_job', {
    p_tenant_id: input.identity.tenantId,
    p_owner_principal_id: input.identity.principalId,
    p_kind: input.kind,
    p_capability: input.capability,
    p_request: input.request,
    p_request_hash: requestHash(input.request),
    p_idempotency_scope: input.idempotencyScope ?? 'conversation',
    p_idempotency_key: input.idempotencyKey,
    p_model_alias: APOCRYPHA_MODEL_ALIAS,
    p_profile_hash: APOCRYPHA_PROFILE_HASH,
    p_tool_registry_version: APOCRYPHA_TOOL_REGISTRY_VERSION,
    p_memory_manifest_hash: APOCRYPHA_MEMORY_MANIFEST_HASH,
    p_priority: input.priority ?? 0,
    p_max_attempts: input.maxAttempts ?? 3,
    p_available_at: new Date().toISOString(),
    p_parent_job_id: null,
    p_job_role: 'primary',
  });
  if (error) {
    const code = error.message?.toLowerCase().includes('idempotency') ? 'IDEMPOTENCY_CONFLICT' : (error.code ?? 'unknown');
    throw new Error(`JOB_ENQUEUE_FAILED:${code}`);
  }
  if (!data?.id) throw new Error('JOB_ENQUEUE_EMPTY');
  return data as ApocryphaJobRow;
}

export async function readApocryphaJob(jobId: string, identity: JobIdentity) {
  const client = getApocryphaServiceClient();
  const { data: job, error: jobError } = await client
    .from('apocrypha_job')
    .select('*')
    .eq('id', jobId)
    .eq('tenant_id', identity.tenantId)
    .eq('owner_principal_id', identity.principalId)
    .maybeSingle();
  if (jobError) throw new Error(`JOB_READ_FAILED:${jobError.code ?? 'unknown'}`);
  if (!job) return null;

  const attemptId = job.current_attempt_id ? String(job.current_attempt_id) : null;
  const [chunksResult, revisionsResult, eventsResult] = await Promise.all([
    attemptId
      ? client.from('apocrypha_job_chunk').select('seq,chunk_kind,delta,metadata,created_at').eq('attempt_id', attemptId).order('seq', { ascending: true })
      : Promise.resolve({ data: [], error: null }),
    client.from('apocrypha_job_revision').select('id,revision_no,revision_role,content,provenance,usage,created_at').eq('job_id', jobId).order('revision_no', { ascending: false }).limit(4),
    client.from('apocrypha_job_event')
      .select('ordinal,event_type,outcome,severity,source,flagged,metadata,occurred_at')
      .eq('job_id', jobId)
      .order('ordinal', { ascending: false })
      .limit(256),
  ]);
  if (chunksResult.error) throw new Error(`JOB_CHUNKS_FAILED:${chunksResult.error.code ?? 'unknown'}`);
  if (revisionsResult.error) throw new Error(`JOB_REVISIONS_FAILED:${revisionsResult.error.code ?? 'unknown'}`);
  if (eventsResult.error) throw new Error(`JOB_EVENTS_FAILED:${eventsResult.error.code ?? 'unknown'}`);
  return {
    job: job as ApocryphaJobRow,
    chunks: chunksResult.data ?? [],
    revisions: revisionsResult.data ?? [],
    events: [...(eventsResult.data ?? [])].reverse(),
  };
}

const OWNER_CHAT_CONVERSATION_LIMIT = 256;
const OWNER_CHAT_DETAIL_JOB_LIMIT = 64;
const OWNER_CHAT_DETAIL_BYTES = 512 * 1024;
const OWNER_CHAT_HISTORY_MESSAGES = 20;
const OWNER_CHAT_HISTORY_MESSAGE_BYTES = 10_000;
const OWNER_CHAT_HISTORY_BYTES = 128 * 1024;
const OWNER_CHAT_RESPONSE_BYTES = 65_536;
const OWNER_CHAT_REVISION_QUERY_CHUNK = 8;
const OWNER_CHAT_ID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

interface OwnerChatJobProjection {
  id: string;
  status: ApocryphaJobStatus;
  request: Record<string, unknown>;
  terminal_revision_id: string | null;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
}

interface OwnerChatJobRead {
  jobs: OwnerChatJobProjection[];
  rowWindowTruncated: boolean;
}

interface OwnerChatRevisionProjection {
  id: string;
  job_id: string;
  content: string;
  content_truncated: boolean;
  provenance: Record<string, unknown> | null;
  usage: Record<string, unknown> | null;
  created_at: string;
}

function record(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
}

function ownerChatConversationId(value: unknown): string | null {
  return typeof value === 'string' && OWNER_CHAT_ID.test(value) ? value.toLowerCase() : null;
}

function ownerChatJobConversationId(job: Pick<OwnerChatJobProjection, 'id' | 'request'>): string | null {
  return ownerChatConversationId(job.request.conversation_id) ?? ownerChatConversationId(job.id);
}

function ownerChatPrompt(request: Record<string, unknown>): string | null {
  return typeof request.prompt === 'string' && request.prompt.trim()
    ? request.prompt.trim().slice(0, 32_000)
    : null;
}

function ownerChatRetryOf(request: Record<string, unknown>): string | null {
  return ownerChatConversationId(request.retry_of_job_id);
}

export async function readOwnerChatRetrySource(identity: JobIdentity, jobId: string) {
  const normalized = ownerChatConversationId(jobId);
  if (!normalized) throw new Error('OWNER_RETRY_JOB_ID_INVALID');
  const client = getApocryphaServiceClient();
  const { data, error } = await client.from('apocrypha_job').select('*')
    .eq('id', normalized).eq('tenant_id', identity.tenantId)
    .eq('owner_principal_id', identity.principalId).eq('kind', 'apocky_chat')
    .eq('capability', 'apocky_owner_chat').limit(1).maybeSingle();
  if (error) throw new Error(`OWNER_RETRY_READ_FAILED:${error.code ?? 'unknown'}`);
  return data ? { job: data as ApocryphaJobRow, request: record((data as ApocryphaJobRow).request) } : null;
}

function utf8Prefix(value: string, maximumBytes: number): string {
  if (Buffer.byteLength(value, 'utf8') <= maximumBytes) return value;
  let result = '';
  let used = 0;
  for (const character of value) {
    const bytes = Buffer.byteLength(character, 'utf8');
    if (used + bytes > maximumBytes) break;
    result += character;
    used += bytes;
  }
  return result;
}

function ownerChatToolTrace(value: unknown): OwnerChatToolCall[] {
  if (!Array.isArray(value)) return [];
  return value.slice(0, 64).flatMap((item): OwnerChatToolCall[] => {
    const tool = record(item);
    if (typeof tool.name !== 'string' || !tool.name.trim() || typeof tool.ok !== 'boolean') return [];
    return [{
      name: tool.name.trim().slice(0, 160),
      ok: tool.ok,
      ...(typeof tool.elapsed_ms === 'number' && Number.isFinite(tool.elapsed_ms)
        ? { elapsed_ms: Math.max(0, tool.elapsed_ms) }
        : {}),
      ...(typeof tool.error === 'string' ? { error: tool.error.slice(0, 500) } : {}),
    }];
  });
}

async function readOwnerChatJobRows(
  identity: JobIdentity,
  conversationId: string,
): Promise<OwnerChatJobRead> {
  const client = getApocryphaServiceClient();
  const query = () => client
    .from('apocrypha_job')
    .select('id,status,request_prompt:request->>prompt,request_conversation_id:request->>conversation_id,request_retry_of_job_id:request->>retry_of_job_id,terminal_revision_id,created_at,updated_at,completed_at')
    .eq('tenant_id', identity.tenantId)
    .eq('owner_principal_id', identity.principalId)
    .eq('kind', 'apocky_chat')
    .eq('capability', 'apocky_owner_chat');
  const results = await Promise.all([
    query()
      .eq('request->>conversation_id', conversationId)
      .order('created_at', { ascending: false })
      .order('id', { ascending: false })
      .limit(OWNER_CHAT_DETAIL_JOB_LIMIT + 1),
    query().eq('id', conversationId).limit(1),
  ]);
  const failed = results.find((result) => result.error)?.error;
  if (failed) throw new Error(`OWNER_HISTORY_READ_FAILED:${failed.code ?? 'unknown'}`);
  const projectRows = (items: unknown[]): OwnerChatJobProjection[] => items.flatMap((item): OwnerChatJobProjection[] => {
    const row = record(item);
    const storedRequest = record(row.request);
    const request = {
      prompt: typeof row.request_prompt === 'string' ? row.request_prompt : storedRequest.prompt,
      conversation_id: typeof row.request_conversation_id === 'string'
        ? row.request_conversation_id
        : storedRequest.conversation_id,
      retry_of_job_id: typeof row.request_retry_of_job_id === 'string'
        ? row.request_retry_of_job_id
        : storedRequest.retry_of_job_id,
    };
    if (typeof row.id !== 'string'
      || typeof row.status !== 'string'
      || typeof row.created_at !== 'string'
      || typeof row.updated_at !== 'string'
      || !ownerChatConversationId(row.id)) return [];
    return [{
      id: row.id,
      status: row.status as ApocryphaJobStatus,
      request,
      terminal_revision_id: typeof row.terminal_revision_id === 'string' ? row.terminal_revision_id : null,
      created_at: row.created_at,
      updated_at: row.updated_at,
      completed_at: typeof row.completed_at === 'string' ? row.completed_at : null,
    }];
  });
  const primaryRows = projectRows(results[0]?.data ?? [])
    .sort((left, right) => (
      right.created_at.localeCompare(left.created_at) || right.id.localeCompare(left.id)
    ));
  const legacyRows = projectRows(results[1]?.data ?? []);
  const primaryIds = new Set(primaryRows.map((job) => job.id));
  const distinctLegacyRows = legacyRows.filter((job) => !primaryIds.has(job.id)).slice(0, 1);
  const primaryLimit = OWNER_CHAT_DETAIL_JOB_LIMIT - distinctLegacyRows.length;
  const rowWindowTruncated = primaryRows.length > primaryLimit;
  const jobs = [...primaryRows.slice(0, primaryLimit), ...distinctLegacyRows];
  return { jobs, rowWindowTruncated };
}

async function readOwnerChatRevisionRows(
  _identity: JobIdentity,
  jobs: OwnerChatJobProjection[],
): Promise<Map<string, OwnerChatRevisionProjection>> {
  const terminalJobs = jobs.filter((job): job is OwnerChatJobProjection & { terminal_revision_id: string } => (
    Boolean(job.terminal_revision_id)
  ));
  if (terminalJobs.length === 0) return new Map();
  const client = getApocryphaServiceClient();
  const revisions = new Map<string, OwnerChatRevisionProjection>();
  for (let index = 0; index < terminalJobs.length; index += OWNER_CHAT_REVISION_QUERY_CHUNK) {
    const chunk = terminalJobs.slice(index, index + OWNER_CHAT_REVISION_QUERY_CHUNK);
    const expectedJobs = new Map(chunk.map((job) => [job.terminal_revision_id, job.id]));
    const result = await client.rpc('apocrypha_project_owner_chat_revisions', {
      p_tenant_id: _identity.tenantId,
      p_owner_principal_id: _identity.principalId,
      p_job_ids: chunk.map((job) => job.id),
      p_revision_ids: chunk.map((job) => job.terminal_revision_id),
    });
    if (result.error) throw new Error(`OWNER_HISTORY_REVISIONS_FAILED:${result.error.code ?? 'unknown'}`);
    for (const item of result.data ?? []) {
      const row = record(item);
      if (typeof row.id !== 'string'
        || typeof row.job_id !== 'string'
        || typeof row.content !== 'string'
        || typeof row.created_at !== 'string'
        || expectedJobs.get(row.id) !== row.job_id) continue;
      revisions.set(row.id, {
        id: row.id,
        job_id: row.job_id,
        content: utf8Prefix(row.content, OWNER_CHAT_RESPONSE_BYTES),
        content_truncated: row.content_truncated === true
          || Buffer.byteLength(row.content, 'utf8') > OWNER_CHAT_RESPONSE_BYTES,
        provenance: Object.keys(record(row.provenance)).length ? record(row.provenance) : null,
        usage: Object.keys(record(row.usage)).length ? record(row.usage) : null,
        created_at: row.created_at,
      });
    }
  }
  return revisions;
}

function orderedOwnerChatJobs(
  conversationId: string,
  jobs: OwnerChatJobProjection[],
): OwnerChatJobProjection[] {
  return jobs
    .filter((job) => ownerChatJobConversationId(job) === conversationId)
    .sort((left, right) => left.created_at.localeCompare(right.created_at) || left.id.localeCompare(right.id));
}

function summarizeOwnerChatConversation(
  conversationId: string,
  jobs: OwnerChatJobProjection[],
  messageCountIsLowerBound = false,
): OwnerChatConversationSummary | null {
  const ordered = orderedOwnerChatJobs(conversationId, jobs);
  if (ordered.length === 0) return null;
  const firstPrompt = ordered.map((job) => ownerChatPrompt(job.request)).find(Boolean) ?? 'New conversation';
  const lastActive = ordered.reduce(
    (latest, job) => latest.localeCompare(job.updated_at) >= 0 ? latest : job.updated_at,
    ordered[0]?.updated_at ?? ordered[0]?.created_at ?? new Date(0).toISOString(),
  );
  const seenJobIds = new Set<string>();
  const messageCount = ordered.reduce((count, job) => {
    const suppressPrompt = Boolean(ownerChatRetryOf(job.request) && seenJobIds.has(ownerChatRetryOf(job.request)!));
    seenJobIds.add(job.id);
    return count + (!suppressPrompt && ownerChatPrompt(job.request) ? 1 : 0)
      + (job.status === 'succeeded' && job.terminal_revision_id ? 1 : 0);
  }, 0);
  return {
    id: conversationId,
    title: firstPrompt.replace(/\s+/g, ' ').slice(0, 80),
    last_active_iso: lastActive,
    message_count: messageCount,
    message_count_is_lower_bound: messageCountIsLowerBound,
    state: 'active',
  };
}

function boundedOwnerChatMessages(
  conversation: OwnerChatConversationSummary,
  messages: OwnerChatMessage[],
  rowWindowTruncated: boolean,
): { messages: OwnerChatMessage[]; history_window: OwnerChatConversation['history_window'] } {
  const historyWindow = {
    truncated: rowWindowTruncated,
    row_window_truncated: rowWindowTruncated,
    maximum_jobs: OWNER_CHAT_DETAIL_JOB_LIMIT,
    maximum_bytes: OWNER_CHAT_DETAIL_BYTES,
  };
  const selected: OwnerChatMessage[] = [];
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const candidate = [messages[index]!, ...selected];
    const projected = { conversation, messages: candidate, history_window: historyWindow };
    if (Buffer.byteLength(JSON.stringify(projected), 'utf8') > OWNER_CHAT_DETAIL_BYTES) break;
    selected.unshift(messages[index]!);
  }
  if (selected[0]?.role === 'apocrypha') selected.shift();
  historyWindow.truncated = historyWindow.truncated || selected.length < messages.length;
  return { messages: selected, history_window: historyWindow };
}

function projectOwnerChatConversation(
  conversationId: string,
  jobs: OwnerChatJobProjection[],
  revisions: Map<string, OwnerChatRevisionProjection>,
  rowWindowTruncated: boolean,
): OwnerChatConversation | null {
  const ordered = orderedOwnerChatJobs(conversationId, jobs);
  if (ordered.length === 0) return null;
  const messages: OwnerChatMessage[] = [];
  const seenJobIds = new Set<string>();
  for (const job of ordered) {
    const prompt = ownerChatPrompt(job.request);
    const suppressPrompt = Boolean(ownerChatRetryOf(job.request) && seenJobIds.has(ownerChatRetryOf(job.request)!));
    if (prompt && !suppressPrompt) messages.push({
      id: `${job.id}:user`, role: 'user', text: prompt, ts_iso: job.created_at, tool_trace: [],
    });
    const revision = job.status === 'succeeded' && job.terminal_revision_id
      ? revisions.get(job.terminal_revision_id)
      : undefined;
    if (revision?.content) messages.push({
      id: `${job.id}:apocrypha`,
      role: 'apocrypha',
      text: revision.content,
      ts_iso: revision.created_at || job.completed_at || job.updated_at,
      tool_trace: ownerChatToolTrace(revision.provenance?.tool_calls),
      truncated: revision.content_truncated,
    });
    seenJobIds.add(job.id);
  }
  const conversation = summarizeOwnerChatConversation(conversationId, ordered, rowWindowTruncated);
  if (!conversation) return null;
  const bounded = boundedOwnerChatMessages(conversation, messages, rowWindowTruncated);
  return {
    conversation,
    ...bounded,
  };
}

export async function listOwnerChatConversations(identity: JobIdentity): Promise<OwnerChatConversationList> {
  const client = getApocryphaServiceClient();
  const { data, error } = await client.rpc('apocrypha_list_owner_chat_conversations', {
    p_tenant_id: identity.tenantId,
    p_owner_principal_id: identity.principalId,
    p_limit: OWNER_CHAT_CONVERSATION_LIMIT + 1,
  });
  if (error) throw new Error(`OWNER_HISTORY_LIST_FAILED:${error.code ?? 'unknown'}`);
  const projected = (data ?? []).flatMap((item: unknown): OwnerChatConversationSummary[] => {
    const row = record(item);
    const id = ownerChatConversationId(row.conversation_id);
    if (!id || typeof row.last_active_iso !== 'string') return [];
    const rawCount = Number(row.message_count);
    return [{
      id,
      title: typeof row.title === 'string' && row.title.trim()
        ? row.title.replace(/\s+/g, ' ').slice(0, 80)
        : 'New conversation',
      last_active_iso: row.last_active_iso,
      message_count: Number.isSafeInteger(rawCount) && rawCount >= 0 ? rawCount : 0,
      message_count_is_lower_bound: false,
      state: 'active',
    }];
  });
  return {
    conversations: projected.slice(0, OWNER_CHAT_CONVERSATION_LIMIT),
    truncated: projected.length > OWNER_CHAT_CONVERSATION_LIMIT,
    maximum_conversations: OWNER_CHAT_CONVERSATION_LIMIT,
  };
}

export async function readOwnerChatIdempotentJob(
  identity: JobIdentity,
  conversationId: string,
  idempotencyKey: string,
): Promise<OwnerChatIdempotentJob | null> {
  const normalized = ownerChatConversationId(conversationId);
  if (!normalized) throw new Error('OWNER_CONVERSATION_ID_INVALID');
  const client = getApocryphaServiceClient();
  const { data, error } = await client
    .from('apocrypha_job')
    .select('*')
    .eq('tenant_id', identity.tenantId)
    .eq('owner_principal_id', identity.principalId)
    .eq('kind', 'apocky_chat')
    .eq('capability', 'apocky_owner_chat')
    .eq('idempotency_scope', `owner-chat:${normalized}`)
    .eq('idempotency_key', idempotencyKey)
    .limit(1)
    .maybeSingle();
  if (error) throw new Error(`OWNER_IDEMPOTENCY_READ_FAILED:${error.code ?? 'unknown'}`);
  if (!data) return null;
  const job = data as ApocryphaJobRow;
  return { job, request: record(job.request) };
}

export async function readOwnerChatConversation(
  identity: JobIdentity,
  conversationId: string,
): Promise<OwnerChatConversation | null> {
  const normalized = ownerChatConversationId(conversationId);
  if (!normalized) throw new Error('OWNER_CONVERSATION_ID_INVALID');
  const { jobs, rowWindowTruncated } = await readOwnerChatJobRows(identity, normalized);
  const selectedJobs = jobs.filter((job) => ownerChatJobConversationId(job) === normalized);
  const revisions = await readOwnerChatRevisionRows(identity, selectedJobs);
  return projectOwnerChatConversation(normalized, selectedJobs, revisions, rowWindowTruncated);
}

export async function readOwnerChatConversationHistory(
  identity: JobIdentity,
  conversationId: string,
): Promise<Array<{ role: 'user' | 'assistant'; content: string }>> {
  const normalized = ownerChatConversationId(conversationId);
  if (!normalized) throw new Error('OWNER_CONVERSATION_ID_INVALID');
  const { jobs } = await readOwnerChatJobRows(identity, normalized);
  const selectedJobs = jobs.filter((job) => ownerChatJobConversationId(job) === normalized);
  const revisions = await readOwnerChatRevisionRows(identity, selectedJobs);
  const candidates: Array<{ role: 'user' | 'assistant'; content: string }> = [];
  const seenJobIds = new Set<string>();
  for (const job of orderedOwnerChatJobs(normalized, selectedJobs)) {
    const prompt = ownerChatPrompt(job.request);
    const suppressPrompt = Boolean(ownerChatRetryOf(job.request) && seenJobIds.has(ownerChatRetryOf(job.request)!));
    if (prompt && !suppressPrompt) candidates.push({
      role: 'user',
      content: utf8Prefix(prompt, OWNER_CHAT_HISTORY_MESSAGE_BYTES),
    });
    const revision = job.status === 'succeeded' && job.terminal_revision_id
      ? revisions.get(job.terminal_revision_id)
      : undefined;
    if (revision?.content) candidates.push({
      role: 'assistant',
      content: utf8Prefix(revision.content, OWNER_CHAT_HISTORY_MESSAGE_BYTES),
    });
    seenJobIds.add(job.id);
  }
  const recentCandidates = candidates.slice(-OWNER_CHAT_HISTORY_MESSAGES);
  const selected: typeof recentCandidates = [];
  let serializedBytes = 2;
  for (let index = recentCandidates.length - 1; index >= 0; index -= 1) {
    const candidate = recentCandidates[index]!;
    const additionalBytes = Buffer.byteLength(JSON.stringify(candidate), 'utf8') + (selected.length ? 1 : 0);
    if (serializedBytes + additionalBytes > OWNER_CHAT_HISTORY_BYTES) break;
    selected.unshift(candidate);
    serializedBytes += additionalBytes;
  }
  if (selected[0]?.role === 'assistant') selected.shift();
  return selected;
}

const EXTERNAL_JOB_PAGE_SIZE = 256;

function externalChunk(chunk: Record<string, unknown>, committed: boolean) {
  const rawSequence = Math.max(0, Number(chunk.seq ?? 0));
  return {
    section_index: Number((chunk.metadata as Record<string, unknown> | undefined)?.section_index ?? 0),
    chunk_index: rawSequence,
    text: String(chunk.delta ?? ''),
    committed,
    // Worker chunk ordinals start at zero. The public cursor starts at zero and
    // therefore exposes one-based sequences so `after=0` includes the first chunk.
    sequence: rawSequence + 1,
    received_at: typeof chunk.created_at === 'string' ? chunk.created_at : null,
  };
}

function eventExpectation(outcome: unknown): { expected: boolean | null; fired: boolean | null } {
  switch (outcome) {
    case 'expected_fired': return { expected: true, fired: true };
    case 'expected_missed': return { expected: true, fired: false };
    case 'unexpected_fired': return { expected: false, fired: true };
    case 'unexpected_absent': return { expected: false, fired: false };
    default: return { expected: null, fired: null };
  }
}

function externalEvent(event: Record<string, unknown>) {
  const metadata = event.metadata && typeof event.metadata === 'object' && !Array.isArray(event.metadata)
    ? event.metadata as Record<string, unknown>
    : {};
  const inferred = eventExpectation(event.outcome);
  return {
    sequence: Math.max(0, Number(event.ordinal ?? 0)),
    phase: typeof metadata.phase === 'string'
      ? metadata.phase
      : typeof event.source === 'string' ? event.source : 'unknown',
    kind: typeof event.event_type === 'string' ? event.event_type : 'state_changed',
    expected: typeof metadata.expected === 'boolean' ? metadata.expected : inferred.expected,
    fired: typeof metadata.fired === 'boolean' ? metadata.fired : inferred.fired,
    outcome: typeof event.outcome === 'string' ? event.outcome : null,
    occurred_at: typeof event.occurred_at === 'string' ? event.occurred_at : null,
    // The control plane persists the canonical occurrence timestamp; there is
    // no distinct ingestion timestamp in the current schema.
    received_at: typeof event.occurred_at === 'string' ? event.occurred_at : null,
  };
}

export async function readExternalJobChunksPage(jobId: string, identity: JobIdentity, after: number) {
  const client = getApocryphaServiceClient();
  const { data: job, error: jobError } = await client
    .from('apocrypha_job')
    .select('id,status,current_attempt_id')
    .eq('id', jobId)
    .eq('tenant_id', identity.tenantId)
    .eq('owner_principal_id', identity.principalId)
    .maybeSingle();
  if (jobError) throw new Error(`JOB_READ_FAILED:${jobError.code ?? 'unknown'}`);
  if (!job) return null;
  if (!job.current_attempt_id) {
    return { job_id: jobId, chunks: [], next_after: null, status: job.status as ApocryphaJobStatus };
  }

  // Public sequences are raw worker ordinals + 1. Convert the exclusive public
  // cursor back to the raw ordinal used in the database query.
  const rawAfter = Math.max(-1, Math.trunc(after) - 1);
  const { data, error } = await client
    .from('apocrypha_job_chunk')
    .select('seq,chunk_kind,delta,metadata,created_at')
    .eq('attempt_id', String(job.current_attempt_id))
    .gt('seq', rawAfter)
    .order('seq', { ascending: true })
    .limit(EXTERNAL_JOB_PAGE_SIZE);
  if (error) throw new Error(`JOB_CHUNKS_FAILED:${error.code ?? 'unknown'}`);
  const chunks = (data ?? []).map((chunk) => externalChunk(chunk as Record<string, unknown>, job.status === 'succeeded'));
  return {
    job_id: jobId,
    chunks,
    next_after: chunks.at(-1)?.sequence ?? null,
    // Same reason as the events page: a streaming reader needs to know the job
    // is finished, not merely quiet, or it never stops asking.
    status: job.status as ApocryphaJobStatus,
  };
}

export async function readExternalJobEventsPage(jobId: string, identity: JobIdentity, after: number) {
  const client = getApocryphaServiceClient();
  const { data: job, error: jobError } = await client
    .from('apocrypha_job')
    .select('id,status')
    .eq('id', jobId)
    .eq('tenant_id', identity.tenantId)
    .eq('owner_principal_id', identity.principalId)
    .maybeSingle();
  if (jobError) throw new Error(`JOB_READ_FAILED:${jobError.code ?? 'unknown'}`);
  if (!job) return null;

  const { data, error } = await client
    .from('apocrypha_job_event')
    .select('ordinal,event_type,outcome,severity,source,flagged,metadata,occurred_at')
    .eq('job_id', jobId)
    .gt('ordinal', Math.max(0, Math.trunc(after)))
    .order('ordinal', { ascending: true })
    .limit(EXTERNAL_JOB_PAGE_SIZE);
  if (error) throw new Error(`JOB_EVENTS_FAILED:${error.code ?? 'unknown'}`);
  const events = (data ?? []).map((event) => externalEvent(event as Record<string, unknown>));
  return {
    job_id: jobId,
    events,
    next_after: events.at(-1)?.sequence ?? null,
    // Carried so a streaming reader can tell "no events yet" from "no events
    // ever again" without a second round trip. Without it a stream has no way
    // to end on its own and every consumer polls a finished job forever.
    status: job.status as ApocryphaJobStatus,
  };
}

export function externalJobSnapshot(snapshot: NonNullable<Awaited<ReturnType<typeof readApocryphaJob>>>) {
  const statusMap: Record<ApocryphaJobStatus, string> = {
    queued: 'queued', leased: 'claimed', running: 'generating', cancel_requested: 'generating',
    succeeded: 'complete', failed: 'failed', cancelled: 'cancelled',
  };
  const latest = snapshot.revisions[0] as Record<string, unknown> | undefined;
  const chunks = (snapshot.chunks as Array<Record<string, unknown>>)
    .map((chunk) => externalChunk(chunk, snapshot.job.status === 'succeeded'));
  const events = (snapshot.events as Array<Record<string, unknown>>).map(externalEvent);
  const lastSequence = events.reduce((maximum, event) => Math.max(maximum, event.sequence), 0);
  const finalText = typeof latest?.content === 'string' ? latest.content : null;
  const previewText = finalText ?? (chunks.map((chunk) => chunk.text).join('') || null);
  return {
    job: {
      id: snapshot.job.id,
      kind: snapshot.job.kind,
      status: statusMap[snapshot.job.status],
      progress: snapshot.job.status === 'succeeded' ? 1 : snapshot.job.status === 'running' ? 0.5 : 0,
      phase: snapshot.job.status,
      queue_position: null,
      preview_text: previewText,
      final_text: finalText,
      public_error_summary: snapshot.job.status === 'failed' ? 'The model attempt failed. Your request was preserved.' : null,
      last_sequence: lastSequence,
      created_at: snapshot.job.created_at,
      updated_at: snapshot.job.updated_at,
      completed_at: snapshot.job.completed_at,
    },
    chunks,
    events,
  };
}

export async function cancelApocryphaJob(jobId: string, identity: JobIdentity, reason?: string) {
  const client = getApocryphaServiceClient();
  const existing = await readApocryphaJob(jobId, identity);
  if (!existing) return null;
  const { data, error } = await client.rpc('apocrypha_cancel_job', {
    p_job_id: jobId,
    p_requested_by_principal_id: identity.principalId,
    p_reason: reason ?? 'user_requested',
  });
  if (error) throw new Error(`JOB_CANCEL_FAILED:${error.code ?? 'unknown'}`);
  return data as ApocryphaJobRow;
}

export function publicJobError(error: unknown): { status: number; code: string; message: string } {
  const raw = error instanceof Error ? error.message : String(error);
  if (raw.includes('UNAUTHORIZED')) return { status: 401, code: 'UNAUTHORIZED', message: 'Authentication failed.' };
  if (raw.includes('WORKER_FENCE_LOST')) return { status: 409, code: 'STALE_FENCE', message: 'This worker lease is no longer active.' };
  if (raw.includes('IDEMPOTENCY_CONFLICT')) return { status: 409, code: 'IDEMPOTENCY_CONFLICT', message: 'This request key was already used for different content.' };
  if (raw.includes('OWNER_RETRY_JOB_ID_INVALID') || raw.includes('OWNER_RETRY_MISMATCH')) {
    return { status: 400, code: 'RETRY_INVALID', message: 'This failed attempt cannot be retried with changed content.' };
  }
  if (raw.includes('OWNER_RETRY_NOT_FOUND')) return { status: 404, code: 'RETRY_NOT_FOUND', message: 'The failed attempt was not found.' };
  if (raw.includes('OWNER_RETRY_NOT_FAILED')) return { status: 409, code: 'RETRY_NOT_FAILED', message: 'Only a failed attempt can be retried.' };
  if (raw.includes('UNCONFIGURED')) return { status: 503, code: 'CONTROL_PLANE_UNAVAILABLE', message: 'Apocrypha cannot accept work right now.' };
  return { status: 503, code: 'APOCRYPHA_JOB_ERROR', message: 'Apocrypha could not update this request. It is safe to retry.' };
}
