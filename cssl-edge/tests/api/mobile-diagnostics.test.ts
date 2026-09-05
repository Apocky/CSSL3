import assert from 'node:assert/strict';
import type { NextApiRequest, NextApiResponse } from 'next';
import { createAccountHandler } from '@/lib/mobile/account-api';
import { AccountRuntimeError } from '@/lib/mobile/account-runtime';
import { accountDiagnostic, accountDiagnosticText, diagnosticForAccount, diagnosticFromBody, readAccountDiagnostic, type AccountDiagnostic } from '@/lib/mobile/diagnostics';

const canary = 'PRIVATE_PROMPT_USER_TOKEN_KEY_STACK_SENTINEL';
const subject = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const trace = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';
const requestId = 'cccccccc-cccc-4ccc-8ccc-cccccccccccc';
const user = async () => ({ authConfigured: true, user: { id: subject, email: `${canary}@example.test`, provider: 'email', createdAt: new Date(0).toISOString() } });
type Dependencies = NonNullable<Parameters<typeof createAccountHandler>[1]>;
async function invoke(surface: 'turn' | 'sessions' | 'status', dependencies: Dependencies = {}, override: Partial<NextApiRequest> = {}) {
  const logs: AccountDiagnostic[] = []; const headers: Record<string, string> = {}; let status = 0; let body: Record<string, unknown> = {};
  const req = { method: surface === 'turn' ? 'POST' : 'GET', headers: { origin: 'https://www.apocky.com', 'content-type': 'application/json', authorization: `Bearer ${canary}`, 'x-apocky-trace-id': trace }, query: {}, body: { text: canary, session_id: requestId, request_id: requestId }, ...override } as NextApiRequest;
  const res = { setHeader(name: string, value: string) { headers[name.toLowerCase()] = value; return this; }, status(value: number) { status = value; return this; }, json(value: Record<string, unknown>) { body = value; return this; } } as unknown as NextApiResponse;
  await createAccountHandler(surface, { user, configured: () => true, log: event => logs.push(event), ...dependencies })(req, res);
  const serialized = JSON.stringify({ logs, headers, body });
  assert(!serialized.includes(canary)); assert(!JSON.stringify(logs).includes(subject));
  assert.equal(logs.length, 1); assert.equal(logs[0]?.status, status);
  assert.deepEqual(Object.keys(logs[0] ?? {}).sort(), ['schema_version', 'time', 'operation', 'status', 'code', 'stage', 'duration_ms', 'trace_id'].sort());
  assert.equal(logs[0]?.trace_id, headers['x-apocky-trace-id']); assert.notEqual(headers['x-apocky-trace-id'], trace);
  assert.match(headers['x-apocky-trace-id'] ?? '', /^[a-f0-9-]{36}$/);
  return { logs, headers, body, status };
}
async function run(): Promise<void> {
  for (const [code, stage] of [['ACCOUNT_RESPONSE_TIMEOUT', 'transport'], ['ACCOUNT_HISTORY_UNVERIFIED', 'history'], ['ACCOUNT_RESPONSE_SCOPE_MISMATCH', 'transport'], [canary, 'transport']] as const) {
    const output = await invoke('sessions', { call: async () => { const error = new AccountRuntimeError(code); error.message = canary; error.stack = canary; throw error; } });
    assert.equal(output.status, 502); assert.equal(output.body.stage, stage);
    assert.equal(output.body.code, code === canary ? 'ACCOUNT_SERVICE_UNAVAILABLE' : code);
    const detail = output.body.diagnostic as AccountDiagnostic; assert.equal(detail.trace_id, output.headers['x-apocky-trace-id']);
    assert.deepEqual(output.logs[0], detail);
  }
  const unexpected = await invoke('turn', { call: async () => { throw new Error(canary); } }); assert.equal(unexpected.status, 502);
  const badAuth = await invoke('turn', { user: async () => { throw new Error(canary); } }); assert.equal(badAuth.body.code, 'ACCOUNT_SIGN_IN_UNAVAILABLE');
  const signedOut = await invoke('status', { user: async () => ({ user: null, authConfigured: true }) }); assert.equal(signedOut.status, 401);
  const configuration = await invoke('status', { configured: () => false }); assert.equal(configuration.status, 200); assert.equal(configuration.body.code, 'ACCOUNT_CONFIGURATION_UNAVAILABLE'); assert.equal(configuration.body.stage, 'configuration');
  const brokenConfiguration = await invoke('status', { configured: () => { throw new Error(canary); } }); assert.equal(brokenConfiguration.body.code, 'ACCOUNT_CONFIGURATION_UNAVAILABLE');
  const faculty = await invoke('status', { call: async () => ({ schema_version: 'apocky.mobile.status.v1', status: 'degraded', message: canary }) }); assert.equal(faculty.body.code, 'ACCOUNT_FACULTY_UNREADY'); assert.equal(faculty.body.stage, 'faculty');
  const badStatus = await invoke('status', { call: async () => ({ schema_version: 'apocky.mobile.status.v1', status: ['live'], code: canary }) }); assert.equal(badStatus.body.code, 'ACCOUNT_STATUS_UNVERIFIED');
  const live = await invoke('status', { call: async () => ({ schema_version: 'apocky.mobile.status.v1', status: 'live', private: canary }) }); assert.deepEqual(live.body, { schema_version: 'apocky.mobile.status.v1', status: 'live' }); assert.equal(live.logs[0]?.code, 'ACCOUNT_OK');
  const malformed = await invoke('turn', {}, { body: { text: canary, user_id: subject } }); assert.equal(malformed.status, 400); assert.equal(malformed.logs[0]?.stage, 'request');
  const denied = await invoke('turn', {}, { headers: { origin: canary } }); assert.equal(denied.status, 403);
  const invalidOrigin = await invoke('turn', { user: async () => { throw new Error('must not authenticate'); } }, { headers: { origin: 'http://localhost:99999', host: 'localhost:99999' } }); assert.equal(invalidOrigin.status, 403); assert.equal(invalidOrigin.body.code, 'ACCOUNT_ORIGIN_DENIED');
  const detail = accountDiagnostic({ operation: 'turn', status: 504, code: 'ACCOUNT_RESPONSE_TIMEOUT', trace_id: trace, duration_ms: 120, time: '2026-09-04T12:00:00.000Z' });
  assert.deepEqual(diagnosticForAccount({ account: subject, value: detail }, subject), detail);
  assert.equal(diagnosticForAccount({ account: subject, value: detail }, requestId), null, 'switch immediately hides another account diagnostic before effects run');
  assert.equal(diagnosticForAccount({ account: subject, value: detail }, null), null, 'sign-out hides prior diagnostic');
  assert.equal(diagnosticForAccount(null, subject), null, 'cleared diagnostics cannot reappear');
  assert(!accountDiagnosticText({ ...detail, prompt: canary, user_id: subject } as AccountDiagnostic).includes(canary));
  assert(!accountDiagnosticText({ ...detail, prompt: canary, user_id: subject } as AccountDiagnostic).includes(subject));
  for (const code of [canary, ['ACCOUNT_RESPONSE_TIMEOUT'], '__proto__', 'ACCOUNT_OK']) {
    const projected = diagnosticFromBody({ code, error: canary, stage: canary, diagnostic: { code, time: canary, trace_id: canary, duration_ms: canary } }, 'turn', 503, canary);
    assert.equal(projected.code, 'ACCOUNT_SERVICE_UNAVAILABLE'); assert.equal(projected.trace_id, null); assert(!JSON.stringify(projected).includes(canary));
  }
  assert.equal(diagnosticFromBody(live.body, 'status', 200, trace).code, 'ACCOUNT_OK');
  assert.equal(diagnosticFromBody({ schema_version: 'apocky.mobile.status.v1', status: 'degraded', code: 'ACCOUNT_OK' }, 'status', 200, trace, 'ACCOUNT_STATUS_UNVERIFIED').code, 'ACCOUNT_STATUS_UNVERIFIED');
  const errorResponse = new Response(JSON.stringify({ code: 'ACCOUNT_RESPONSE_TIMEOUT', error: canary, diagnostic: { ...detail, user_id: subject } }), { status: 504, headers: { 'content-type': 'application/json', 'x-apocky-trace-id': trace } });
  assert.deepEqual(await readAccountDiagnostic(errorResponse, 'turn'), detail);
  for (const body of [canary, JSON.stringify({ code: 'ACCOUNT_HISTORY_UNVERIFIED', extra: 'x'.repeat(8193) }), '[]', '{']) {
    const result = await readAccountDiagnostic(new Response(body, { status: 502, headers: { 'content-type': 'application/json', 'x-apocky-trace-id': trace } }), 'sessions');
    assert.equal(result.code, 'ACCOUNT_SERVICE_UNAVAILABLE'); assert.equal(result.trace_id, trace);
  }
  console.log('mobile diagnostics: safe single-call logs, correlation, typed stages, bounded untrusted input, and account-switch isolation passed');
}
void run().catch(error => { console.error(error); process.exitCode = 1; });
