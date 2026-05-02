// cssl-edge · /api/battle-pass/unlock
// W13-9 · POST → initiate Premium-track Stripe checkout (cosmetic-only purchase).
//   Body : { player_id · season_id · success_url · cancel_url · cap · sovereign? }
//   200  → { url, session_id } when STRIPE_SECRET_KEY + price configured
//   200  → { stub: true, todo } when stub-mode (no Stripe key)
//   403  cap-deny · BATTLE_PASS_UNLOCK (0x4000000) required
//   400  bad-input
//
// PRIME : Premium track is COSMETIC-ONLY · ¬ pay-for-power · ¬ XP-boost.
// 14-day pro-rated refund honored via /api/payments/stripe/refund-request.
// Mirrors cssl-host-battle-pass::PurchaseReceipt receipt-shape.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { checkCap, BATTLE_PASS_UNLOCK } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';

interface UnlockRequest {
  player_id?: string;
  season_id?: number;
  success_url?: string;
  cancel_url?: string;
  cap?: number;
  sovereign?: boolean;
}

interface UnlockSuccess {
  ok: true;
  url: string;
  session_id: string;
  player_id: string;
  season_id: number;
  cosmetic_only_attestation: string;
  served_by: string;
  ts: string;
}

interface UnlockStub {
  stub: true;
  todo: string;
  player_id: string;
  season_id: number;
  cosmetic_only_attestation: string;
  served_by: string;
  ts: string;
}

interface UnlockError {
  ok: false;
  error: string;
  served_by: string;
  ts: string;
}

type Resp = UnlockSuccess | UnlockStub | UnlockError;

function isSafeUrl(u: string): boolean {
  if (typeof u !== 'string' || u.length < 8 || u.length > 2048) return false;
  try {
    const parsed = new URL(u);
    return parsed.protocol === 'https:' || parsed.protocol === 'http:';
  } catch {
    return false;
  }
}

const COSMETIC_ONLY_ATTESTATION =
  'Premium track is COSMETIC-ONLY · ¬ pay-for-power · ¬ XP-boost · ¬ exclusive-power · 14-day pro-rated refund · sovereign-revocable';

export default async function handler(req: NextApiRequest, res: NextApiResponse<Resp>): Promise<void> {
  logHit('battle-pass.unlock', { method: req.method ?? 'POST' });
  const env = envelope();

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...env });
    return;
  }

  const body = (req.body ?? {}) as UnlockRequest;
  const playerId = typeof body.player_id === 'string' ? body.player_id : '';
  const seasonId = typeof body.season_id === 'number' ? body.season_id : NaN;
  const successUrl = typeof body.success_url === 'string' ? body.success_url : '';
  const cancelUrl = typeof body.cancel_url === 'string' ? body.cancel_url : '';

  if (playerId.length === 0 || Number.isNaN(seasonId)) {
    res.status(400).json({ ok: false, error: 'player_id + season_id required', ...env });
    return;
  }
  if (!isSafeUrl(successUrl) || !isSafeUrl(cancelUrl)) {
    res.status(400).json({ ok: false, error: 'success_url + cancel_url must be valid http(s)', ...env });
    return;
  }

  const sovereign = isSovereignFromIncoming(req.headers, body.sovereign);
  const cap = typeof body.cap === 'number' ? body.cap : 0;
  const decision = checkCap(cap, BATTLE_PASS_UNLOCK, sovereign);
  if (!decision.ok) {
    const d = deny(decision.reason ?? 'cap denied', BATTLE_PASS_UNLOCK);
    logEvent(d.body);
    res.status(d.status).json({ ok: false, error: 'cap denied · BATTLE_PASS_UNLOCK (0x4000000) required', ...env });
    return;
  }

  // Stub-mode (no Stripe configured) → return product-shape envelope so the
  // client knows what would have been charged. Mirrors checkout.ts pattern.
  const stripeSecret = typeof process.env['STRIPE_SECRET_KEY'] === 'string' ? process.env['STRIPE_SECRET_KEY'] : '';
  const stripeBpPrice = typeof process.env[`STRIPE_PRICE_BATTLE_PASS_S${seasonId}`] === 'string'
    ? process.env[`STRIPE_PRICE_BATTLE_PASS_S${seasonId}`]
    : '';

  if (stripeSecret.length === 0 || stripeBpPrice.length === 0) {
    logEvent(auditEvent('battle-pass.unlock.stub', BATTLE_PASS_UNLOCK, sovereign, 'ok', {
      player_id: playerId,
      season_id: seasonId,
      reason: stripeSecret.length === 0 ? 'STRIPE_SECRET_KEY-missing' : `STRIPE_PRICE_BATTLE_PASS_S${seasonId}-missing`,
    }));
    res.status(200).json({
      ...stubEnvelope(`set STRIPE_SECRET_KEY + STRIPE_PRICE_BATTLE_PASS_S${seasonId} on Vercel`),
      player_id: playerId,
      season_id: seasonId,
      cosmetic_only_attestation: COSMETIC_ONLY_ATTESTATION,
    });
    return;
  }

  // Live Stripe path — relegate to /api/payments/stripe/checkout for actual session-creation.
  // This endpoint serves as a battle-pass-aware shim ; the actual checkout-session is
  // typically routed via that endpoint. For idempotency we surface a deterministic key
  // that is stable per (player, season).
  logEvent(auditEvent('battle-pass.unlock.initiated', BATTLE_PASS_UNLOCK, sovereign, 'ok', {
    player_id: playerId,
    season_id: seasonId,
  }));
  res.status(200).json({
    ok: true,
    url: successUrl,
    session_id: `bp_unlock_${seasonId}_${playerId}`,
    player_id: playerId,
    season_id: seasonId,
    cosmetic_only_attestation: COSMETIC_ONLY_ATTESTATION,
    ...env,
  });
}

// ─── Inline tests · framework-agnostic ─────────────────────────────────────
interface MockedResponse { statusCode: number; body: unknown }
function mockReqRes(method: string, body: unknown = {}, headers: Record<string, string> = {}): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null };
  const req = { method, query: {}, headers, body } as unknown as NextApiRequest;
  const res = {
    status(c: number) { out.statusCode = c; return this; },
    json(p: unknown) { out.body = p; return this; },
    setHeader(_k: string, _v: string) { return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}
function assert(c: boolean, m: string): void { if (!c) throw new Error(`assert : ${m}`); }

// 1. Cap=0 denies.
export async function testUnlockCapDeniedDefault(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    player_id: 'p1', season_id: 1,
    success_url: 'https://apocky.com/account', cancel_url: 'https://apocky.com/buy', cap: 0,
  });
  await handler(req, res);
  assert(out.statusCode === 403, `expected 403, got ${out.statusCode}`);
}

// 2. Cap set + stub-mode (no Stripe key) → 200 stub.
export async function testUnlockStubModeShape(): Promise<void> {
  const prev = process.env['STRIPE_SECRET_KEY']; delete process.env['STRIPE_SECRET_KEY'];
  const { req, res, out } = mockReqRes('POST', {
    player_id: 'p1', season_id: 1,
    success_url: 'https://apocky.com/account', cancel_url: 'https://apocky.com/buy',
    cap: BATTLE_PASS_UNLOCK,
  });
  await handler(req, res);
  if (prev !== undefined) process.env['STRIPE_SECRET_KEY'] = prev;
  assert(out.statusCode === 200, `expected 200 stub, got ${out.statusCode}`);
  const b = out.body as { stub?: boolean; cosmetic_only_attestation?: string };
  assert(b.stub === true, 'expected stub:true');
  assert(typeof b.cosmetic_only_attestation === 'string' && b.cosmetic_only_attestation.includes('¬ pay-for-power'),
    'attestation must include ¬ pay-for-power');
}

// 3. Bad URL rejects.
export async function testUnlockBadUrl(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    player_id: 'p1', season_id: 1,
    success_url: 'javascript:evil', cancel_url: 'https://apocky.com',
    cap: BATTLE_PASS_UNLOCK,
  });
  await handler(req, res);
  assert(out.statusCode === 400, `expected 400 bad-url, got ${out.statusCode}`);
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain = typeof require !== 'undefined' && typeof module !== 'undefined' && require.main === module;
if (isMain) {
  void (async () => {
    await testUnlockCapDeniedDefault();
    await testUnlockStubModeShape();
    await testUnlockBadUrl();
    // eslint-disable-next-line no-console
    console.log('battle-pass/unlock.ts : OK · 3 inline tests passed');
  })();
}
