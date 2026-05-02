// cssl-edge · /api/gacha/pull
// W13-10 · Gacha pull-mechanic with transparency + pity + sigma-anchor.
//
// POST { player_pubkey · banner_id · mode · starting_pull_index? · pity_in? }
//   → { ok:true · outcomes[] · pity_out · attestations[] · sigma_anchors[] }  (200)
//   → { ok:false · error · disclosed_drop_rates }  (4xx) when transparency-fail
//   → { evt:'audit', status:'denied', reason }  (403) when cap-gate fails
//
// PRIME-DIRECTIVE :
//   - cosmetic-only-axiom : every outcome carries cosmetic-handle (¬ stat-buff)
//   - transparency-mandate : drop-rates returned in the response BEFORE result
//   - pity-system : guaranteed Mythic at PITY_THRESHOLD pulls (publicly-known)
//   - sigma-anchor : every pull anchored for-immutable-history
//
// All routes audit-emit · sovereign-bypass-RECORDED.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope } from '@/lib/response';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { checkCap, GACHA_CAP_PULL } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';

// ─── public-knowledge constants (mirror cssl-host-gacha) ───────────────────

const DROP_RATES_BPS: Record<string, number> = {
  common: 60_000,
  uncommon: 25_000,
  rare: 10_000,
  epic: 4_000,
  legendary: 900,
  mythic: 100,
};
const TOTAL_BPS = 100_000;
const PITY_THRESHOLD = 90;
const ATTESTATIONS = [
  '¬ pay-for-power (cosmetic-only-axiom)',
  '¬ near-miss-animation',
  '¬ countdown-FOMO',
  '¬ exclusive-cosmetic-AT-ALL',
  '¬ loss-aversion-framing',
  '¬ social-comparison',
  '¬ celebrity-endorsement',
  '¬ in-game-grind-loop for-pull-currency',
  'transparency-mandate (drop-rates + pity publicly-disclosed)',
  'sovereign-revocable (7d full-refund · player-pubkey-tied)',
];

const PULL_MODE_TOTALS: Record<string, number> = {
  single: 1,
  ten_pull: 11,
  hundred_pull: 111,
};

// ─── request/response types ────────────────────────────────────────────────

interface PullRequestBody {
  player_pubkey?: string; // hex-encoded Ed25519
  banner_id?: string;
  mode?: string;
  starting_pull_index?: number;
  pity_in_pulls_since_mythic?: number;
  cap?: number;
  sovereign?: boolean;
}

interface PullOutcome {
  pull_index: number;
  rarity: string;
  cosmetic_handle: string;
  forced_by_pity: boolean;
  roll_bps: number;
  sigma_anchor_id: string;
}

interface PullSuccess {
  ok: true;
  mode: string;
  outcomes: PullOutcome[];
  pity_out_pulls_since_mythic: number;
  disclosed_drop_rates: Record<string, string>;
  pity_threshold: number;
  attestations: string[];
  attestations_count: number;
  served_by: string;
  ts: string;
}

interface PullError {
  ok: false;
  error: string;
  disclosed_drop_rates?: Record<string, string>;
  served_by: string;
  ts: string;
}

type Resp = PullSuccess | PullError;

// ─── helpers ──────────────────────────────────────────────────────────────

function disclosedDropRatePct(): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [k, bps] of Object.entries(DROP_RATES_BPS)) {
    const whole = Math.floor(bps / 1000);
    const frac = bps % 1000;
    out[k] = `${whole}.${String(frac).padStart(3, '0')}%`;
  }
  return out;
}

// xoshiro128++ — port of cssl-host-gacha::DetRng for stage-0 stub-mode rolls.
// In live-mode the Rust crate is invoked via FFI ; this stub keeps the API
// stable for pre-FFI integration testing.
class DetRng {
  private state: [number, number, number, number];

  constructor(seed: bigint) {
    const lanes: [number, number, number, number] = [
      Number(seed & 0xFFFF_FFFFn),
      Number((seed >> 32n) & 0xFFFF_FFFFn),
      Number((seed >> 64n) & 0xFFFF_FFFFn),
      Number((seed >> 96n) & 0xFFFF_FFFFn),
    ];
    const anyNonZero = lanes[0] | lanes[1] | lanes[2] | lanes[3];
    this.state = anyNonZero === 0
      ? [0x9E37_79B9, 0x517C_C1B7, 0x6510_5C53, 0xCBA9_4F2C]
      : lanes;
  }

  nextU32(): number {
    const result = this.rotl((this.state[0] + this.state[3]) >>> 0, 7) + this.state[0];
    const t = (this.state[1] << 9) >>> 0;
    this.state[2] ^= this.state[0];
    this.state[3] ^= this.state[1];
    this.state[1] ^= this.state[2];
    this.state[0] ^= this.state[3];
    this.state[2] ^= t;
    this.state[3] = this.rotl(this.state[3], 11);
    return result >>> 0;
  }

  private rotl(x: number, k: number): number {
    return ((x << k) | (x >>> (32 - k))) >>> 0;
  }

  nextBelow(ceil: number): number {
    if (ceil === 0) return 0;
    return this.nextU32() % ceil;
  }
}

// FNV-1a 64-bit · stage-0 lightweight stand-in for blake3 seed derivation.
// Live impl uses Rust FFI to cssl_host_gacha::derive_seed_from_pubkey.
function deriveSeed(pubkey: string, bannerId: string, pullIndex: number): bigint {
  let h = 0xcbf29ce484222325n;
  const p = 0x100000001b3n;
  const mask = (1n << 128n) - 1n;
  const all = `${pubkey}|banner=${bannerId}|pull=${pullIndex}`;
  for (let i = 0; i < all.length; i++) {
    h ^= BigInt(all.charCodeAt(i));
    h = (h * p) & mask;
  }
  return h;
}

function mapRollToRarity(rollBps: number): string {
  const order = ['common', 'uncommon', 'rare', 'epic', 'legendary', 'mythic'];
  let cum = 0;
  for (const r of order) {
    const bps = DROP_RATES_BPS[r];
    if (typeof bps === 'number') {
      cum += bps;
      if (rollBps < cum) return r;
    }
  }
  return 'mythic';
}

// 32-hex-char anchor-id (FNV-1a-128-derived for stage-0). Live: FFI to blake3.
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
  // 32 hex chars = 16 bytes.
  return h.toString(16).padStart(32, '0').slice(0, 32);
}

// ─── handler ──────────────────────────────────────────────────────────────

export default async function handler(req: NextApiRequest, res: NextApiResponse<Resp>): Promise<void> {
  logHit('gacha.pull', { method: req.method ?? 'POST' });
  const env = envelope();

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...env });
    return;
  }

  const body = (req.body ?? {}) as PullRequestBody;
  const pubkey = typeof body.player_pubkey === 'string' ? body.player_pubkey : '';
  const bannerId = typeof body.banner_id === 'string' ? body.banner_id : '';
  const mode = typeof body.mode === 'string' ? body.mode : '';
  const starting = typeof body.starting_pull_index === 'number' ? body.starting_pull_index : 0;
  const pityInRaw = typeof body.pity_in_pulls_since_mythic === 'number'
    ? body.pity_in_pulls_since_mythic
    : 0;

  if (pubkey.length === 0) {
    res.status(400).json({
      ok: false,
      error: 'player_pubkey required',
      disclosed_drop_rates: disclosedDropRatePct(),
      ...env,
    });
    return;
  }
  if (bannerId.length === 0) {
    res.status(400).json({
      ok: false,
      error: 'banner_id required',
      disclosed_drop_rates: disclosedDropRatePct(),
      ...env,
    });
    return;
  }
  if (!Object.prototype.hasOwnProperty.call(PULL_MODE_TOTALS, mode)) {
    res.status(400).json({
      ok: false,
      error: 'mode must be single | ten_pull | hundred_pull',
      disclosed_drop_rates: disclosedDropRatePct(),
      ...env,
    });
    return;
  }

  // ── cap-gate · default-DENY ────────────────────────────────────────────
  const sovereign = isSovereignFromIncoming(req.headers, body.sovereign);
  const callerCap = typeof body.cap === 'number' ? body.cap : 0;
  const decision = checkCap(callerCap, GACHA_CAP_PULL, sovereign);
  if (!decision.ok) {
    const d = deny(decision.reason ?? 'cap denied', GACHA_CAP_PULL);
    logEvent(d.body);
    res.status(d.status).json({
      ok: false,
      error: 'cap denied · GACHA_CAP_PULL (0x10000000) required',
      disclosed_drop_rates: disclosedDropRatePct(),
      ...env,
    });
    return;
  }

  // ── execute pull ───────────────────────────────────────────────────────
  const total = PULL_MODE_TOTALS[mode] ?? 1;
  const outcomes: PullOutcome[] = [];
  let pity = Math.max(0, Math.min(PITY_THRESHOLD - 1, Math.floor(pityInRaw)));
  const ts = Date.now();

  for (let i = 0; i < total; i++) {
    const pullIndex = starting + i;
    const seed = deriveSeed(pubkey, bannerId, pullIndex);
    const rng = new DetRng(seed);
    const rollBps = rng.nextBelow(TOTAL_BPS);
    const forcedByPity = (pity + 1) >= PITY_THRESHOLD;
    const rarity = forcedByPity ? 'mythic' : mapRollToRarity(rollBps);
    const cosmeticHandle = `cosmetic:${rarity}:${bannerId}:${String(rollBps % 1000).padStart(3, '0')}`;
    const pullId = `${pubkey.slice(0, 8)}-${bannerId}-${pullIndex}`;
    const sigmaAnchor = makeAnchorId(
      'pull',
      pubkey,
      bannerId,
      pullId,
      ts,
      `${rarity}|${rollBps}|${forcedByPity}`
    );
    if (rarity === 'mythic') pity = 0;
    else pity = Math.min(PITY_THRESHOLD - 1, pity + 1);

    outcomes.push({
      pull_index: pullIndex,
      rarity,
      cosmetic_handle: cosmeticHandle,
      forced_by_pity: forcedByPity,
      roll_bps: rollBps,
      sigma_anchor_id: sigmaAnchor,
    });
  }

  logEvent(
    auditEvent('gacha.pull.completed', GACHA_CAP_PULL, sovereign, 'ok', {
      banner_id: bannerId,
      mode,
      total,
      mythic_count: outcomes.filter((o) => o.rarity === 'mythic').length,
    })
  );

  res.status(200).json({
    ok: true,
    mode,
    outcomes,
    pity_out_pulls_since_mythic: pity,
    disclosed_drop_rates: disclosedDropRatePct(),
    pity_threshold: PITY_THRESHOLD,
    attestations: ATTESTATIONS,
    attestations_count: ATTESTATIONS.length,
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

export async function testGachaPullMissingPubkey(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { cap: GACHA_CAP_PULL });
  await handler(req, res);
  assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
}

export async function testGachaPullCapDenied(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    player_pubkey: 'aabbccddeeff'.repeat(4),
    banner_id: 'b1',
    mode: 'single',
    cap: 0,
  });
  await handler(req, res);
  assert(out.statusCode === 403, `expected 403, got ${out.statusCode}`);
}

export async function testGachaPullSingleSucceeds(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    player_pubkey: 'aabbccddeeff'.repeat(4),
    banner_id: 'b1',
    mode: 'single',
    cap: GACHA_CAP_PULL,
  });
  await handler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const body = out.body as PullSuccess;
  assert(body.ok === true, 'ok must be true');
  assert(body.outcomes.length === 1, 'single mode → 1 outcome');
  const first = body.outcomes[0];
  assert(first !== undefined && first.cosmetic_handle.startsWith('cosmetic:'),
    'cosmetic-only-axiom : handle must start with cosmetic:');
  assert(body.attestations.length === 10, '10 attestations disclosed');
  assert(body.disclosed_drop_rates.mythic === '0.100%', 'mythic = 0.100%');
}

export async function testGachaPullTenPullReturnsBundle(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    player_pubkey: 'aabbccddeeff'.repeat(4),
    banner_id: 'b1',
    mode: 'ten_pull',
    cap: GACHA_CAP_PULL,
  });
  await handler(req, res);
  assert(out.statusCode === 200, 'OK');
  const body = out.body as PullSuccess;
  assert(body.outcomes.length === 11, 'ten_pull = 10 + 1 bonus = 11 outcomes');
}

export async function testGachaPullPityForcesMythic(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    player_pubkey: 'aabbccddeeff'.repeat(4),
    banner_id: 'b1',
    mode: 'single',
    pity_in_pulls_since_mythic: PITY_THRESHOLD - 1, // next pull MUST be Mythic
    cap: GACHA_CAP_PULL,
  });
  await handler(req, res);
  assert(out.statusCode === 200, 'OK');
  const body = out.body as PullSuccess;
  const first = body.outcomes[0];
  assert(first !== undefined && first.rarity === 'mythic', 'pity-forced must be mythic');
  assert(first !== undefined && first.forced_by_pity === true, 'forced_by_pity must be true');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  void (async () => {
    await testGachaPullMissingPubkey();
    await testGachaPullCapDenied();
    await testGachaPullSingleSucceeds();
    await testGachaPullTenPullReturnsBundle();
    await testGachaPullPityForcesMythic();
    // eslint-disable-next-line no-console
    console.log('gacha/pull.ts : OK · 5 inline tests passed');
  })();
}
