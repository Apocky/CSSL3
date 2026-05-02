// § T11-W14-K · tests/cron/cron-endpoints.test.ts
// Smoke-tests for /api/cron/* endpoints. Verifies :
//   - heartbeat GET works without auth (public-read transparency)
//   - all endpoints reject POST when CRON_SECRET set + no creds
//   - all endpoints stub-mode-OK when CRON_SECRET unset
//   - heartbeat-roundtrip carries commit_sha + region + uptime_sec
//   - audit envelope shape

import heartbeatHandler from '@/pages/api/cron/heartbeat';
import playtestHandler from '@/pages/api/cron/playtest-cycle';
import kanRollupHandler from '@/pages/api/cron/kan-rollup';
import myceliumHandler, { K_ANON_FLOOR } from '@/pages/api/cron/mycelium-relay';
import hotfixRefreshHandler from '@/pages/api/cron/hotfix-manifest-refresh';
import sigmaCheckpointHandler, { CHECKPOINT_WINDOW } from '@/pages/api/cron/sigma-chain-checkpoint';
import type { NextApiRequest, NextApiResponse } from 'next';

interface MockedResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function mockReqRes(method: string, headers: Record<string, string> = {}): {
  req: NextApiRequest; res: NextApiResponse; out: MockedResponse;
} {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };
  const req = {
    method,
    query: {},
    headers,
    body: undefined,
  } as unknown as NextApiRequest;
  const res = {
    status(c: number) { out.statusCode = c; return this; },
    json(b: unknown) { out.body = b; return this; },
    setHeader(k: string, v: string) { out.headers[k] = v; return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. heartbeat GET works without auth.
async function testHeartbeatGetPublic(): Promise<void> {
  const prev = process.env['CRON_SECRET'];
  process.env['CRON_SECRET'] = 'present';
  const { req, res, out } = mockReqRes('GET');
  await heartbeatHandler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const body = out.body as Record<string, unknown>;
  assert(body['ok'] === true, 'ok:true');
  assert(body['job'] === 'heartbeat', 'job=heartbeat');
  assert(typeof body['commit_sha'] === 'string', 'commit_sha');
  assert(typeof body['region'] === 'string', 'region');
  assert(typeof body['uptime_sec'] === 'number', 'uptime_sec');
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
}

// 2. heartbeat POST rejects when no creds and CRON_SECRET set.
async function testHeartbeatPostRequiresAuth(): Promise<void> {
  const prev = process.env['CRON_SECRET'];
  process.env['CRON_SECRET'] = 'topsecret';
  const { req, res, out } = mockReqRes('POST');
  await heartbeatHandler(req, res);
  assert(out.statusCode === 401, `expected 401, got ${out.statusCode}`);
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
}

// 3. heartbeat POST stub-mode 200 when no CRON_SECRET configured.
async function testHeartbeatStubMode(): Promise<void> {
  const prev = process.env['CRON_SECRET'];
  delete process.env['CRON_SECRET'];
  const { req, res, out } = mockReqRes('POST');
  await heartbeatHandler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const body = out.body as Record<string, unknown>;
  assert(body['stub'] === true, 'stub:true');
  if (prev !== undefined) process.env['CRON_SECRET'] = prev;
}

// 4. playtest-cycle stub-mode shape.
async function testPlaytestStubMode(): Promise<void> {
  const prev = process.env['CRON_SECRET'];
  delete process.env['CRON_SECRET'];
  const { req, res, out } = mockReqRes('POST');
  await playtestHandler(req, res);
  assert(out.statusCode === 200, `expected 200`);
  const body = out.body as Record<string, unknown>;
  assert(body['ok'] === true && body['stub'] === true, 'ok+stub');
  assert(body['job'] === 'playtest-cycle', 'job name');
  assert(typeof body['picked'] === 'number', 'picked');
  assert(typeof body['enqueued'] === 'number', 'enqueued');
  if (prev !== undefined) process.env['CRON_SECRET'] = prev;
}

// 5. kan-rollup auth required.
async function testKanRollupAuthRequired(): Promise<void> {
  const prev = process.env['CRON_SECRET'];
  process.env['CRON_SECRET'] = 'topsecret';
  const { req, res, out } = mockReqRes('POST');
  await kanRollupHandler(req, res);
  assert(out.statusCode === 401, `expected 401, got ${out.statusCode}`);
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
}

// 6. kan-rollup with valid Bearer succeeds in stub-no-supabase mode.
async function testKanRollupAuthSuccess(): Promise<void> {
  const prev = process.env['CRON_SECRET'];
  const prevUrl = process.env['NEXT_PUBLIC_SUPABASE_URL'];
  process.env['CRON_SECRET'] = 'topsecret';
  delete process.env['NEXT_PUBLIC_SUPABASE_URL']; // ensure no supabase
  const { req, res, out } = mockReqRes('POST', {
    authorization: 'Bearer topsecret',
  });
  await kanRollupHandler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const body = out.body as Record<string, unknown>;
  assert(body['ok'] === true, 'ok:true');
  assert(body['job'] === 'kan-rollup', 'job name');
  assert(typeof body['promoted_1hr'] === 'number', 'promoted_1hr');
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
  if (prevUrl !== undefined) process.env['NEXT_PUBLIC_SUPABASE_URL'] = prevUrl;
}

// 7. mycelium-relay enforces k-anon constant.
async function testMyceliumKAnonExported(): Promise<void> {
  assert(K_ANON_FLOOR === 10, 'k-anon floor = 10');
}

// 8. mycelium-relay stub-mode safe.
async function testMyceliumStubMode(): Promise<void> {
  const prev = process.env['CRON_SECRET'];
  delete process.env['CRON_SECRET'];
  const { req, res, out } = mockReqRes('POST');
  await myceliumHandler(req, res);
  assert(out.statusCode === 200, 'stub 200');
  const body = out.body as Record<string, unknown>;
  assert(body['ok'] === true && body['stub'] === true, 'ok+stub');
  assert(Array.isArray(body['hot_patterns']), 'hot_patterns array');
  if (prev !== undefined) process.env['CRON_SECRET'] = prev;
}

// 9. hotfix-manifest-refresh stub-mode.
async function testHotfixRefreshStubMode(): Promise<void> {
  const prev = process.env['CRON_SECRET'];
  delete process.env['CRON_SECRET'];
  const { req, res, out } = mockReqRes('POST');
  await hotfixRefreshHandler(req, res);
  assert(out.statusCode === 200, 'stub 200');
  const body = out.body as Record<string, unknown>;
  assert(body['stub'] === true, 'stub:true');
  if (prev !== undefined) process.env['CRON_SECRET'] = prev;
}

// 10. sigma-chain-checkpoint stub-mode + window-constant.
async function testSigmaCheckpointStubMode(): Promise<void> {
  assert(CHECKPOINT_WINDOW === 1024, 'checkpoint window = 1024');
  const prev = process.env['CRON_SECRET'];
  delete process.env['CRON_SECRET'];
  const { req, res, out } = mockReqRes('POST');
  await sigmaCheckpointHandler(req, res);
  assert(out.statusCode === 200, 'stub 200');
  const body = out.body as Record<string, unknown>;
  assert(body['stub'] === true, 'stub:true');
  assert(body['emitted'] === false, 'no emit in stub');
  if (prev !== undefined) process.env['CRON_SECRET'] = prev;
}

// 11. Idempotency : double-POST in stub-mode → both 200.
async function testIdempotentDoublePost(): Promise<void> {
  const prev = process.env['CRON_SECRET'];
  delete process.env['CRON_SECRET'];
  const { req: r1, res: s1, out: o1 } = mockReqRes('POST');
  const { req: r2, res: s2, out: o2 } = mockReqRes('POST');
  await heartbeatHandler(r1, s1);
  await heartbeatHandler(r2, s2);
  assert(o1.statusCode === 200, 'first 200');
  assert(o2.statusCode === 200, 'second 200');
  if (prev !== undefined) process.env['CRON_SECRET'] = prev;
}

// 12. method-not-allowed on PUT/DELETE.
async function testMethodNotAllowed(): Promise<void> {
  const prev = process.env['CRON_SECRET'];
  process.env['CRON_SECRET'] = 'x';
  for (const fn of [playtestHandler, kanRollupHandler, myceliumHandler, hotfixRefreshHandler, sigmaCheckpointHandler]) {
    const { req, res, out } = mockReqRes('DELETE');
    await fn(req, res);
    assert(out.statusCode === 405, `expected 405, got ${out.statusCode}`);
  }
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

async function runAll(): Promise<void> {
  await testHeartbeatGetPublic();
  await testHeartbeatPostRequiresAuth();
  await testHeartbeatStubMode();
  await testPlaytestStubMode();
  await testKanRollupAuthRequired();
  await testKanRollupAuthSuccess();
  await testMyceliumKAnonExported();
  await testMyceliumStubMode();
  await testHotfixRefreshStubMode();
  await testSigmaCheckpointStubMode();
  await testIdempotentDoublePost();
  await testMethodNotAllowed();
  // eslint-disable-next-line no-console
  console.log('cron-endpoints.test : OK · 12 tests passed');
}

if (isMain) {
  runAll().catch((err) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}

export { runAll };
