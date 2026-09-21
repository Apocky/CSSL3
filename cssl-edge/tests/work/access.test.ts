/**
 * Cloudflare Access verification at the origin.
 *
 * This is the lock that makes remote read AND WRITE tool access defensible. So the tests are
 * mostly forgeries: real RSA keys, real signatures, and every way a token can be wrong.
 *
 * The one that matters most is `wrong_audience`. A JWT minted by the SAME Cloudflare team for a
 * DIFFERENT application is genuinely signed, unexpired, and issued by the right issuer. Skipping
 * the aud check is the classic Access mistake, and it would let any other app in the team open
 * this filesystem.
 */
import { createPrivateKey, createPublicKey, generateKeyPairSync, sign as cryptoSign } from 'node:crypto';

import {
  AccessVerifier,
  buildAccessGate,
  parseAllowedEmails,
} from '@/scripts/apocrypha-work/access';

function assert(cond: boolean, message: string): asserts cond {
  if (!cond) throw new Error('assert failed: ' + message);
}

const TEAM = 'winter-snowflake-e14a';
const ISS = 'https://' + TEAM + '.cloudflareaccess.com';
const AUD = '6ac37243a25d9c2d9044e8c817c24810183af2761148e4b711f27ad72c216588';
const EMAIL = 'apocky13@gmail.com';
const KID = 'test-key-1';

const { publicKey, privateKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
const jwk = { ...(publicKey.export({ format: 'jwk' }) as Record<string, unknown>), kid: KID, alg: 'RS256', use: 'sig' };

// A second, unrelated key: used to forge a signature that is real but from the wrong signer.
const other = generateKeyPairSync('rsa', { modulusLength: 2048 });

function b64url(input: Buffer | string): string {
  return Buffer.from(input).toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

interface Claims {
  iss?: string;
  aud?: string | string[];
  email?: string;
  sub?: string;
  exp?: number;
  nbf?: number;
}

function makeToken(claims: Claims = {}, opts: { alg?: string; kid?: string; key?: ReturnType<typeof createPrivateKey> } = {}): string {
  const now = Math.floor(Date.now() / 1000);
  const header = { alg: opts.alg ?? 'RS256', kid: opts.kid ?? KID, typ: 'JWT' };
  const payload = {
    iss: ISS,
    aud: [AUD],
    email: EMAIL,
    sub: 'sub-123',
    iat: now,
    exp: now + 3600,
    ...claims,
  };
  const head = b64url(JSON.stringify(header));
  const body = b64url(JSON.stringify(payload));
  const signature = cryptoSign('RSA-SHA256', Buffer.from(head + '.' + body, 'utf8'), opts.key ?? privateKey);
  return head + '.' + body + '.' + b64url(signature);
}

function verifier(over: Partial<ConstructorParameters<typeof AccessVerifier>[0]> = {}): AccessVerifier {
  let served = 0;
  const fetchImpl = (async () => {
    served += 1;
    return new Response(JSON.stringify({ keys: [jwk] }), { status: 200 });
  }) as unknown as typeof fetch;
  const v = new AccessVerifier({
    teamName: TEAM,
    audTag: AUD,
    allowedEmails: new Set([EMAIL]),
    fetchImpl,
    ...over,
  });
  (v as unknown as { served: () => number }).served = () => served;
  return v;
}

async function expectReason(token: string | null | undefined, reason: string, what: string): Promise<void> {
  const result = await verifier().verify(token);
  assert(!result.ok, what + ': expected refusal, got acceptance');
  assert(result.reason === reason,
    what + ': expected reason ' + reason + ', got ' + (result as { reason: string }).reason);
}

async function testValidTokenIsAccepted(): Promise<void> {
  // The null for every refusal test below. Without this, a verifier that refused EVERYTHING
  // would pass the entire rest of this file.
  const result = await verifier().verify(makeToken());
  assert(result.ok, 'a correctly signed, current, in-audience token must be accepted; got '
    + (result as { reason?: string }).reason);
  assert(result.identity.email === EMAIL, 'the identity must carry the email');
  assert(result.identity.subject === 'sub-123', 'the identity must carry the subject');
}

async function testMissingAndMalformed(): Promise<void> {
  await expectReason(undefined, 'missing_assertion', 'no header at all');
  await expectReason('', 'missing_assertion', 'empty header');
  await expectReason('not-a-jwt', 'malformed_jwt', 'not three parts');
  await expectReason('a.b.c', 'undecodable_jwt', 'three parts of garbage');
}

async function testAlgorithmIsNotTakenFromTheToken(): Promise<void> {
  // alg:none and RS256->HS256 confusion both work by trusting header.alg.
  const none = b64url(JSON.stringify({ alg: 'none', kid: KID }))
    + '.' + b64url(JSON.stringify({ iss: ISS, aud: [AUD], email: EMAIL, exp: Math.floor(Date.now() / 1000) + 600 }))
    + '.';
  const result = await verifier().verify(none);
  assert(!result.ok, 'alg:none must never be accepted');

  await expectReason(makeToken({}, { alg: 'HS256' }), 'unexpected_alg', 'alg swapped to HS256');
}

async function testForgedSignature(): Promise<void> {
  // Signed for real, by the wrong key. This is what an attacker who cannot reach Cloudflare's
  // private key would produce.
  await expectReason(
    makeToken({}, { key: createPrivateKey(other.privateKey.export({ type: 'pkcs8', format: 'pem' })) }),
    'bad_signature',
    'signed by an unrelated key',
  );
}

async function testTamperedPayload(): Promise<void> {
  const token = makeToken({ email: 'someone.else@example.com' });
  const [head, , sig] = token.split('.');
  const swapped = b64url(JSON.stringify({ iss: ISS, aud: [AUD], email: EMAIL, exp: Math.floor(Date.now() / 1000) + 600 }));
  await expectReason(head + '.' + swapped + '.' + sig, 'bad_signature', 'payload swapped after signing');
}

async function testUnknownKid(): Promise<void> {
  await expectReason(makeToken({}, { kid: 'some-other-kid' }), 'unknown_kid', 'kid not in the JWKS');
}

async function testWrongIssuer(): Promise<void> {
  await expectReason(makeToken({ iss: 'https://evil.cloudflareaccess.com' }), 'wrong_issuer', 'another team');
}

async function testWrongAudience(): Promise<void> {
  // THE important one. Correctly signed by the real team, for a different application.
  await expectReason(
    makeToken({ aud: ['a-different-access-application-tag'] }),
    'wrong_audience',
    'a valid token for another app in the same team',
  );
  await expectReason(makeToken({ aud: [] }), 'wrong_audience', 'empty audience');
  // aud as a bare string, which Cloudflare also emits, must still be honoured.
  const asString = await verifier().verify(makeToken({ aud: AUD }));
  assert(asString.ok, 'a string aud matching the tag must be accepted');
}

async function testExpiryAndNotBefore(): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  await expectReason(makeToken({ exp: now - 3600 }), 'expired', 'an hour stale');
  await expectReason(makeToken({ nbf: now + 3600 }), 'not_yet_valid', 'not valid for an hour');

  // Inside the skew window, a just-expired token is still fine -- clocks drift.
  const justExpired = await verifier().verify(makeToken({ exp: now - 5 }));
  assert(justExpired.ok, 'a token 5s past expiry must survive the skew allowance');
}

async function testEmailAllowList(): Promise<void> {
  await expectReason(makeToken({ email: 'stranger@example.com' }), 'email_not_allowed', 'not on the list');
  await expectReason(makeToken({ email: '' }), 'no_email', 'no email claim');

  // Case must not matter; Cloudflare may emit either.
  const upper = await verifier().verify(makeToken({ email: EMAIL.toUpperCase() }));
  assert(upper.ok, 'email comparison must be case insensitive');

  // An EMPTY allow list denies everyone. It must never mean "allow all".
  const nobody = await verifier({ allowedEmails: new Set<string>() }).verify(makeToken());
  assert(!nobody.ok && nobody.reason === 'email_not_allowed',
    'an empty allow list must deny, not admit everyone');
}

function testParseAllowedEmails(): void {
  const parsed = parseAllowedEmails(' Apocky13@Gmail.com , second@example.com ,, ');
  assert(parsed.has('apocky13@gmail.com'), 'emails are lower-cased and trimmed');
  assert(parsed.has('second@example.com'), 'multiple emails are supported');
  assert(parsed.size === 2, 'empty entries are dropped, got ' + String(parsed.size));
  assert(parseAllowedEmails(undefined).size === 0, 'unset means empty, which means deny');
}

function testGateIsRequiredWheneverItCouldBeReached(): void {
  const full = {
    APOCRYPHA_WORK_ACCESS_TEAM: TEAM,
    APOCRYPHA_WORK_ACCESS_AUD: AUD,
    APOCRYPHA_WORK_ACCESS_EMAILS: EMAIL,
  } as NodeJS.ProcessEnv;

  const loopback = buildAccessGate({} as NodeJS.ProcessEnv, '127.0.0.1');
  assert(!loopback.required, 'a loopback-only listener does not need the edge gate');

  const exposed = buildAccessGate(full, '0.0.0.0');
  assert(exposed.required && exposed.verifier !== null, 'a non-loopback bind must require Access');

  const forced = buildAccessGate({ ...full, APOCRYPHA_WORK_REQUIRE_ACCESS: '1' }, '127.0.0.1');
  assert(forced.required && forced.verifier !== null, 'the explicit flag must force the gate on');
}

function testMisconfigurationFailsClosed(): void {
  // Access required, settings missing. This must NOT degrade into "no gate".
  for (const partial of [
    { APOCRYPHA_WORK_REQUIRE_ACCESS: '1' },
    { APOCRYPHA_WORK_REQUIRE_ACCESS: '1', APOCRYPHA_WORK_ACCESS_TEAM: TEAM },
    { APOCRYPHA_WORK_REQUIRE_ACCESS: '1', APOCRYPHA_WORK_ACCESS_TEAM: TEAM, APOCRYPHA_WORK_ACCESS_AUD: AUD },
  ] as NodeJS.ProcessEnv[]) {
    const gate = buildAccessGate(partial, '127.0.0.1');
    assert(gate.required, 'an incomplete Access config must stay REQUIRED');
    assert(gate.verifier === null, 'and must produce no verifier, so the server refuses to serve');
    assert(Boolean(gate.disabledReason), 'and must say what is missing');
  }
}

async function testKeysAreCachedButRefreshable(): Promise<void> {
  const v = verifier();
  await v.verify(makeToken());
  await v.verify(makeToken());
  const served = (v as unknown as { served: () => number }).served();
  assert(served === 1, 'the JWKS must be cached across verifications, fetched ' + String(served) + ' times');
}

async function testNetworkFailureDoesNotAdmit(): Promise<void> {
  const failing = (async () => { throw new TypeError('fetch failed'); }) as unknown as typeof fetch;
  const v = new AccessVerifier({
    teamName: TEAM, audTag: AUD, allowedEmails: new Set([EMAIL]), fetchImpl: failing,
  });
  const result = await v.verify(makeToken());
  assert(!result.ok && result.reason === 'unknown_kid',
    'if the keys cannot be fetched the token must be REFUSED, never admitted; got '
    + JSON.stringify(result));
}

const TESTS: [string, () => void | Promise<void>][] = [
  ['a valid token is accepted (the null for everything below)', testValidTokenIsAccepted],
  ['missing and malformed assertions are refused', testMissingAndMalformed],
  ['the algorithm is never taken from the token', testAlgorithmIsNotTakenFromTheToken],
  ['a signature from the wrong key is refused', testForgedSignature],
  ['a payload swapped after signing is refused', testTamperedPayload],
  ['an unknown key id is refused', testUnknownKid],
  ['another team is refused', testWrongIssuer],
  ['another application in the SAME team is refused', testWrongAudience],
  ['expiry and not-before are enforced, with clock skew', testExpiryAndNotBefore],
  ['the email allow list is enforced and empty means deny', testEmailAllowList],
  ['the allow list parses as documented', testParseAllowedEmails],
  ['the gate is required whenever it could be reached', testGateIsRequiredWheneverItCouldBeReached],
  ['an incomplete Access config fails CLOSED', testMisconfigurationFailsClosed],
  ['signing keys are cached', testKeysAreCachedButRefreshable],
  ['a JWKS fetch failure refuses rather than admits', testNetworkFailureDoesNotAdmit],
];

async function main(): Promise<void> {
  let failed = 0;
  for (const [name, fn] of TESTS) {
    try {
      await fn();
      console.log('  ok   ' + name);
    } catch (err) {
      failed += 1;
      console.error('  FAIL ' + name + '\n       ' + (err as Error).message);
    }
  }
  console.log(failed === 0
    ? String(TESTS.length) + ' access tests passed'
    : String(failed) + ' of ' + String(TESTS.length) + ' access tests FAILED');
  if (failed > 0) process.exitCode = 1;
}

void main();
