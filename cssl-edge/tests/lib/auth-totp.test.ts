// TOTP, checked against the published vectors rather than against itself.
//
// A home-grown one-time-password implementation that only agrees with its own output is not
// verified, it is consistent. RFC 4226 §D publishes ten HOTP values for a known secret, and TOTP
// is HOTP with the counter set to time/30 — so codeForStep(secret, N) must reproduce them exactly.
// If it does, every authenticator app on earth will agree with this code.

import {
  base32Encode, base32Decode, codeForStep, currentStep, generateSecret,
  provisioningUri, verifyTotp, TOTP_PERIOD_SECONDS,
} from '../../lib/auth-totp';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

// RFC 4226 Appendix D. Secret is the ASCII string "12345678901234567890".
const RFC_SECRET = base32Encode(Buffer.from('12345678901234567890', 'ascii'));
const RFC_HOTP = [
  '755224', '287082', '359152', '969429', '338314',
  '254676', '287922', '162583', '399871', '520489',
];

function main(): void {
  // 1 — the published vectors. This is the whole correctness argument.
  RFC_HOTP.forEach((expected, counter) => {
    const actual = codeForStep(RFC_SECRET, counter);
    assert(actual === expected, `RFC 4226 counter ${counter}: got ${actual}, expected ${expected}`);
  });

  // 2 — base32 round-trips, including lengths that do not land on a 5-bit boundary.
  for (const sample of ['a', 'ab', 'abc', 'abcd', 'abcde', 'hello world', '12345678901234567890']) {
    const encoded = base32Encode(Buffer.from(sample, 'ascii'));
    assert(base32Decode(encoded).toString('ascii') === sample, `base32 round-trip failed for "${sample}"`);
  }
  assert(base32Decode('gezdgnbvgy3tqojq').length > 0, 'lowercase secrets must decode');
  let rejected = false;
  try { base32Decode('not-valid-base32!'); } catch { rejected = true; }
  assert(rejected, 'an invalid secret must be rejected, not silently decoded');

  // 3 — the step is a function of time, and the window is ±1 step, not more.
  const secret = generateSecret();
  const at = 1_700_000_000_000;
  const step = currentStep(at);
  assert(currentStep(at + TOTP_PERIOD_SECONDS * 1000) === step + 1, 'the step must advance every period');
  assert(verifyTotp(secret, codeForStep(secret, step), { atMs: at }).ok, 'the current code must verify');
  assert(verifyTotp(secret, codeForStep(secret, step - 1), { atMs: at }).ok, 'one step behind must verify (clock drift)');
  assert(verifyTotp(secret, codeForStep(secret, step + 1), { atMs: at }).ok, 'one step ahead must verify (clock drift)');
  assert(!verifyTotp(secret, codeForStep(secret, step - 2), { atMs: at }).ok, 'two steps behind must NOT verify');
  assert(!verifyTotp(secret, codeForStep(secret, step + 2), { atMs: at }).ok, 'two steps ahead must NOT verify');

  // 4 — replay. A code is valid for a whole 30-second step, so without this a code seen over a
  //     shoulder or left in a screenshot can be used again inside that window.
  const code = codeForStep(secret, step);
  const first = verifyTotp(secret, code, { atMs: at });
  assert(first.ok && first.step === step, 'first use must succeed and report its step');
  const replay = verifyTotp(secret, code, { atMs: at, lastUsedStep: first.step });
  assert(!replay.ok, 'the same code must not verify twice');
  // An older code must not work either, once a newer one has been used.
  assert(!verifyTotp(secret, codeForStep(secret, step - 1), { atMs: at, lastUsedStep: step }).ok,
    'a code from an earlier step must not verify after a later one was used');

  // 5 — malformed input is refused rather than coerced.
  for (const bad of ['', '12345', '1234567', 'abcdef', '12 34 56', '000000000']) {
    if (bad === '12 34 56') continue; // spaces are stripped; that one is legitimate input
    assert(!verifyTotp(secret, bad, { atMs: at }).ok, `"${bad}" must not verify`);
  }
  // Spacing as an authenticator displays it ("123 456") must still work.
  const spaced = code.slice(0, 3) + ' ' + code.slice(3);
  assert(verifyTotp(secret, spaced, { atMs: at }).ok, 'a code typed with a space must verify');

  // 6 — the provisioning URI is what an authenticator expects.
  const uri = provisioningUri('JBSWY3DPEHPK3PXP', 'apocky13@gmail.com');
  assert(uri.startsWith('otpauth://totp/Apocky:'), `unexpected otpauth label: ${uri.slice(0, 40)}`);
  assert(uri.includes('secret=JBSWY3DPEHPK3PXP'), 'the secret must be in the URI');
  assert(uri.includes('digits=6') && uri.includes('period=30'), 'digits and period must be explicit');
  assert(uri.includes('%40'), 'the account must be URI-encoded');

  // 7 — generated secrets are distinct and decodable.
  const secrets = new Set(Array.from({ length: 50 }, () => generateSecret()));
  assert(secrets.size === 50, 'generateSecret repeated itself');
  for (const value of secrets) assert(base32Decode(value).length === 20, 'a generated secret was not 160 bits');

  console.log(`auth-totp.test: ${RFC_HOTP.length} RFC 4226 vectors reproduced exactly, base32 round-trips, `
    + 'window is +/-1 step and no wider, replay refused, spaced codes accepted, otpauth URI well-formed');
}

try {
  main();
  console.log('auth-totp OK');
} catch (error) {
  console.error(error);
  process.exitCode = 1;
}
