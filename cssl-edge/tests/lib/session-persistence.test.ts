// Staying signed in across app restarts.
//
// The reported symptom: "every time I close the app I have to sign back in."
//
// The cause is an asymmetry. The HttpOnly cookie mirror lives at most as long as the access token
// inside it -- MAX_SESSION_SECONDS is one hour -- while the browser client keeps a refresh token
// and can mint new access tokens for weeks. So an hour after you close the app, the cookie is gone
// and the server reports signed-out, even though the client is still perfectly able to prove who
// you are. Nothing was wrong with the credentials; the mirror had simply expired and nobody
// re-silvered it.
//
// Two moments matter, and both are asserted here: app boot, and the app coming back to the
// foreground. Missing either one puts the sign-in screen in front of someone who never signed out.

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (path: string): string => readFileSync(resolve(process.cwd(), path), 'utf8');
const auth = read('lib/auth.ts');
const session = read('components/hub/SiteSession.tsx');
const authSession = read('lib/auth-session.ts');

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

function main(): void {
  // 1 -- the asymmetry that causes this is real and still present, so the repair must stay.
  //      If the cookie ever outlives the token this test should be revisited, not deleted.
  assert(/MAX_SESSION_SECONDS\s*=\s*60\s*\*\s*60/.test(authSession),
    'session cookie lifetime is no longer one hour -- re-check whether boot repair is still needed');

  // 2 -- recovery asks the client for a session rather than trusting a token handed to it.
  //      getSession() refreshes an expired access token; the token delivered to an auth-state
  //      callback can already be stale, and posting a stale token is rejected -- which is exactly
  //      how the cookie failed to come back.
  assert(/export async function reestablishSessionCookie/.test(auth),
    'no boot-time session recovery exists');
  const body = auth.slice(auth.indexOf('export async function reestablishSessionCookie'));
  assert(/auth\.getSession\(\)/.test(body.slice(0, 700)),
    'recovery must call getSession(), which refreshes, not reuse a possibly-stale token');
  assert(/persistSessionToCookie\(/.test(body.slice(0, 700)),
    'recovery must re-mint the cookie');

  // 3 -- BOOT: the server saying "signed out" is a statement about the cookie, not about the
  //      person. Recovery must be attempted before that answer reaches the UI.
  assert(/reestablishSessionCookie/.test(session), 'boot never attempts session recovery');
  const boot = session.slice(session.indexOf('const first = await resolveSiteAccess()'));
  assert(boot.length > 0, 'the boot path no longer resolves access first');
  const beforeSet = boot.slice(0, boot.indexOf('setSession(restored'));
  assert(/reestablishSessionCookie\(\)/.test(beforeSet),
    'boot declares a signed-out session before trying to recover it');

  // 4 -- FOREGROUND: reopening the app is the exact reported moment. A hidden tab does not
  //      reliably run the client refresh timer, so returning to the foreground must re-mint.
  assert(/visibilitychange/.test(session), 'no foreground listener: reopening the app will not recover');
  assert(/addEventListener\('focus'/.test(session), 'no focus listener for shells that do not fire visibilitychange');
  const visible = session.slice(session.indexOf('visibilitychange') - 600);
  assert(/reestablishSessionCookie/.test(visible.slice(0, 900)),
    'the foreground listener does not re-establish the cookie');

  // 5 -- an authenticated answer must NOT be second-guessed: recovery runs only when the server
  //      reported no session. Re-minting on every boot would add a request to every page load.
  assert(/first\.access === 'member' \|\| first\.access === 'owner'/.test(session),
    'recovery is not gated on the server having reported no session');

  // 6 -- the refresh token still never enters a cookie. Fixing persistence must not quietly widen
  //      what the browser stores.
  assert(/sb-refresh-token=; Path=\/; Max-Age=0/.test(authSession),
    'the refresh token must still be cleared from cookies');
  assert(!/refresh[_-]?token=\$\{/i.test(authSession),
    'a refresh token is being written into a cookie');

  console.log('session-persistence.test: boot recovery before signed-out, foreground recovery, '
    + 'getSession() not a stale callback token, authenticated answers untouched, '
    + 'refresh token still never cookied');
}

try {
  main();
  console.log('session-persistence OK');
} catch (error) {
  console.error(error);
  process.exitCode = 1;
}
