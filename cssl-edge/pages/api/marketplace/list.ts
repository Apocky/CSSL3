// cssl-edge · /api/marketplace/list
// GET listing of available gear-shares. Cap-gated MARKETPLACE_CAP_LIST · 0x10.
// Sovereign-bypass supported via x-loa-sovereign-cap header.
//
// Gift-economy framing : these are SHARE-RECEIPTS · not commerce listings.
// No prices · no leaderboards · no PvP scoring (per MULTIPLAYER_MATRIX axiom).
//
// Query :
//   - GET ?cap=<number>&rarity=<string>&slot=<string>&page=<int>&page_size=<int>
//   - 200 : envelope({ listings: GearShareReceipt[], total, page, page_size })
//   - 403 : cap-deny
//   - 405 : non-GET method
//
// NOTE : tests run framework-agnostic via `npx tsx`. See bottom of file.

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { MARKETPLACE_CAP_LIST } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { envelope, logHit } from '@/lib/response';

// One row of the listing — the seed + metadata, NOT the actual gear bytes.
// Receiver re-rolls from the seed. Gift-economy : creator gets echo-back bonus
// when a friend completes their seed (per ROGUELIKE_LOOP § RUN-SHARING).
export interface GearShareReceipt {
  receipt_id: string;
  creator_player_id: string;
  rarity: string;
  slot: string;
  seed: string;
  posted_at: string;
  echoes_received: number;
  note: string;
}

interface ListOk {
  served_by: string;
  ts: string;
  listings: GearShareReceipt[];
  total: number;
  page: number;
  page_size: number;
  filter: { rarity: string; slot: string };
  framing: 'gift-economy';
}

interface ListError {
  error: string;
  served_by: string;
  ts: string;
}

const DEFAULT_PAGE_SIZE = 20;
const MAX_PAGE_SIZE = 100;

// Deterministic stub catalog · used until cssl-supabase migration 0016 lands.
// 12 rows spanning rarity tiers + slot kinds so the gallery renders meaningfully.
const STUB_RECEIPTS: ReadonlyArray<GearShareReceipt> = [
  { receipt_id: 'r-001', creator_player_id: 'alice', rarity: 'common', slot: 'weapon', seed: 'seed-aaa-001', posted_at: '2026-04-30T10:00:00.000Z', echoes_received: 3, note: 'Forged in glade · roll a tier-2 wand' },
  { receipt_id: 'r-002', creator_player_id: 'bob', rarity: 'uncommon', slot: 'armor', seed: 'seed-bbb-002', posted_at: '2026-04-30T10:15:00.000Z', echoes_received: 1, note: 'Mossroot leather · slow-bleed res' },
  { receipt_id: 'r-003', creator_player_id: 'carol', rarity: 'rare', slot: 'amulet', seed: 'seed-ccc-003', posted_at: '2026-04-30T10:30:00.000Z', echoes_received: 7, note: 'Whisper-of-tide · evade tick' },
  { receipt_id: 'r-004', creator_player_id: 'dave', rarity: 'epic', slot: 'weapon', seed: 'seed-ddd-004', posted_at: '2026-04-30T10:45:00.000Z', echoes_received: 12, note: 'Lattice blade · phase damage' },
  { receipt_id: 'r-005', creator_player_id: 'eve', rarity: 'legendary', slot: 'ring', seed: 'seed-eee-005', posted_at: '2026-04-30T11:00:00.000Z', echoes_received: 23, note: 'Ember-of-still-time · rare drop' },
  { receipt_id: 'r-006', creator_player_id: 'frank', rarity: 'common', slot: 'helm', seed: 'seed-fff-006', posted_at: '2026-04-30T11:15:00.000Z', echoes_received: 0, note: 'Nightcap of guard' },
  { receipt_id: 'r-007', creator_player_id: 'grace', rarity: 'uncommon', slot: 'boots', seed: 'seed-ggg-007', posted_at: '2026-04-30T11:30:00.000Z', echoes_received: 4, note: 'Quickstep cloth · +5% dash' },
  { receipt_id: 'r-008', creator_player_id: 'heidi', rarity: 'rare', slot: 'weapon', seed: 'seed-hhh-008', posted_at: '2026-04-30T11:45:00.000Z', echoes_received: 8, note: 'Glassbow · crit on still-water' },
  { receipt_id: 'r-009', creator_player_id: 'ivan', rarity: 'epic', slot: 'armor', seed: 'seed-iii-009', posted_at: '2026-04-30T12:00:00.000Z', echoes_received: 15, note: 'Glacier mantle · cold-shroud' },
  { receipt_id: 'r-010', creator_player_id: 'judy', rarity: 'legendary', slot: 'amulet', seed: 'seed-jjj-010', posted_at: '2026-04-30T12:15:00.000Z', echoes_received: 31, note: 'Loom-of-Echoes · resonant' },
  { receipt_id: 'r-011', creator_player_id: 'mallory', rarity: 'rare', slot: 'ring', seed: 'seed-kkk-011', posted_at: '2026-04-30T12:30:00.000Z', echoes_received: 6, note: 'Tidewatcher band · brine + lift' },
  { receipt_id: 'r-012', creator_player_id: 'oscar', rarity: 'uncommon', slot: 'helm', seed: 'seed-lll-012', posted_at: '2026-04-30T12:45:00.000Z', echoes_received: 2, note: 'Foxhood · stealth-on-still' },
];

function readQuery(
  q: Record<string, string | string[] | undefined>,
  key: string
): string | undefined {
  const v = q[key];
  if (Array.isArray(v)) return v[0];
  return v;
}

function isObjectQuery(req: NextApiRequest): Record<string, string | string[] | undefined> {
  return req.query as Record<string, string | string[] | undefined>;
}

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<ListOk | ListError>
): void {
  logHit('marketplace.list', { method: req.method ?? 'GET' });

  if (req.method !== 'GET') {
    const env = envelope();
    res.setHeader('Allow', 'GET');
    res.status(405).json({
      error: 'Method Not Allowed — GET ?cap=&rarity=&slot=&page=&page_size=',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const q = isObjectQuery(req);
  const capRaw = readQuery(q, 'cap');
  const cap = capRaw !== undefined ? parseInt(capRaw, 10) || 0 : 0;
  const sovereignRaw = readQuery(q, 'sovereign');
  const sovereignFlag = sovereignRaw === 'true';
  const sovereignAllowed = isSovereignFromIncoming(req.headers, sovereignFlag);

  // DEFAULT-DENY : cap-bit absent + no sovereign-header → 403.
  const capAllowed = (cap & MARKETPLACE_CAP_LIST) !== 0;
  if (!capAllowed && !sovereignAllowed) {
    const d = deny('cap MARKETPLACE_CAP_LIST=0x10 required (or sovereign-header)', cap);
    logEvent(d.body);
    const env = envelope();
    res.status(d.status).json({
      error: d.body.extra?.['reason'] as string ?? 'denied',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const rarity = (readQuery(q, 'rarity') ?? '').toLowerCase();
  const slot = (readQuery(q, 'slot') ?? '').toLowerCase();
  const pageStr = readQuery(q, 'page') ?? '1';
  const pageSizeStr = readQuery(q, 'page_size') ?? String(DEFAULT_PAGE_SIZE);
  const page = Math.max(1, parseInt(pageStr, 10) || 1);
  const pageSizeParsed = parseInt(pageSizeStr, 10) || DEFAULT_PAGE_SIZE;
  const page_size = Math.max(1, Math.min(pageSizeParsed, MAX_PAGE_SIZE));

  // Filter the catalog · then paginate.
  let filtered: GearShareReceipt[] = STUB_RECEIPTS.slice();
  if (rarity.length > 0) {
    filtered = filtered.filter((r) => r.rarity.toLowerCase() === rarity);
  }
  if (slot.length > 0) {
    filtered = filtered.filter((r) => r.slot.toLowerCase() === slot);
  }
  const total = filtered.length;
  const start = (page - 1) * page_size;
  const slice = filtered.slice(start, start + page_size);

  logEvent(
    auditEvent('marketplace.list', cap, sovereignAllowed, 'ok', {
      rarity,
      slot,
      page,
      page_size,
      returned: slice.length,
    })
  );

  const env = envelope();
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    listings: slice,
    total,
    page,
    page_size,
    filter: { rarity, slot },
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
  query: Record<string, string | string[]> = {},
  headers: Record<string, string> = {}
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };
  const req = { method, query, headers, body: undefined } as unknown as NextApiRequest;
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

// 1. cap-bit absent → 403 deny.
export function testCapsZeroDenies(): void {
  const { req, res, out } = mockReqRes('GET', { cap: '0' });
  handler(req, res);
  assert(out.statusCode === 403, `cap=0 must yield 403, got ${out.statusCode}`);
}

// 2. cap-bit set → 200 with gift-economy framing + paginated listings.
export function testCapsSetReturnsListings(): void {
  const { req, res, out } = mockReqRes('GET', {
    cap: String(MARKETPLACE_CAP_LIST),
    page: '1',
    page_size: '5',
  });
  handler(req, res);
  assert(out.statusCode === 200, `cap-set must yield 200, got ${out.statusCode}`);
  const body = out.body as ListOk;
  assert(Array.isArray(body.listings), 'listings must be array');
  assert(body.listings.length === 5, `expected page_size=5, got ${body.listings.length}`);
  assert(body.framing === 'gift-economy', `framing must be gift-economy, got ${body.framing}`);
  assert(body.total === 12, `total must be 12 (catalog size), got ${body.total}`);
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  testCapsZeroDenies();
  testCapsSetReturnsListings();
  // eslint-disable-next-line no-console
  console.log('marketplace/list.ts : OK · 2 inline tests passed');
}
