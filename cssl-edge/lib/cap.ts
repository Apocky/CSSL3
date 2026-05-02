// cssl-edge · lib/cap.ts
// Cap-bit constants + cap-gate predicate for /api/* routes.
//
// Cap-bit layout :
//   bit 0   (0x001) · COMPANION_REMOTE_RELAY · /api/companion
//   bit 0   (0x001) · MP_CAP_HOST_ROOM       · /api/signaling/create-room
//   bit 1   (0x002) · MP_CAP_JOIN_ROOM       · /api/signaling/join-room
//   bit 2   (0x004) · MP_CAP_RELAY_DATA      · /api/signaling/{post-signal,poll}
//   bit 4   (0x010) · MARKETPLACE_CAP_LIST   · /api/marketplace/list
//   bit 5   (0x020) · MARKETPLACE_CAP_POST   · /api/marketplace/post
//   bit 6   (0x040) · RUN_SHARE_CAP_SUBMIT   · /api/run-share/submit
//   bit 7   (0x080) · RUN_SHARE_CAP_RECEIVE  · /api/run-share/feed
//   bit 8   (0x100) · MP_CAP_RENDEZVOUS      · /api/mp-rendezvous/lobby
//   bit 9   (0x200) · STRIPE_CHECKOUT_INIT   · /api/payments/stripe/checkout
//   bit 10  (0x400) · STRIPE_REFUND_REQUEST  · /api/payments/stripe/refund-request
//
// Multiplayer caps + companion cap + marketplace + run-share caps share the
// caller-supplied `cap` integer (distinct bit-spaces). Callers OR-compose.

// Multiplayer signaling cap-bits.
export const MP_CAP_HOST_ROOM = 1;
export const MP_CAP_JOIN_ROOM = 2;
export const MP_CAP_RELAY_DATA = 4;

// Companion cap-bit (mirrors CAP_COMPANION_REMOTE_RELAY in /api/companion).
export const COMPANION_REMOTE_RELAY = 1;

// Marketplace + run-share + rendezvous cap-bits (POD-4 D3 expansion).
// Distinct bit-space from companion (0x1) and signaling (0x1/0x2/0x4).
export const MARKETPLACE_CAP_LIST = 0x10;
export const MARKETPLACE_CAP_POST = 0x20;
export const RUN_SHARE_CAP_SUBMIT = 0x40;
export const RUN_SHARE_CAP_RECEIVE = 0x80;
export const MP_CAP_RENDEZVOUS = 0x100;

// Stripe payment cap-bits (W9 expansion). Caller supplies via header
// `x-loa-cap` integer when initiating checkout / requesting refund.
// DEFAULT-DENY when neither cap-bit set AND sovereign header absent.
export const STRIPE_CHECKOUT_INIT = 0x200;
export const STRIPE_REFUND_REQUEST = 0x400;

// UGC content-publish cap-bits (W12-5). Creators present `CONTENT_CAP_PUBLISH`
// to /api/content/publish/* ; moderators present `CONTENT_CAP_REVOKE_ANY` to
// /api/content/publish/revoke when revoking content they did not author.
export const CONTENT_CAP_PUBLISH = 0x800;
export const CONTENT_CAP_REVOKE_ANY = 0x1000;

// UGC content-remix cap-bits (W12-9). Creators present `CONTENT_CAP_REMIX`
// to /api/content/remix/init when forking another creator's content.
// `CONTENT_CAP_TIP` is presented to /api/content/tip when sending a gift.
// Both are creator-class caps · default-deny when absent + ¬ sovereign.
export const CONTENT_CAP_REMIX = 0x2000;
export const CONTENT_CAP_TIP = 0x4000;

// UGC content-rating cap-bits (W12-7). Raters present these to
// /api/content/{rate,review,aggregate}. The DB-side k-anon-floor
// (5 raters single · 10 raters trending) gates aggregate exposure.
//   bit 16 (0x10000) · CONTENT_CAP_RATE             · /api/content/rate
//   bit 17 (0x20000) · CONTENT_CAP_REVIEW_BODY      · /api/content/review
//   bit 18 (0x40000) · CONTENT_CAP_AGGREGATE_PUBLIC · row contributes to public aggregate
export const CONTENT_CAP_RATE = 0x10000;
export const CONTENT_CAP_REVIEW_BODY = 0x20000;
export const CONTENT_CAP_AGGREGATE_PUBLIC = 0x40000;

// Content-moderation cap-bits (W12-11). Σ-mask · revocable ANYTIME.
//   bit 19 (0x80000)  · CONTENT_CAP_FLAG          · /api/content/moderation/flag
//   bit 20 (0x100000) · CONTENT_CAP_APPEAL        · /api/content/moderation/appeal
//   bit 21 (0x200000) · CONTENT_CAP_CURATE_A      · community-elected curator
//   bit 22 (0x400000) · CONTENT_CAP_CURATE_B      · substrate-team curator
//   bit 23 (0x800000) · CONTENT_CAP_CHAIN_ANCHOR  · curator Σ-Chain-write
//   bit 24 (0x1000000)· CONTENT_CAP_AGGREGATE_READ· author-transparency-read
export const CONTENT_CAP_FLAG = 0x80000;
export const CONTENT_CAP_APPEAL = 0x100000;
export const CONTENT_CAP_CURATE_A = 0x200000;
export const CONTENT_CAP_CURATE_B = 0x400000;
export const CONTENT_CAP_CHAIN_ANCHOR = 0x800000;
export const CONTENT_CAP_AGGREGATE_READ = 0x1000000;
export const CONTENT_CAP_CURATE_ANY = CONTENT_CAP_CURATE_A | CONTENT_CAP_CURATE_B;

// Battle-pass cap-bits (W13-9). Default-DENY when none set ; sovereign-bypass
// recorded. Cosmetic-only · ¬ pay-for-power · ¬ FOMO + sovereign-revocable.
//   bit 25 (0x2000000)  · BATTLE_PASS_PROGRESS · /api/battle-pass/progress
//   bit 26 (0x4000000)  · BATTLE_PASS_UNLOCK   · /api/battle-pass/unlock (Premium purchase)
//   bit 27 (0x8000000)  · BATTLE_PASS_REDEEM   · /api/battle-pass/redeem (claim reward)
export const BATTLE_PASS_PROGRESS = 0x2000000;
export const BATTLE_PASS_UNLOCK = 0x4000000;
export const BATTLE_PASS_REDEEM = 0x8000000;

// Gacha cap-bits (W13-10). Transparency-mandate · cosmetic-only · sovereign-7d-refund.
//   bit 28 (0x10000000) · GACHA_CAP_PULL    · /api/gacha/pull
//   bit 29 (0x20000000) · GACHA_CAP_HISTORY · /api/gacha/history
//   bit 30 (0x40000000) · GACHA_CAP_REFUND  · /api/gacha/refund
export const GACHA_CAP_PULL = 0x10000000;
export const GACHA_CAP_HISTORY = 0x20000000;
export const GACHA_CAP_REFUND = 0x40000000;

// Cap-gate result. `ok=false` carries a reason for audit-log + 403 body.
export interface CapDecision {
  ok: boolean;
  reason?: string;
}

// Predicate : caller cap-mask must include `required` bits OR sovereign==true.
// DEFAULT-DENY : `cap=0 + sovereign=false` → ok:false.
export function checkCap(cap: number, required: number, sovereign: boolean): CapDecision {
  if (sovereign) return { ok: true };
  if ((cap & required) === required) return { ok: true };
  return {
    ok: false,
    reason: `cap 0x${required.toString(16)} required (caller=0x${cap.toString(16)})`,
  };
}

// ─── inline tests · framework-agnostic ─────────────────────────────────────
// Run via `npx tsx lib/cap.ts` (when invoked directly).

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

export function testCheckCapZeroDenies(): void {
  const d = checkCap(0, MP_CAP_HOST_ROOM, false);
  assert(d.ok === false, 'cap=0 + sovereign=false → must deny');
  assert(typeof d.reason === 'string', 'deny must include reason');
}

export function testCheckCapBitSetAllows(): void {
  const d = checkCap(MP_CAP_HOST_ROOM, MP_CAP_HOST_ROOM, false);
  assert(d.ok === true, 'cap-bit set → must allow');
}

export function testCheckCapSovereignBypass(): void {
  const d = checkCap(0, MP_CAP_RELAY_DATA, true);
  assert(d.ok === true, 'sovereign=true → must allow even with cap=0');
}

export function testCheckCapCompositeMask(): void {
  // Caller has HOST + JOIN bits → JOIN-required succeeds, RELAY-required fails.
  const composite = MP_CAP_HOST_ROOM | MP_CAP_JOIN_ROOM;
  assert(checkCap(composite, MP_CAP_JOIN_ROOM, false).ok === true, 'JOIN bit present → allow');
  assert(checkCap(composite, MP_CAP_RELAY_DATA, false).ok === false, 'RELAY bit absent → deny');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  testCheckCapZeroDenies();
  testCheckCapBitSetAllows();
  testCheckCapSovereignBypass();
  testCheckCapCompositeMask();
  // eslint-disable-next-line no-console
  console.log('cap.ts : OK · 4 inline tests passed');
}
