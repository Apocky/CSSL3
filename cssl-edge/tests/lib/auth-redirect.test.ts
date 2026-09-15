// cssl-edge · tests/lib/auth-redirect.test.ts
// Plain tsx self-test for trusted auth redirect resolution.

import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

import { resolveAuthRedirect } from '@/lib/auth';
import { readAuthCallbackParams } from '@/lib/auth-callback';
import { buildAuthCallbackUrl, loginHrefForReturnPath, normalizeAuthReturnPath } from '@/lib/auth-return';

function assertEqual(name: string, actual: string, expected: string): void {
  if (actual !== expected) {
    throw new Error(`${name}: expected ${expected}, got ${actual}`);
  }
}

function assertRejected(name: string, actual: string): void {
  assertEqual(name, actual, 'https://www.apocky.com/account');
}

export function testProductionRedirects(): void {
  const headers = { host: 'www.apocky.com', 'x-forwarded-proto': 'https' };
  assertEqual(
    'same-origin callback',
    resolveAuthRedirect('https://www.apocky.com/auth/callback', headers),
    'https://www.apocky.com/auth/callback',
  );
  assertEqual('relative callback', resolveAuthRedirect('/auth/callback', headers), 'https://www.apocky.com/auth/callback');
  assertRejected('external host rejected', resolveAuthRedirect('https://example.com/auth/callback', headers));
  assertRejected('lookalike host rejected', resolveAuthRedirect('https://apocky.com.example.com/auth/callback', headers));
}

export function testPreviewAndLocalhostRedirects(): void {
  const previewHeaders = {
    host: 'apocky-i7x34808c-shawn-bakers-projects-cb1c9715.vercel.app',
    'x-forwarded-proto': 'https',
  };
  assertEqual(
    'preview same-host callback',
    resolveAuthRedirect('https://apocky-i7x34808c-shawn-bakers-projects-cb1c9715.vercel.app/auth/callback', previewHeaders),
    'https://apocky-i7x34808c-shawn-bakers-projects-cb1c9715.vercel.app/auth/callback',
  );

  const localHeaders = { host: 'localhost:3000', 'x-forwarded-proto': 'http' };
  assertEqual(
    'localhost callback',
    resolveAuthRedirect('http://localhost:3000/auth/callback', localHeaders),
    'http://localhost:3000/auth/callback',
  );
}

export function testAuthCallbackParamParsing(): void {
  const pkce = readAuthCallbackParams('?code=abc-123&state=xyz', '');
  if (!pkce.hasCallback || pkce.code !== 'abc-123') {
    throw new Error('PKCE callback query was not detected');
  }

  const implicit = readAuthCallbackParams('', '#access_token=a&refresh_token=r&expires_in=3600');
  if (!implicit.hasCallback || implicit.accessToken !== 'a' || implicit.refreshToken !== 'r') {
    throw new Error('implicit callback hash was not detected');
  }

  const plain = readAuthCallbackParams('?x=1', '#section');
  if (plain.hasCallback) {
    throw new Error('non-auth URL was incorrectly detected as callback');
  }
}

export function testAuthReturnPathNormalization(): void {
  assertEqual('available admin return preserved', normalizeAuthReturnPath('/admin'), '/admin');
  assertEqual('primary private route preserved', normalizeAuthReturnPath('/apocrypha'), '/apocrypha');
  // These three used to assert the DEFECT. /admin/chat returns 200 and /chat 308s to /apocrypha,
  // an explicitly allowed return target — so "rejecting" them meant signing in from an admin
  // console silently dropped the owner on /account instead of the page they were looking at.
  assertEqual('live admin console preserved', normalizeAuthReturnPath('/admin/chat'), '/admin/chat');
  assertEqual('live admin diagnostics preserved', normalizeAuthReturnPath('/admin/diagnostics'), '/admin/diagnostics');
  assertEqual('chat alias preserved', normalizeAuthReturnPath('/chat'), '/chat');
  assertEqual('chat alias keeps its query', normalizeAuthReturnPath('/chat?x=1'), '/chat?x=1');
  // Genuinely retired by middleware.ts, and still rejected.
  assertEqual('retired apoc rejected', normalizeAuthReturnPath('/apoc'), '/account');
  assertEqual('retired apx rejected', normalizeAuthReturnPath('/apx'), '/account');
  assertEqual('retired prefix rejected', normalizeAuthReturnPath('/apocrypha/session/abc'), '/account');
  assertEqual('external return rejected', normalizeAuthReturnPath('https://evil.example/admin/chat'), '/account');
  assertEqual('callback loop rejected', normalizeAuthReturnPath('/auth/callback?next=/admin/chat'), '/account');
  assertEqual('login href includes available next', loginHrefForReturnPath('/admin'), '/login?next=%2Fadmin');
  assertEqual('login href includes primary private next', loginHrefForReturnPath('/apocrypha'), '/login?next=%2Fapocrypha');
  assertEqual('login href keeps a live console', loginHrefForReturnPath('/admin/chat'), '/login?next=%2Fadmin%2Fchat');
  // Re-pointed at a path that is ACTUALLY retired, so this case can still fail. Against
  // /admin/chat it passed vacuously — it agreed with the bug.
  assertEqual('login href recovers a dead next', loginHrefForReturnPath('/apoc'), '/login?next=%2Faccount');
  assertEqual(
    'callback URL carries safe next',
    buildAuthCallbackUrl('https://www.apocky.com', '/admin'),
    'https://www.apocky.com/auth/callback?next=%2Fadmin',
  );
  assertEqual(
    'callback URL carries a live console',
    buildAuthCallbackUrl('https://www.apocky.com', '/admin/chat'),
    'https://www.apocky.com/auth/callback?next=%2Fadmin%2Fchat',
  );
  assertEqual(
    'callback URL drops a genuinely dead next',
    buildAuthCallbackUrl('https://www.apocky.com', '/apx'),
    'https://www.apocky.com/auth/callback',
  );
}

/**
 * Every path excluded from auth-return must actually be dead.
 *
 * The list drifted because its comment asserted "retired by middleware" and nothing checked it.
 * Middleware membership is the wrong invariant anyway — /chat/ and /admin/apocrypha/ are correctly
 * excluded and appear in no middleware list. What matters is that no page file resolves them.
 */
export function testRetiredReturnPathsHaveNoPage(): void {
  const pagesDir = resolve(process.cwd(), 'pages');
  const resolves = (pathname: string): boolean => {
    const rel = pathname.replace(/^\//u, '').replace(/\/$/u, '');
    if (rel === '') return true;
    return existsSync(resolve(pagesDir, `${rel}.tsx`))
      || existsSync(resolve(pagesDir, rel, 'index.tsx'));
  };
  for (const pathname of ['/apoc', '/apx']) {
    if (resolves(pathname)) {
      throw new Error(`assert failed : ${pathname} is excluded from auth-return but pages/ resolves it`);
    }
  }
  // And the inverse drift: a console that DOES resolve must not be excluded.
  for (const pathname of ['/admin/chat', '/admin/diagnostics', '/admin/controls', '/admin/tools']) {
    if (!resolves(pathname)) continue;
    assertEqual(`live page kept as a return target: ${pathname}`, normalizeAuthReturnPath(pathname), pathname);
  }
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  testProductionRedirects();
  testPreviewAndLocalhostRedirects();
  testAuthCallbackParamParsing();
  testAuthReturnPathNormalization();
  testRetiredReturnPathsHaveNoPage();
  // eslint-disable-next-line no-console
  // A fragment has to survive the round trip, or "sign in and I will take you back to the
// authenticator" quietly lands on the top of the account page instead — the panel is further down,
// and the reader concludes the QR does not exist. The normaliser keeps url.hash; this pins it.
assertEqual(
  'return path keeps its fragment',
  normalizeAuthReturnPath('/account#authenticator'),
  '/account#authenticator',
);
assertEqual(
  'login href round-trips the fragment',
  loginHrefForReturnPath('/account#authenticator'),
  '/login?next=%2Faccount%23authenticator',
);
// A protocol-relative host with a fragment tacked on must fall back, not be read as a path.
assertEqual(
  'a fragment cannot smuggle an offsite return',
  normalizeAuthReturnPath('//evil.example#/account'),
  '/account',
);

console.log('auth-redirect.test : OK');
}
