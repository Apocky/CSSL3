import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { AuthFrame } from '../components/hub/AuthFrame';
import { AUTH_PROVIDERS, beginAuthenticationAttempt, closeAuthenticationAfterPrivateLockFailure, getAuthClient, persistSessionToCookie } from '../lib/auth';
import { lockMiniBrainForSignedOutSession, stageMiniBrainOwnerRebindAfterReauthentication } from '../lib/brain/mini-brain';
import { buildAuthCallbackUrl, normalizeAuthReturnPath } from '../lib/auth-return';

type Notice = {
  tone: 'info' | 'error' | 'warning';
  text: string;
};

type VerifiedSessionCandidate = {
  accessToken: string;
  subjectKey: string;
  authAttempt: string;
};

const RESEND_COOLDOWN_SECONDS = 30;
const AUTO_RESUME_STORAGE_KEY = 'apocky.auth.auto-resume.v1';
const AUTO_RESUME_GUARD_MS = 60_000;

const Login: NextPage = () => {
  const [email, setEmail] = useState('');
  const [pendingEmail, setPendingEmail] = useState<string | null>(null);
  const [otp, setOtp] = useState('');
  const [operation, setOperation] = useState<'send' | 'resend' | 'verify' | null>(null);
  const [serverSessionPending, setServerSessionPending] = useState(false);
  const [verifiedSessionCandidate, setVerifiedSessionCandidate] = useState<VerifiedSessionCandidate | null>(null);
  const [authAttempt, setAuthAttempt] = useState<string | null>(null);
  const [resendCooldown, setResendCooldown] = useState(0);
  const [oauthLoading, setOauthLoading] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [localhostCallback, setLocalhostCallback] = useState<string | null>(null);
  const [returnTo, setReturnTo] = useState('/account');

  useEffect(() => {
    const next = new URLSearchParams(location.search).get('next');
    const destination = normalizeAuthReturnPath(next);
    setReturnTo(destination);
    if (location.hostname === 'localhost') {
      setLocalhostCallback(`http://localhost:${location.port || 3000}/auth/callback`);
    }

    let guarded = false;
    try {
      const raw = sessionStorage.getItem(AUTO_RESUME_STORAGE_KEY);
      const prior = raw ? JSON.parse(raw) as { destination?: unknown; attempted_at?: unknown } : null;
      guarded = prior?.destination === destination
        && typeof prior.attempted_at === 'number'
        && Date.now() - prior.attempted_at < AUTO_RESUME_GUARD_MS;
    } catch { /* private mode can deny or corrupt session storage */ }
    if (guarded) return;

    const client = getAuthClient();
    if (!client) return;
    let cancelled = false;
    void (async () => {
      const current = await client.auth.getSession();
      if (cancelled || !current.data.session) return;
      try {
        sessionStorage.setItem(AUTO_RESUME_STORAGE_KEY, JSON.stringify({ destination, attempted_at: Date.now() }));
      } catch { /* the guard is best-effort; authentication remains explicit */ }
      setNotice({ tone: 'info', text: 'Restoring your secure server session…' });
      const refreshAttempt = await beginAuthenticationAttempt('refresh');
      if (!refreshAttempt) {
        setNotice({ tone: 'warning', text: 'This saved session cannot cross the current sign-out boundary. Sign in again to continue.' });
        return;
      }
      const refreshed = await client.auth.refreshSession();
      if (cancelled) return;
      if (refreshed.error || !refreshed.data.session) {
        setNotice({ tone: 'warning', text: 'Your saved browser session could not be renewed. Sign in again to continue.' });
        return;
      }
      const mirrored = await persistSessionToCookie(refreshed.data.session.access_token, { authAttempt: refreshAttempt });
      if (cancelled) return;
      if (mirrored.status !== 'established') {
        setNotice({ tone: 'warning', text: 'Your browser session is current, but the secure server session could not be restored. You can retry sign-in below.' });
        return;
      }
      location.replace(destination);
    })().catch(() => {
      if (!cancelled) setNotice({ tone: 'warning', text: 'Your saved session could not be restored. Sign in again to continue.' });
    });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    if (resendCooldown <= 0) return;
    const timer = window.setTimeout(() => {
      setResendCooldown((seconds) => Math.max(0, seconds - 1));
    }, 1000);
    return () => window.clearTimeout(timer);
  }, [resendCooldown]);

  function currentReturnPath(): string {
    if (typeof location === 'undefined') return '/account';
    return normalizeAuthReturnPath(new URLSearchParams(location.search).get('next'));
  }

  function callbackUrl(): string {
    return buildAuthCallbackUrl(location.origin, currentReturnPath());
  }

  async function sendEmailCode(address: string, kind: 'send' | 'resend'): Promise<void> {
    if (operation) return;
    if (kind === 'resend' && resendCooldown > 0) return;
    const normalizedEmail = address.trim();
    if (!normalizedEmail) return;
    setOperation(kind);
    setNotice(null);

    const client = getAuthClient();
    if (!client) {
      setNotice({ tone: 'warning', text: 'Email sign-in is not connected in this environment. No address was submitted.' });
      setOperation(null);
      return;
    }

    try {
      const freshAttempt = await beginAuthenticationAttempt('fresh');
      if (!freshAttempt) {
        setNotice({ tone: 'error', text: 'The secure sign-in boundary could not start. Retry before requesting a code.' });
        return;
      }
      const { error } = await client.auth.signInWithOtp({
        email: normalizedEmail,
        options: {
          emailRedirectTo: callbackUrl(),
          shouldCreateUser: false,
        },
      });
      if (error) {
        setNotice({ tone: 'error', text: 'We could not send a sign-in code. Check the address, wait a moment, and try again.' });
        return;
      }
      setPendingEmail(normalizedEmail);
      setOtp('');
      setServerSessionPending(false);
      setVerifiedSessionCandidate(null);
      setAuthAttempt(freshAttempt);
      setResendCooldown(RESEND_COOLDOWN_SECONDS);
      setNotice({
        tone: 'info',
        text: kind === 'resend'
          ? `A new sign-in email was sent to ${normalizedEmail}. Enter its code if shown, or use its secure link.`
          : `Check ${normalizedEmail}. Enter the one-time code if the email shows one, or use its secure link.`,
      });
    } catch {
      setNotice({ tone: 'error', text: 'The sign-in service could not be reached. Please try again.' });
    } finally {
      setOperation(null);
    }
  }

  async function handleEmailSubmit(event: React.FormEvent): Promise<void> {
    event.preventDefault();
    await sendEmailCode(email, 'send');
  }

  async function handleVerifyCode(event: React.FormEvent): Promise<void> {
    event.preventDefault();
    if (!pendingEmail || operation) return;
    setOperation('verify');
    setNotice(null);

    const client = getAuthClient();
    if (!client) {
      setNotice({ tone: 'warning', text: 'Email sign-in is not connected in this environment.' });
      setOperation(null);
      return;
    }

    try {
      let verified: VerifiedSessionCandidate | null = null;
      if (serverSessionPending) {
        const { data, error } = await client.auth.getSession();
        if (
          error
          || !data.session
          || !verifiedSessionCandidate
          || data.session.access_token !== verifiedSessionCandidate.accessToken
          || data.session.user.id !== verifiedSessionCandidate.subjectKey
        ) {
          setServerSessionPending(false);
          setVerifiedSessionCandidate(null);
          setNotice({ tone: 'error', text: 'The verified browser session is no longer available. Request a new code and try again.' });
          return;
        }
        verified = verifiedSessionCandidate;
      } else {
        if (!authAttempt) {
          setNotice({ tone: 'error', text: 'The sign-in boundary expired. Request a new code and try again.' });
          return;
        }
        const { data, error } = await client.auth.verifyOtp({
          email: pendingEmail,
          token: otp.trim(),
          type: 'email',
        });
        if (error || !data.session) {
          setNotice({ tone: 'error', text: 'That code could not be verified. It may have expired or already been used.' });
          return;
        }
        verified = { accessToken: data.session.access_token, subjectKey: data.session.user.id, authAttempt };
        setVerifiedSessionCandidate(verified);
      }

      const mirrored = await persistSessionToCookie(verified.accessToken, { reauthenticated: true, authAttempt: verified.authAttempt });
      if (mirrored.status !== 'established') {
        const lock = await lockMiniBrainForSignedOutSession();
        if (lock.status === 'durability_unconfirmed') {
          await closeAuthenticationAfterPrivateLockFailure();
          setServerSessionPending(false);
          setVerifiedSessionCandidate(null);
          setAuthAttempt(null);
          setOtp('');
          setNotice({
            tone: 'error',
            text: 'Your identity was verified, but neither the server-session commit nor a durable private Brain lock could be confirmed. Authentication was closed where possible. Enable browser storage or clear this site\'s data, then request a new code. (AUTH_SESSION_COMMIT_AND_BRAIN_LOCK_UNCONFIRMED · MINI_BRAIN_LOCK_DURABILITY_UNCONFIRMED)',
          });
          return;
        }
        setServerSessionPending(true);
        setNotice({
          tone: 'error',
          text: mirrored.status === 'commit_uncertain'
            ? 'Your code was verified and the server may have committed the secure session, but the browser could not verify the final handoff. The private Brain was locked. Retry without requesting another code. (AUTH_SESSION_COMMIT_UNCERTAIN)'
            : 'Your code was verified, but the secure server session was not established. The private Brain was locked. Retry without requesting another code.',
        });
        return;
      }
      const brainStage = await stageMiniBrainOwnerRebindAfterReauthentication(verified.subjectKey, verified.authAttempt);
      if (brainStage.status === 'durability_unconfirmed') {
        await closeAuthenticationAfterPrivateLockFailure();
        setServerSessionPending(false);
        setVerifiedSessionCandidate(null);
        setAuthAttempt(null);
        setOtp('');
        setNotice({
          tone: 'error',
          text: 'Your identity was verified, but this browser could not confirm a durable private Brain lock. The new site session was closed where possible. Enable browser storage or clear this site\'s data, then request a new code. (MINI_BRAIN_LOCK_DURABILITY_UNCONFIRMED)',
        });
        return;
      }
      setVerifiedSessionCandidate(null);
      location.replace(currentReturnPath());
    } catch {
      setNotice({ tone: 'error', text: 'Verification could not be completed. Please try again.' });
    } finally {
      setOperation(null);
    }
  }

  function changeEmail(): void {
    setEmail(pendingEmail ?? email);
    setPendingEmail(null);
    setOtp('');
    setServerSessionPending(false);
    setVerifiedSessionCandidate(null);
    setAuthAttempt(null);
    setResendCooldown(0);
    setNotice(null);
  }

  async function handleOAuth(provider: string) {
    if (oauthLoading) return;
    setNotice(null);
    setOauthLoading(provider);
    const client = getAuthClient();
    if (!client) {
      setNotice({ tone: 'warning', text: 'Provider sign-in is not connected in this environment.' });
      setOauthLoading(null);
      return;
    }

    try {
      const freshAttempt = await beginAuthenticationAttempt('fresh');
      if (!freshAttempt) {
        setNotice({ tone: 'error', text: 'The secure provider handoff could not start. Please retry.' });
        setOauthLoading(null);
        return;
      }
      const { data, error } = await client.auth.signInWithOAuth({
        provider: provider as 'google' | 'apple' | 'github' | 'discord',
        options: {
          redirectTo: callbackUrl(),
          skipBrowserRedirect: true,
          queryParams: provider === 'google' ? { prompt: 'select_account' } : undefined,
        },
      });
      if (error || !data?.url) {
        setNotice({
          tone: 'error',
          text: error ? 'Provider sign-in could not start. Please try again.' : 'The provider did not return a sign-in address.',
        });
        setOauthLoading(null);
        return;
      }
      location.assign(data.url);
    } catch {
      setNotice({ tone: 'error', text: 'The provider could not be reached. Please try again.' });
      setOauthLoading(null);
    }
  }

  const destination = returnTo === '/account'
    ? 'your account'
    : returnTo === '/chat'
      ? 'the private page'
      : returnTo.replace(/^\//, '').replaceAll('-', ' ');

  return (
    <>
      <Head>
        <title>Sign in · Apocky</title>
        <meta name="description" content="Sign in once to continue securely across Apocky." />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="robots" content="noindex,nofollow" />
      </Head>
      <a className="apx-skip-link" href="#main-content">Skip to sign in</a>
      <AuthFrame mode="sign-in">
        <div className="apx-auth-card">
          <p className="apx-auth-context">Continue to {destination}</p>
          <h2>Sign in to Apocky</h2>
          <p className="apx-auth-subtitle">Use a one-time email code, the secure link in that email, or a provider you already trust. No password required.</p>

          {localhostCallback && (
            <details className="apx-auth-warning">
              <summary>Local development callback</summary>
              <p>Allow this address in the authentication provider before testing locally:</p>
              <code>{localhostCallback}</code>
            </details>
          )}

          {!pendingEmail ? (
            <form className="apx-auth-form" onSubmit={handleEmailSubmit}>
              <label className="apx-label" htmlFor="login-email">Email address</label>
              <div className="apx-input-row">
                <input
                  id="login-email"
                  className="apx-input"
                  type="email"
                  autoComplete="email"
                  inputMode="email"
                  required
                  value={email}
                  onChange={(event) => setEmail(event.target.value)}
                  placeholder="you@example.com"
                />
                <button className="apx-button apx-button--primary" type="submit" disabled={Boolean(operation) || !email.trim()}>
                  {operation === 'send' ? 'Sending…' : 'Send sign-in email'}
                </button>
              </div>
              <p className="apx-field-help">The email also includes a single-use link that returns you to {destination}.</p>
            </form>
          ) : (
            <form className="apx-auth-form" onSubmit={handleVerifyCode}>
              <p className="apx-field-help" id="login-code-destination">Code sent to <strong>{pendingEmail}</strong>.</p>
              <label className="apx-label" htmlFor="login-code">One-time code</label>
              <input
                id="login-code"
                className="apx-input"
                type="text"
                autoComplete="one-time-code"
                inputMode="numeric"
                pattern="[0-9]{6,8}"
                minLength={6}
                maxLength={8}
                required
                value={otp}
                onChange={(event) => setOtp(event.target.value)}
                aria-describedby="login-code-destination login-code-help"
                placeholder="000000"
                autoFocus
              />
              <p className="apx-field-help" id="login-code-help">Codes are single-use. Do not share this code with anyone.</p>
              <button className="apx-button apx-button--primary" type="submit" disabled={Boolean(operation) || (!serverSessionPending && otp.trim().length < 6)} style={{ width: '100%', marginTop: 18 }}>
                {operation === 'verify'
                  ? 'Verifying…'
                  : serverSessionPending
                    ? 'Retry secure session'
                    : 'Verify and continue'}
              </button>
              <div className="apx-actions" style={{ marginTop: 12 }}>
                <button
                  className="apx-button"
                  type="button"
                  onClick={() => void sendEmailCode(pendingEmail, 'resend')}
                  disabled={Boolean(operation) || resendCooldown > 0}
                  aria-describedby="login-resend-help"
                >
                  {operation === 'resend'
                    ? 'Resending…'
                    : resendCooldown > 0
                      ? `Resend email in ${resendCooldown}s`
                      : 'Resend email'}
                </button>
                <button className="apx-button" type="button" onClick={changeEmail} disabled={Boolean(operation)}>Change email</button>
              </div>
              <p className="apx-field-help" id="login-resend-help">A short resend delay helps prevent accidental duplicate emails.</p>
            </form>
          )}

          {!pendingEmail && (
            <>
              <div className="apx-divider">or use a provider</div>
              <div className="apx-provider-grid" aria-label="Sign-in providers">
                {AUTH_PROVIDERS.filter((provider) => provider.enabled).map((provider) => {
                  const loading = oauthLoading === provider.id;
                  return (
                    <button
                      key={provider.id}
                      className="apx-provider"
                      type="button"
                      onClick={() => void handleOAuth(provider.id)}
                      disabled={Boolean(oauthLoading) || Boolean(operation)}
                    >
                      <span>{loading ? `Opening ${provider.label}…` : provider.label}</span>
                      <span className="apx-provider-state" aria-hidden="true">{loading ? 'Working' : 'Open'}</span>
                    </button>
                  );
                })}
              </div>
            </>
          )}

          {notice && (
            <div
              className={notice.tone === 'info' ? 'apx-auth-message' : 'apx-auth-warning'}
              role={notice.tone === 'error' ? 'alert' : 'status'}
              aria-live={notice.tone === 'error' ? 'assertive' : 'polite'}
              aria-atomic="true"
            >
              {notice.text}
            </div>
          )}

          <p className="apx-auth-switch">New here? <Link href={`/register?next=${encodeURIComponent(returnTo)}`}>Create an account</Link></p>
        </div>
      </AuthFrame>
    </>
  );
};

export default Login;
