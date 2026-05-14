// cssl-edge · tests/lib/auth-redirect.test.ts
// Plain tsx self-test for trusted auth redirect resolution.

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
  assertEqual('admin return preserved', normalizeAuthReturnPath('/admin/chat'), '/admin/chat');
  assertEqual('legacy chat normalized', normalizeAuthReturnPath('/chat'), '/admin/chat');
  assertEqual('legacy chat query normalized', normalizeAuthReturnPath('/chat?x=1'), '/admin/chat?x=1');
  assertEqual('external return rejected', normalizeAuthReturnPath('https://evil.example/admin/chat'), '/account');
  assertEqual('callback loop rejected', normalizeAuthReturnPath('/auth/callback?next=/admin/chat'), '/account');
  assertEqual('login href includes next', loginHrefForReturnPath('/admin/chat'), '/login?next=%2Fadmin%2Fchat');
  assertEqual(
    'callback URL carries safe next',
    buildAuthCallbackUrl('https://www.apocky.com', '/admin/chat'),
    'https://www.apocky.com/auth/callback?next=%2Fadmin%2Fchat',
  );
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
  // eslint-disable-next-line no-console
  console.log('auth-redirect.test : OK');
}