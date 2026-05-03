// cssl-edge/lib/mneme/sigma.ts
// MNEME — SigmaMask helpers (packed 19B layout, padded to 32B on the wire).
//
// Spec : ../../specs/27_SIGMA_MASK_RUNTIME.csl § Σ-MASK-BIT-LAYOUT
//
// PACKED LAYOUT (19 bytes — wire-padded to 32B):
//   [0..7]   audience_bits     u64 LE       (bit 0 = SELF)
//   [8..10]  cap_bits          u24 LE       (READ|WRITE|BROADCAST|PURGE|DERIVE...)
//   [11..14] revoked_at        u32 LE       (unix-seconds; 0 = active)
//   [15..18] flags             u32 LE       (forward/backward compat flags)
//
// All cssl-edge code reads/writes the 32B representation. Helpers convert
// to/from compact 19B form when needed (e.g. CLI tooling).
//
// SUPABASE WIRE FORMAT
//   Postgres `bytea` columns accept literals like  '\x' || hex  (when sending
//   raw SQL) OR Buffer-like Uint8Array via supabase-js. We always serialise
//   as a hex string for round-trip safety in JSON envelopes.

import { MnemeError } from './types';

export const SIGMA_PACKED_LEN = 19;
export const SIGMA_WIRE_LEN   = 32;

// Bit positions in cap field — kept in sync with specs/27.
export const CAP_READ      = 0x01;
export const CAP_WRITE     = 0x02;
export const CAP_BROADCAST = 0x04;
export const CAP_PURGE     = 0x08;
export const CAP_DERIVE    = 0x10;

// Default audience bit.
export const AUD_SELF = 0x01n;

// ── Construction ───────────────────────────────────────────────────────

// Build a default 32-byte sigma mask (audience=SELF · cap=READ|WRITE · active).
export function defaultMask(opts?: {
    audience_bits?: bigint;
    cap_bits?:      number;
    flags?:         number;
}): Uint8Array {
    const aud  = opts?.audience_bits ?? AUD_SELF;
    const cap  = opts?.cap_bits      ?? (CAP_READ | CAP_WRITE);
    const flag = opts?.flags         ?? 0;

    const buf = new Uint8Array(SIGMA_WIRE_LEN);
    writeU64LE(buf, 0, aud);
    writeU24LE(buf, 8, cap & 0xffffff);
    writeU32LE(buf, 11, 0);          // revoked_at = 0 (active)
    writeU32LE(buf, 15, flag >>> 0);
    // bytes 19..31 stay zero — wire padding
    return buf;
}

// Mark a mask revoked at the given unix-seconds timestamp. Returns a NEW mask
// (originals are immutable for safety).
export function revokeMask(mask: Uint8Array, ts_unix: number): Uint8Array {
    if (mask.length !== SIGMA_WIRE_LEN && mask.length !== SIGMA_PACKED_LEN) {
        throw new MnemeError('SIGMA_LEN',
            `mask length must be ${SIGMA_PACKED_LEN} or ${SIGMA_WIRE_LEN}, got ${mask.length}`,
            500);
    }
    const out = new Uint8Array(SIGMA_WIRE_LEN);
    out.set(toWireMask(mask));
    writeU32LE(out, 11, Math.max(0, Math.floor(ts_unix)) >>> 0);
    return out;
}

// Read revoked_at unix-seconds from a mask (0 = active).
export function maskRevokedAt(mask: Uint8Array): number {
    const m = toWireMask(mask);
    return readU32LE(m, 11);
}

export function isRevoked(mask: Uint8Array): boolean {
    return maskRevokedAt(mask) > 0;
}

// Read audience bits as bigint.
export function maskAudienceBits(mask: Uint8Array): bigint {
    const m = toWireMask(mask);
    return readU64LE(m, 0);
}

// Read cap bits.
export function maskCapBits(mask: Uint8Array): number {
    const m = toWireMask(mask);
    return readU24LE(m, 8);
}

// Check audience-class match (caller-class ⊆ row-audience).
export function audienceMatch(rowMask: Uint8Array, callerClass: bigint): boolean {
    const rowAud = maskAudienceBits(rowMask);
    return (rowAud & callerClass) !== 0n;
}

// Check cap match.
export function capMatch(callerCap: number, requiredCap: number): boolean {
    return (callerCap & requiredCap) === requiredCap;
}

// ── Codec : 19↔32 byte conversion ──────────────────────────────────────

// Coerce any input to the 32-byte wire form (zero-pads packed 19B).
export function toWireMask(mask: Uint8Array): Uint8Array {
    if (mask.length === SIGMA_WIRE_LEN) return mask;
    if (mask.length === SIGMA_PACKED_LEN) {
        const padded = new Uint8Array(SIGMA_WIRE_LEN);
        padded.set(mask);
        return padded;
    }
    throw new MnemeError('SIGMA_LEN',
        `mask length must be ${SIGMA_PACKED_LEN} or ${SIGMA_WIRE_LEN}, got ${mask.length}`,
        500);
}

// Truncate to packed 19B (asserts the trailing bytes are zero).
export function toPackedMask(mask: Uint8Array): Uint8Array {
    if (mask.length === SIGMA_PACKED_LEN) return mask;
    if (mask.length === SIGMA_WIRE_LEN) {
        for (let i = SIGMA_PACKED_LEN; i < SIGMA_WIRE_LEN; i++) {
            if (mask[i] !== 0) {
                throw new MnemeError('SIGMA_PADDING',
                    `wire mask has nonzero pad at byte ${i}`, 500);
            }
        }
        return mask.subarray(0, SIGMA_PACKED_LEN);
    }
    throw new MnemeError('SIGMA_LEN',
        `mask length must be ${SIGMA_PACKED_LEN} or ${SIGMA_WIRE_LEN}, got ${mask.length}`,
        500);
}

// ── Hex codec ──────────────────────────────────────────────────────────

const HEX_RE = /^[0-9a-fA-F]+$/;

export function maskToHex(mask: Uint8Array): string {
    const buf = toWireMask(mask);
    let out = '';
    for (let i = 0; i < buf.length; i++) {
        const b = buf[i] ?? 0;
        out += b.toString(16).padStart(2, '0');
    }
    return out;
}

export function maskFromHex(hex: string): Uint8Array {
    const trimmed = hex.startsWith('0x') ? hex.slice(2) : hex;
    if (trimmed.length !== SIGMA_PACKED_LEN * 2 && trimmed.length !== SIGMA_WIRE_LEN * 2) {
        throw new MnemeError('SIGMA_HEX_LEN',
            `hex must be ${SIGMA_PACKED_LEN * 2} or ${SIGMA_WIRE_LEN * 2} chars, got ${trimmed.length}`,
            400);
    }
    if (!HEX_RE.test(trimmed)) {
        throw new MnemeError('SIGMA_HEX_CHARS', 'hex contains non-hex characters', 400);
    }
    const buf = new Uint8Array(trimmed.length / 2);
    for (let i = 0; i < buf.length; i++) {
        buf[i] = parseInt(trimmed.substr(i * 2, 2), 16);
    }
    return toWireMask(buf);
}

// Postgres bytea wire literal. Used in SQL templates.
export function maskToPgBytea(mask: Uint8Array): string {
    return '\\x' + maskToHex(mask);
}

// ── Low-level u64/u32/u24 LE helpers (no DataView dependency) ──────────

function writeU64LE(buf: Uint8Array, off: number, val: bigint): void {
    let v = val & 0xffffffffffffffffn;
    for (let i = 0; i < 8; i++) {
        buf[off + i] = Number(v & 0xffn);
        v >>= 8n;
    }
}

function readU64LE(buf: Uint8Array, off: number): bigint {
    let out = 0n;
    for (let i = 7; i >= 0; i--) {
        out = (out << 8n) | BigInt(buf[off + i] ?? 0);
    }
    return out;
}

function writeU32LE(buf: Uint8Array, off: number, val: number): void {
    let v = val >>> 0;
    for (let i = 0; i < 4; i++) {
        buf[off + i] = v & 0xff;
        v >>>= 8;
    }
}

function readU32LE(buf: Uint8Array, off: number): number {
    return ((buf[off] ?? 0))
         | ((buf[off + 1] ?? 0) << 8)
         | ((buf[off + 2] ?? 0) << 16)
         | (((buf[off + 3] ?? 0) << 24) >>> 0);
}

function writeU24LE(buf: Uint8Array, off: number, val: number): void {
    const v = val & 0xffffff;
    buf[off]     = v & 0xff;
    buf[off + 1] = (v >> 8) & 0xff;
    buf[off + 2] = (v >> 16) & 0xff;
}

function readU24LE(buf: Uint8Array, off: number): number {
    return ((buf[off] ?? 0))
         | ((buf[off + 1] ?? 0) << 8)
         | ((buf[off + 2] ?? 0) << 16);
}

// ── Default mask resolution from env (used by route handlers) ──────────

// Read MNEME_DEFAULT_MASK_HEX env-var, falling back to a fresh defaultMask().
export function envDefaultMask(): Uint8Array {
    const hex = process.env['MNEME_DEFAULT_MASK_HEX'];
    if (hex && hex.length > 0) {
        try {
            return maskFromHex(hex);
        } catch {
            // fall through to constructed default
        }
    }
    return defaultMask();
}

// ── Inline self-test (run via `npx tsx lib/mneme/sigma.ts`) ────────────

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;

function _selfTest(): void {
    function assert(cond: boolean, msg: string): void {
        if (!cond) throw new Error('assert: ' + msg);
    }
    const m = defaultMask();
    assert(m.length === SIGMA_WIRE_LEN, 'wire length');
    assert(maskRevokedAt(m) === 0, 'fresh mask is active');
    assert(!isRevoked(m), 'fresh mask isRevoked false');
    assert(maskAudienceBits(m) === AUD_SELF, 'audience defaults to SELF');
    assert(maskCapBits(m) === (CAP_READ | CAP_WRITE), 'cap defaults to R|W');

    const m2 = revokeMask(m, 1714521600);
    assert(maskRevokedAt(m2) === 1714521600, 'revoke set');
    assert(isRevoked(m2), 'isRevoked true');

    const hex = maskToHex(m);
    assert(hex.length === SIGMA_WIRE_LEN * 2, 'hex length');
    const round = maskFromHex(hex);
    assert(round.length === SIGMA_WIRE_LEN, 'hex round-trip length');
    for (let i = 0; i < m.length; i++) {
        assert(m[i] === round[i], `hex round-trip byte ${i}`);
    }

    const packed = toPackedMask(m);
    assert(packed.length === SIGMA_PACKED_LEN, 'packed length');
    const wired = toWireMask(packed);
    assert(wired.length === SIGMA_WIRE_LEN, 'packed→wire length');

    assert(audienceMatch(m, AUD_SELF), 'self audience matches');
    assert(capMatch(CAP_READ | CAP_WRITE, CAP_READ), 'cap match read');
    assert(!capMatch(CAP_READ, CAP_WRITE), 'cap mismatch');
    // eslint-disable-next-line no-console
    console.log('sigma.ts : OK · 12 self-tests passed');
}

const _isMain = typeof require !== 'undefined'
             && typeof module !== 'undefined'
             && require.main === module;
if (_isMain) _selfTest();
