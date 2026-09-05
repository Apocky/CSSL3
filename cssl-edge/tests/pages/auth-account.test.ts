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
const callbackLib = source('lib/auth-callback.ts');
const account = source('pages/account.tsx');
const auth = source('lib/auth.ts');

for (const [name, page] of Object.entries({ login, register })) {
  assert(page.includes('client.auth.signInWithOtp'), `${name} sends OTP through the existing browser client`);
  assert(page.includes('client.auth.verifyOtp'), `${name} verifies a user-entered email code`);
  assert(page.indexOf('if (!authAttempt)') < page.indexOf('client.auth.verifyOtp'), `${name} refuses to mutate the provider session after its fresh-attempt proof is gone`);
  assert(page.includes("type: 'email'"), `${name} uses the email OTP verification type`);
  assert(page.includes("beginAuthenticationAttempt('fresh')"), `${name} binds provider work to the current logout fence before authentication`);
  assert(page.includes('persistSessionToCookie(verified.accessToken'), `${name} mirrors only the retained freshly verified token`);
  assert(page.includes("mirrored.status !== 'established'"), `${name} distinguishes a rejected mirror from an uncertain server commit`);
  assert(page.indexOf('await lockMiniBrainForSignedOutSession()') < page.indexOf('setServerSessionPending(true)'), `${name} locks the private Brain before offering a verified-session retry`);
  assert(page.includes('AUTH_SESSION_COMMIT_UNCERTAIN'), `${name} exposes a potentially committed server handoff accurately`);
  assert(page.includes('AUTH_SESSION_COMMIT_AND_BRAIN_LOCK_UNCONFIRMED') && page.includes('MINI_BRAIN_LOCK_DURABILITY_UNCONFIRMED'), `${name} preserves both exact codes at the irreducible double-failure boundary`);
  assert(page.includes('authAttempt: verified.authAttempt'), `${name} binds the cookie mirror to that exact authentication attempt`);
  assert(page.includes('stageMiniBrainOwnerRebindAfterReauthentication(verified.subjectKey, verified.authAttempt)'), `${name} binds the encrypted Brain handoff to the verified subject and auth attempt`);
  assert(!page.includes('if (!staged)'), `${name} does not trap a valid non-owner member behind the owner-only Brain rebind`);
  assert(page.includes("brainStage.status === 'durability_unconfirmed'"), `${name} distinguishes a denied owner rebind from unverified lock durability`);
  assert(page.includes('closeAuthenticationAfterPrivateLockFailure()'), `${name} closes the new site session when no durable Brain lock can be proven`);
  assert(page.includes('MINI_BRAIN_LOCK_DURABILITY_UNCONFIRMED'), `${name} exposes the exact private-storage failure`);
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
assert(login.includes('client.auth.getSession()'), 'sign-in can detect a refreshable browser session after the server mirror expires');
assert(login.includes('client.auth.refreshSession()'), 'sign-in explicitly renews the browser session before rebuilding the server mirror');
assert(login.includes('persistSessionToCookie(refreshed.data.session.access_token, { authAttempt: refreshAttempt })'), 'sign-in remirrors the renewed session with a refresh-bound fence ticket');
assert(login.includes('AUTO_RESUME_GUARD_MS = 60_000'), 'automatic server-session repair has a bounded redirect-loop guard');
assert(login.includes('location.replace(destination)'), 'automatic server-session repair returns to the normalized destination');
assert((login.match(/await stageMiniBrainOwnerRebindAfterReauthentication\(verified\.subjectKey, verified\.authAttempt\)/g) ?? []).length === 1, 'only the retained freshly verified subject and attempt can stage owner-only proof for a durable local Brain rebind');
assert(!login.slice(login.indexOf('client.auth.refreshSession()'), login.indexOf('function currentReturnPath')).includes('stageMiniBrainOwnerRebindAfterReauthentication'), 'saved-session auto-resume never stages a durable local Brain rebind');
assert(register.includes('shouldCreateUser: true'), 'registration explicitly permits account creation');
assert(register.includes('htmlFor="register-agreement"'), 'registration agreement has an associated label');

assert(callback.includes('<AuthFrame mode="callback">'), 'callback uses the shared authentication frame');
assert(callback.includes('normalizeAuthReturnPath'), 'callback normalizes its return destination');
assert(callback.includes('processCallback(true)'), 'callback exposes a real retry path');
assert(callback.includes('client.auth.getSession()'), 'callback retry can resume a browser session after cookie-mirror failure');
assert(callback.includes('protectMirrorFailure(mirrored.status)'), 'callback locks private presentation after a failed or uncertain fresh mirror before retaining retry');
assert(callback.includes('result.freshSession && result.mirrorStatus'), 'first-pass callback mirror uncertainty takes the same private lock path as retry');
assert(callback.includes('result.providerSessionUncertain'), 'callback routes a non-definitive provider mutation through explicit protection');
assert(callback.includes('result.providerSessionAuthAttempt'), 'callback binds uncertain cleanup to the exact originating auth attempt');
assert(callback.includes('closeAuthenticationAttemptAfterPrivateLockFailure'), 'callback atomically supersedes stale cleanup before Brain or session mutation');
assert(callback.includes('AUTH_CALLBACK_SUPERSEDED'), 'callback exposes a stable superseded-attempt state');
assert(callback.includes('closeUncertainProviderSession('), 'callback locks private presentation and closes unsettled provider authentication');
assert(callback.includes('AUTH_PROVIDER_SESSION_UNCERTAIN'), 'callback exposes a stable provider uncertainty code');
assert(callback.includes('authAttempt: expected.authAttempt'), 'fresh callback retry remains bound to the original authentication attempt');
assert(callback.includes('retry && freshCallbackSessionRef.current'), 'callback retry only resumes a session created by this live callback attempt');
assert(callback.includes('freshCallbackSessionRef.current.authAttempt'), 'callback completion binds owner-only Brain proof to the exact verified attempt');
assert(!callback.includes('if (!staged)'), 'callback does not trap a valid non-owner member behind the owner-only Brain rebind');
assert(callback.includes("brainStage.status === 'durability_unconfirmed'"), 'callback distinguishes owner denial from unverified lock durability');
assert(callback.includes('closeAuthenticationAfterPrivateLockFailure()'), 'callback closes an authentication session whose private lock cannot be proven');
assert(callback.includes('MINI_BRAIN_LOCK_DURABILITY_UNCONFIRMED'), 'callback exposes the exact private-storage failure');
assert(callbackLib.includes("currentAuthenticationAttempt('fresh')"), 'callback captures its initiating logout-fence ticket before consuming provider credentials');
assert(callbackLib.includes('authAttempt,'), 'callback retains the exact ticket for deterministic mirror retry');
assert(callbackLib.includes('mirrorStatus: mirrored.status'), 'callback transports the exact mirror outcome without collapsing commit uncertainty');
assert(callbackLib.includes('providerSessionUncertain: providerMutationDispatched'), 'a timed-out or non-definitive provider mutation remains typed for caller cleanup');
assert(callbackLib.includes('providerSessionAuthAttempt:'), 'provider uncertainty carries its originating fresh-auth ticket');
assert(callback.includes('aria-live='), 'callback status is announced');
for (const leakedFragment of ['debugInfo', 'code.slice(', 'access_token present', 'location.hash']) {
  assert(!callback.includes(leakedFragment), `callback does not render or inspect auth fragment detail: ${leakedFragment}`);
}

assert(account.includes("client.auth.signOut({ scope: 'local' })"), 'account clears the browser Supabase session');
assert(account.includes("fetch('/api/auth/logout'"), 'account clears the server HttpOnly session mirror');
assert(account.includes('withBrowserAuthSignOut'), 'account serializes browser-wide cookie mutations before completing server logout');
assert(account.includes('await lockMiniBrainForSignedOutSession()'), 'sign-out locks the encrypted local Brain before leaving the owner route');
assert(account.includes("localProtectionCleared = result.status === 'locked'"), 'account reports sign-out success only when a durable Brain lock is confirmed');
assert(account.includes("credentials: 'same-origin'"), 'account session requests stay same-origin');
assert(account.includes('Identity and boundaries'), 'account states its narrow purpose');
assert(account.includes('Current identity'), 'account identifies the session information it actually reports');
assert(account.includes('does not currently offer') && account.includes('provider-management controls'), 'provider boundary is explicit');
assert(account.includes('does not currently perform export or deletion'), 'data-operation boundary is explicit');
assert(account.includes('mailto:apocky13@gmail.com?subject=%5Bprivacy%5D'), 'account provides a real privacy-request path');
assert(account.includes('aria-live="polite"'), 'account loading status is announced');
for (const removedControl of [
  'Local-only profile drafts',
  'Export account data — unavailable',
  'Delete account — unavailable',
  'Status: unavailable.',
]) {
  assert(!account.includes(removedControl), `account omits disconnected control: ${removedControl}`);
}
assert(account.includes('href="/docs/sovereignty"'), 'account links to an existing sovereignty route');
assert(!account.includes('apx-skip-link'), 'account relies on the enclosing SiteShell skip link');
for (const falseClaim of ['No purchases yet', '14-day window', 'alert(', 'confirm(']) {
  assert(!account.includes(falseClaim), `account omits nonfunctional or unsupported claim: ${falseClaim}`);
}

assert(auth.includes("status: 'established'") && auth.includes("status: 'not_established'") && auth.includes("status: 'commit_uncertain'"), 'browser-to-server session mirroring has a three-state commit contract');
assert(auth.includes('let requestDispatched = false'), 'session mirroring tracks whether a network failure may have followed a server commit');
assert(auth.includes('return requestDispatched ? SESSION_COMMIT_UNCERTAIN : SESSION_NOT_ESTABLISHED'), 'post-dispatch failures never masquerade as proven non-commit');

for (const [name, page] of Object.entries({ login, register, callback, account })) {
  assert(page.includes('<meta name="robots" content="noindex,nofollow" />'), `${name} is noindex,nofollow`);
}

// eslint-disable-next-line no-console
console.log('auth-account.test : OK · OTP, callback, session, accessibility, and truthful account boundaries passed');
