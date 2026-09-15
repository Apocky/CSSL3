// What the sign-in box accepts.
//
// Reported 2026-09-14: "the sign in email gives a magic link not the code that the webpage asks
// for". Whether a code appears at all is decided by the mail template, so the page cannot demand
// one. And in the app the link is worse than useless: tapping it opens the system browser, the
// session is established THERE, and the app stays signed out.
//
// So the box takes either. These cases pin that, and the input constraints that would silently
// defeat it.

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { credentialFromInput } from '../../lib/auth-credential';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

const CASES: ReadonlyArray<readonly [string, 'code' | 'link' | null, string | null]> = [
  ['123456', 'code', '123456'],
  ['  654321  ', 'code', '654321'],
  ['not-a-url-but-some-token', 'code', 'not-a-url-but-some-token'],
  ['https://www.apocky.com/auth/callback?token_hash=abc123&type=email', 'link', 'abc123'],
  ['https://ref.supabase.co/auth/v1/verify?token=deadbeef&type=magiclink', 'link', 'deadbeef'],
  ['https://www.apocky.com/auth/callback#token_hash=fragmented', 'link', 'fragmented'],
  ['HTTPS://WWW.APOCKY.COM/auth/callback?token_hash=upper', 'link', 'upper'],
  // A link whose fragment already carries a session belongs to the callback page. Returning null
  // lets the caller say that, rather than reporting "invalid code" for something never a code.
  ['https://www.apocky.com/auth/callback#access_token=xyz&refresh_token=abc', null, null],
  ['https://www.apocky.com/auth/callback', null, null],
  ['', null, null],
  ['   ', null, null],
];

function main(): void {
  for (const [input, kind, token] of CASES) {
    const result = credentialFromInput(input);
    const actual = result?.kind ?? null;
    assert(actual === kind, `"${input.slice(0, 44)}" gave ${String(actual)}, expected ${String(kind)}`);
    if (token !== null) {
      assert(result?.token === token, `"${input.slice(0, 44)}" token was ${String(result?.token)}, expected ${token}`);
    }
  }

  // A six-digit code and a link must not be confused for one another.
  assert(credentialFromInput('123456')?.kind === 'code', 'a plain code was read as a link');
  assert(credentialFromInput('https://x/y?token=123456')?.kind === 'link', 'a link was read as a code');

  // The input itself must not reject a pasted URL before the parser ever runs. A numeric pattern
  // or a numeric keypad silently defeats the whole paste path, which is how this was first broken.
  const login = readFileSync(resolve(process.cwd(), 'pages/login.tsx'), 'utf8');
  const codeField = login.slice(login.indexOf('id="login-code"'), login.indexOf('login-code-help'));
  assert(!/pattern="\[0-9\]/.test(codeField), 'a numeric pattern would reject a pasted sign-in link');
  assert(!/inputMode="numeric"/.test(codeField), 'a numeric keypad makes the paste path impractical');
  assert(/paste/i.test(login), 'the page never tells anyone they may paste the link');

  console.log(`auth-credential.test: ${CASES.length} inputs, code vs link separated, `
    + 'session-carrying links deferred to the callback, and the field does not reject a pasted link');
}

try {
  main();
  console.log('auth-credential OK');
} catch (error) {
  console.error(error);
  process.exitCode = 1;
}
