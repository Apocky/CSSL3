import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

function source(path: string): string {
  return readFileSync(resolve(process.cwd(), path), 'utf8');
}

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

const login = source('pages/login.tsx');
const register = source('pages/register.tsx');
const callback = source('pages/auth/callback.tsx');
const account = source('pages/account.tsx');

for (const [name, page] of Object.entries({ login, register })) {
  assert(page.includes('client.auth.signInWithOtp'), `${name} sends OTP through the existing browser client`);
  assert(page.includes('client.auth.verifyOtp'), `${name} verifies a user-entered email code`);
  assert(page.includes("type: 'email'"), `${name} uses the email OTP verification type`);
  assert(page.includes('persistSessionToCookie(accessToken)'), `${name} preserves the server cookie mirror boundary`);
  assert(page.includes('location.replace(currentReturnPath())'), `${name} returns only through the normalized next path`);
  assert(page.includes('Resend email'), `${name} exposes resend state without assuming the remote email template`);
  assert(page.includes('RESEND_COOLDOWN_SECONDS = 30'), `${name} rate-limits resend interaction`);
  assert(page.includes('resendCooldown > 0'), `${name} disables resend during its cooldown`);
  assert(page.includes('Change email'), `${name} exposes change-email state`);
  assert(page.includes('autoComplete="one-time-code"'), `${name} identifies the OTP field accessibly`);
  assert(page.includes('aria-live='), `${name} announces send and verification status`);
  assert(!page.includes('/api/auth/magic-link'), `${name} does not fall back to the server-created auth client`);
  assert(!page.includes('createClient('), `${name} reuses the configured PKCE browser client`);
}

assert(login.includes('shouldCreateUser: false'), 'sign-in does not silently register an unknown email');
assert(register.includes('shouldCreateUser: true'), 'registration explicitly permits account creation');
assert(register.includes('htmlFor="register-agreement"'), 'registration agreement has an associated label');

assert(callback.includes('<AuthFrame mode="callback">'), 'callback uses the shared authentication frame');
assert(callback.includes('normalizeAuthReturnPath'), 'callback normalizes its return destination');
assert(callback.includes('processCallback(true)'), 'callback exposes a real retry path');
assert(callback.includes('client.auth.getSession()'), 'callback retry can resume a browser session after cookie-mirror failure');
assert(callback.includes('persistSessionToCookie(data.session.access_token)'), 'callback retry preserves the cookie boundary');
assert(callback.includes('aria-live='), 'callback status is announced');
for (const leakedFragment of ['debugInfo', 'code.slice(', 'access_token present', 'location.hash']) {
  assert(!callback.includes(leakedFragment), `callback does not render or inspect auth fragment detail: ${leakedFragment}`);
}

assert(account.includes("client.auth.signOut({ scope: 'local' })"), 'account clears the browser Supabase session');
assert(account.includes("fetch('/api/auth/logout'"), 'account clears the server HttpOnly session mirror');
assert(account.includes("credentials: 'same-origin'"), 'account session requests stay same-origin');
assert(account.includes('Local-only profile drafts'), 'account labels browser-only settings');
assert(account.includes('Nothing was uploaded or published.'), 'account save result is truthful about persistence');
assert(account.includes('htmlFor={inputId}') && account.includes('id={inputId}'), 'account settings inputs have associated labels');
assert(account.includes('aria-live={savedNotice'), 'account save status is announced');
assert(account.includes('Link or unlink providers — unavailable'), 'provider management is explicitly unavailable');
assert(account.includes('Export account data — unavailable'), 'data export is explicitly unavailable');
assert(account.includes('Delete account — unavailable'), 'account deletion is explicitly unavailable');
assert(account.includes('Status: unavailable.'), 'entitlements are not presented as loaded data');
assert(account.includes('href="/docs/sovereignty"'), 'account links to an existing sovereignty route');
assert(!account.includes('apx-skip-link'), 'account relies on the enclosing SiteShell skip link');
for (const falseClaim of ['No purchases yet', '14-day window', 'alert(', 'confirm(']) {
  assert(!account.includes(falseClaim), `account omits nonfunctional or unsupported claim: ${falseClaim}`);
}

for (const [name, page] of Object.entries({ login, register, callback, account })) {
  assert(page.includes('<meta name="robots" content="noindex,nofollow" />'), `${name} is noindex,nofollow`);
}

// eslint-disable-next-line no-console
console.log('auth-account.test : OK · OTP, callback, session, accessibility, and truthful account boundaries passed');
