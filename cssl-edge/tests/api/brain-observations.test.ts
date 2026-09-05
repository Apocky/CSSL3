import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import type { NextApiRequest, NextApiResponse } from 'next';
import type { AdminAuthorizationResult } from '../../lib/admin-auth';
import { createObservationHandler, observationRequest } from '../../lib/brain/observations';

const actor = '11111111-1111-4111-8111-111111111111';
const subject = '22222222-2222-4222-8222-222222222222';
const digest = 'a'.repeat(64); const stamp = '2026-09-04T00:00:00.000Z'; const canary = 'SECRET_SENTINEL_PRIVATE_PROMPT';
const hash = (value: Record<string, string>) => createHash('sha256').update(JSON.stringify(Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b))))).digest('hex');
const partition = hash({ schema_version: 'apocv4.runtime-auth.v1', privacy_partition: 'owner:apocky' });
const binding = hash({ schema_version: 'apocv4.runtime-auth.v1', principal_ref: digest, privacy_partition_ref: partition });
const headers = { 'x-apocv4-auth-mode': 'STRICT_REGISTRY', 'x-apocv4-auth-registry-ref': digest, 'x-apocv4-principal-ref': digest, 'x-apocv4-privacy-partition-ref': partition, 'x-apocv4-binding-ref': binding };
const user = { id: actor, email: 'owner@example.test', provider: 'email', createdAt: stamp };
const authorize = async (): Promise<AdminAuthorizationResult> => ({ authorized: true, user, authConfigured: true });
const event = { schema_version: 'apocv4.interoception-event.v1', event_id: 'ev-synthetic', trace_id: 'tr-synthetic', span_id: 'sp-synthetic',
  parent_span_id: null, cause_ref: null, occurred_at: stamp, component: 'apex', operation: 'model.inference', state: 'FAILED',
  severity: 'error', privacy_partition_ref: partition, error_code: 'APOC-APEX-MODEL-INFERENCE-v1', payload_digest: digest, event_digest: digest,
  duration_ns: 2000000, retryability: 'after_recovery', private_payload: canary };
const status = { schema_version: 'apocv4.interoception-status.v1', state: 'ACTIVE', canonical_store: 'append_only_jsonl', projection: 'rebuildable_sqlite',
  event_count: 1, error_count: 1, ring_size: 1, ring_capacity: 100, effect_authority: 'NONE', credentials: canary };
const fixtures: Record<string, unknown> = {
  status,
  events: { schema_version: 'apocv4.interoception-query.v1', events: [event], has_more: false, next_cursor: null, effect_authority: 'NONE' },
  trace: { schema_version: 'apocv4.interoception-trace-view.v1', trace_id: event.trace_id, events: [event], causal_edges: [{ ...event, private: canary }], effect_authority: 'NONE' },
  errors: { schema_version: 'apocv4.interoception-error-explanation.v1', definition: { schema_version: 'apocv4.interoception-error.v1', code: event.error_code, public_message: 'The model request failed.', retryability: 'after_recovery', recovery_ref: 'recover:model', rollback_ref: null, private: canary }, occurrences: [event], has_more: false, effect_authority: 'NONE' },
  metrics: { schema_version: 'apocv4.interoception-latency-metrics.v1', stages: { 'model.inference': { duration_ns: { count: 1, p50: 2, p95: 3, p99: 4, private: canary }, private: canary } }, quantiles: 'nearest-rank', effect_authority: 'NONE' },
  shards: { schema_version: 'apocv4.interoception-shard-integrity.v1', state: 'VERIFIED', event_count: 1, shard_count: 1, shards: [{ shard_ref: digest, terminal_digest: digest, event_count: 1, private_path: canary }], effect_authority: 'NONE' },
};
type Dependencies = NonNullable<Parameters<typeof createObservationHandler>[0]>;
async function invoke(query: NextApiRequest['query'] = { view: 'status' }, dependencies: Dependencies = {}, override: Partial<NextApiRequest> = {}) {
  const result = { status: 0, body: null as any, headers: {} as Record<string, string> };
  const req = { method: 'GET', query, headers: { host: 'www.apocky.com', origin: 'https://www.apocky.com' }, ...override } as NextApiRequest;
  const res = { setHeader(name: string, value: string) { result.headers[name.toLowerCase()] = value; return this; }, status(value: number) { result.status = value; return this; }, json(value: unknown) { result.body = value; return this; } } as unknown as NextApiResponse;
  await createObservationHandler({ authorize, now: () => new Date(stamp), bridge: async input => {
    assert.equal(input.channel, 'owner'); assert.equal(input.subject, subject); assert.equal(input.method, 'GET'); assert.equal(input.body.length, 0);
    assert.equal(new URL('https://fixture' + input.target).searchParams.get('privacy_partition'), 'owner:apocky');
    return Response.json(fixtures[query.view as string], { headers });
  }, ...dependencies })(req, res);
  assert.match(result.headers['cache-control'] ?? '', /private, no-store/);
  assert.match(result.headers['x-apocky-trace-id'] ?? '', /^[a-f0-9-]{36}$/);
  assert(!JSON.stringify(result).includes(canary));
  return result;
}
async function run() {
  const old = process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID; process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID = subject;
  try {
    for (const view of Object.keys(fixtures)) {
      const query = { view, ...(view === 'trace' ? { trace_id: event.trace_id } : view === 'errors' ? { error_code: event.error_code } : {}) };
      const result = await invoke(query); assert.equal(result.status, 200, JSON.stringify(result.body));
      assert.equal(result.body.schema_version, 'apocky.brain.observation.v1'); assert.equal(result.body.view, view);
      assert.equal(result.body.observed_at, stamp); assert.equal(result.body.trace_id, result.headers['x-apocky-trace-id']);
    }
    const events = observationRequest({ view: 'events', trace_id: 'tr-synthetic', error_code: event.error_code, component: 'apex', cursor: 'ev-old', limit: '100' });
    assert.equal(events?.target, '/v1/observe/events?privacy_partition=owner%3Aapocky&trace_id=tr-synthetic&error_code=APOC-APEX-MODEL-INFERENCE-v1&component=apex&cursor=ev-old&limit=100');
    const invalid = [{ view: ['status'] }, { view: 'status', privacy_partition: 'owner:other' }, { view: 'events', limit: '101' }, { view: 'events', limit: '01' }, { view: 'trace' }, { view: 'errors', error_code: 'private' }, { view: 'trace', trace_id: 'x\r\nInjected' }, { view: 'metrics', cursor: 'x' }];
    for (const query of invalid) {
      const result = await invoke(query, { bridge: async () => { throw new Error('must not call'); } });
      assert.equal(result.status, 400);
    }
    assert.equal((await invoke({}, {}, { method: 'POST' })).status, 405);
    assert.equal((await invoke({ view: 'status' }, {}, { headers: { origin: 'https://foreign.example' } })).status, 403);
    assert.equal((await invoke({ view: 'status' }, {}, { headers: { origin: 'http://localhost:99999', host: 'localhost:99999' } })).status, 403);
    assert.equal((await invoke({ view: 'status' }, {}, { headers: { 'sec-fetch-site': 'cross-site' } })).status, 403);
    for (const signedIn of [false, true]) {
      const result = await invoke({ view: 'status' }, { authorize: async () => ({ authorized: false, user: signedIn ? user : null, authConfigured: true }), bridge: async () => { throw new Error('must not call'); } });
      assert.equal(result.status, signedIn ? 403 : 401);
    }
    assert.equal((await invoke({ view: 'status' }, { authorize: async () => { throw new Error(canary); } })).status, 503);
    for (const changes of [{ 'x-apocv4-auth-mode': 'DEGRADED_LOCAL_SINGLE_TOKEN' }, { 'x-apocv4-binding-ref': 'b'.repeat(64) }, { 'x-apocv4-privacy-partition-ref': 'c'.repeat(64) }, { 'x-apocv4-auth-registry-ref': '' }]) {
      const result = await invoke({ view: 'status' }, { bridge: async () => Response.json(status, { headers: { ...headers, ...changes } }) });
      assert.equal(result.status, 502); assert.equal(result.body.code, 'OBSERVATION_RESPONSE_UNVERIFIED');
    }
    const crossed = { ...(fixtures.events as object), events: [{ ...event, privacy_partition_ref: 'b'.repeat(64) }] };
    assert.equal((await invoke({ view: 'events' }, { bridge: async () => Response.json(crossed, { headers }) })).status, 502);
    for (const payload of [{ ...status, state: ['ACTIVE'] }, { ...status, event_count: -1 }, { ...status, effect_authority: 'EXECUTE' }]) {
      assert.equal((await invoke({ view: 'status' }, { bridge: async () => Response.json(payload, { headers }) })).status, 502);
    }
    assert.equal((await invoke({ view: 'events', trace_id: 'tr-other' })).status, 502);
    assert.equal((await invoke({ view: 'status' }, { bridge: async () => new Response('x'.repeat(512 * 1024 + 1), { headers: { ...headers, 'content-type': 'application/json' } }) })).body.code, 'OBSERVATION_RESPONSE_TOO_LARGE');
    assert.equal((await invoke({ view: 'status' }, { bridge: async () => new Response(canary, { status: 503 }) })).status, 503);
    assert.equal((await invoke({ view: 'status' }, { bridge: async () => { throw new Error(canary); } })).status, 503);
    delete process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID;
    assert.equal((await invoke()).body.code, 'OBSERVATION_BRIDGE_UNCONFIGURED');
    console.log('Brain observation facade: six live schema projections, privacy/auth boundaries, exact filters, bounded transport, and safe failure tests passed');
  } finally { if (old === undefined) delete process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID; else process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID = old; }
}
void run().catch(error => { console.error(error); process.exitCode = 1; });
