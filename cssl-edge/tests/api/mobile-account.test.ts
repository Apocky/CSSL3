import assert from 'node:assert/strict';
import { createHash, createHmac } from 'node:crypto';
import type { NextApiRequest, NextApiResponse } from 'next';
import { accountReference, accountSigningKey, signAccountRequest, validAccountTarget } from '@/lib/mobile/account-grant';
import { accountRuntimeConfigured, accountRuntimeOrigin, AccountRuntimeError, callAccountRuntime } from '@/lib/mobile/account-runtime';
import { createAccountHandler } from '@/lib/mobile/account-api';

const subject = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const other = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';
const sessionId = 'cccccccc-cccc-4ccc-8ccc-cccccccccccc';
const requestId = 'dddddddd-dddd-4ddd-8ddd-dddddddddddd';
const now = 1_789_000_000;
const secret = Buffer.alloc(32, 13);
const body = { text: 'A bounded fixture question.', session_id: sessionId, request_id: requestId };
const bytes = Buffer.from(JSON.stringify(body));
const grant = signAccountRequest({ subject, method: 'POST', target: '/v1/account/turn', body: bytes, key: secret, kid: 'test-key', now, nonce: requestId });
const [prefix, encoded, signature] = grant.split('.');
assert(encoded && signature);
const payload = JSON.parse(Buffer.from(encoded, 'base64url').toString('utf8'));
assert.deepEqual(Object.keys(payload), Object.keys(payload).sort(), 'grant payload keys retain the canonical cross-language signing order');
assert.equal(prefix, 'apoc-account-v1');
assert.equal(signature, createHmac('sha256', secret).update(`${prefix}.${encoded}`, 'ascii').digest('base64url'));
assert.deepEqual(payload, { alg: 'HS256', aud: 'apocv4-runtime', body_sha256: createHash('sha256').update(bytes).digest('hex'), exp: now + 60, iat: now, iss: 'https://www.apocky.com', jti: requestId, kid: 'test-key', method: 'POST', schema_version: 'apocv4.account-request-grant.v1', sub: subject, target: '/v1/account/turn' });
assert.notEqual(accountReference(subject), accountReference(other));
assert.equal(accountReference(subject).length, 64);
assert(!accountReference(subject).includes(subject));
assert.throws(() => accountReference('owner:apocky'));
for (const target of ['/v1/owner/turn', '/v1/account/turn?scope=owner', '//evil.test', '/v1/account/../owner', '/v1/account/sessions?session_id=owner', `/v1/account/sessions?session_id=${sessionId}&account=${other}`]) {
  assert.equal(validAccountTarget('GET', target), false);
  assert.equal(validAccountTarget('POST', target), false);
}
assert(validAccountTarget('GET', `/v1/account/sessions?session_id=${sessionId}`));
assert.throws(() => signAccountRequest({ subject, method: 'GET', target: '/v1/account/status', body: bytes, key: secret, kid: 'test-key' }));
assert.throws(() => signAccountRequest({ subject, method: 'POST', target: '/v1/account/turn', body: Buffer.alloc(100_001), key: secret, kid: 'test-key' }));
const escapedBody = { ...body, text: '"\\\n\u0001'.repeat(4096) };
const escapedBytes = Buffer.from(JSON.stringify(escapedBody));
assert.equal(Buffer.byteLength(escapedBody.text), 16_384);
assert(escapedBytes.length > 20_000 && escapedBytes.length <= 100_000);
assert.doesNotThrow(() => signAccountRequest({ subject, method: 'POST', target: '/v1/account/turn', body: escapedBytes, key: secret, kid: 'test-key' }), 'valid maximum-size text must remain admissible after JSON escaping');
assert.throws(() => accountSigningKey({ NODE_ENV: 'test', APOCV4_ACCOUNT_GRANT_KEY_B64: Buffer.alloc(16).toString('base64'), APOCV4_ACCOUNT_GRANT_KEY_ID: 'test-key' }));
assert.throws(() => accountSigningKey({ NODE_ENV: 'test', SUPABASE_SERVICE_ROLE_KEY: secret.toString('base64') }));

const user = async () => ({ user: { id: subject, email: 'ordinary-account@example.test', provider: 'email', createdAt: new Date(0).toISOString() }, authConfigured: true });
const stamp = '2026-09-04T12:00:00Z';
const result = { schema_version: 'apocky.mobile.turn.v1', status: 'completed', text: 'A verified fixture reply.', session_id: sessionId, request_id: requestId, model_id: 'fixture', response_digest: 'a'.repeat(64) };
const privateData = { account_ref: accountReference(subject), owner_cognition: 'PRIVATE_TEST_SENTINEL', credentials: 'PRIVATE_TEST_SENTINEL' };
type Handler = ReturnType<typeof createAccountHandler>;
async function invoke(handler: Handler, override: Partial<NextApiRequest> = {}) {
  const output = { status: 200, headers: {} as Record<string, string>, body: null as unknown };
  const req = { method: 'POST', headers: { origin: 'https://www.apocky.com', host: 'www.apocky.com', 'content-type': 'application/json', authorization: 'Bearer USER_TOKEN_TEST_SENTINEL' }, query: {}, body, ...override } as NextApiRequest;
  const res = { setHeader(name: string, value: string) { output.headers[name.toLowerCase()] = value; return this; }, status(status: number) { output.status = status; return this; }, json(value: unknown) { output.body = value; return this; } } as unknown as NextApiResponse;
  await handler(req, res);
  assert(!JSON.stringify(output).includes('PRIVATE_TEST_SENTINEL'));
  assert(!JSON.stringify(output).includes('USER_TOKEN_TEST_SENTINEL'));
  assert.match(output.headers['cache-control'] ?? '', /private, no-store/);
  return output;
}

async function run(): Promise<void> {
  let calls = 0;
  const handler = createAccountHandler('turn', { user, configured: () => true, call: async input => {
    calls += 1;
    assert.deepEqual(input, { subject, method: 'POST', target: '/v1/account/turn', body });
    return { ...result, ...privateData };
  } });
  assert.deepEqual((await invoke(handler)).body, result, 'ordinary account accepted and response strictly projected');
  const maximumModel = { ...result, model_id: 'm'.repeat(512) };
  const maximumModelHandler = createAccountHandler('turn', { user, configured: () => true, call: async () => maximumModel });
  assert.deepEqual((await invoke(maximumModelHandler)).body, maximumModel, 'the complete 512-byte runtime model identifier is accepted');
  const oversizedModel = createAccountHandler('turn', { user, configured: () => true, call: async () => ({ ...result, model_id: 'm'.repeat(513) }) });
  assert.equal((await invoke(oversizedModel)).status, 502);
  const escapedHandler = createAccountHandler('turn', { user, configured: () => true, call: async input => { assert.deepEqual(input.body, escapedBody); return result; } });
  assert.equal((await invoke(escapedHandler, { body: escapedBody })).status, 200, 'quoted, control and multiline text admits within the text-byte limit');
  for (const [override, status] of [
    [{ method: 'GET' }, 405], [{ headers: { origin: 'https://evil.test', 'content-type': 'application/json' } }, 403],
    [{ headers: { origin: 'https://www.apocky.com.evil.test', 'content-type': 'application/json' } }, 403],
    [{ headers: { origin: 'https://www.apocky.com', 'content-type': 'text/plain' } }, 415],
    [{ body: { ...body, subject: other } }, 400], [{ body: { ...body, profile_id: other } }, 400],
    [{ body: { ...body, text: ' leading space' } }, 400], [{ body: { ...body, text: '😀'.repeat(4097) } }, 400],
    [{ body: { ...body, request_id: 'owner:apocky' } }, 400], [{ query: { account_id: other } }, 400],
  ] as Array<[Partial<NextApiRequest>, number]>) assert.equal((await invoke(handler, override)).status, status);
  assert.equal(calls, 1, 'all denied requests stop before runtime admission');
  const unauthenticated = createAccountHandler('turn', { configured: () => true, call: async () => { throw new Error('must not execute'); } });
  assert.equal((await invoke(unauthenticated, { headers: { origin: 'https://www.apocky.com', 'content-type': 'application/json' } })).status, 401, 'real no-token auth path denies before execution');
  for (const failureKind of ['invalid-session', 'upstream-unavailable', 'unconfigured'] as const) {
    const denied = createAccountHandler('turn', { user: async () => ({ user: null, authConfigured: false, failureKind }), configured: () => true });
    assert.equal((await invoke(denied)).status, failureKind === 'invalid-session' ? 401 : 503);
  }
  for (const invalid of [{ session_id: other }, { request_id: other }, { response_digest: 'bad' }, { schema_version: 'apocky.brain.turn.v1' }, { text: '' }]) {
    const malformed = createAccountHandler('turn', { user, configured: () => true, call: async () => ({ ...result, ...privateData, ...invalid }) });
    assert.equal((await invoke(malformed)).status, 502);
  }
  for (const error of [new Error('PRIVATE_TEST_SENTINEL'), new AccountRuntimeError('ACCOUNT_RESPONSE_SCOPE_MISMATCH'), new AccountRuntimeError('ACCOUNT_RESPONSE_TIMEOUT', 504)]) {
    const uncertain = createAccountHandler('turn', { user, configured: () => true, call: async () => { throw error; } });
    assert((await invoke(uncertain)).status >= 500, 'post-admission failures remain uncertain for request-identity retention');
  }
  const message = { role: 'assistant', content: 'Saved fixture reply.', request_id: requestId, recorded_at: stamp, event_digest: 'b'.repeat(64) };
  const history = { schema_version: 'apocky.mobile.session.v1', status: 'live', session: { schema_version: 'apocky.mobile.history-session.v1', session_id: sessionId, title: 'My conversation', created_at: stamp, updated_at: stamp, events_truncated: false, messages: [message] } };
  const historyHandler = createAccountHandler('sessions', { user, configured: () => true, call: async input => {
    assert.equal(input.subject, subject); assert.equal(input.target, `/v1/account/sessions?session_id=${sessionId}`);
    return { ...history, ...privateData, session: { ...history.session, ...privateData, messages: [{ ...message, ...privateData }] } };
  } });
  assert.deepEqual((await invoke(historyHandler, { method: 'GET', query: { session_id: sessionId } })).body, history);
  assert.equal((await invoke(historyHandler, { method: 'GET', query: { session_id: sessionId, account_id: other } })).status, 400);
  for (const invalid of [{ session_id: other }, { messages: [{ ...message, role: 'system' }] }, { messages: [{ ...message, role: ['assistant'] }] }, { messages: [{ ...message, event_digest: 'bad' }] }]) {
    const badHistory = createAccountHandler('sessions', { user, configured: () => true, call: async () => ({ ...history, session: { ...history.session, ...invalid } }) });
    assert.equal((await invoke(badHistory, { method: 'GET', query: { session_id: sessionId } })).status, 502);
  }
  const list = { schema_version: 'apocky.mobile.sessions.v1', status: 'live', discovery_scope: 'account_conversations', sessions: [{ session_id: sessionId, title: 'My conversation', updated_at: stamp, message_count: 2 }], count: 1 };
  const listHandler = createAccountHandler('sessions', { user, configured: () => true, call: async () => ({ ...list, ...privateData }) });
  assert.deepEqual((await invoke(listHandler, { method: 'GET' })).body, list);
  const badList = createAccountHandler('sessions', { user, configured: () => true, call: async () => ({ ...list, discovery_scope: ['account_conversations'] }) });
  assert.equal((await invoke(badList, { method: 'GET' })).status, 502);
  const badStatus = createAccountHandler('status', { user, configured: () => true, call: async () => ({ schema_version: 'apocky.mobile.status.v1', status: ['live'] }) });
  const degraded = await invoke(badStatus, { method: 'GET' });
  assert.equal(degraded.status, 200);
  assert.equal((degraded.body as Record<string, unknown>).status, 'degraded');
  const off = createAccountHandler('status', { user, configured: () => false, call: async () => { throw new Error('must not execute'); } });
  assert.equal(((await invoke(off, { method: 'GET' })).body as Record<string, unknown>).status, 'degraded');

  const names = ['NODE_ENV', 'APOCV4_ACCOUNT_RUNTIME_URL', 'APOCV4_ACCOUNT_GRANT_KEY_B64', 'APOCV4_ACCOUNT_GRANT_KEY_ID',
    'APOCV4_ACCOUNT_CF_ACCESS_CLIENT_ID', 'APOCV4_ACCOUNT_CF_ACCESS_CLIENT_SECRET',
    'APOCV4_RUNTIME_TRANSPORT', 'APOCV4_RUNTIME_URL', 'APOCRYPHA_TUNNEL_HOST', 'CF_ACCESS_CLIENT_ID', 'CF_ACCESS_CLIENT_SECRET'] as const;
  const environment: Record<string, string | undefined> = process.env;
  const saved = Object.fromEntries(names.map(name => [name, process.env[name]]));
  const realFetch = globalThis.fetch;
  const realTimeout = globalThis.setTimeout;
  const runtimeInput = { subject, method: 'POST' as const, target: '/v1/account/turn', body };
  try {
    environment.NODE_ENV = 'test';
    process.env.APOCV4_ACCOUNT_RUNTIME_URL = 'http://127.0.0.1:19999';
    process.env.APOCV4_ACCOUNT_GRANT_KEY_B64 = secret.toString('base64');
    process.env.APOCV4_ACCOUNT_GRANT_KEY_ID = 'test-key';
    assert(accountRuntimeConfigured());
    const json = (value: unknown) => new Response(JSON.stringify(value), { headers: { 'content-type': 'application/json' } });
    let fetched = 0;
    globalThis.fetch = async (url, init) => {
      fetched += 1;
      assert.equal(url, 'http://127.0.0.1:19999/v1/account/turn');
      assert.equal(init?.redirect, 'manual'); assert.equal(init?.cache, 'no-store');
      assert.equal(new Headers(init?.headers).get('cf-access-client-id'), null);
      assert.equal(new Headers(init?.headers).get('cf-access-client-secret'), null);
      const auth = new Headers(init?.headers).get('authorization') ?? '';
      assert(auth.startsWith('Bearer apoc-account-v1.'));
      assert(!auth.includes('USER_TOKEN_TEST_SENTINEL'));
      const admissionPart = auth.split('.')[1]; assert(admissionPart);
      const admission = JSON.parse(Buffer.from(admissionPart, 'base64url').toString());
      assert.equal(admission.sub, subject);
      assert.equal(admission.body_sha256, createHash('sha256').update(init?.body as Uint8Array).digest('hex'));
      return json({ ...result, account_ref: accountReference(subject) });
    };
    assert.equal((await callAccountRuntime(runtimeInput)).text, result.text);
    assert.equal(fetched, 1);
    environment.NODE_ENV = 'production';
    assert.equal(accountRuntimeConfigured(), false, 'production rejects a loopback account origin');
    process.env.APOCV4_ACCOUNT_RUNTIME_URL = 'https://apocrypha.apocky.com';
    process.env.CF_ACCESS_CLIENT_ID = 'OWNER_ID_TEST_SENTINEL';
    process.env.CF_ACCESS_CLIENT_SECRET = 'OWNER_SECRET_TEST_SENTINEL';
    delete process.env.APOCV4_ACCOUNT_CF_ACCESS_CLIENT_ID;
    delete process.env.APOCV4_ACCOUNT_CF_ACCESS_CLIENT_SECRET;
    globalThis.fetch = async () => { throw new Error('missing account credentials must stop before fetch'); };
    assert.equal(accountRuntimeConfigured(), false, 'owner credentials cannot configure account transport');
    await assert.rejects(callAccountRuntime(runtimeInput), (error: unknown) => error instanceof AccountRuntimeError && error.publicStatus === 503);
    process.env.APOCV4_ACCOUNT_CF_ACCESS_CLIENT_ID = 'ACCOUNT_ID_TEST_SENTINEL';
    assert.equal(accountRuntimeConfigured(), false, 'both dedicated account Access credentials are required');
    process.env.APOCV4_ACCOUNT_CF_ACCESS_CLIENT_SECRET = 'ACCOUNT_SECRET_TEST_SENTINEL';
    let accountFetches = 0;
    globalThis.fetch = async (url, init) => {
      accountFetches += 1;
      assert.equal(url, 'https://apocrypha.apocky.com/v1/account/turn');
      assert.equal(init?.redirect, 'manual'); assert.equal(init?.cache, 'no-store');
      const headers = new Headers(init?.headers);
      assert.equal(headers.get('cf-access-client-id'), 'ACCOUNT_ID_TEST_SENTINEL');
      assert.equal(headers.get('cf-access-client-secret'), 'ACCOUNT_SECRET_TEST_SENTINEL');
      assert(headers.get('authorization')?.startsWith('Bearer apoc-account-v1.'));
      assert(!JSON.stringify([...headers]).includes('OWNER_'));
      return json({ ...result, account_ref: accountReference(subject) });
    };
    for (const transport of ['direct', 'cloudflare-access', 'invalid-owner-transport']) {
      process.env.APOCV4_RUNTIME_TRANSPORT = transport;
      process.env.APOCV4_RUNTIME_URL = 'http://127.0.0.1:19998';
      process.env.APOCRYPHA_TUNNEL_HOST = 'owner-only.invalid';
      assert(accountRuntimeConfigured(), 'owner transport, URL and tunnel host cannot invalidate dedicated account configuration');
      assert.equal((await callAccountRuntime(runtimeInput)).text, result.text);
    }
    assert.equal(accountFetches, 3);
    delete process.env.APOCV4_ACCOUNT_RUNTIME_URL;
    process.env.APOCV4_RUNTIME_URL = 'https://apocrypha.apocky.com';
    assert.equal(accountRuntimeConfigured(), false, 'owner URL cannot substitute for missing account URL');
    await assert.rejects(callAccountRuntime(runtimeInput), (error: unknown) => error instanceof AccountRuntimeError && error.publicStatus === 503);
    process.env.APOCV4_ACCOUNT_RUNTIME_URL = 'https://apocrypha.apocky.com';
    for (const credential of ['APOCV4_ACCOUNT_CF_ACCESS_CLIENT_ID', 'APOCV4_ACCOUNT_CF_ACCESS_CLIENT_SECRET'] as const) {
      const valid = process.env[credential];
      for (const invalid of ['', ' leading-space', 'line\r\nbreak', 'é', 'x'.repeat(4097)]) {
        process.env[credential] = invalid;
        assert.equal(accountRuntimeConfigured(), false, 'malformed dedicated credentials fail closed');
        await assert.rejects(callAccountRuntime(runtimeInput), (error: unknown) => error instanceof AccountRuntimeError && error.publicStatus === 503 && !error.message.includes('SENTINEL'));
      }
      process.env[credential] = valid;
    }
    assert.equal(accountFetches, 3, 'invalid credentials never invoke the transport');
    globalThis.fetch = async (_url, init) => {
      assert.equal(init?.redirect, 'manual');
      return new Response(null, { status: 302, headers: { location: 'https://owner-only.invalid/', 'content-type': 'application/json' } });
    };
    await assert.rejects(callAccountRuntime(runtimeInput), /ACCOUNT_UPSTREAM_UNVERIFIED/, 'redirects cannot forward account credentials');
    environment.NODE_ENV = 'test';
    process.env.APOCV4_ACCOUNT_RUNTIME_URL = 'http://127.0.0.1:19999';
    const historyInput = { subject, method: 'GET' as const, target: `/v1/account/sessions?session_id=${sessionId}` };
    const missing = (account: string, code = 'ACCOUNT_SESSION_NOT_FOUND') => new Response(JSON.stringify({ account_ref: accountReference(account), code }), { status: 404, headers: { 'content-type': 'application/json' } });
    globalThis.fetch = async () => missing(subject);
    await assert.rejects(callAccountRuntime(historyInput), (error: unknown) => error instanceof AccountRuntimeError && error.code === 'ACCOUNT_SESSION_NOT_FOUND' && error.publicStatus === 404, 'only exact account-bound absent-session response returns a definite 404');
    globalThis.fetch = async () => missing(other);
    await assert.rejects(callAccountRuntime(historyInput), (error: unknown) => error instanceof AccountRuntimeError && error.code === 'ACCOUNT_RESPONSE_SCOPE_MISMATCH' && error.publicStatus === 502, 'foreign-account absent-session response cannot reveal definite absence');
    globalThis.fetch = async () => missing(subject);
    await assert.rejects(callAccountRuntime(runtimeInput), (error: unknown) => error instanceof AccountRuntimeError && error.publicStatus === 502, 'POST 404 remains unverified and keeps uncertain request identity');
    await assert.rejects(callAccountRuntime({ subject, method: 'GET', target: '/v1/account/sessions' }), (error: unknown) => error instanceof AccountRuntimeError && error.publicStatus === 502, 'list 404 is not an absent individual session');
    globalThis.fetch = async () => missing(subject, 'OTHER_NOT_FOUND');
    await assert.rejects(callAccountRuntime(historyInput), (error: unknown) => error instanceof AccountRuntimeError && error.publicStatus === 502, 'a matching account alone does not admit an unrecognized 404 code');
    globalThis.fetch = async () => missing(subject);
    const missingHistoryHandler = createAccountHandler('sessions', { user, configured: () => true });
    const missingResponse = await invoke(missingHistoryHandler, { method: 'GET', query: { session_id: sessionId } });
    assert.equal(missingResponse.status, 404);
    assert.equal((missingResponse.body as Record<string, unknown>).code, 'ACCOUNT_SESSION_NOT_FOUND');
    assert(!JSON.stringify(missingResponse.body).includes(accountReference(subject)), 'account binding remains server-only');
    for (const [response, code] of [
      [json({ ...result, account_ref: accountReference(other) }), 'ACCOUNT_RESPONSE_SCOPE_MISMATCH'],
      [new Response('PRIVATE_TEST_SENTINEL', { status: 403 }), 'ACCOUNT_UPSTREAM_UNVERIFIED'],
      [new Response('[]', { headers: { 'content-type': 'application/json' } }), 'ACCOUNT_RESPONSE_INVALID'],
      [new Response('invalid', { headers: { 'content-type': 'application/json' } }), 'ACCOUNT_RESPONSE_INVALID'],
      [new Response(new Uint8Array([0xff]), { headers: { 'content-type': 'application/json' } }), 'ACCOUNT_RESPONSE_INVALID'],
      [new Response('x'.repeat(262_145), { headers: { 'content-type': 'application/json' } }), 'ACCOUNT_RESPONSE_TOO_LARGE'],
    ] as Array<[Response, string]>) {
      globalThis.fetch = async () => response;
      await assert.rejects(callAccountRuntime(runtimeInput), (error: unknown) => error instanceof AccountRuntimeError && error.code === code && error.publicStatus >= 500);
    }
    const redirected = json({ ...result, account_ref: accountReference(subject) });
    Object.defineProperty(redirected, 'redirected', { value: true });
    globalThis.fetch = async () => redirected;
    await assert.rejects(callAccountRuntime(runtimeInput), /ACCOUNT_UPSTREAM_UNVERIFIED/);
    globalThis.setTimeout = ((callback: () => void, delay?: number) => realTimeout(callback, delay === 115_000 ? 0 : delay)) as typeof setTimeout;
    globalThis.fetch = async (_url, init) => new Promise<Response>((_resolve, reject) => init?.signal?.addEventListener('abort', () => reject(new Error('aborted')), { once: true }));
    await assert.rejects(callAccountRuntime(runtimeInput), (error: unknown) => error instanceof AccountRuntimeError && error.code === 'ACCOUNT_RESPONSE_TIMEOUT' && error.publicStatus === 504);
    globalThis.setTimeout = realTimeout;
    for (const url of ['https://evil.test', 'https://apocrypha.apocky.com.evil.test', 'https://user:pass@apocrypha.apocky.com',
      'https://apocrypha.apocky.com/', 'https://apocrypha.apocky.com:444', 'https://apocrypha.apocky.com?owner=1', 'https://apocrypha.apocky.com#owner',
      'http://127.0.0.1:99999', 'http://localhost:19999', 'http://127.0.0.1:19999/path']) {
      process.env.APOCV4_ACCOUNT_RUNTIME_URL = url;
      assert.throws(() => accountRuntimeOrigin());
    }
    process.env.APOCV4_ACCOUNT_RUNTIME_URL = 'http://127.0.0.1:19999';
    environment.NODE_ENV = 'production';
    assert.throws(() => accountRuntimeOrigin(), 'production must not admit local runtime origins');
  } finally {
    globalThis.fetch = realFetch; globalThis.setTimeout = realTimeout;
    for (const name of names) { if (saved[name] === undefined) delete environment[name]; else environment[name] = saved[name]; }
  }
  console.log('mobile account: signed subject/body/target grant, ordinary-account admission, private-field projection, scope binding, timeout and fail-closed transport passed');
}
void run().catch(error => { console.error(error); process.exitCode = 1; });
