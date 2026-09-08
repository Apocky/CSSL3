import { createHash, createHmac, timingSafeEqual } from 'node:crypto';

import { createClient, type SupabaseClient } from '@supabase/supabase-js';

export const APOCRYPHA_MODEL_ALIAS = process.env.APOCRYPHA_MODEL_ALIAS ?? 'qwen35-35b-a3b-q4';
export const APOCRYPHA_PROFILE_HASH = process.env.APOCRYPHA_PROFILE_HASH ?? '5d390055297aed74dbba092eb313dc8c4bf4e551ca4bf2c50fed16c8cb3a21a9';
export const APOCRYPHA_TOOL_REGISTRY_VERSION = process.env.APOCRYPHA_TOOL_REGISTRY_VERSION ?? 'apocrypha-readonly-v1';
export const APOCRYPHA_MEMORY_MANIFEST_HASH = process.env.APOCRYPHA_MEMORY_MANIFEST_HASH ?? '681b1dc6e1c61e0dd9883533b8fb04e28e90f4060161613ca1628f0a500d8b0e';

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
  if (!job.current_attempt_id) return { job_id: jobId, chunks: [], next_after: null };

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
  };
}

export async function readExternalJobEventsPage(jobId: string, identity: JobIdentity, after: number) {
  const client = getApocryphaServiceClient();
  const { data: job, error: jobError } = await client
    .from('apocrypha_job')
    .select('id')
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
  if (raw.includes('IDEMPOTENCY_CONFLICT')) return { status: 409, code: 'IDEMPOTENCY_CONFLICT', message: 'This request key was already used for different content.' };
  if (raw.includes('UNCONFIGURED')) return { status: 503, code: 'CONTROL_PLANE_UNAVAILABLE', message: 'Apocrypha cannot accept work right now.' };
  return { status: 503, code: 'APOCRYPHA_JOB_ERROR', message: 'Apocrypha could not update this request. It is safe to retry.' };
}
