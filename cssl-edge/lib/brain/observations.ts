import type { NextApiRequest, NextApiResponse } from 'next';
import { createHash, randomUUID } from 'node:crypto';
import { getAdminAuthorization } from '../admin-auth';
import { fetchBridge } from '../bridge/queue';
import { ACCOUNT_UUID } from '../mobile/account-grant';
import { setBrainPrivateHeaders } from './owner';

export type ObservationView = 'status' | 'events' | 'trace' | 'errors' | 'metrics' | 'shards';
export interface BrainObservation {
  schema_version: 'apocky.brain.observation.v1';
  view: ObservationView;
  observed_at: string;
  trace_id: string;
  data: Record<string, unknown>;
}
const MAX_RESPONSE = 512 * 1024;
const SHA = /^[a-f0-9]{64}$/;
const LABEL = /^[A-Za-z0-9][A-Za-z0-9:._/@+-]{0,191}$/;
const ERROR_CODE = /^APOC-[A-Z0-9]+-[A-Z0-9]+-[A-Z0-9]+-v[1-9][0-9]*$/;
const VIEWS: readonly ObservationView[] = ['status', 'events', 'trace', 'errors', 'metrics', 'shards'];
const hash = (value: Record<string, string>) => createHash('sha256').update(JSON.stringify(Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b))))).digest('hex');
const PARTITION = hash({ schema_version: 'apocv4.runtime-auth.v1', privacy_partition: 'owner:apocky' });
class ObservationError extends Error {
  constructor(readonly code: string, readonly status = 502) { super(code); }
}
function record(value: unknown): value is Record<string, unknown> { return Boolean(value && typeof value === 'object' && !Array.isArray(value)); }
function label(value: unknown): value is string { return typeof value === 'string' && LABEL.test(value); }
function sha(value: unknown): value is string { return typeof value === 'string' && SHA.test(value); }
function number(value: unknown): value is number { return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0; }
function stamp(value: unknown): value is string { return typeof value === 'string' && value.length <= 40 && /^\d{4}-\d\d-\d\dT/.test(value) && Number.isFinite(Date.parse(value)); }
function demand(value: unknown): asserts value { if (!value) throw new ObservationError('OBSERVATION_RESPONSE_UNVERIFIED'); }
function field(source: Record<string, unknown>, key: string): string { const value = source[key]; demand(label(value)); return value; }
function nullableLabel(value: unknown): string | null { demand(value === null || label(value)); return value; }
function count(source: Record<string, unknown>, key: string): number { const value = source[key]; demand(number(value)); return value; }
function allowedOrigin(req: NextApiRequest): boolean {
  const origin = req.headers.origin;
  if (origin === undefined || origin === 'https://www.apocky.com') return true;
  if (process.env.NODE_ENV === 'production' || typeof origin !== 'string' || !/^http:\/\/(?:localhost|127\.0\.0\.1):[0-9]{1,5}$/.test(origin)) return false;
  try { return new URL(origin).host === req.headers.host; } catch { return false; }
}

export function observationRequest(query: NextApiRequest['query']): { view: ObservationView; target: string; filters: Record<string, string> } | null {
  const view = query.view;
  if (typeof view !== 'string' || !VIEWS.includes(view as ObservationView)) return null;
  const allowed = view === 'events' ? ['view', 'trace_id', 'error_code', 'component', 'cursor', 'limit']
    : view === 'trace' ? ['view', 'trace_id'] : view === 'errors' ? ['view', 'error_code', 'limit'] : ['view'];
  if (Object.keys(query).some(key => !allowed.includes(key) || typeof query[key] !== 'string')) return null;
  if (view === 'trace' && !label(query.trace_id)) return null;
  if (view === 'errors' && (typeof query.error_code !== 'string' || !ERROR_CODE.test(query.error_code))) return null;
  const filters: Record<string, string> = {};
  for (const key of ['trace_id', 'error_code', 'component', 'cursor']) {
    const value = query[key];
    if (value !== undefined) {
      if (!label(value) || (key === 'error_code' && !ERROR_CODE.test(value))) return null;
      filters[key] = value;
    }
  }
  const limit = query.limit;
  if (limit !== undefined && (typeof limit !== 'string' || !/^(?:[1-9][0-9]?|100)$/.test(limit))) return null;
  const params = new URLSearchParams({ privacy_partition: 'owner:apocky', ...filters });
  if (view === 'events' || view === 'errors') params.set('limit', typeof limit === 'string' ? limit : '25');
  return { view: view as ObservationView, target: `/v1/observe/${view}?${params.toString()}`, filters };
}

function verifyIdentity(response: Response): void {
  const principal = response.headers.get('x-apocv4-principal-ref');
  demand(sha(principal) && response.headers.get('x-apocv4-auth-mode') === 'STRICT_REGISTRY'
    && sha(response.headers.get('x-apocv4-auth-registry-ref'))
    && response.headers.get('x-apocv4-privacy-partition-ref') === PARTITION
    && response.headers.get('x-apocv4-binding-ref') === hash({ schema_version: 'apocv4.runtime-auth.v1', principal_ref: principal, privacy_partition_ref: PARTITION }));
}
function event(value: unknown, partitionRequired = true): Record<string, unknown> {
  demand(record(value));
  if (partitionRequired) demand(value.schema_version === 'apocv4.interoception-event.v1' && value.privacy_partition_ref === PARTITION);
  demand(stamp(value.occurred_at));
  const projected: Record<string, unknown> = { event_id: field(value, 'event_id'), trace_id: field(value, 'trace_id'), occurred_at: value.occurred_at,
    component: field(value, 'component'), operation: field(value, 'operation'), state: field(value, 'state'), cause_ref: nullableLabel(value.cause_ref) };
  for (const key of ['span_id', 'parent_span_id', 'error_code', 'severity', 'retryability', 'recovery_ref', 'rollback_ref']) {
    if (key in value) projected[key] = nullableLabel(value[key]);
  }
  for (const key of ['event_digest', 'payload_digest']) {
    if (key in value) { demand(sha(value[key])); projected[key] = value[key]; }
  }
  for (const key of ['duration_ns', 'queue_wait_ns', 'service_time_ns']) {
    if (key in value) { demand(value[key] === null || number(value[key])); projected[key] = value[key]; }
  }
  return projected;
}
function events(value: unknown, partitionRequired = true): Record<string, unknown>[] {
  demand(Array.isArray(value) && value.length <= 100); return value.map(item => event(item, partitionRequired));
}
function project(value: unknown, request: NonNullable<ReturnType<typeof observationRequest>>): Record<string, unknown> {
  demand(record(value) && value.effect_authority === 'NONE');
  if (request.view === 'status') {
    demand(value.schema_version === 'apocv4.interoception-status.v1' && value.state === 'ACTIVE'
      && value.canonical_store === 'append_only_jsonl' && value.projection === 'rebuildable_sqlite');
    return { state: 'ACTIVE', event_count: count(value, 'event_count'), error_count: count(value, 'error_count'),
      ring_size: count(value, 'ring_size'), ring_capacity: count(value, 'ring_capacity') };
  }
  if (request.view === 'events') {
    demand(value.schema_version === 'apocv4.interoception-query.v1' && typeof value.has_more === 'boolean');
    const entries = events(value.events);
    demand(entries.every(entry => ['trace_id', 'error_code', 'component'].every(key => !request.filters[key] || entry[key] === request.filters[key])));
    return { events: entries, has_more: value.has_more, next_cursor: nullableLabel(value.next_cursor) };
  }
  if (request.view === 'trace') {
    demand(value.schema_version === 'apocv4.interoception-trace-view.v1' && value.trace_id === request.filters.trace_id);
    const entries = events(value.events); demand(entries.every(entry => entry.trace_id === value.trace_id));
    return { trace_id: value.trace_id, events: entries };
  }
  if (request.view === 'errors') {
    demand(value.schema_version === 'apocv4.interoception-error-explanation.v1' && typeof value.has_more === 'boolean' && record(value.definition));
    const definition = value.definition;
    demand(definition.schema_version === 'apocv4.interoception-error.v1'
      && definition.code === request.filters.error_code && typeof definition.public_message === 'string'
      && definition.public_message.length <= 256 && !/[\x00-\x1f]/.test(definition.public_message)
      && typeof definition.retryability === 'string' && ['never', 'bounded', 'after_recovery'].includes(definition.retryability));
    return { definition: { code: definition.code, public_message: definition.public_message, retryability: definition.retryability,
      recovery_ref: field(definition, 'recovery_ref'), rollback_ref: nullableLabel(definition.rollback_ref) },
      occurrences: events(value.occurrences, false), has_more: value.has_more };
  }
  if (request.view === 'metrics') {
    demand(value.schema_version === 'apocv4.interoception-latency-metrics.v1' && value.quantiles === 'nearest-rank' && record(value.stages) && Object.keys(value.stages).length <= 128);
    const stages: Record<string, unknown> = {};
    for (const [stage, kinds] of Object.entries(value.stages)) {
      demand(label(stage) && record(kinds)); const row: Record<string, unknown> = {};
      for (const key of ['duration_ns', 'queue_wait_ns', 'service_time_ns']) {
        if (!(key in kinds)) continue;
        const metric = kinds[key]; demand(record(metric));
        row[key] = { count: count(metric, 'count'), p50: count(metric, 'p50'), p95: count(metric, 'p95'), p99: count(metric, 'p99') };
      }
      stages[stage] = row;
    }
    return { stages, quantiles: 'nearest-rank' };
  }
  demand(value.schema_version === 'apocv4.interoception-shard-integrity.v1' && value.state === 'VERIFIED'
    && Array.isArray(value.shards) && value.shards.length <= 512);
  const shards = value.shards.map(item => { demand(record(item) && sha(item.shard_ref) && sha(item.terminal_digest));
    return { shard_ref: item.shard_ref, event_count: count(item, 'event_count'), terminal_digest: item.terminal_digest }; });
  demand(value.shard_count === shards.length);
  return { state: 'VERIFIED', event_count: count(value, 'event_count'), shard_count: shards.length, shards };
}
async function boundedJson(response: Response): Promise<unknown> {
  if (response.headers.get('content-type')?.split(';', 1)[0]?.trim() !== 'application/json') throw new ObservationError('OBSERVATION_RESPONSE_UNVERIFIED');
  const reader = response.body?.getReader(); if (!reader) throw new ObservationError('OBSERVATION_RESPONSE_UNVERIFIED');
  let length = 0; const pieces: Uint8Array[] = [];
  try {
    while (true) { const part = await reader.read(); if (part.done) break;
      length += part.value.byteLength; if (length > MAX_RESPONSE) throw new ObservationError('OBSERVATION_RESPONSE_TOO_LARGE'); pieces.push(part.value); }
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(pieces)));
  } finally { await reader.cancel().catch(() => {}); reader.releaseLock(); }
}
export function createObservationHandler(dependencies: { authorize?: typeof getAdminAuthorization; bridge?: typeof fetchBridge; now?: () => Date } = {}) {
  return async (req: NextApiRequest, res: NextApiResponse): Promise<void> => {
    setBrainPrivateHeaders(res); res.setHeader('Allow', 'GET');
    const trace = randomUUID(); res.setHeader('X-Apocky-Trace-Id', trace);
    const fail = (status: number, code: string) => res.status(status).json({ code, trace_id: trace, error: 'Desktop diagnostics are unavailable. Keep this support code and try Refresh after reconnecting.' });
    if (req.method !== 'GET') { fail(405, 'OBSERVATION_METHOD_DENIED'); return; }
    if (req.headers['sec-fetch-site'] === 'cross-site') { fail(403, 'OBSERVATION_ORIGIN_DENIED'); return; }
    if (!allowedOrigin(req)) {
      fail(403, 'OBSERVATION_ORIGIN_DENIED'); return;
    }
    const request = observationRequest(req.query);
    if (!request) { fail(400, 'OBSERVATION_REQUEST_INVALID'); return; }
    let authorization;
    try { authorization = await (dependencies.authorize ?? getAdminAuthorization)(req); }
    catch { fail(503, 'OBSERVATION_AUTH_UNAVAILABLE'); return; }
    if (!authorization.authorized || !authorization.user) { fail(authorization.user ? 403 : 401, 'OBSERVATION_OWNER_REQUIRED'); return; }
    const subject = process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID ?? '';
    if (!ACCOUNT_UUID.test(subject)) { fail(503, 'OBSERVATION_BRIDGE_UNCONFIGURED'); return; }
    try {
      const response = await (dependencies.bridge ?? fetchBridge)({ channel: 'owner', subject, method: 'GET', target: request.target, body: Buffer.alloc(0), signal: AbortSignal.timeout(25000) });
      if (!response.ok) throw new ObservationError('OBSERVATION_UPSTREAM_UNAVAILABLE', response.status === 503 ? 503 : 502);
      verifyIdentity(response);
      const data = project(await boundedJson(response), request);
      const result: BrainObservation = { schema_version: 'apocky.brain.observation.v1', view: request.view, observed_at: (dependencies.now?.() ?? new Date()).toISOString(), trace_id: trace, data };
      res.status(200).json(result);
    } catch (error) { fail(error instanceof ObservationError ? error.status : 503, error instanceof ObservationError ? error.code : 'OBSERVATION_UNAVAILABLE'); }
  };
}
