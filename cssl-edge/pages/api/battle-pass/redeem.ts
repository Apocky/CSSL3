// cssl-edge · /api/battle-pass/redeem
// W13-9 · POST → claim a (season, tier, track) reward to inventory.
//   Body : { player_id · season_id · tier · track · cap · sovereign? }
//   200  → { cosmetic_id · kind · track · re_purchase_window? }
//   400  bad input · unreached-tier · invalid track
//   403  cap-deny · premium-locked · BATTLE_PASS_REDEEM (0x8000000) required
//   409  already-redeemed (anti-double-claim)
//
// Anti-FOMO : when season is archived AND reward.re_purchasable_after has passed,
// the same redeem-flow works at gift-cost (tracked by gift_cost_echo_shards in 0032
// migration). We surface re_purchase_window in the response when applicable.
//
// Mirrors cssl-host-battle-pass::Progression::try_redeem state-machine.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope } from '@/lib/response';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { checkCap, BATTLE_PASS_REDEEM } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';

type Track = 'free' | 'premium';

interface RedeemRequest {
  player_id?: string;
  season_id?: number;
  tier?: number;
  track?: Track;
  cap?: number;
  sovereign?: boolean;
}

interface RedeemSuccess {
  ok: true;
  player_id: string;
  season_id: number;
  tier: number;
  track: Track;
  cosmetic_id: string;
  kind: string;
  re_purchasable: boolean;
  served_by: string;
  ts: string;
}

interface RedeemError {
  ok: false;
  error: string;
  served_by: string;
  ts: string;
}

type Resp = RedeemSuccess | RedeemError;

const MIN_TIER = 1;
const MAX_TIER = 100;

// Per-player redemption store (anti-double-claim) + a tiny reward catalog
// stub. Real implementation queries battle_pass_rewards via service-role.
const REDEMPTIONS = new Set<string>();
function rkey(player_id: string, season_id: number, tier: number): string {
  return `${player_id}:${season_id}:${tier}`;
}

// Stub catalog for tests : tier 5 free skin · tier 10 premium emote.
interface CatalogRow { cosmetic_id: string; kind: string; track: Track }
const CATALOG = new Map<string, CatalogRow>([
  ['1:5:free', { cosmetic_id: 'cm_lantern_basic', kind: 'home_decor', track: 'free' }],
  ['1:10:premium', { cosmetic_id: 'cm_skin_silver', kind: 'skin', track: 'premium' }],
  ['1:50:free', { cosmetic_id: 'cm_emote_bow', kind: 'emote', track: 'free' }],
]);
function ckey(season_id: number, tier: number, track: Track): string {
  return `${season_id}:${tier}:${track}`;
}

// In-memory player progression view used to gate redeem (premium-bit + reached-tier).
const PROG = new Map<string, { tier: number; is_premium: boolean }>();
function pkey(player_id: string, season_id: number): string {
  return `${player_id}:${season_id}`;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse<Resp>): Promise<void> {
  logHit('battle-pass.redeem', { method: req.method ?? 'POST' });
  const env = envelope();

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...env });
    return;
  }

  const body = (req.body ?? {}) as RedeemRequest;
  const playerId = typeof body.player_id === 'string' ? body.player_id : '';
  const seasonId = typeof body.season_id === 'number' ? body.season_id : NaN;
  const tier = typeof body.tier === 'number' ? body.tier : NaN;
  const track: Track | null = body.track === 'free' || body.track === 'premium' ? body.track : null;

  if (playerId.length === 0 || Number.isNaN(seasonId) || Number.isNaN(tier) || track === null) {
    res.status(400).json({ ok: false, error: 'player_id · season_id · tier · track required', ...env });
    return;
  }
  if (tier < MIN_TIER || tier > MAX_TIER) {
    res.status(400).json({ ok: false, error: `tier ${tier} out of range [${MIN_TIER}, ${MAX_TIER}]`, ...env });
    return;
  }

  const sovereign = isSovereignFromIncoming(req.headers, body.sovereign);
  const cap = typeof body.cap === 'number' ? body.cap : 0;
  const decision = checkCap(cap, BATTLE_PASS_REDEEM, sovereign);
  if (!decision.ok) {
    const d = deny(decision.reason ?? 'cap denied', BATTLE_PASS_REDEEM);
    logEvent(d.body);
    res.status(d.status).json({ ok: false, error: 'cap denied · BATTLE_PASS_REDEEM (0x8000000) required', ...env });
    return;
  }

  // Anti-double-claim : tier-level UNIQUE (matches DB constraint).
  if (REDEMPTIONS.has(rkey(playerId, seasonId, tier))) {
    res.status(409).json({ ok: false, error: `tier ${tier} already redeemed · ¬ double-claim`, ...env });
    return;
  }

  // Tier-reached gate.
  const prog = PROG.get(pkey(playerId, seasonId)) ?? { tier: 0, is_premium: false };
  if (tier > prog.tier) {
    res.status(403).json({ ok: false, error: `tier ${tier} not reached (current=${prog.tier})`, ...env });
    return;
  }
  // Premium-lock gate.
  if (track === 'premium' && !prog.is_premium) {
    res.status(403).json({ ok: false, error: 'premium track locked · purchase required', ...env });
    return;
  }

  // Catalog lookup.
  const reward = CATALOG.get(ckey(seasonId, tier, track));
  if (reward === undefined) {
    res.status(400).json({ ok: false, error: `no reward catalog entry for season=${seasonId} tier=${tier} track=${track}`, ...env });
    return;
  }

  REDEMPTIONS.add(rkey(playerId, seasonId, tier));
  logEvent(auditEvent('battle-pass.redeem.ok', BATTLE_PASS_REDEEM, sovereign, 'ok', {
    player_id: playerId,
    season_id: seasonId,
    tier,
    track,
    cosmetic_id: reward.cosmetic_id,
  }));

  res.status(200).json({
    ok: true,
    player_id: playerId,
    season_id: seasonId,
    tier,
    track,
    cosmetic_id: reward.cosmetic_id,
    kind: reward.kind,
    // Anti-FOMO : surface that the reward is also available post-season at gift-cost.
    // Real implementation would consult battle_pass_rewards.re_purchasable_after.
    re_purchasable: true,
    ...env,
  });
}

// ─── Test utilities ──────────────────────────────────────────────────────
export function _resetForTests(): void {
  REDEMPTIONS.clear();
  PROG.clear();
}
export function _setProgressionForTests(player_id: string, season_id: number, tier: number, is_premium: boolean): void {
  PROG.set(pkey(player_id, season_id), { tier, is_premium });
}

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

// 1. Cap=0 → 403.
export async function testRedeemCapDeniedDefault(): Promise<void> {
  _resetForTests();
  const { req, res, out } = mockReqRes('POST', { player_id: 'p1', season_id: 1, tier: 5, track: 'free', cap: 0 });
  await handler(req, res);
  assert(out.statusCode === 403, `expected 403, got ${out.statusCode}`);
}

// 2. Free track + reached tier → 200.
export async function testRedeemFreeTrackReachedTier(): Promise<void> {
  _resetForTests();
  _setProgressionForTests('p1', 1, 50, false);
  const { req, res, out } = mockReqRes('POST', { player_id: 'p1', season_id: 1, tier: 5, track: 'free', cap: BATTLE_PASS_REDEEM });
  await handler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const b = out.body as { cosmetic_id?: string; track?: string };
  assert(b.cosmetic_id === 'cm_lantern_basic', `expected cm_lantern_basic, got ${b.cosmetic_id}`);
}

// 3. Premium track when not premium → 403.
export async function testRedeemPremiumLockedDenied(): Promise<void> {
  _resetForTests();
  _setProgressionForTests('p1', 1, 50, false);
  const { req, res, out } = mockReqRes('POST', { player_id: 'p1', season_id: 1, tier: 10, track: 'premium', cap: BATTLE_PASS_REDEEM });
  await handler(req, res);
  assert(out.statusCode === 403, `expected 403 premium-locked, got ${out.statusCode}`);
}

// 4. Already-redeemed → 409.
export async function testRedeemDoubleClaimDenied(): Promise<void> {
  _resetForTests();
  _setProgressionForTests('p1', 1, 50, false);
  // First redeem.
  const r1 = mockReqRes('POST', { player_id: 'p1', season_id: 1, tier: 5, track: 'free', cap: BATTLE_PASS_REDEEM });
  await handler(r1.req, r1.res);
  assert(r1.out.statusCode === 200, 'first redeem should succeed');
  // Second redeem.
  const r2 = mockReqRes('POST', { player_id: 'p1', season_id: 1, tier: 5, track: 'free', cap: BATTLE_PASS_REDEEM });
  await handler(r2.req, r2.res);
  assert(r2.out.statusCode === 409, `expected 409 double-claim, got ${r2.out.statusCode}`);
}

// 5. Tier not reached → 403.
export async function testRedeemTierNotReachedDenied(): Promise<void> {
  _resetForTests();
  _setProgressionForTests('p1', 1, 3, false);
  const { req, res, out } = mockReqRes('POST', { player_id: 'p1', season_id: 1, tier: 50, track: 'free', cap: BATTLE_PASS_REDEEM });
  await handler(req, res);
  assert(out.statusCode === 403, `expected 403 tier-not-reached, got ${out.statusCode}`);
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain = typeof require !== 'undefined' && typeof module !== 'undefined' && require.main === module;
if (isMain) {
  void (async () => {
    await testRedeemCapDeniedDefault();
    await testRedeemFreeTrackReachedTier();
    await testRedeemPremiumLockedDenied();
    await testRedeemDoubleClaimDenied();
    await testRedeemTierNotReachedDenied();
    // eslint-disable-next-line no-console
    console.log('battle-pass/redeem.ts : OK · 5 inline tests passed');
  })();
}
