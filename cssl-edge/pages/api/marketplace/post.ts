// cssl-edge · /api/marketplace/post
// POST a gear-share-receipt. Cap-gated MARKETPLACE_CAP_POST · 0x20.
// Sovereign-bypass supported via x-loa-sovereign-cap header.
//
// CRITICAL : posts the SEED + metadata · not the gear itself. Receiver re-rolls.
// Gift-economy framing : creator gets echo-back bonus when friend completes
// their seed (per ROGUELIKE_LOOP § RUN-SHARING). NO commerce · NO PvP scoring.
//
// Body :
//   {
//     creator_player_id: string · required
//     rarity: string · required (common|uncommon|rare|epic|legendary)
//     slot: string · required (weapon|armor|helm|boots|amulet|ring)
//     seed: string · required (re-roll seed string)
//     note?: string · optional creator-supplied gift note
//     cap: number · cap-bit mask
//     sovereign?: boolean · sovereign-bypass flag
//   }
// Response : 200 envelope({ receipt: GearShareReceipt, accepted: true })

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { MARKETPLACE_CAP_POST } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { envelope, logHit } from '@/lib/response';

interface PostRequest {
  creator_player_id?: unknown;
  rarity?: unknown;
  slot?: unknown;
  seed?: unknown;
  note?: unknown;
  cap?: unknown;
  sovereign?: unknown;
}

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

interface PostOk {
  served_by: string;
  ts: string;
  receipt: GearShareReceipt;
  accepted: true;
  framing: 'gift-economy';
}

interface PostError {
  error: string;
  served_by: string;
  ts: string;
}

const ALLOWED_RARITY: ReadonlySet<string> = new Set([
  'common', 'uncommon', 'rare', 'epic', 'legendary',
]);
const ALLOWED_SLOT: ReadonlySet<string> = new Set([
  'weapon', 'armor', 'helm', 'boots', 'amulet', 'ring',
]);

function isObject(b: unknown): b is Record<string, unknown> {
  return typeof b === 'object' && b !== null;
}

function genReceiptId(): string {
  // 12 hex chars · sufficient for stub-distinct tracing.
  const r = Math.floor(Math.random() * 0xffffffffffff);
  return `r-${r.toString(16).padStart(12, '0')}`;
}

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<PostOk | PostError>
): void {
  logHit('marketplace.post', { method: req.method ?? 'GET' });

  if (req.method !== 'POST') {
    const env = envelope();
    res.setHeader('Allow', 'POST');
    res.status(405).json({
      error: 'Method Not Allowed — POST {creator_player_id, rarity, slot, seed, cap, sovereign?, note?}',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const body: unknown = req.body;
  if (!isObject(body)) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — body must be JSON object',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const reqBody = body as PostRequest;
  const cap = typeof reqBody.cap === 'number' ? reqBody.cap : 0;
  const sovereignFlag = reqBody.sovereign === true;
  const sovereignAllowed = isSovereignFromIncoming(req.headers, sovereignFlag);

  // Cap-gate : DEFAULT-DENY.
  const capAllowed = (cap & MARKETPLACE_CAP_POST) !== 0;
  if (!capAllowed && !sovereignAllowed) {
    const d = deny('cap MARKETPLACE_CAP_POST=0x20 required (or sovereign-header)', cap);
    logEvent(d.body);
    const env = envelope();
    res.status(d.status).json({
      error: d.body.extra?.['reason'] as string ?? 'denied',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  // Validate required fields.
  const creator_player_id = typeof reqBody.creator_player_id === 'string' ? reqBody.creator_player_id : '';
  const rarity = (typeof reqBody.rarity === 'string' ? reqBody.rarity : '').toLowerCase();
  const slot = (typeof reqBody.slot === 'string' ? reqBody.slot : '').toLowerCase();
  const seed = typeof reqBody.seed === 'string' ? reqBody.seed : '';
  const note = typeof reqBody.note === 'string' ? reqBody.note : '';

  if (creator_player_id.length === 0 || seed.length === 0) {
    logEvent(
      auditEvent('marketplace.post', cap, sovereignAllowed, 'denied', {
        reason: 'missing creator_player_id or seed',
      })
    );
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — creator_player_id and seed are required strings',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  if (!ALLOWED_RARITY.has(rarity)) {
    logEvent(
      auditEvent('marketplace.post', cap, sovereignAllowed, 'denied', {
        reason: `rarity must be one of ${Array.from(ALLOWED_RARITY).join(',')}`,
      })
    );
    const env = envelope();
    res.status(400).json({
      error: `Bad Request — rarity must be one of ${Array.from(ALLOWED_RARITY).join(',')}`,
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  if (!ALLOWED_SLOT.has(slot)) {
    logEvent(
      auditEvent('marketplace.post', cap, sovereignAllowed, 'denied', {
        reason: `slot must be one of ${Array.from(ALLOWED_SLOT).join(',')}`,
      })
    );
    const env = envelope();
    res.status(400).json({
      error: `Bad Request — slot must be one of ${Array.from(ALLOWED_SLOT).join(',')}`,
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const receipt: GearShareReceipt = {
    receipt_id: genReceiptId(),
    creator_player_id,
    rarity,
    slot,
    seed,
    posted_at: new Date().toISOString(),
    echoes_received: 0,
    note,
  };

  logEvent(
    auditEvent('marketplace.post', cap, sovereignAllowed, 'ok', {
      receipt_id: receipt.receipt_id,
      rarity,
      slot,
    })
  );

  const env = envelope();
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    receipt,
    accepted: true,
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
  body?: unknown,
  headers: Record<string, string> = {}
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };
  const req = { method, query: {}, headers, body } as unknown as NextApiRequest;
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

const SAMPLE_BODY = {
  creator_player_id: 'alice',
  rarity: 'rare',
  slot: 'weapon',
  seed: 'seed-test-001',
  note: 'forged at dawn',
  cap: MARKETPLACE_CAP_POST,
};

// 1. cap=0 → 403.
export function testCapsZeroDenies(): void {
  const { req, res, out } = mockReqRes('POST', { ...SAMPLE_BODY, cap: 0 });
  handler(req, res);
  assert(out.statusCode === 403, `cap=0 must yield 403, got ${out.statusCode}`);
}

// 2. cap-bit set + valid body → 200 with receipt + gift-economy framing.
export function testCapsSetCreatesReceipt(): void {
  const { req, res, out } = mockReqRes('POST', SAMPLE_BODY);
  handler(req, res);
  assert(out.statusCode === 200, `cap-set must yield 200, got ${out.statusCode}`);
  const b = out.body as PostOk;
  assert(b.accepted === true, 'accepted must be true');
  assert(b.framing === 'gift-economy', 'framing must be gift-economy');
  assert(typeof b.receipt.receipt_id === 'string', 'receipt_id must be string');
  assert(b.receipt.echoes_received === 0, 'echoes_received starts at 0');
  assert(b.receipt.creator_player_id === 'alice', 'creator_player_id echoed');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  testCapsZeroDenies();
  testCapsSetCreatesReceipt();
  // eslint-disable-next-line no-console
  console.log('marketplace/post.ts : OK · 2 inline tests passed');
}
