// What a person can paste into the sign-in box — and whether the box will hold it.
//
// Apocky: "I can't paste the magic link into the code field because it doesn't fit."
//
// The parser below already accepted pasted links, the label already offered "Code or sign-in link",
// and the help text already said "paste it here". All three were correct. The input carried
// maxLength={8}, so the browser truncated every pasted link to eight characters before any of that
// code ran, and the reader got a rejection that blamed them for a limit they could not see.
//
// That is why the bound is asserted against the markup here and not just eyeballed: a parser and
// the field that feeds it can disagree silently, and did.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { credentialFromInput, EMAIL_CREDENTIAL_MAX_LENGTH } from '@/lib/auth-credential';

const login = readFileSync(resolve(process.cwd(), 'pages/login.tsx'), 'utf8');
const register = readFileSync(resolve(process.cwd(), 'pages/register.tsx'), 'utf8');

// A Supabase magic link, in the shapes it actually arrives in.
const QUERY_LINK = 'https://pzirbmyfmrbtkllrtcmx.supabase.co/auth/v1/verify?token_hash=pkce_8f2c1d4a9b7e6f3c0d5a2b8e4f1c7a9d&type=magiclink&redirect_to=https%3A%2F%2Fwww.apocky.com%2Fauth%2Fcallback%3Fnext%3D%252Fapocrypha';
const HASH_LINK = 'https://www.apocky.com/auth/callback#token_hash=3c9e7a1f5d2b8c4e0a6f9d3b7e1c5a8f&type=email';
const LEGACY_LINK = 'https://pzirbmyfmrbtkllrtcmx.supabase.co/auth/v1/verify?token=abc123def456&type=magiclink';

// ── the field must physically hold what the parser accepts ────────────────────────────────────
//
// Scoped to the ELEMENT, not the file: the first draft of this check matched `maxLength={8}` inside
// the comment that explains the bug, and failed on prose. A guard that reads commentary is not
// reading the thing it guards.
const opensAt = login.indexOf('id="login-code"');
assert.ok(opensAt > 0, 'the pasteable sign-in field must still exist');
const element = login.slice(opensAt, login.indexOf('/>', opensAt));
const cap = /maxLength=\{([^}]+)\}/u.exec(element)?.[1]?.trim();
assert.equal(
  cap,
  'EMAIL_CREDENTIAL_MAX_LENGTH',
  'the field that accepts a pasted link must take its bound from the parser, not a literal',
);
assert.ok(
  EMAIL_CREDENTIAL_MAX_LENGTH >= QUERY_LINK.length,
  `the bound (${EMAIL_CREDENTIAL_MAX_LENGTH}) must hold a real magic link (${QUERY_LINK.length} chars)`,
);
assert.ok(
  element.includes('placeholder="000000 or paste the link"'),
  'the field still advertises that a link may be pasted, so the bound above must stay honest',
);
// A numeric literal here is the exact regression: it is how the cap and the parser drifted apart.
assert.ok(
  !/maxLength=\{\d+\}/u.test(element),
  'a literal cap on this field is how the original truncation happened',
);

// The SAME checks against register, because there are two of these fields and only one was
// guarded. /login was fixed for the pasted-link defect; /register kept maxLength={8} AND a
// numeric `pattern` — which blocks submit in the browser before any handler runs, so a link was
// refused even with the cap raised. A guard covering one of two identical fields is how the
// second one survived a full commit cycle.
const FIELDS: ReadonlyArray<readonly [string, string, string]> = [
  ['login', login, 'login-code'],
  ['register', register, 'register-code'],
];
for (const [name, source, id] of FIELDS) {
  const at = source.indexOf(`id="${id}"`);
  assert.ok(at > 0, `${name}: the pasteable field must exist`);
  const element = source.slice(at, source.indexOf('/>', at));
  assert.equal(
    /maxLength=\{([^}]+)\}/u.exec(element)?.[1]?.trim(),
    'EMAIL_CREDENTIAL_MAX_LENGTH',
    `${name}: the field must take its bound from the parser, not a literal`,
  );
  assert.ok(!/maxLength=\{\d+\}/u.test(element), `${name}: a literal cap is the original truncation`);
  assert.ok(!/pattern=/u.test(element), `${name}: a pattern attribute silently refuses a pasted link before any handler runs`);
  assert.ok(!/inputMode="numeric"/u.test(element), `${name}: a numeric keypad makes a link impossible to type and awkward to paste`);
  assert.ok(element.includes('paste the link'), `${name}: the field must advertise what it accepts`);
  assert.ok(source.includes('credentialFromInput'), `${name}: the pasted value must reach the parser`);
}

// ── codes stay codes ──────────────────────────────────────────────────────────────────────────
assert.deepEqual(credentialFromInput('123456'), { kind: 'code', token: '123456' }, 'a plain code is a code');
assert.deepEqual(credentialFromInput('  123456  '), { kind: 'code', token: '123456' }, 'surrounding whitespace is forgiven');
assert.deepEqual(
  credentialFromInput('ABC-123'),
  { kind: 'code', token: 'ABC-123' },
  'an unfamiliar code shape is passed through rather than guessed at',
);

// ── links yield their token ───────────────────────────────────────────────────────────────────
assert.deepEqual(
  credentialFromInput(QUERY_LINK),
  { kind: 'link', token: 'pkce_8f2c1d4a9b7e6f3c0d5a2b8e4f1c7a9d' },
  'a link carrying token_hash in the query resolves to that token',
);
assert.deepEqual(
  credentialFromInput(HASH_LINK),
  { kind: 'link', token: '3c9e7a1f5d2b8c4e0a6f9d3b7e1c5a8f' },
  'a link carrying token_hash in the fragment resolves to that token',
);
assert.deepEqual(
  credentialFromInput(LEGACY_LINK),
  { kind: 'link', token: 'abc123def456' },
  'the older token= shape still resolves',
);
assert.deepEqual(
  credentialFromInput(`  ${QUERY_LINK}\n`),
  { kind: 'link', token: 'pkce_8f2c1d4a9b7e6f3c0d5a2b8e4f1c7a9d' },
  'a link copied with trailing whitespace still resolves — copying from an email adds it',
);

// ── a link with nothing to verify says so, rather than failing as a bad code ───────────────────
assert.equal(credentialFromInput('https://www.apocky.com/apocrypha'), null, 'a link with no token is not a credential');
assert.equal(credentialFromInput(''), null, 'empty input is not a credential');
assert.equal(credentialFromInput('   '), null, 'whitespace is not a credential');
assert.equal(credentialFromInput('https://'), null, 'an unparseable URL is not a credential');
// A link whose fragment already carries a session belongs to the callback page, not to this field.
assert.equal(
  credentialFromInput('https://www.apocky.com/auth/callback#access_token=xyz&refresh_token=abc'),
  null,
  'an already-established session is not a token to verify here',
);

console.log('auth-credential.test : OK · both sign-in fields accept a pasted link and resolve it');
