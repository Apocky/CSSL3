import type { NextApiRequest, NextApiResponse } from 'next';

import {
  assertWorkerRequest,
  resetApocryphaServiceClientForTests,
} from '@/lib/apocrypha/job-control';
import claimHandler from '@/pages/api/apocrypha/worker/claim';
import heartbeatHandler from '@/pages/api/apocrypha/worker/heartbeat';

interface MockResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, number | string | readonly string[]>;
}

function assert(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function request(
  headers: Record<string, string | string[]>,
  body: Record<string, unknown>,
): NextApiRequest {
  return { method: 'POST', headers, body } as unknown as NextApiRequest;
}

function response(): { res: NextApiResponse; out: MockResponse } {
  const out: MockResponse = { statusCode: 0, body: null, headers: {} };
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(body: unknown) { out.body = body; return this; },
    setHeader(name: string, value: number | string | readonly string[]) {
      out.headers[name.toLowerCase()] = value;
      return this;
    },
  } as unknown as NextApiResponse;
  return { res, out };
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function assertUnauthorized(body: unknown, message: string): void {
  const payload = body as Record<string, unknown>;
  assert(payload?.ok === false && payload.code === 'UNAUTHORIZED', message);
}

async function invokeClaim(token: string | null): Promise<MockResponse> {
  const { res, out } = response();
  await claimHandler(request({
    ...(token === null ? {} : { authorization: `Bearer ${token}` }),
    'idempotency-key': 'claim-test-key',
  }, { node_id: '11111111-1111-4111-8111-111111111111' }), res);
  return out;
}

async function testEdgeChecksShapeWithoutOwningTheSecret(): Promise<void> {
  const shapedToken = `apn_${'a'.repeat(64)}`;
  delete process.env.APOCRYPHA_WORKER_TOKEN;
  assert(assertWorkerRequest(`Bearer ${shapedToken}`) === shapedToken, 'shape-valid worker token was not forwarded');

  for (const header of [
    undefined,
    'Basic credentials',
    'Bearer too-short',
    `Bearer apn_${'g'.repeat(64)}`,
    [`Bearer ${shapedToken}`, `Bearer apn_${'b'.repeat(64)}`],
  ]) {
    let rejected = false;
    try {
      assertWorkerRequest(header);
    } catch (error) {
      rejected = error instanceof Error && error.message === 'WORKER_UNAUTHORIZED';
    }
    assert(rejected, `malformed or ambiguous worker header was accepted: ${String(header)}`);
  }
}

async function testWorkerRpcMapsDatabaseAuthenticationTo401(): Promise<void> {
  const shapedToken = `apn_${'b'.repeat(64)}`;
  let calls = 0;
  globalThis.fetch = async () => {
    calls += 1;
    return jsonResponse({ code: '28000', message: 'worker authentication failed' }, 400);
  };
  resetApocryphaServiceClientForTests();

  const out = await invokeClaim(shapedToken);
  assert(out.statusCode === 401, `database authentication failure returned ${out.statusCode}`);
  assertUnauthorized(out.body, 'database authentication failure did not keep the 401 contract');
  assert(calls === 1, `database authentication failure made ${calls} requests`);
}

async function testWorkerRpcDoesNotMisclassifyAnother28000(): Promise<void> {
  const shapedToken = `apn_${'f'.repeat(64)}`;
  globalThis.fetch = async () => jsonResponse({
    code: '28000',
    message: 'claim replay integrity check failed',
  }, 400);
  resetApocryphaServiceClientForTests();

  const out = await invokeClaim(shapedToken);
  assert(out.statusCode === 503, `non-authentication database failure returned ${out.statusCode}`);
  const payload = out.body as Record<string, unknown>;
  assert(payload.code === 'APOCRYPHA_JOB_ERROR', 'non-authentication database failure was exposed as unauthorized');
}

async function testWorkerRpcForwardsShapeValidTokenToDatabase(): Promise<void> {
  const shapedToken = `apn_${'c'.repeat(64)}`;
  let capturedUrl = '';
  let capturedInit: RequestInit | undefined;
  globalThis.fetch = async (input, init) => {
    capturedUrl = String(input);
    capturedInit = init;
    return jsonResponse([]);
  };
  resetApocryphaServiceClientForTests();

  const out = await invokeClaim(shapedToken);
  assert(out.statusCode === 200, `shape-valid worker request returned ${out.statusCode}`);
  assert(capturedUrl.includes('/rest/v1/rpc/apocrypha_claim_job'), 'claim did not reach the authoritative database RPC');
  const rpcBody = JSON.parse(String(capturedInit?.body ?? '{}')) as Record<string, unknown>;
  assert(rpcBody.p_node_token === shapedToken, 'claim did not forward the bearer token to database verification');
}

async function testMalformedBearerStopsBeforeDatabase(): Promise<void> {
  let calls = 0;
  globalThis.fetch = async () => {
    calls += 1;
    return jsonResponse([]);
  };
  resetApocryphaServiceClientForTests();

  const missing = await invokeClaim(null);
  assert(missing.statusCode === 401, `missing bearer returned ${missing.statusCode}`);
  assertUnauthorized(missing.body, 'missing bearer did not keep the 401 contract');
  assert(calls === 0, 'missing bearer reached the database');
}

async function testHeartbeatAuthenticatesAtDatabaseBeforeUpdate(): Promise<void> {
  const shapedToken = `apn_${'d'.repeat(64)}`;
  const nodeId = '22222222-2222-4222-8222-222222222222';
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  globalThis.fetch = async (input, init) => {
    const call = { url: String(input), init };
    calls.push(call);
    if (call.url.includes('/rest/v1/rpc/apocrypha_require_worker')) {
      return jsonResponse({ id: nodeId });
    }
    return jsonResponse({ id: nodeId, last_seen_at: '2026-09-07T12:00:00.000Z' });
  };
  resetApocryphaServiceClientForTests();

  const { res, out } = response();
  await heartbeatHandler(request({ authorization: `Bearer ${shapedToken}` }, {
    node_id: nodeId,
    status: 'idle',
    model_alias: 'test-model',
  }), res);

  assert(out.statusCode === 200, `authenticated heartbeat returned ${out.statusCode}`);
  assert(calls.length === 2, `heartbeat made ${calls.length} database requests instead of auth then update`);
  assert(calls[0]?.url.includes('/rest/v1/rpc/apocrypha_require_worker') === true, 'heartbeat did not authenticate first');
  const authBody = JSON.parse(String(calls[0]?.init?.body ?? '{}')) as Record<string, unknown>;
  assert(authBody.p_node_id === nodeId, 'heartbeat database authentication used the wrong node');
  assert(authBody.p_node_token === shapedToken, 'heartbeat did not forward the bearer token for hash verification');
  assert(authBody.p_require_active === true, 'heartbeat did not require an active worker');
  assert(calls[1]?.url.includes('/rest/v1/apocrypha_worker_node') === true, 'heartbeat did not update after authentication');
}

async function testHeartbeatDatabaseAuthenticationFailureIs401(): Promise<void> {
  const shapedToken = `apn_${'e'.repeat(64)}`;
  let calls = 0;
  globalThis.fetch = async () => {
    calls += 1;
    return jsonResponse({ code: '28000', message: 'worker authentication failed' }, 400);
  };
  resetApocryphaServiceClientForTests();

  const { res, out } = response();
  await heartbeatHandler(request({ authorization: `Bearer ${shapedToken}` }, {
    node_id: '33333333-3333-4333-8333-333333333333',
    status: 'idle',
  }), res);

  assert(out.statusCode === 401, `heartbeat database authentication failure returned ${out.statusCode}`);
  assertUnauthorized(out.body, 'heartbeat database authentication failure did not keep the 401 contract');
  assert(calls === 1, `rejected heartbeat continued to the update (${calls} requests)`);
}

async function main(): Promise<void> {
  const originalFetch = globalThis.fetch;
  const originalEnv = {
    url: process.env.APOCKY_HUB_SUPABASE_URL,
    publicUrl: process.env.NEXT_PUBLIC_SUPABASE_URL,
    serviceKey: process.env.SUPABASE_SERVICE_ROLE_KEY,
    workerToken: process.env.APOCRYPHA_WORKER_TOKEN,
  };
  process.env.APOCKY_HUB_SUPABASE_URL = 'https://supabase.test';
  process.env.SUPABASE_SERVICE_ROLE_KEY = 'service-role-test-key';
  delete process.env.NEXT_PUBLIC_SUPABASE_URL;

  try {
    await testEdgeChecksShapeWithoutOwningTheSecret();
    await testMalformedBearerStopsBeforeDatabase();
    await testWorkerRpcMapsDatabaseAuthenticationTo401();
    await testWorkerRpcDoesNotMisclassifyAnother28000();
    await testWorkerRpcForwardsShapeValidTokenToDatabase();
    await testHeartbeatAuthenticatesAtDatabaseBeforeUpdate();
    await testHeartbeatDatabaseAuthenticationFailureIs401();
  } finally {
    globalThis.fetch = originalFetch;
    resetApocryphaServiceClientForTests();
    for (const [key, value] of Object.entries({
      APOCKY_HUB_SUPABASE_URL: originalEnv.url,
      NEXT_PUBLIC_SUPABASE_URL: originalEnv.publicUrl,
      SUPABASE_SERVICE_ROLE_KEY: originalEnv.serviceKey,
      APOCRYPHA_WORKER_TOKEN: originalEnv.workerToken,
    })) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
  console.log('apocrypha-worker-http.test: OK');
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
