// cssl-edge · /api/gacha/refund
// W13-10 · Sovereign 7-day-window full-refund · automated-via-API.
//
// POST { player_pubkey · pull_id · banner_id · cosmetic_handle · rarity_at_pull
//        · pull_ts_epoch_secs · pity_after_pull_pulls_since_mythic }
//   → { ok:true · refunded:true · pity_after_refund · sigma_refund_anchor }  (200)
//   → { ok:false · error:"window expired" }   (410) when >7 days
//   → { evt:'audit', status:'denied', reason }  (403) when cap-gate fails
//
// PRIME-DIRECTIVE :
//   - 7-day-window structurally enforced
//   - sovereign-revocable : pull cancelled · cosmetic removed · pity rolled-back
//   - sigma-anchor refund-event distinct from pull-anchor (kind tag)
//
// Stage-0 stub-mode : computes the refund-window check + state-transition + anchor
// without persisting. Live mode invokes the SQL helper `record_gacha_refund` and
// triggers cosmetic-removal in the inventory crate.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope } from '@/lib/response';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { checkCap, GACHA_CAP_REFUND } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';

const REFUND_WINDOW_SECS = 7 * 24 * 60 * 60;
const PITY_THRESHOLD = 90;

interface RefundRequestBody {
  player_pubkey?: string;
  pull_id?: string;
  banner_id?: string;
  cosmetic_handle?: string;
  rarity_at_pull?: string;
  pull_ts_epoch_secs?: number;
  pity_after_pull_pulls_since_mythic?: number;
  cap?: number;
  sovereign?: boolean;
}

interface RefundSuccess {
  ok: true;
  refunded: true;
  pull_id: string;
  banner_id: string;
  refund_ts_epoch_secs: number;
  removed_cosmetic_handle: string;
  pity_after_refund_pulls_since_mythic: number;
  original_was_mythic: boolean;
  sigma_refund_anchor: string;
  refund_window_secs: number;
  served_by: string;
  ts: string;
}

interface RefundError {
  ok: false;
  error: string;
  served_by: string;
  ts: string;
}

type Resp = RefundSuccess | RefundError;

function makeAnchorId(
  kind: 'pull' | 'refund',
  pubkey: string,
  bannerId: string,
  pullId: string,
  ts: number,
  payload: string
): string {
  const all = `${kind}|${pubkey}|${bannerId}|${pullId}|${ts}|${payload}`;
  let h = 0xcbf29ce484222325n;
  const p = 0x100000001b3n;
  const mask = (1n << 128n) - 1n;
  for (let i = 0; i < all.length; i++) {
    h ^= BigInt(all.charCodeAt(i));
    h = (h * p) & mask;
  }
  return h.toString(16).padStart(32, '0').slice(0, 32);
}

export default async function handler(req: NextApiRequest, res: NextApiResponse<Resp>): Promise<void> {
  logHit('gacha.refund', { method: req.method ?? 'POST' });
  const env = envelope();

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...env });
    return;
  }

  const body = (req.body ?? {}) as RefundRequestBody;
  const pubkey = typeof body.player_pubkey === 'string' ? body.player_pubkey : '';
  const pullId = typeof body.pull_id === 'string' ? body.pull_id : '';
  const bannerId = typeof body.banner_id === 'string' ? body.banner_id : '';
  const cosmeticHandle = typeof body.cosmetic_handle === 'string' ? body.cosmetic_handle : '';
  const rarity = typeof body.rarity_at_pull === 'string' ? body.rarity_at_pull : '';
  const pullTs = typeof body.pull_ts_epoch_secs === 'number' ? body.pull_ts_epoch_secs : 0;
  const pityAfterPull = typeof body.pity_after_pull_pulls_since_mythic === 'number'
    ? body.pity_after_pull_pulls_since_mythic
    : 0;

  // ── input validation ────────────────────────────────────────────────────
  if (pubkey.length === 0) {
    res.status(400).json({ ok: false, error: 'player_pubkey required', ...env });
    return;
  }
  if (pullId.length === 0) {
    res.status(400).json({ ok: false, error: 'pull_id required', ...env });
    return;
  }
  if (bannerId.length === 0) {
    res.status(400).json({ ok: false, error: 'banner_id required', ...env });
    return;
  }
  if (!cosmeticHandle.startsWith('cosmetic:')) {
    res.status(400).json({
      ok: false,
      error: 'cosmetic_handle must start with "cosmetic:" (cosmetic-only-axiom)',
      ...env,
    });
    return;
  }
  if (pullTs <= 0) {
    res.status(400).json({ ok: false, error: 'pull_ts_epoch_secs required', ...env });
    return;
  }

  // ── cap-gate · default-DENY ────────────────────────────────────────────
  const sovereign = isSovereignFromIncoming(req.headers, body.sovereign);
  const callerCap = typeof body.cap === 'number' ? body.cap : 0;
  const decision = checkCap(callerCap, GACHA_CAP_REFUND, sovereign);
  if (!decision.ok) {
    const d = deny(decision.reason ?? 'cap denied', GACHA_CAP_REFUND);
    logEvent(d.body);
    res.status(d.status).json({
      ok: false,
      error: 'cap denied · GACHA_CAP_REFUND (0x40000000) required',
      ...env,
    });
    return;
  }

  // ── 7-day-window structural enforcement ────────────────────────────────
  const nowSecs = Math.floor(Date.now() / 1000);
  if (nowSecs < pullTs) {
    // Future-dated pull · clock-skew · reject.
    res.status(400).json({ ok: false, error: 'pull_ts_epoch_secs in the future', ...env });
    return;
  }
  const elapsed = nowSecs - pullTs;
  if (elapsed > REFUND_WINDOW_SECS) {
    logEvent(
      auditEvent('gacha.refund.window_expired', GACHA_CAP_REFUND, sovereign, 'denied', {
        pull_id: pullId,
        elapsed_secs: elapsed,
      })
    );
    res.status(410).json({
      ok: false,
      error: `refund window expired (>7 days · elapsed=${elapsed}s · window=${REFUND_WINDOW_SECS}s)`,
      ...env,
    });
    return;
  }

  // ── execute refund · state-transition ──────────────────────────────────
  const originalWasMythic = rarity === 'mythic';

  // Pity rollback :
  //   non-Mythic → pulls_since_mythic -= 1 (saturating at 0)
  //   Mythic     → pulls_since_mythic += 1 (approximation : we don't know
  //                 the pre-mythic value · documented in the spec)
  let pityAfterRefund: number;
  if (originalWasMythic) {
    pityAfterRefund = Math.min(PITY_THRESHOLD - 1, pityAfterPull + 1);
  } else {
    pityAfterRefund = Math.max(0, pityAfterPull - 1);
  }

  const refundTs = nowSecs;
  const sigmaRefundAnchor = makeAnchorId(
    'refund',
    pubkey,
    bannerId,
    pullId,
    refundTs,
    `${rarity}|${cosmeticHandle}|${pityAfterRefund}`
  );

  logEvent(
    auditEvent('gacha.refund.completed', GACHA_CAP_REFUND, sovereign, 'ok', {
      pull_id: pullId,
      banner_id: bannerId,
      removed_cosmetic_handle: cosmeticHandle,
      pity_after_refund: pityAfterRefund,
      original_was_mythic: originalWasMythic,
    })
  );

  res.status(200).json({
    ok: true,
    refunded: true,
    pull_id: pullId,
    banner_id: bannerId,
    refund_ts_epoch_secs: refundTs,
    removed_cosmetic_handle: cosmeticHandle,
    pity_after_refund_pulls_since_mythic: pityAfterRefund,
    original_was_mythic: originalWasMythic,
    sigma_refund_anchor: sigmaRefundAnchor,
    refund_window_secs: REFUND_WINDOW_SECS,
    ...env,
  });
}

// ─── inline tests · framework-agnostic ─────────────────────────────────────

interface MockedResponse {
  statusCode: number;
  body: unknown;
}

function mockReqRes(
  method: string,
  body: unknown = {},
  headers: Record<string, string> = {}
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null };
  const req = { method, query: {}, headers, body } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(payload: unknown) { out.body = payload; return this; },
    setHeader(_k: string, _v: string) { return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

function recentTs(): number {
  return Math.floor(Date.now() / 1000) - 60; // 1 min ago
}

export async function testRefundRequiresPubkey(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    cap: GACHA_CAP_REFUND,
    pull_id: 'p1',
    banner_id: 'b1',
    cosmetic_handle: 'cosmetic:rare:b1:001',
    rarity_at_pull: 'rare',
    pull_ts_epoch_secs: recentTs(),
  });
  await handler(req, res);
  assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
}

export async function testRefundRejectsNonCosmeticHandle(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    player_pubkey: 'aabbccdd'.repeat(8),
    pull_id: 'p1',
    banner_id: 'b1',
    cosmetic_handle: 'powerup:rare:b1:001', // ¬ cosmetic-only
    rarity_at_pull: 'rare',
    pull_ts_epoch_secs: recentTs(),
    cap: GACHA_CAP_REFUND,
  });
  await handler(req, res);
  assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
}

export async function testRefundCapDenied(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    player_pubkey: 'aabbccdd'.repeat(8),
    pull_id: 'p1',
    banner_id: 'b1',
    cosmetic_handle: 'cosmetic:rare:b1:001',
    rarity_at_pull: 'rare',
    pull_ts_epoch_secs: recentTs(),
    cap: 0,
  });
  await handler(req, res);
  assert(out.statusCode === 403, `expected 403, got ${out.statusCode}`);
}

export async function testRefundWithinWindowSucceeds(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    player_pubkey: 'aabbccdd'.repeat(8),
    pull_id: 'p1',
    banner_id: 'b1',
    cosmetic_handle: 'cosmetic:rare:b1:001',
    rarity_at_pull: 'rare',
    pull_ts_epoch_secs: recentTs(),
    pity_after_pull_pulls_since_mythic: 5,
    cap: GACHA_CAP_REFUND,
  });
  await handler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const body = out.body as RefundSuccess;
  assert(body.ok === true, 'ok = true');
  assert(body.refunded === true, 'refunded');
  assert(body.pity_after_refund_pulls_since_mythic === 4, 'pity rolled back by 1');
  assert(body.sigma_refund_anchor.length === 32, 'anchor 32 hex chars');
}

export async function testRefundOutsideWindowReturns410(): Promise<void> {
  const oldTs = Math.floor(Date.now() / 1000) - REFUND_WINDOW_SECS - 60;
  const { req, res, out } = mockReqRes('POST', {
    player_pubkey: 'aabbccdd'.repeat(8),
    pull_id: 'p1',
    banner_id: 'b1',
    cosmetic_handle: 'cosmetic:common:b1:050',
    rarity_at_pull: 'common',
    pull_ts_epoch_secs: oldTs,
    cap: GACHA_CAP_REFUND,
  });
  await handler(req, res);
  assert(out.statusCode === 410, `expected 410, got ${out.statusCode}`);
}

export async function testRefundMythicTicksPityUp(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    player_pubkey: 'aabbccdd'.repeat(8),
    pull_id: 'p1',
    banner_id: 'b1',
    cosmetic_handle: 'cosmetic:mythic:b1:001',
    rarity_at_pull: 'mythic',
    pull_ts_epoch_secs: recentTs(),
    pity_after_pull_pulls_since_mythic: 0,
    cap: GACHA_CAP_REFUND,
  });
  await handler(req, res);
  assert(out.statusCode === 200, 'OK');
  const body = out.body as RefundSuccess;
  assert(body.original_was_mythic === true, 'original mythic flagged');
  assert(body.pity_after_refund_pulls_since_mythic === 1, 'pity ticks up by 1 for mythic refund');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  void (async () => {
    await testRefundRequiresPubkey();
    await testRefundRejectsNonCosmeticHandle();
    await testRefundCapDenied();
    await testRefundWithinWindowSucceeds();
    await testRefundOutsideWindowReturns410();
    await testRefundMythicTicksPityUp();
    // eslint-disable-next-line no-console
    console.log('gacha/refund.ts : OK · 6 inline tests passed');
  })();
}
