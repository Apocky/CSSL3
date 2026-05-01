// cssl-edge · tests/api/signaling.test.ts
// Lightweight self-tests for /api/signaling/*. Framework-agnostic — runs via
// `npx tsx tests/api/signaling.test.ts`. Mirrors tests/api/companion.test.ts
// style ; supabase env-vars are deliberately UNSET so each route exercises
// its null-fallback (stub) branch.

import createRoomHandler from '@/pages/api/signaling/create-room';
import joinRoomHandler from '@/pages/api/signaling/join-room';
import postSignalHandler from '@/pages/api/signaling/post-signal';
import pollHandler from '@/pages/api/signaling/poll';
import { MP_CAP_HOST_ROOM, MP_CAP_JOIN_ROOM, MP_CAP_RELAY_DATA } from '@/lib/cap';
import { SOVEREIGN_CAP_HEX, SOVEREIGN_HEADER_NAME } from '@/lib/sovereign';
import { _resetSupabaseForTests } from '@/lib/supabase';
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
  query: Record<string, string | string[]> = {}
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };

  const req = {
    method,
    query,
    headers,
    body,
  } as unknown as NextApiRequest;

  const res = {
    status(code: number) {
      out.statusCode = code;
      return this;
    },
    json(payload: unknown) {
      out.body = payload;
      return this;
    },
    setHeader(key: string, val: string) {
      out.headers[key] = val;
      return this;
    },
  } as unknown as NextApiResponse;

  return { req, res, out };
}

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// Force every route into the "Supabase unconfigured → stub" branch.
function ensureSupabaseUnconfigured(): void {
  delete process.env['NEXT_PUBLIC_SUPABASE_URL'];
  delete process.env['SUPABASE_ANON_KEY'];
  _resetSupabaseForTests();
}

// 1. create-room caps=0 → 403
export async function testCreateRoomCapZeroDenied(): Promise<void> {
  ensureSupabaseUnconfigured();
  const { req, res, out } = mockReqRes('POST', {
    host_player_id: 'alice',
    cap: 0,
  });
  await createRoomHandler(req, res);
  assert(
    out.statusCode === 403,
    `create-room cap=0 must yield 403, got ${out.statusCode}`
  );
}

// 2. create-room MP_CAP_HOST_ROOM=1 → 200 stub room
export async function testCreateRoomCapSetAllows(): Promise<void> {
  ensureSupabaseUnconfigured();
  const { req, res, out } = mockReqRes('POST', {
    host_player_id: 'alice',
    cap: MP_CAP_HOST_ROOM,
  });
  await createRoomHandler(req, res);
  assert(
    out.statusCode === 200,
    `create-room cap=MP_CAP_HOST_ROOM must yield 200, got ${out.statusCode}`
  );
  const body = out.body as {
    room_id?: unknown;
    code?: unknown;
    expires_at?: unknown;
    stub?: unknown;
  };
  assert(typeof body.room_id === 'string', 'expected room_id:string');
  assert(typeof body.code === 'string', 'expected code:string');
  assert(typeof body.expires_at === 'string', 'expected expires_at:string');
  assert(body.stub === true, 'expected stub:true (env-less)');
}

// 3. join-room cap-required (caps=0 → 403)
export async function testJoinRoomCapRequired(): Promise<void> {
  ensureSupabaseUnconfigured();
  const { req, res, out } = mockReqRes('POST', {
    code: 'STUB42',
    player_id: 'bob',
    cap: 0,
  });
  await joinRoomHandler(req, res);
  assert(
    out.statusCode === 403,
    `join-room cap=0 must yield 403, got ${out.statusCode}`
  );
}

// 4. post-signal payload > 64 KiB → 400
export async function testPostSignalPayloadCapRejected(): Promise<void> {
  ensureSupabaseUnconfigured();
  // Construct payload that JSON-stringifies above 64 KiB. A 70 KiB ASCII
  // string round-trips as itself + 2 quote-chars — comfortably over the cap.
  const huge = 'x'.repeat(70 * 1024);
  const { req, res, out } = mockReqRes('POST', {
    room_id: 'room-uuid',
    from_peer: 'alice',
    to_peer: 'bob',
    kind: 'offer',
    payload: huge,
    cap: MP_CAP_RELAY_DATA,
  });
  await postSignalHandler(req, res);
  assert(
    out.statusCode === 400,
    `post-signal oversize must yield 400, got ${out.statusCode}`
  );
  const body = out.body as { error?: unknown };
  assert(
    typeof body.error === 'string' && body.error.includes('payload exceeds'),
    `expected payload-cap error, got ${JSON.stringify(body.error)}`
  );
}

// 5. poll cap-set → 200 with empty signals (env-less stub)
export async function testPollReturnsPending(): Promise<void> {
  ensureSupabaseUnconfigured();
  const { req, res, out } = mockReqRes('GET', undefined, {}, {
    room_id: 'room-uuid',
    peer_id: 'peer-uuid',
    since: '0',
    cap: String(MP_CAP_RELAY_DATA),
  });
  await pollHandler(req, res);
  assert(
    out.statusCode === 200,
    `poll cap=MP_CAP_RELAY_DATA must yield 200, got ${out.statusCode}`
  );
  const body = out.body as {
    signals?: unknown;
    next_since?: unknown;
    stub?: unknown;
  };
  assert(Array.isArray(body.signals), 'expected signals:Array');
  assert(typeof body.next_since === 'number', 'expected next_since:number');
  assert(body.stub === true, 'expected stub:true (env-less)');
}

// 6. cap-bypass with sovereign header → 200 even when caps=0
export async function testCapBypassWithSovereignHeader(): Promise<void> {
  ensureSupabaseUnconfigured();
  const { req, res, out } = mockReqRes(
    'POST',
    {
      host_player_id: 'alice',
      cap: 0,
      sovereign: true,
    },
    { [SOVEREIGN_HEADER_NAME]: SOVEREIGN_CAP_HEX }
  );
  await createRoomHandler(req, res);
  assert(
    out.statusCode === 200,
    `sovereign-bypass on create-room must yield 200, got ${out.statusCode}`
  );
}

// Bonus shape-tests — invalid kind and code-format catch-alls.
export async function testPostSignalInvalidKind(): Promise<void> {
  ensureSupabaseUnconfigured();
  const { req, res, out } = mockReqRes('POST', {
    room_id: 'room-uuid',
    from_peer: 'alice',
    to_peer: 'bob',
    kind: 'not-a-real-kind',
    payload: { hello: 'world' },
    cap: MP_CAP_RELAY_DATA,
  });
  await postSignalHandler(req, res);
  assert(
    out.statusCode === 400,
    `post-signal invalid-kind must yield 400, got ${out.statusCode}`
  );
}

export async function testJoinRoomInvalidCodeFormat(): Promise<void> {
  ensureSupabaseUnconfigured();
  const { req, res, out } = mockReqRes('POST', {
    // Lowercase + 5 chars = invalid in legibility alphabet.
    code: 'abc12',
    player_id: 'bob',
    cap: MP_CAP_JOIN_ROOM,
  });
  await joinRoomHandler(req, res);
  assert(
    out.statusCode === 400,
    `join-room invalid-code must yield 400, got ${out.statusCode}`
  );
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

async function runAll(): Promise<void> {
  await testCreateRoomCapZeroDenied();
  await testCreateRoomCapSetAllows();
  await testJoinRoomCapRequired();
  await testPostSignalPayloadCapRejected();
  await testPollReturnsPending();
  await testCapBypassWithSovereignHeader();
  await testPostSignalInvalidKind();
  await testJoinRoomInvalidCodeFormat();
  // eslint-disable-next-line no-console
  console.log('signaling.test : OK · 8 tests passed');
}

if (isMain) {
  runAll().catch((err) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}
