// cssl-edge · /api/run-share/feed
// GET friend-list run-share-feed for player. Cap-gated RUN_SHARE_CAP_RECEIVE · 0x80.
// Sovereign-bypass supported via x-loa-sovereign-cap header.
//
// Gift-economy framing : opt-in friend run-replays · NO leaderboards · NO scoring
// against creator (per ROGUELIKE_LOOP § RUN-SHARING + MULTIPLAYER_MATRIX axiom).
//
// Query :
//   - GET ?player_id=&cap=&limit=&friend_list=<comma-sep-ids>
//   - 200 : envelope({ feed: RunShareReceipt[], total })
//   - 403 : cap-deny
//   - 405 : non-GET method

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { RUN_SHARE_CAP_RECEIVE } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { envelope, logHit } from '@/lib/response';

export interface RunShareFeedItem {
  receipt_id: string;
  player_id: string;
  seed: string;
  scoring: { runtime_s: number; depth: number; completed: boolean };
  screenshot_handle: string;
  note: string;
  posted_at: string;
  echoes_received: number;
}

interface FeedOk {
  served_by: string;
  ts: string;
  feed: RunShareFeedItem[];
  total: number;
  player_id: string;
  framing: 'gift-economy';
}

interface FeedError {
  error: string;
  served_by: string;
  ts: string;
}

const DEFAULT_LIMIT = 20;
const MAX_LIMIT = 100;

// 10-row stub feed · used until cssl-supabase migration 0017 lands.
const STUB_FEED: ReadonlyArray<RunShareFeedItem> = [
  { receipt_id: 'rs-001', player_id: 'alice', seed: 'seed-r1', scoring: { runtime_s: 423, depth: 7, completed: true }, screenshot_handle: 'cdn:thumb-001', note: 'survived the glassroot', posted_at: '2026-04-30T10:00:00.000Z', echoes_received: 4 },
  { receipt_id: 'rs-002', player_id: 'bob', seed: 'seed-r2', scoring: { runtime_s: 612, depth: 11, completed: true }, screenshot_handle: 'cdn:thumb-002', note: 'tough boss', posted_at: '2026-04-30T10:30:00.000Z', echoes_received: 7 },
  { receipt_id: 'rs-003', player_id: 'carol', seed: 'seed-r3', scoring: { runtime_s: 188, depth: 4, completed: false }, screenshot_handle: 'cdn:thumb-003', note: 'died to the tide-wraith', posted_at: '2026-04-30T11:00:00.000Z', echoes_received: 1 },
  { receipt_id: 'rs-004', player_id: 'dave', seed: 'seed-r4', scoring: { runtime_s: 945, depth: 17, completed: true }, screenshot_handle: 'cdn:thumb-004', note: 'first clear', posted_at: '2026-04-30T11:30:00.000Z', echoes_received: 12 },
  { receipt_id: 'rs-005', player_id: 'eve', seed: 'seed-r5', scoring: { runtime_s: 322, depth: 5, completed: true }, screenshot_handle: 'cdn:thumb-005', note: 'speed run · low depth', posted_at: '2026-04-30T12:00:00.000Z', echoes_received: 2 },
  { receipt_id: 'rs-006', player_id: 'frank', seed: 'seed-r6', scoring: { runtime_s: 1102, depth: 21, completed: true }, screenshot_handle: 'cdn:thumb-006', note: 'long haul', posted_at: '2026-04-30T12:30:00.000Z', echoes_received: 9 },
  { receipt_id: 'rs-007', player_id: 'grace', seed: 'seed-r7', scoring: { runtime_s: 230, depth: 3, completed: false }, screenshot_handle: 'cdn:thumb-007', note: 'unfortunate brushwood', posted_at: '2026-04-30T13:00:00.000Z', echoes_received: 0 },
  { receipt_id: 'rs-008', player_id: 'heidi', seed: 'seed-r8', scoring: { runtime_s: 781, depth: 13, completed: true }, screenshot_handle: 'cdn:thumb-008', note: 'tide-room cleared', posted_at: '2026-04-30T13:30:00.000Z', echoes_received: 6 },
  { receipt_id: 'rs-009', player_id: 'ivan', seed: 'seed-r9', scoring: { runtime_s: 412, depth: 8, completed: true }, screenshot_handle: 'cdn:thumb-009', note: 'glade-only build', posted_at: '2026-04-30T14:00:00.000Z', echoes_received: 3 },
  { receipt_id: 'rs-010', player_id: 'judy', seed: 'seed-r10', scoring: { runtime_s: 600, depth: 10, completed: true }, screenshot_handle: 'cdn:thumb-010', note: 'tide + ember combo', posted_at: '2026-04-30T14:30:00.000Z', echoes_received: 5 },
];

function readQuery(
  q: Record<string, string | string[] | undefined>,
  key: string
): string | undefined {
  const v = q[key];
  if (Array.isArray(v)) return v[0];
  return v;
}

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<FeedOk | FeedError>
): void {
  logHit('run-share.feed', { method: req.method ?? 'GET' });

  if (req.method !== 'GET') {
    const env = envelope();
    res.setHeader('Allow', 'GET');
    res.status(405).json({
      error: 'Method Not Allowed — GET ?player_id=&cap=&friend_list=&limit=',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const q = req.query as Record<string, string | string[] | undefined>;
  const capRaw = readQuery(q, 'cap');
  const cap = capRaw !== undefined ? parseInt(capRaw, 10) || 0 : 0;
  const sovereignRaw = readQuery(q, 'sovereign');
  const sovereignFlag = sovereignRaw === 'true';
  const sovereignAllowed = isSovereignFromIncoming(req.headers, sovereignFlag);

  // Cap-gate.
  const capAllowed = (cap & RUN_SHARE_CAP_RECEIVE) !== 0;
  if (!capAllowed && !sovereignAllowed) {
    const d = deny('cap RUN_SHARE_CAP_RECEIVE=0x80 required (or sovereign-header)', cap);
    logEvent(d.body);
    const env = envelope();
    res.status(d.status).json({
      error: d.body.extra?.['reason'] as string ?? 'denied',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const player_id = readQuery(q, 'player_id') ?? 'anonymous';
  const limitRaw = readQuery(q, 'limit');
  const limitParsed = limitRaw !== undefined ? parseInt(limitRaw, 10) : DEFAULT_LIMIT;
  const limit = Math.max(
    1,
    Math.min(Number.isFinite(limitParsed) ? limitParsed : DEFAULT_LIMIT, MAX_LIMIT)
  );
  const friend_list_raw = readQuery(q, 'friend_list') ?? '';
  const friends = friend_list_raw
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s.length > 0);

  // Filter to friend-list ; if empty, surface all 10 stub rows (default behavior).
  let filtered: RunShareFeedItem[] = STUB_FEED.slice();
  if (friends.length > 0) {
    const friendSet = new Set(friends);
    filtered = filtered.filter((r) => friendSet.has(r.player_id));
  }
  const total = filtered.length;
  const slice = filtered.slice(0, limit);

  logEvent(
    auditEvent('run-share.feed', cap, sovereignAllowed, 'ok', {
      player_id,
      friend_count: friends.length,
      returned: slice.length,
    })
  );

  const env = envelope();
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    feed: slice,
    total,
    player_id,
    framing: 'gift-economy',
  });
}

// ─── Inline tests · framework-agnostic ─────────────────────────────────────

interface MockedResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function mockReqRes(
  method: string,
  query: Record<string, string | string[]> = {}
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };
  const req = { method, query, headers: {}, body: undefined } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(payload: unknown) { out.body = payload; return this; },
    setHeader(key: string, val: string) { out.headers[key] = val; return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. cap=0 → 403.
export function testCapsZeroDenies(): void {
  const { req, res, out } = mockReqRes('GET', { player_id: 'alice', cap: '0' });
  handler(req, res);
  assert(out.statusCode === 403, `cap=0 must yield 403, got ${out.statusCode}`);
}

// 2. cap-bit set → 200 with feed + friend filter + gift-economy framing.
export function testCapsSetReturnsFilteredFeed(): void {
  const { req, res, out } = mockReqRes('GET', {
    player_id: 'self',
    cap: String(RUN_SHARE_CAP_RECEIVE),
    friend_list: 'alice,bob,carol',
  });
  handler(req, res);
  assert(out.statusCode === 200, `cap-set must yield 200, got ${out.statusCode}`);
  const b = out.body as FeedOk;
  assert(Array.isArray(b.feed), 'feed must be array');
  assert(b.feed.length === 3, `expected 3 friend rows, got ${b.feed.length}`);
  assert(b.framing === 'gift-economy', 'framing must be gift-economy');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  testCapsZeroDenies();
  testCapsSetReturnsFilteredFeed();
  // eslint-disable-next-line no-console
  console.log('run-share/feed.ts : OK · 2 inline tests passed');
}
