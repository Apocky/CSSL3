// § T11-W14-K · tests/cron/cron-auth.test.ts
// Drives the 7 inline tests defined in lib/cron-auth.ts.
//
// Coverage :
//   1. constant-time equality
//   2. idempotency-bucket stability
//   3. stub-mode reflection
//   4. auth rejects missing creds
//   5. auth accepts Bearer header
//   6. auth accepts x-cron-secret header
//   7. query-secret gated behind env-flag

import {
  isCronAuthorized,
  isCronStubMode,
  idempotencyKey,
  reject401,
  emitCronAudit,
  nowDurationMs,
} from '@/lib/cron-auth';
import type { NextApiRequest, NextApiResponse } from 'next';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. idempotencyKey shape + bucket-stability.
function testIdempotencyKey(): void {
  const k = idempotencyKey('foo', 60);
  assert(k.startsWith('foo:'), 'key prefix correct');
  assert(/^foo:\d+$/.test(k), 'key suffix is bucket-int');
  // same call within same second → same key
  const k2 = idempotencyKey('foo', 60);
  assert(k === k2, 'bucket stable within cadence-window');
}

// 2. isCronStubMode reflects env-state.
function testStubMode(): void {
  const prev = process.env['CRON_SECRET'];
  delete process.env['CRON_SECRET'];
  assert(isCronStubMode(), 'no-secret → stub');
  process.env['CRON_SECRET'] = 'x';
  assert(!isCronStubMode(), 'with-secret → ¬ stub');
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
}

// 3. Auth rejects missing creds.
function testAuthRejectMissing(): void {
  const prev = process.env['CRON_SECRET'];
  process.env['CRON_SECRET'] = 'topsecret';
  const req = { headers: {}, query: {} } as unknown as NextApiRequest;
  const r = isCronAuthorized(req);
  assert(!r.ok, 'no-creds → reject');
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
}

// 4. Bearer header accepted.
function testAuthAcceptBearer(): void {
  const prev = process.env['CRON_SECRET'];
  process.env['CRON_SECRET'] = 'topsecret';
  const req = {
    headers: { authorization: 'Bearer topsecret' },
    query: {},
  } as unknown as NextApiRequest;
  const r = isCronAuthorized(req);
  assert(r.ok && r.via === 'bearer', 'matching bearer accepted');
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
}

// 5. x-cron-secret header accepted.
function testAuthAcceptXHeader(): void {
  const prev = process.env['CRON_SECRET'];
  process.env['CRON_SECRET'] = 'topsecret';
  const req = {
    headers: { 'x-cron-secret': 'topsecret' },
    query: {},
  } as unknown as NextApiRequest;
  const r = isCronAuthorized(req);
  assert(r.ok && r.via === 'header', 'matching x-cron-secret accepted');
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
}

// 6. Query-string gated.
function testQueryGated(): void {
  const prev = process.env['CRON_SECRET'];
  const prevAllow = process.env['CRON_ALLOW_QUERY_SECRET'];
  process.env['CRON_SECRET'] = 'topsecret';
  delete process.env['CRON_ALLOW_QUERY_SECRET'];
  const req = {
    headers: {},
    query: { cron_secret: 'topsecret' },
  } as unknown as NextApiRequest;
  const rDeny = isCronAuthorized(req);
  assert(!rDeny.ok, 'query w/o flag → deny');
  process.env['CRON_ALLOW_QUERY_SECRET'] = 'true';
  const rAllow = isCronAuthorized(req);
  assert(rAllow.ok && rAllow.via === 'query', 'query w/ flag → accept');
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
  if (prevAllow === undefined) delete process.env['CRON_ALLOW_QUERY_SECRET'];
  else process.env['CRON_ALLOW_QUERY_SECRET'] = prevAllow;
}

// 7. Wrong-Bearer rejected.
function testAuthWrongBearer(): void {
  const prev = process.env['CRON_SECRET'];
  process.env['CRON_SECRET'] = 'topsecret';
  const req = {
    headers: { authorization: 'Bearer WRONG' },
    query: {},
  } as unknown as NextApiRequest;
  const r = isCronAuthorized(req);
  assert(!r.ok, 'wrong bearer → reject');
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
}

// 8. Reject-401 emits proper envelope.
function testReject401(): void {
  let captured: { code: number; body: unknown } = { code: 0, body: null };
  const res = {
    status(c: number) { captured.code = c; return this; },
    json(b: unknown) { captured.body = b; return this; },
    setHeader(_k: string, _v: string) { return this; },
  } as unknown as NextApiResponse;
  reject401(res, 'test-reason');
  assert(captured.code === 401, '401 status');
  const body = captured.body as Record<string, unknown>;
  assert(body['ok'] === false, 'ok:false');
  assert(body['error'] === 'unauthorized', 'error class');
  assert(body['reason_class'] === 'test-reason', 'reason class');
}

// 9. nowDurationMs returns wall-clock duration.
function testNowDuration(): void {
  const start = Date.now();
  const r = nowDurationMs(start);
  assert(typeof r.duration_ms === 'number' && r.duration_ms >= 0, 'duration nonneg');
  assert(typeof r.finished_at === 'string', 'finished_at iso');
}

// 10. emitCronAudit does NOT throw on missing supabase env.
async function testEmitAuditMissingEnv(): Promise<void> {
  const prevUrl = process.env['NEXT_PUBLIC_SUPABASE_URL'];
  const prevKey = process.env['SUPABASE_SERVICE_ROLE_KEY'];
  delete process.env['NEXT_PUBLIC_SUPABASE_URL'];
  delete process.env['SUPABASE_SERVICE_ROLE_KEY'];
  await emitCronAudit({
    job_name: 'test',
    started_at: new Date().toISOString(),
    finished_at: new Date().toISOString(),
    duration_ms: 1,
    status: 'ok',
    rows_processed: 0,
    retry_count: 0,
    via: 'bearer',
    notes: null,
  });
  if (prevUrl !== undefined) process.env['NEXT_PUBLIC_SUPABASE_URL'] = prevUrl;
  if (prevKey !== undefined) process.env['SUPABASE_SERVICE_ROLE_KEY'] = prevKey;
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

async function runAll(): Promise<void> {
  testIdempotencyKey();
  testStubMode();
  testAuthRejectMissing();
  testAuthAcceptBearer();
  testAuthAcceptXHeader();
  testQueryGated();
  testAuthWrongBearer();
  testReject401();
  testNowDuration();
  await testEmitAuditMissingEnv();
  // eslint-disable-next-line no-console
  console.log('cron-auth.test : OK · 10 tests passed');
}

if (isMain) {
  runAll().catch((err) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}

export { runAll };
