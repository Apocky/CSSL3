// cssl-edge · /api/battle-pass/progress
// W13-9 · GET / POST current-season pass-progression for player.
//   GET  ?player_id=&season_id=&cap= → { tier · cumulative_xp · is_premium · paused · xp_to_next_tier }
//   POST { player_id · season_id · delta_xp · cap · sovereign? } → { tier_before · tier_after · tier_changed }
//   403  cap-deny when BATTLE_PASS_PROGRESS not set + sovereign=false
//   400  bad-input / unknown season
//
// Mirrors compiler-rs/crates/cssl-host-battle-pass::Progression state-machine.
// Sovereign-bypass RECORDED via auditEvent. Pause-state respected : when paused,
// delta-XP is silently ignored (not an error · sovereign-revocable behavior).

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope } from '@/lib/response';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { checkCap, BATTLE_PASS_PROGRESS } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';

interface ProgressGetQuery {
  player_id?: string;
  season_id?: string;
  cap?: string;
}

interface ProgressPostBody {
  player_id?: string;
  season_id?: number;
  delta_xp?: number;
  cap?: number;
  sovereign?: boolean;
}

interface ProgressGetResponse {
  ok: true;
  player_id: string;
  season_id: number;
  tier: number;
  cumulative_xp: number;
  is_premium: boolean;
  paused: boolean;
  xp_to_next_tier: number;
  served_by: string;
  ts: string;
}

interface ProgressPostResponse {
  ok: true;
  player_id: string;
  season_id: number;
  tier_before: number;
  tier_after: number;
  tier_changed: boolean;
  cumulative_xp: number;
  served_by: string;
  ts: string;
}

interface ProgressError {
  ok: false;
  error: string;
  served_by: string;
  ts: string;
}

type Resp = ProgressGetResponse | ProgressPostResponse | ProgressError;

// Mirror of cssl-host-battle-pass xp-curve. Three regimes : early-fast, mid-steady,
// late-gradual. Stays in lock-step with src/xp.rs ; if you change one, change both.
const REGIME_1_END = 30;
const REGIME_2_END = 70;
const MAX_TIER = 100;
const XP_R1 = 1_000;
const XP_R2 = 2_500;
const XP_R3 = 5_000;

function xpRequiredForTier(tier: number): number {
  if (tier < 1 || tier >= MAX_TIER) return 0;
  if (tier <= REGIME_1_END) return XP_R1;
  if (tier <= REGIME_2_END) return XP_R2;
  return XP_R3;
}

function cumulativeXpForTier(tier: number): number {
  if (tier <= 1) return 0;
  const target = Math.min(tier, MAX_TIER);
  let total = 0;
  const r1 = Math.min(REGIME_1_END, target - 1);
  total += r1 * XP_R1;
  if (target > REGIME_1_END + 1) {
    const r2 = Math.min(REGIME_2_END, target - 1) - REGIME_1_END;
    total += r2 * XP_R2;
  }
  if (target > REGIME_2_END + 1) {
    const r3 = (target - 1) - REGIME_2_END;
    total += r3 * XP_R3;
  }
  return total;
}

function tierForCumulativeXp(cum: number): number {
  let tier = 1;
  while (tier < MAX_TIER) {
    const next = cumulativeXpForTier(tier + 1);
    if (cum < next) return tier;
    tier += 1;
  }
  return MAX_TIER;
}

// In-memory shim store. Real implementation talks to Supabase via service-role.
// Persistence + RLS lives in cssl-supabase/migrations/0032_battle_pass.sql.
const STORE = new Map<string, { tier: number; cumulative_xp: number; is_premium: boolean; paused: boolean }>();
function key(player_id: string, season_id: number): string {
  return `${player_id}:${season_id}`;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse<Resp>): Promise<void> {
  logHit('battle-pass.progress', { method: req.method ?? 'GET' });
  const env = envelope();
  const sovereign = isSovereignFromIncoming(req.headers, (req.body as ProgressPostBody | undefined)?.sovereign);

  if (req.method === 'GET') {
    const q = req.query as ProgressGetQuery;
    const playerId = typeof q.player_id === 'string' ? q.player_id : '';
    const seasonId = typeof q.season_id === 'string' ? parseInt(q.season_id, 10) : NaN;
    const cap = typeof q.cap === 'string' ? parseInt(q.cap, 16) : 0;

    if (playerId.length === 0 || Number.isNaN(seasonId)) {
      res.status(400).json({ ok: false, error: 'player_id + season_id required', ...env });
      return;
    }

    const decision = checkCap(cap, BATTLE_PASS_PROGRESS, sovereign);
    if (!decision.ok) {
      const d = deny(decision.reason ?? 'cap denied', BATTLE_PASS_PROGRESS);
      logEvent(d.body);
      res.status(d.status).json({ ok: false, error: 'cap denied · BATTLE_PASS_PROGRESS (0x2000000) required', ...env });
      return;
    }

    const row = STORE.get(key(playerId, seasonId)) ?? { tier: 1, cumulative_xp: 0, is_premium: false, paused: false };
    const xpToNext = row.tier >= MAX_TIER ? 0 : cumulativeXpForTier(row.tier + 1) - row.cumulative_xp;

    logEvent(auditEvent('battle-pass.progress.get', BATTLE_PASS_PROGRESS, sovereign, 'ok', {
      player_id: playerId,
      season_id: seasonId,
      tier: row.tier,
    }));

    res.status(200).json({
      ok: true,
      player_id: playerId,
      season_id: seasonId,
      tier: row.tier,
      cumulative_xp: row.cumulative_xp,
      is_premium: row.is_premium,
      paused: row.paused,
      xp_to_next_tier: Math.max(0, xpToNext),
      ...env,
    });
    return;
  }

  if (req.method === 'POST') {
    const body = (req.body ?? {}) as ProgressPostBody;
    const playerId = typeof body.player_id === 'string' ? body.player_id : '';
    const seasonId = typeof body.season_id === 'number' ? body.season_id : NaN;
    const deltaXp = typeof body.delta_xp === 'number' ? body.delta_xp : 0;
    const cap = typeof body.cap === 'number' ? body.cap : 0;

    if (playerId.length === 0 || Number.isNaN(seasonId) || deltaXp < 0) {
      res.status(400).json({ ok: false, error: 'player_id + season_id required · delta_xp >= 0', ...env });
      return;
    }

    const decision = checkCap(cap, BATTLE_PASS_PROGRESS, sovereign);
    if (!decision.ok) {
      const d = deny(decision.reason ?? 'cap denied', BATTLE_PASS_PROGRESS);
      logEvent(d.body);
      res.status(d.status).json({ ok: false, error: 'cap denied · BATTLE_PASS_PROGRESS (0x2000000) required', ...env });
      return;
    }

    const k = key(playerId, seasonId);
    const row = STORE.get(k) ?? { tier: 1, cumulative_xp: 0, is_premium: false, paused: false };

    if (row.paused) {
      // Sovereign-pause respected : XP is ignored, NOT an error.
      logEvent(auditEvent('battle-pass.progress.paused', BATTLE_PASS_PROGRESS, sovereign, 'ok', {
        player_id: playerId,
        season_id: seasonId,
        delta_xp: deltaXp,
      }));
      res.status(200).json({
        ok: true,
        player_id: playerId,
        season_id: seasonId,
        tier_before: row.tier,
        tier_after: row.tier,
        tier_changed: false,
        cumulative_xp: row.cumulative_xp,
        ...env,
      });
      return;
    }

    const tierBefore = row.tier;
    row.cumulative_xp += deltaXp;
    row.tier = tierForCumulativeXp(row.cumulative_xp);
    STORE.set(k, row);

    logEvent(auditEvent('battle-pass.progress.awarded', BATTLE_PASS_PROGRESS, sovereign, 'ok', {
      player_id: playerId,
      season_id: seasonId,
      delta_xp: deltaXp,
      tier_before: tierBefore,
      tier_after: row.tier,
    }));

    res.status(200).json({
      ok: true,
      player_id: playerId,
      season_id: seasonId,
      tier_before: tierBefore,
      tier_after: row.tier,
      tier_changed: row.tier !== tierBefore,
      cumulative_xp: row.cumulative_xp,
      ...env,
    });
    return;
  }

  res.status(405).json({ ok: false, error: 'GET or POST only', ...env });
}

// ─── Test utilities ──────────────────────────────────────────────────────
export function _resetStoreForTests(): void {
  STORE.clear();
}

export function _setPausedForTests(player_id: string, season_id: number, paused: boolean): void {
  const k = key(player_id, season_id);
  const row = STORE.get(k) ?? { tier: 1, cumulative_xp: 0, is_premium: false, paused: false };
  row.paused = paused;
  STORE.set(k, row);
}

export function _setPremiumForTests(player_id: string, season_id: number): void {
  const k = key(player_id, season_id);
  const row = STORE.get(k) ?? { tier: 1, cumulative_xp: 0, is_premium: false, paused: false };
  row.is_premium = true;
  STORE.set(k, row);
}

interface MockedResponse { statusCode: number; body: unknown }
function mockReqRes(method: string, body: unknown = {}, query: Record<string, string> = {}, headers: Record<string, string> = {}): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null };
  const req = { method, query, headers, body } as unknown as NextApiRequest;
  const res = {
    status(c: number) { out.statusCode = c; return this; },
    json(p: unknown) { out.body = p; return this; },
    setHeader(_k: string, _v: string) { return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}
function assert(cond: boolean, msg: string): void { if (!cond) throw new Error(`assert : ${msg}`); }

// 1. Cap=0 denies.
export async function testProgressCapDeniedDefault(): Promise<void> {
  _resetStoreForTests();
  const { req, res, out } = mockReqRes('POST', { player_id: 'p1', season_id: 1, delta_xp: 100, cap: 0 });
  await handler(req, res);
  assert(out.statusCode === 403, `expected 403 cap-deny, got ${out.statusCode}`);
}

// 2. Cap-bit set → POST advances tier.
export async function testProgressXpAdvancesTier(): Promise<void> {
  _resetStoreForTests();
  const { req, res, out } = mockReqRes('POST', { player_id: 'p1', season_id: 1, delta_xp: 1500, cap: BATTLE_PASS_PROGRESS });
  await handler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const b = out.body as { tier_after?: number; tier_changed?: boolean };
  assert((b.tier_after ?? 0) >= 2, `expected tier >= 2, got ${b.tier_after}`);
  assert(b.tier_changed === true, 'expected tier_changed=true');
}

// 3. Paused row ignores XP (sovereign-pause).
export async function testProgressPausedSilentlyIgnoresXp(): Promise<void> {
  _resetStoreForTests();
  // Initialize a row first.
  const { req: r1, res: rs1 } = mockReqRes('POST', { player_id: 'p2', season_id: 1, delta_xp: 0, cap: BATTLE_PASS_PROGRESS });
  await handler(r1, rs1);
  _setPausedForTests('p2', 1, true);
  const { req, res, out } = mockReqRes('POST', { player_id: 'p2', season_id: 1, delta_xp: 50_000, cap: BATTLE_PASS_PROGRESS });
  await handler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const b = out.body as { tier_after?: number; tier_changed?: boolean; cumulative_xp?: number };
  assert(b.tier_changed === false, 'paused row must not advance tier');
  assert(b.cumulative_xp === 0, 'paused row must not accumulate XP');
}

// 4. Sovereign-bypass allows even with cap=0.
//    Requires both `sovereign:true` body-flag AND the magic header value.
export async function testProgressSovereignBypassAllows(): Promise<void> {
  _resetStoreForTests();
  const { req, res, out } = mockReqRes(
    'POST',
    { player_id: 'p3', season_id: 1, delta_xp: 100, cap: 0, sovereign: true },
    {},
    { 'x-loa-sovereign-cap': '0xCAFEBABEDEADBEEF' }
  );
  await handler(req, res);
  assert(out.statusCode === 200, `expected 200 sovereign, got ${out.statusCode}`);
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain = typeof require !== 'undefined' && typeof module !== 'undefined' && require.main === module;
if (isMain) {
  void (async () => {
    await testProgressCapDeniedDefault();
    await testProgressXpAdvancesTier();
    await testProgressPausedSilentlyIgnoresXp();
    await testProgressSovereignBypassAllows();
    // eslint-disable-next-line no-console
    console.log('battle-pass/progress.ts : OK · 4 inline tests passed');
  })();
}
