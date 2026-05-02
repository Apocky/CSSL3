// § T11-W14-MYCELIUM-HEARTBEAT · tests/mycelium/heartbeat-endpoints.test.ts
// Smoke-tests for /api/mycelium/heartbeat + /api/mycelium/digest. Verifies :
//   - POST /heartbeat rejects non-POST methods (405)
//   - POST /heartbeat short-circuits in stub-mode (no CRON_SECRET set)
//   - POST /heartbeat rejects missing cron-secret (401) when secret set
//   - POST /heartbeat rejects bad-protocol-version (400)
//   - POST /heartbeat rejects empty patterns (400)
//   - POST /heartbeat rejects oversized bundle (>256 patterns) (400)
//   - GET  /digest returns rows + cursor_next + k_anon_floor
//   - GET  /digest reject-non-GET (405)

import heartbeatHandler from '@/pages/api/mycelium/heartbeat';
import digestHandler from '@/pages/api/mycelium/digest';
import type { NextApiRequest, NextApiResponse } from 'next';

interface MockedResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function mockReqRes(
  method: string,
  body?: unknown,
  headers: Record<string, string> = {},
  query: Record<string, string> = {},
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };
  const req = {
    method,
    query,
    headers,
    body,
  } as unknown as NextApiRequest;
  const res = {
    status(c: number) {
      out.statusCode = c;
      return this;
    },
    json(b: unknown) {
      out.body = b;
      return this;
    },
    setHeader(k: string, v: string) {
      out.headers[k] = v;
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

function withSecret<T>(secret: string | null, fn: () => Promise<T>): Promise<T> {
  const prev = process.env['CRON_SECRET'];
  if (secret === null) {
    delete process.env['CRON_SECRET'];
  } else {
    process.env['CRON_SECRET'] = secret;
  }
  return fn().finally(() => {
    if (prev === undefined) delete process.env['CRON_SECRET'];
    else process.env['CRON_SECRET'] = prev;
  });
}

function mkPatternRaw(seed: number): number[] {
  const raw = new Array<number>(32).fill(0);
  raw[0] = 1; // kind = CellState
  raw[1] = 0x0F; // cap_flags = ALL
  raw[2] = 12; // cohort_size
  raw[3] = 128; // confidence_q8
  for (let i = 0; i < 4; i++) raw[4 + i] = (1000 >> (i * 8)) & 0xff;
  for (let i = 0; i < 8; i++) raw[8 + i] = (seed * 7 + i) & 0xff;
  for (let i = 0; i < 8; i++) raw[16 + i] = (seed * 13 + i) & 0xff;
  for (let i = 0; i < 8; i++) raw[24 + i] = (seed * 19 + i) & 0xff;
  return raw;
}

function mkBundle(numPatterns: number): unknown {
  return {
    protocol_version: 1,
    tick_id: 1,
    emitter_handle: 0,
    ts_bucketed: 1000,
    patterns: Array.from({ length: numPatterns }, (_, i) => ({
      raw: mkPatternRaw(i + 1),
    })),
    bundle_blake3: '0'.repeat(64),
  };
}

// 1. POST /heartbeat rejects non-POST methods.
async function testHeartbeatRejectsNonPost(): Promise<void> {
  await withSecret('present', async () => {
    const { req, res, out } = mockReqRes('GET');
    await heartbeatHandler(req, res);
    assert(out.statusCode === 405, `expected 405, got ${out.statusCode}`);
  });
}

// 2. POST /heartbeat short-circuits in stub-mode.
async function testHeartbeatStubModeShortCircuits(): Promise<void> {
  await withSecret(null, async () => {
    const { req, res, out } = mockReqRes('POST', mkBundle(1));
    await heartbeatHandler(req, res);
    assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
    const body = out.body as Record<string, unknown>;
    assert(body['stub'] === true, 'stub envelope');
  });
}

// 3. POST /heartbeat rejects missing cron-secret.
async function testHeartbeatRejectsMissingCronSecret(): Promise<void> {
  await withSecret('topsecret', async () => {
    const { req, res, out } = mockReqRes('POST', mkBundle(1));
    await heartbeatHandler(req, res);
    assert(out.statusCode === 401, `expected 401, got ${out.statusCode}`);
  });
}

// 4. POST /heartbeat with valid auth + bad protocol version → 400.
async function testHeartbeatRejectsBadProtocol(): Promise<void> {
  await withSecret('topsecret', async () => {
    const bundle = mkBundle(1) as Record<string, unknown>;
    bundle['protocol_version'] = 999;
    const { req, res, out } = mockReqRes('POST', bundle, {
      authorization: 'Bearer topsecret',
    });
    await heartbeatHandler(req, res);
    assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
  });
}

// 5. POST /heartbeat empty patterns → 400.
async function testHeartbeatRejectsEmptyPatterns(): Promise<void> {
  await withSecret('topsecret', async () => {
    const bundle = mkBundle(0);
    const { req, res, out } = mockReqRes('POST', bundle, {
      authorization: 'Bearer topsecret',
    });
    await heartbeatHandler(req, res);
    assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
  });
}

// 6. POST /heartbeat oversized bundle → 400.
async function testHeartbeatRejectsOversize(): Promise<void> {
  await withSecret('topsecret', async () => {
    const bundle = mkBundle(257);
    const { req, res, out } = mockReqRes('POST', bundle, {
      authorization: 'Bearer topsecret',
    });
    await heartbeatHandler(req, res);
    assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
  });
}

// 7. POST /heartbeat → cap-denied counts up but request still 200 (stub).
async function testHeartbeatCapDeniedDropsButReturns200(): Promise<void> {
  await withSecret('topsecret', async () => {
    const bundle = mkBundle(1) as { patterns: { raw: number[] }[] };
    bundle.patterns[0]!.raw[1] = 0x00; // strip cap_flags → ingest denied
    const { req, res, out } = mockReqRes('POST', bundle, {
      authorization: 'Bearer topsecret',
    });
    await heartbeatHandler(req, res);
    // Note : in stub-mode (no Supabase env), this returns 200.
    assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
    const body = out.body as Record<string, unknown>;
    assert(body['dropped_cap'] === 1, `expected dropped_cap=1, got ${body['dropped_cap']}`);
  });
}

// 8. GET /digest returns rows array + cursor_next.
async function testDigestReturnsRows(): Promise<void> {
  const { req, res, out } = mockReqRes('GET', undefined, {}, { since: '0' });
  await digestHandler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const body = out.body as Record<string, unknown>;
  assert(Array.isArray(body['rows']), 'rows array');
  assert(typeof body['cursor_next'] === 'number', 'cursor_next number');
  assert(body['k_anon_floor'] === 10, 'k_anon_floor=10');
}

// 9. GET /digest rejects non-GET.
async function testDigestRejectsNonGet(): Promise<void> {
  const { req, res, out } = mockReqRes('POST');
  await digestHandler(req, res);
  assert(out.statusCode === 405, `expected 405, got ${out.statusCode}`);
}

// 10. GET /digest accepts auth-headers without rejecting.
async function testDigestAcceptsAuthHeader(): Promise<void> {
  await withSecret('topsecret', async () => {
    const { req, res, out } = mockReqRes(
      'GET',
      undefined,
      { authorization: 'Bearer topsecret' },
      { since: '0' },
    );
    await digestHandler(req, res);
    assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
    const body = out.body as Record<string, unknown>;
    assert(body['authed'] === true, 'authed:true');
  });
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;

const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  (async () => {
    await testHeartbeatRejectsNonPost();
    await testHeartbeatStubModeShortCircuits();
    await testHeartbeatRejectsMissingCronSecret();
    await testHeartbeatRejectsBadProtocol();
    await testHeartbeatRejectsEmptyPatterns();
    await testHeartbeatRejectsOversize();
    await testHeartbeatCapDeniedDropsButReturns200();
    await testDigestReturnsRows();
    await testDigestRejectsNonGet();
    await testDigestAcceptsAuthHeader();
    // eslint-disable-next-line no-console
    console.log('heartbeat-endpoints.test.ts : OK · 10 tests passed');
  })().catch((e) => {
    // eslint-disable-next-line no-console
    console.error(e);
    process.exit(1);
  });
}
