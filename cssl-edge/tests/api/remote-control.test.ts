import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import type { NextApiRequest, NextApiResponse } from 'next';
import type { AdminAuthorizationResult } from '../../lib/admin-auth';
import { createRemoteControlHandler } from '../../lib/brain/remote-control';

const actor = '11111111-1111-4111-8111-111111111111';
const subject = '22222222-2222-4222-8222-222222222222';
const operation = '33333333-3333-4333-8333-333333333333';
const stamp = '2026-09-04T00:00:00.000Z'; const digest = 'a'.repeat(64);
const hash = (value: Record<string, string>) => createHash('sha256').update(JSON.stringify(Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b))))).digest('hex');
const partition = hash({ schema_version: 'apocv4.runtime-auth.v1', privacy_partition: 'owner:apocky' });
const binding = hash({ schema_version: 'apocv4.runtime-auth.v1', principal_ref: digest, privacy_partition_ref: partition });
const headers = { 'x-apocv4-auth-mode': 'STRICT_REGISTRY', 'x-apocv4-auth-registry-ref': digest, 'x-apocv4-principal-ref': digest, 'x-apocv4-privacy-partition-ref': partition, 'x-apocv4-binding-ref': binding };
const runtimeResponse = (value: unknown) => Response.json(value, { headers });
const owner = { id: actor, email: 'operator@example.test', provider: 'email', createdAt: stamp };
const authorize = async (): Promise<AdminAuthorizationResult> => ({ authorized: true, user: owner, authConfigured: true });
const capabilities = { schema_version: 'apocv4.remote-code.capabilities.v1', enabled: true, workspace_ref: digest, execution: 'DIRECT_RUN', enforcement: 'EFFECT_GATEWAY_ENFORCE', actions: ['run', 'read', 'rollback'], test_command_digest: digest, limits: { objective_chars: 32768, allowed_paths: 32 }, principal_ref: digest, privacy_partition_ref: partition };
const result = { schema_version: 'apocv4.remote-code.operation.v1', operation_id: operation, input_digest: digest, state: 'PROMOTED', created_at: stamp, updated_at: stamp, workspace_ref: digest, principal_ref: digest, privacy_partition_ref: partition,
  result: { terminal_event_digest: digest, promotion_event_digest: digest, rollback_event_digest: null, request_digest: digest, proposal_digest: digest, changed_paths: ['tests/fixture.txt'], test: { passed: true, exit_code: 0, timed_out: false, receipt_digest: digest } } };
const runBody = { action: 'run', operation_id: operation, objective: 'Change one synthetic fixture.', allowed_paths: ['tests/fixture.txt'] };
type Handler = ReturnType<typeof createRemoteControlHandler>;
async function invoke(handler: Handler, override: Partial<NextApiRequest> = {}) {
  const result = { status: 0, body: null as any, headers: {} as Record<string, string> };
  const req = { method: 'POST', headers: { origin: 'https://www.apocky.com', host: 'www.apocky.com', 'content-type': 'application/json' }, query: {}, body: runBody, ...override } as NextApiRequest;
  const res = { setHeader(k: string, v: string) { result.headers[k.toLowerCase()] = v; return this; }, status(value: number) { result.status = value; return this; }, json(value: unknown) { result.body = value; return this; } } as unknown as NextApiResponse;
  await handler(req, res); assert.match(result.headers['cache-control'] ?? '', /private, no-store/);
  assert(!JSON.stringify(result).includes('SECRET_SENTINEL')); return result;
}
async function run() {
  const previous = process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID;
  process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID = subject;
  try {
    const events: string[] = [];
    const handler = createRemoteControlHandler({ authorize, audit: async (_req, who, action) => { assert.equal(who, actor); events.push(`audit:${action.action}`); return true; }, bridge: async input => {
      events.push(`bridge:${input.method}:${input.target}`); assert.equal(input.subject, subject); assert.equal(input.channel, 'owner');
      if (input.target === '/v1/code/capabilities') { assert.equal(input.body.length, 0); return runtimeResponse({ ...capabilities, credentials: 'SECRET_SENTINEL' }); }
      if (input.target.includes('?')) { assert.equal(input.target, `/v1/code/operations?operation_id=${operation}`); assert.equal(input.body.length, 0); }
      else assert.deepEqual(JSON.parse(Buffer.from(input.body).toString('utf8')), input.target.endsWith('/rollback') ? { operation_id: operation } : { operation_id: operation, objective: runBody.objective, allowed_paths: runBody.allowed_paths });
      return runtimeResponse({ ...result, credentials: 'SECRET_SENTINEL', result: { ...result.result, raw_source: 'SECRET_SENTINEL', test: { ...result.result.test, stdout: 'SECRET_SENTINEL' } } });
    } });
    for (const action of [{ action: 'status' }, runBody, { action: 'read', operation_id: operation }, { action: 'rollback', operation_id: operation }]) assert.equal((await invoke(handler, { body: action })).status, 200);
    assert.deepEqual(events, ['audit:status', 'bridge:GET:/v1/code/capabilities', 'audit:run', 'bridge:POST:/v1/code/operations', 'audit:read', `bridge:GET:/v1/code/operations?operation_id=${operation}`, 'audit:rollback', 'bridge:POST:/v1/code/operations/rollback']);
    const beforeInvalid = events.length;
    for (const body of [{ action: ['read'], operation_id: operation }, { ...runBody, operation_id: 'not-uuid' }, { ...runBody, subject: actor }, { ...runBody, objective: ' leading' }, { ...runBody, objective: 'x'.repeat(32769) }, { ...runBody, allowed_paths: [] }, { ...runBody, allowed_paths: ['z', 'a'] }, { ...runBody, allowed_paths: ['a', 'a'] }, ...['../outside', '/outside', '.git/config', 'src/CON.txt', 'src/filename.', 'src/é.ts'].map(path => ({ ...runBody, allowed_paths: [path] }))]) assert.equal((await invoke(handler, { body })).status, 400);
    assert.equal(events.length, beforeInvalid, 'invalid controls stop before audit and bridge admission');
    const unicode = createRemoteControlHandler({ authorize, audit: async () => true, bridge: async input => { assert.equal([...JSON.parse(Buffer.from(input.body).toString()).objective].length, 32768); return runtimeResponse(result); } });
    assert.equal((await invoke(unicode, { body: { ...runBody, objective: '😀'.repeat(32768) } })).status, 200);
    for (const user of [null, { ...owner, email: 'member@example.test' }]) {
      let effects = 0;
      const denied = createRemoteControlHandler({ authorize: async () => ({ authorized: false, user, authConfigured: true }), audit: async () => { effects += 1; return true; }, bridge: async () => { effects += 1; return Response.json(result); } });
      assert.equal((await invoke(denied)).status, user ? 403 : 401); assert.equal(effects, 0);
    }
    for (const audit of [async () => false, async () => { throw new Error('SECRET_SENTINEL'); }]) {
      let effects = 0;
      assert.equal((await invoke(createRemoteControlHandler({ authorize, audit, bridge: async () => { effects += 1; return Response.json(result); } }))).status, 503); assert.equal(effects, 0);
    }
    for (const value of [{ ...result, operation_id: actor }, { ...result, principal_ref: 'not-a-digest' }, { ...result, schema_version: 'foreign' }, { ...result, state: 'MAYBE_OK' }, { ...result, result: { ...result.result, test: { ...result.result.test, passed: 'true' } } }, { ...result, result: { ...result.result, promotion_event_digest: 'invalid' } }, { ...result, result: { ...result.result, changed_paths: ['../outside'] } }]) assert.equal((await invoke(createRemoteControlHandler({ authorize, audit: async () => true, bridge: async () => runtimeResponse(value) }))).status, 502);
    for (const corrupted of [{}, { ...headers, 'x-apocv4-auth-mode': 'DEVELOPMENT' }, { ...headers, 'x-apocv4-binding-ref': digest }, { ...headers, 'x-apocv4-principal-ref': 'b'.repeat(64) }, { ...headers, 'x-apocv4-auth-registry-ref': 'invalid' }, { ...headers, 'x-apocv4-privacy-partition-ref': digest }]) assert.equal((await invoke(createRemoteControlHandler({ authorize, audit: async () => true, bridge: async () => Response.json(result, { headers: corrupted }) }))).status, 502);
    const disabled = { ...capabilities, enabled: false, enforcement: 'UNAVAILABLE', actions: [] };
    const unavailable = await invoke(createRemoteControlHandler({ authorize, audit: async () => true, bridge: async () => runtimeResponse(disabled) }), { body: { action: 'status' } });
    assert.equal(unavailable.status, 200); assert.equal(unavailable.body.enabled, false);
    assert.equal((await invoke(createRemoteControlHandler({ authorize, audit: async () => true, bridge: async () => runtimeResponse({ ...disabled, enabled: true }) }), { body: { action: 'status' } })).status, 502);
    let attempts = 0;
    const uncertain = createRemoteControlHandler({ authorize, audit: async () => true, bridge: async () => { attempts += 1; throw new Error('SECRET_SENTINEL'); } });
    const unknown = await invoke(uncertain); assert.equal(unknown.status, 503); assert.equal(attempts, 1); assert.equal(unknown.body.code, 'CONTROL_RESULT_UNAVAILABLE');
    assert.equal((await invoke(handler, { query: { operation_id: actor } })).status, 400);
    assert.equal((await invoke(handler, { method: 'GET' })).status, 405);
    assert.equal((await invoke(handler, { headers: { origin: 'https://evil.test', 'content-type': 'application/json' } })).status, 403);
    assert.equal((await invoke(handler, { headers: { origin: 'https://www.apocky.com', 'content-type': 'text/plain' } })).status, 415);
    delete process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID;
    assert.equal((await invoke(handler)).status, 503);
  } finally { if (previous === undefined) delete process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID; else process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID = previous; }
  process.stdout.write('Remote control authorization, audit ordering, exact durable IDs, invalid-path rejection and response privacy checks passed.\n');
}
run().catch(error => { process.stderr.write(String(error) + '\n'); process.exitCode = 1; });
