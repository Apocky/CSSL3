import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { AuthFrame } from '../components/hub/AuthFrame';
import { credentialFromInput, EMAIL_CREDENTIAL_MAX_LENGTH } from '@/lib/auth-credential';
import { AUTH_PROVIDERS, getAuthClient, persistSessionToCookie } from '../lib/auth';
import { AuthenticatorSetup } from '../components/auth/AuthenticatorSetup';
import { continueAfterSignIn } from '../lib/auth-continue';
import { buildAuthCallbackUrl, normalizeAuthReturnPath } from '../lib/auth-return';

type Notice = {
  tone: 'info' | 'error' | 'warning';
  text: string;
};

const RESEND_COOLDOWN_SECONDS = 30;

const Login: NextPage = () => {
  const [email, setEmail] = useState('');
  const [pendingEmail, setPendingEmail] = useState<string | null>(null);
  const [otp, setOtp] = useState('');
  const [operation, setOperation] = useState<'send' | 'resend' | 'verify' | null>(null);
  const [serverSessionPending, setServerSessionPending] = useState(false);
  const [resendCooldown, setResendCooldown] = useState(0);
  const [oauthLoading, setOauthLoading] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [localhostCallback, setLocalhostCallback] = useState<string | null>(null);
  const [returnTo, setReturnTo] = useState('/account');
  const [authCode, setAuthCode] = useState('');
  const [showEmailFallback, setShowEmailFallback] = useState(false);
  // Enrolment is the closing step of signing in, reached as ?setup=authenticator so that a
  // refresh or a provider round-trip lands on it instead of silently skipping it.
  const [setupStage, setSetupStage] = useState(false);

  useEffect(() => {
    const params = new URLSearchParams(location.search);
    setReturnTo(normalizeAuthReturnPath(params.get('next')));
    setSetupStage(params.get('setup') === 'authenticator');
    if (location.hostname === 'localhost') {
      setLocalhostCallback(`http://localhost:${location.port || 3000}/auth/callback`);
    }
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
      setResendCooldown(RESEND_COOLDOWN_SECONDS);
      setNotice({
        tone: 'info',
        text: kind === 'resend'
          ? `A new sign-in email was sent to ${normalizedEmail}. Enter its code, or paste its link below.`
          : `Check ${normalizedEmail}. Enter the code if the email shows one; otherwise copy the link from the email and paste it below.`,
      });
    } catch {
      setNotice({ tone: 'error', text: 'The sign-in service could not be reached. Please try again.' });
    } finally {
      setOperation(null);
    }
  }

  /**
   * Authenticator sign-in: one screen, no email anywhere in the path.
   *
   * The server checks the code against the stored TOTP secret and, only then, returns a single-use
   * token which is exchanged here for a real session. Nothing is mailed, so neither the mail
   * template deciding whether a code appears, nor a link opening the system browser and stranding
   * the session there, can break it.
   */
  async function handleAuthenticatorSubmit(event: React.FormEvent): Promise<void> {
    event.preventDefault();
    if (operation) return;
    const address = email.trim();
    const digits = authCode.replace(/[^0-9]/gu, '');
    if (!address || digits.length !== 6) return;
    setOperation('verify');
    setNotice(null);
    try {
      const response = await fetch('/api/auth/totp', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ email: address, code: digits }),
      });
      const payload = await response.json().catch(() => null) as { token_hash?: string; error?: string } | null;
      if (!response.ok || !payload?.token_hash) {
        setNotice({ tone: 'error', text: payload?.error ?? 'That code was not accepted.' });
        setAuthCode('');
        return;
      }
      const client = getAuthClient();
      if (!client) {
        setNotice({ tone: 'warning', text: 'Sign-in is not connected in this environment.' });
        return;
      }
      const { data, error } = await client.auth.verifyOtp({ token_hash: payload.token_hash, type: 'email' });
      if (error || !data.session) {
        setNotice({ tone: 'error', text: 'The code was accepted but a session could not be opened. Try once more.' });
        return;
      }
      if (!await persistSessionToCookie(data.session.access_token)) {
        setNotice({ tone: 'error', text: 'Signed in, but the secure server session could not be established.' });
        return;
      }
      await continueAfterSignIn(currentReturnPath(), { alreadyOffered: setupStage });
    } catch {
      setNotice({ tone: 'error', text: 'Sign-in could not be reached. Please try again.' });
    } finally {
      setOperation(null);
      setAuthCode('');
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
      let accessToken: string | null = null;
      if (serverSessionPending) {
        const { data, error } = await client.auth.getSession();
        if (error || !data.session) {
          setServerSessionPending(false);
          setNotice({ tone: 'error', text: 'The verified browser session is no longer available. Request a new code and try again.' });
          return;
        }
        accessToken = data.session.access_token;
      } else {
        const credential = credentialFromInput(otp);
        if (!credential) {
          setNotice({
            tone: 'error',
            text: 'Paste either the code from the email or the whole sign-in link. If you pasted a link, it did not contain a usable token.',
          });
          return;
        }
        const { data, error } = credential.kind === 'link'
          ? await client.auth.verifyOtp({ token_hash: credential.token, type: 'email' })
          : await client.auth.verifyOtp({ email: pendingEmail, token: credential.token, type: 'email' });
        if (error || !data.session) {
          setNotice({ tone: 'error', text: 'That code or link could not be verified. It may have expired or already been used.' });
          return;
        }
        accessToken = data.session.access_token;
      }

      const mirrored = await persistSessionToCookie(accessToken);
      if (!mirrored) {
        setServerSessionPending(true);
        setNotice({
          tone: 'error',
          text: 'Your code was verified, but the secure server session could not be established. Retry without requesting another code.',
        });
        return;
      }
      await continueAfterSignIn(currentReturnPath(), { alreadyOffered: setupStage });
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

  // The LABEL only. `returnTo` itself keeps its query and fragment — it is the actual href below
  // and the location.replace target, and the fragment is deliberately preserved so that
  // "sign in and I'll take you back" lands on the authenticator panel rather than the top of the
  // account page. Without this split the headline read "Continue to account#authenticator".
  const destinationPath = returnTo.split(/[?#]/)[0] ?? returnTo;
  const destination = destinationPath === '/account'
    ? 'your account'
    : destinationPath.replace(/^\//, '').replaceAll('-', ' ') || 'the site';

  return (
    <>
      <Head>
        <title>Sign in · Apocky</title>
        <meta name="description" content="Sign in once to continue securely across Apocky." />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="robots" content="noindex,nofollow" />
      </Head>
      <a className="apx-skip-link" href="#main-content">Skip to sign in</a>
      <AuthFrame mode={setupStage ? "setup" : "sign-in"} formFirst>
        <div className="apx-auth-card">
          {setupStage ? <>
            {/* The last step of signing in, not a separate errand afterwards. You have just proved
                you are this account, which is the whole gate on adding a credential to it, and you
                are already thinking about sign-in. Asking later means asking someone who has moved
                on — which is exactly how this ended up undiscoverable. */}
            <p className="apx-auth-context">One more step</p>
            <h1>Make next time one step</h1>
            <p className="apx-auth-subtitle">
              You&rsquo;re signed in. Add an authenticator now and you&rsquo;ll sign in with a
              6-digit code from your phone instead of waiting for an email.
            </p>
            <AuthenticatorSetup hideDoneState onComplete={() => { location.replace(returnTo); }} />
            <p className="apx-auth-switch" style={{ marginTop: 18, textAlign: 'center' }}>
              <Link href={returnTo}>Skip for now — continue to {destination}</Link>
            </p>
            <p className="apx-auth-fine" style={{ marginTop: 10, textAlign: 'center' }}>
              You can set this up later from your account page. Skipping changes nothing about how
              you sign in today.
            </p>
          </> : <>
          <p className="apx-auth-context">Continue to {destination}</p>
          <h1>Sign in to Apocky</h1>
          <p className="apx-auth-subtitle">Enter the code from your authenticator app. No password, no email round-trip.</p>

          {localhostCallback && (
            <details className="apx-auth-warning">
              <summary>Local development callback</summary>
              <p>Allow this address in the authentication provider before testing locally:</p>
              <code>{localhostCallback}</code>
            </details>
          )}

          {!pendingEmail ? (
            <>
            <form className="apx-auth-form" onSubmit={handleAuthenticatorSubmit}>
              <label className="apx-label" htmlFor="login-authenticator-email">Email address</label>
              <input
                id="login-authenticator-email"
                className="apx-input"
                type="email"
                autoComplete="email"
                inputMode="email"
                required
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                placeholder="you@example.com"
              />
              <label className="apx-label" htmlFor="login-totp" style={{ marginTop: 14 }}>Authenticator code</label>
              <input
                id="login-totp"
                className="apx-input"
                type="text"
                autoComplete="one-time-code"
                inputMode="numeric"
                maxLength={7}
                required
                value={authCode}
                onChange={(event) => setAuthCode(event.target.value.replace(/[^0-9 ]/gu, ''))}
                placeholder="000000"
              />
              <p className="apx-field-help">
                The 6-digit code from your authenticator app. It changes every 30 seconds.
              </p>
              {/* This screen asks for a code from an authenticator and never said where to get one.
                  Deliberately NOT a link: the QR lives behind a sign-in, so a link here invites a
                  click into an account page that cannot show it yet — a dead end dressed as a way
                  forward. It is an instruction for after you are in, so it reads as one. */}
              <p className="apx-field-help">
                Don&rsquo;t have one yet? Sign in by email below. Once you are in, your account page
                offers <strong>Set up an authenticator</strong> — you scan a QR code once and use the
                6-digit code from then on.
              </p>
              <button
                className="apx-button apx-button--primary"
                type="submit"
                style={{ width: '100%', marginTop: 16 }}
                disabled={Boolean(operation) || !email.trim() || authCode.replace(/[^0-9]/gu, '').length !== 6}
              >
                {operation === 'verify' ? 'Signing in…' : 'Sign in'}
              </button>
            </form>
            <p className="apx-auth-switch" style={{ marginTop: 18, textAlign: 'center' }}>
              {/* A bare <button> inherits the platform's grey chrome and reads as a broken control
                  next to the styled inputs. It is a link in behaviour, so it looks like one. */}
              <button
                type="button"
                onClick={() => setShowEmailFallback((value) => !value)}
                style={{
                  background: 'none',
                  border: 0,
                  padding: 0,
                  font: 'inherit',
                  color: '#9fc6ff',
                  textDecoration: 'underline',
                  cursor: 'pointer',
                }}
              >
                {showEmailFallback ? 'Hide email sign-in' : 'No authenticator? Sign in by email instead'}
              </button>
            </p>
            {showEmailFallback ? <form className="apx-auth-form" onSubmit={handleEmailSubmit}>
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
            </form> : null}
            </>
          ) : (
            <form className="apx-auth-form" onSubmit={handleVerifyCode}>
              <p className="apx-field-help" id="login-code-destination">Code sent to <strong>{pendingEmail}</strong>.</p>
              <label className="apx-label" htmlFor="login-code">Code or sign-in link</label>
              {/* The cap below was maxLength={8}. This field's own label offers "Code or sign-in
                  link" and its placeholder says "paste the link" — and then the browser truncated
                  every pasted link to eight characters, silently, so the paste produced eight
                  characters of a URL and a rejection that blamed the reader. The parser, the label
                  and the help text were all correct; one attribute defeated all three. A magic link
                  runs to a few hundred characters, so the cap is a sanity bound now, not a
                  code-shaped one. */}
              <input
                id="login-code"
                className="apx-input"
                type="text"
                autoComplete="one-time-code"
                inputMode="text"
                minLength={6}
                maxLength={EMAIL_CREDENTIAL_MAX_LENGTH}
                required
                value={otp}
                onChange={(event) => setOtp(event.target.value)}
                aria-describedby="login-code-destination login-code-help"
                placeholder="000000 or paste the link"
                autoFocus
              />
              <p className="apx-field-help" id="login-code-help">If the email shows a code, type it. If it only shows a link, press and hold the link, copy it, and paste it here — that keeps you signed in to this app instead of handing the session to your browser. Either one is single-use; do not share it.</p>
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
          </>}
        </div>
      </AuthFrame>
    </>
  );
};

export default Login;
