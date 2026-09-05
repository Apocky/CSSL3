import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { AuthFrame } from '../components/hub/AuthFrame';
import { AUTH_PROVIDERS, getAuthClient, persistSessionToCookie } from '../lib/auth';
import { buildAuthCallbackUrl, normalizeAuthReturnPath } from '../lib/auth-return';

type Notice = {
  tone: 'info' | 'error' | 'warning';
  text: string;
};

const RESEND_COOLDOWN_SECONDS = 30;

const Register: NextPage = () => {
  const [email, setEmail] = useState('');
  const [pendingEmail, setPendingEmail] = useState<string | null>(null);
  const [otp, setOtp] = useState('');
  const [agreed, setAgreed] = useState(false);
  const [operation, setOperation] = useState<'send' | 'resend' | 'verify' | null>(null);
  const [serverSessionPending, setServerSessionPending] = useState(false);
  const [resendCooldown, setResendCooldown] = useState(0);
  const [oauthLoading, setOauthLoading] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [returnTo, setReturnTo] = useState('/account');

  useEffect(() => {
    const next = new URLSearchParams(location.search).get('next');
    setReturnTo(normalizeAuthReturnPath(next));
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
    if (!agreed) {
      setNotice({ tone: 'error', text: 'Please confirm the account terms before continuing.' });
      return;
    }
    if (operation) return;
    if (kind === 'resend' && resendCooldown > 0) return;
    const normalizedEmail = address.trim();
    if (!normalizedEmail) return;
    setOperation(kind);
    setNotice(null);

    const client = getAuthClient();
    if (!client) {
      setNotice({ tone: 'warning', text: 'Account creation is not connected in this environment. No address was submitted.' });
      setOperation(null);
      return;
    }

    try {
      const { error } = await client.auth.signInWithOtp({
        email: normalizedEmail,
        options: {
          emailRedirectTo: callbackUrl(),
          shouldCreateUser: true,
        },
      });
      if (error) {
        setNotice({ tone: 'error', text: 'We could not send a verification code. Check the address, wait a moment, and try again.' });
        return;
      }
      setPendingEmail(normalizedEmail);
      setOtp('');
      setServerSessionPending(false);
      setResendCooldown(RESEND_COOLDOWN_SECONDS);
      setNotice({
        tone: 'info',
        text: kind === 'resend'
          ? `A new verification email was sent to ${normalizedEmail}. Enter its code if shown, or use its secure link.`
          : `Check ${normalizedEmail}. Enter the one-time code if the email shows one, or use its secure link.`,
      });
    } catch {
      setNotice({ tone: 'error', text: 'The account service could not be reached. Please try again.' });
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
      setNotice({ tone: 'warning', text: 'Account creation is not connected in this environment.' });
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
        const { data, error } = await client.auth.verifyOtp({
          email: pendingEmail,
          token: otp.trim(),
          type: 'email',
        });
        if (error || !data.session) {
          setNotice({ tone: 'error', text: 'That code could not be verified. It may have expired or already been used.' });
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
    setResendCooldown(0);
    setNotice(null);
  }

  async function handleOAuth(provider: string) {
    if (!agreed || oauthLoading) {
      if (!agreed) setNotice({ tone: 'error', text: 'Please confirm the account terms before continuing.' });
      return;
    }
    setNotice(null);
    setOauthLoading(provider);
    const client = getAuthClient();
    if (!client) {
      setNotice({ tone: 'warning', text: 'Provider account creation is not connected in this environment.' });
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
          text: error ? 'Provider account creation could not start. Please try again.' : 'The provider did not return a sign-in address.',
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

  return (
    <>
      <Head>
        <title>Create an account · Apocky</title>
        <meta name="description" content="Create an optional Apocky account for a feature that requires sign-in." />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="robots" content="noindex,nofollow" />
      </Head>
      <a className="apx-skip-link" href="#main-content">Skip to account creation</a>
      <AuthFrame mode="register" formFirst>
        <div className="apx-auth-card">
          <p className="apx-auth-context">Optional account</p>
          <h1>Create your account</h1>
          <p className="apx-auth-subtitle">Start with a one-time email code, the secure link in that email, or a provider. You can sign out at any time.</p>

          {!pendingEmail ? (
            <form className="apx-auth-form" onSubmit={handleEmailSubmit}>
              <label className="apx-label" htmlFor="register-email">Email address</label>
              <input
                id="register-email"
                className="apx-input"
                type="email"
                autoComplete="email"
                inputMode="email"
                required
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                placeholder="you@example.com"
              />

              <label className="apx-checkbox" htmlFor="register-agreement">
                <input id="register-agreement" type="checkbox" checked={agreed} onChange={(event) => setAgreed(event.target.checked)} />
                <span>I agree to the <Link href="/legal/terms">Terms</Link> and <Link href="/legal/privacy">Privacy Policy</Link>, and confirm I meet the stated age requirements.</span>
              </label>

              <button className="apx-button apx-button--primary" type="submit" disabled={Boolean(operation) || !email.trim() || !agreed} style={{ width: '100%', marginTop: 20 }}>
                {operation === 'send' ? 'Sending…' : 'Send verification email'}
              </button>
              <p className="apx-field-help">The email also includes a single-use link that returns you to the page you chose.</p>
            </form>
          ) : (
            <form className="apx-auth-form" onSubmit={handleVerifyCode}>
              <p className="apx-field-help" id="register-code-destination">Code sent to <strong>{pendingEmail}</strong>.</p>
              <label className="apx-label" htmlFor="register-code">One-time verification code</label>
              <input
                id="register-code"
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
                aria-describedby="register-code-destination register-code-help"
                placeholder="000000"
                autoFocus
              />
              <p className="apx-field-help" id="register-code-help">Codes are single-use. Do not share this code with anyone.</p>
              <button className="apx-button apx-button--primary" type="submit" disabled={Boolean(operation) || (!serverSessionPending && otp.trim().length < 6)} style={{ width: '100%', marginTop: 18 }}>
                {operation === 'verify'
                  ? 'Verifying…'
                  : serverSessionPending
                    ? 'Retry secure session'
                    : 'Verify and create account'}
              </button>
              <div className="apx-actions" style={{ marginTop: 12 }}>
                <button
                  className="apx-button"
                  type="button"
                  onClick={() => void sendEmailCode(pendingEmail, 'resend')}
                  disabled={Boolean(operation) || resendCooldown > 0}
                  aria-describedby="register-resend-help"
                >
                  {operation === 'resend'
                    ? 'Resending…'
                    : resendCooldown > 0
                      ? `Resend email in ${resendCooldown}s`
                      : 'Resend email'}
                </button>
                <button className="apx-button" type="button" onClick={changeEmail} disabled={Boolean(operation)}>Change email</button>
              </div>
              <p className="apx-field-help" id="register-resend-help">A short resend delay helps prevent accidental duplicate emails.</p>
            </form>
          )}

          {!pendingEmail && (
            <>
              <div className="apx-divider">or use a provider</div>
              <div className="apx-provider-grid" aria-label="Account providers">
                {AUTH_PROVIDERS.filter((provider) => provider.enabled).map((provider) => {
                  const loading = oauthLoading === provider.id;
                  return (
                    <button
                      key={provider.id}
                      className="apx-provider"
                      type="button"
                      onClick={() => void handleOAuth(provider.id)}
                      disabled={!agreed || Boolean(oauthLoading) || Boolean(operation)}
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
          <p className="apx-auth-switch">Already have an account? <Link href={`/login?next=${encodeURIComponent(returnTo)}`}>Sign in</Link></p>
        </div>
      </AuthFrame>
    </>
  );
};

export default Register;
