// cssl-edge · lib/cap.ts
// Cap-bit constants + cap-gate predicate for /api/* routes.
//
// Cap-bit layout :
//   bit 0 (0x1) · COMPANION_REMOTE_RELAY · /api/companion
//   bit 0 (0x1) · MP_CAP_HOST_ROOM       · /api/signaling/create-room
//   bit 1 (0x2) · MP_CAP_JOIN_ROOM       · /api/signaling/join-room
//   bit 2 (0x4) · MP_CAP_RELAY_DATA      · /api/signaling/{post-signal,poll}
//
// Multiplayer caps + companion cap occupy distinct bit-spaces inside the
// caller-supplied `cap` integer. Callers OR-compose cap-bits per request.

// Multiplayer signaling cap-bits.
export const MP_CAP_HOST_ROOM = 1;
export const MP_CAP_JOIN_ROOM = 2;
export const MP_CAP_RELAY_DATA = 4;

// Companion cap-bit (mirrors CAP_COMPANION_REMOTE_RELAY in /api/companion).
export const COMPANION_REMOTE_RELAY = 1;

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
