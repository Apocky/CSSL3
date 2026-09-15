// TOTP (RFC 6238) — sign in with an authenticator app, no email in the path at all.
//
// Email was the whole problem. Whether the code appears in the message is decided by a mail
// template nobody here can edit, and the link it sends opens the system browser, which hands the
// session to the browser and leaves the app signed out. An authenticator sidesteps every part of
// that: the code is generated on the device, offline, and nothing has to be delivered.
//
// Implemented directly rather than pulled in, because it is thirty lines of HMAC and a dependency
// in the authentication path is a dependency that can be taken over.

import { createHmac, randomBytes, timingSafeEqual } from 'node:crypto';

export const TOTP_DIGITS = 6;
export const TOTP_PERIOD_SECONDS = 30;
/**
 * How many 30s steps either side of now are accepted.
 *
 * One step (±30s) absorbs ordinary clock drift between a phone and a server. Widening this is the
 * usual reflex when a code "does not work" and it is the wrong move: each extra step multiplies
 * the number of codes valid at any instant.
 */
export const TOTP_WINDOW_STEPS = 1;

const BASE32 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

export function generateSecret(bytes = 20): string {
  return base32Encode(randomBytes(bytes));
}

export function base32Encode(input: Buffer): string {
  let bits = 0;
  let value = 0;
  let output = '';
  for (const byte of input) {
    value = (value << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      output += BASE32[(value >>> (bits - 5)) & 31];
      bits -= 5;
    }
  }
  if (bits > 0) output += BASE32[(value << (5 - bits)) & 31];
  return output;
}

export function base32Decode(input: string): Buffer {
  const clean = input.toUpperCase().replace(/[=\s-]/gu, '');
  let bits = 0;
  let value = 0;
  const out: number[] = [];
  for (const character of clean) {
    const index = BASE32.indexOf(character);
    if (index < 0) throw new Error('TOTP_SECRET_INVALID');
    value = (value << 5) | index;
    bits += 5;
    if (bits >= 8) {
      out.push((value >>> (bits - 8)) & 0xff);
      bits -= 8;
    }
  }
  return Buffer.from(out);
}

/** The RFC 6238 code for one counter step. */
export function codeForStep(secret: string, step: number): string {
  const key = base32Decode(secret);
  const counter = Buffer.alloc(8);
  // Step numbers exceed 32 bits only past the year 6000, but writing the high word keeps this
  // correct rather than correct-for-now.
  counter.writeUInt32BE(Math.floor(step / 0x1_0000_0000), 0);
  counter.writeUInt32BE(step >>> 0, 4);
  const digest = createHmac('sha1', key).update(counter).digest();
  const offset = digest[digest.length - 1]! & 0x0f;
  const binary = ((digest[offset]! & 0x7f) << 24)
    | ((digest[offset + 1]! & 0xff) << 16)
    | ((digest[offset + 2]! & 0xff) << 8)
    | (digest[offset + 3]! & 0xff);
  return String(binary % 10 ** TOTP_DIGITS).padStart(TOTP_DIGITS, '0');
}

export function currentStep(atMs: number = Date.now()): number {
  return Math.floor(atMs / 1000 / TOTP_PERIOD_SECONDS);
}

export interface TotpVerification {
  readonly ok: boolean;
  /** The step the code matched, so the caller can refuse a replay of it. */
  readonly step: number | null;
}

/**
 * Check a submitted code.
 *
 * `lastUsedStep` blocks replay: a code stays valid for its whole 30-second step, so without this
 * someone who observes one — over a shoulder, in a screenshot, in a log — can use it again inside
 * that window. Every comparison is constant-time; a fast rejection tells an attacker which digits
 * were right.
 */
export function verifyTotp(
  secret: string,
  submitted: string,
  options: { readonly atMs?: number; readonly lastUsedStep?: number | null } = {},
): TotpVerification {
  const digits = (submitted ?? '').replace(/\D/gu, '');
  if (digits.length !== TOTP_DIGITS) return { ok: false, step: null };

  const now = currentStep(options.atMs ?? Date.now());
  const expected = Buffer.from(digits, 'utf8');
  let matched: number | null = null;

  for (let offset = -TOTP_WINDOW_STEPS; offset <= TOTP_WINDOW_STEPS; offset += 1) {
    const step = now + offset;
    const candidate = Buffer.from(codeForStep(secret, step), 'utf8');
    if (candidate.length === expected.length && timingSafeEqual(candidate, expected)) {
      matched = step;
      // No early break: leaving the loop the moment a step matches makes the response time
      // depend on WHICH step it was, which leaks the device's clock offset.
    }
  }

  if (matched === null) return { ok: false, step: null };
  if (options.lastUsedStep !== null && options.lastUsedStep !== undefined && matched <= options.lastUsedStep) {
    return { ok: false, step: null };
  }
  return { ok: true, step: matched };
}

/** otpauth:// URI for an authenticator app to scan. */
export function provisioningUri(secret: string, account: string, issuer = 'Apocky'): string {
  const label = `${encodeURIComponent(issuer)}:${encodeURIComponent(account)}`;
  const query = new URLSearchParams({
    secret,
    issuer,
    algorithm: 'SHA1',
    digits: String(TOTP_DIGITS),
    period: String(TOTP_PERIOD_SECONDS),
  });
  return `otpauth://totp/${label}?${query.toString()}`;
}
