import type { User } from '@supabase/supabase-js';
import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { lockMiniBrainForSignedOutSession } from '@/lib/brain/mini-brain';
import { useEffect, useState } from 'react';
import { APOCKY_CHANNELS, getAuthClient, withBrowserAuthSignOut } from '../lib/auth';

interface ServerAccountUser {
  email: string;
  id: string;
  provider: string;
  createdAt: string;
}

const SIGN_OUT_BOUNDARY_TIMEOUT_MS = 8_000;

async function boundedBrowserSignOut(operation: Promise<{ error: unknown }>): Promise<boolean> {
  try {
    const result = await Promise.race([
      operation,
      new Promise<never>((_resolve, reject) => window.setTimeout(() => reject(new Error('AUTH_BROWSER_SIGNOUT_TIMEOUT')), SIGN_OUT_BOUNDARY_TIMEOUT_MS)),
    ]);
    return !result.error;
  } catch {
    return false;
  }
}

async function boundedServerSignOut(): Promise<boolean> {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(new DOMException('Timed out', 'TimeoutError')), SIGN_OUT_BOUNDARY_TIMEOUT_MS);
  try {
    const response = await fetch('/api/auth/logout', {
      method: 'POST',
      credentials: 'same-origin',
      cache: 'no-store',
      signal: controller.signal,
    });
    return response.ok;
  } catch {
    return false;
  } finally {
    window.clearTimeout(timeout);
  }
}

interface AccountUser extends ServerAccountUser {
  providers: string[];
  emailConfirmedAt: string | null;
  lastSignInAt: string | null;
}

interface MeResponse {
  user: ServerAccountUser | null;
  stub?: boolean;
  reason?: string;
}

async function fetchJsonWithTimeout<T>(url: string, init: RequestInit, timeoutMs: number): Promise<T> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetch(url, { ...init, signal: controller.signal });
    if (!response.ok) throw new Error(`request failed with status ${response.status}`);
    return await response.json() as T;
  } finally {
    clearTimeout(timer);
  }
}

function serverAccountUser(user: ServerAccountUser): AccountUser {
  return {
    ...user,
    providers: [user.provider],
    emailConfirmedAt: null,
    lastSignInAt: null,
  };
}

function browserAccountUser(user: User): AccountUser {
  const providers = new Set<string>();
  for (const identity of user.identities ?? []) {
    if (identity.provider) providers.add(identity.provider);
  }
  const metadataProviders = user.app_metadata?.providers;
  if (Array.isArray(metadataProviders)) {
    for (const provider of metadataProviders) {
      if (typeof provider === 'string' && provider) providers.add(provider);
    }
  }
  const currentProvider = typeof user.app_metadata?.provider === 'string'
    ? user.app_metadata.provider
    : 'email';
  providers.add(currentProvider);

  return {
    id: user.id,
    email: user.email ?? '(email unavailable)',
    provider: currentProvider,
    providers: [...providers],
    createdAt: user.created_at,
    emailConfirmedAt: user.email_confirmed_at ?? null,
    lastSignInAt: user.last_sign_in_at ?? null,
  };
}

function displayProvider(provider: string): string {
  if (provider === 'email') return 'Email one-time code or link';
  if (provider === 'test') return 'Local test identity';
  return provider;
}

function displayDate(value: string | null): string {
  if (!value) return 'Unavailable';
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? 'Unavailable'
    : date.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

const Account: NextPage = () => {
  const [me, setMe] = useState<{ user: AccountUser | null; stub?: boolean } | null>(null);
  const [loading, setLoading] = useState(true);
  const [sessionReason, setSessionReason] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [signingOut, setSigningOut] = useState(false);
  const [signOutError, setSignOutError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      const client = getAuthClient();
      let browserUser: AccountUser | null = null;

      if (client) {
        try {
          const sessionResult = await Promise.race([
            client.auth.getSession(),
            new Promise<never>((_, reject) => setTimeout(() => reject(new Error('timeout')), 5000)),
          ]) as Awaited<ReturnType<typeof client.auth.getSession>>;
          const userResult = await Promise.race([
            client.auth.getUser(),
            new Promise<never>((_, reject) => setTimeout(() => reject(new Error('timeout')), 5000)),
          ]) as Awaited<ReturnType<typeof client.auth.getUser>>;
          if (userResult.data.user) browserUser = browserAccountUser(userResult.data.user);
        } catch {
          // The server session remains the authority when browser enrichment is unavailable.
        }
      }

      try {
        const response = await fetchJsonWithTimeout<MeResponse>(
          '/api/auth/me',
          { cache: 'no-store', credentials: 'same-origin' },
          5000,
        );
        if (cancelled) return;

        const serverUser = response.user ? serverAccountUser(response.user) : null;
        if (serverUser && browserUser && serverUser.id !== browserUser.id) {
          setMe({ user: serverUser, stub: response.stub });
          setLoadError('Browser and server identities did not match. The server-confirmed identity is shown; sign out before switching accounts.');
        } else {
          setMe({ user: browserUser ?? serverUser, stub: response.stub });
          if (!serverUser && browserUser) {
            setLoadError('Your browser identity is present, but the secure server session could not be confirmed. Retry sign-in before using protected features.');
          }
        }
        setSessionReason(response.user || browserUser ? null : response.reason ?? 'No active account session was found.');
      } catch {
        if (cancelled) return;
        setMe({ user: browserUser });
        setLoadError(browserUser
          ? 'The server account check is unavailable. Browser identity is shown, but protected features may not recognize this session.'
          : 'Account status could not be loaded. Check your connection and try again.');
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  async function handleSignOut(): Promise<void> {
    if (signingOut) return;
    setSigningOut(true);
    setSignOutError(null);

    try {
      let localProtectionCleared = true;
      let browserCleared = true;
      let serverCleared = false;
      await withBrowserAuthSignOut(async () => {
        const client = getAuthClient();
        const [browserOutcome, serverOutcome] = await Promise.all([
          client ? boundedBrowserSignOut(client.auth.signOut({ scope: 'local' })) : Promise.resolve(true),
          boundedServerSignOut(),
        ]);
        browserCleared = browserOutcome;
        serverCleared = serverOutcome;
      }, async () => {
        try {
          const result = await lockMiniBrainForSignedOutSession();
          localProtectionCleared = result.status === 'locked';
        } catch {
          localProtectionCleared = false;
        }
      });
      if (!serverCleared || !browserCleared || !localProtectionCleared) {
        setSignOutError('Sign-out did not clear every session surface. Retry, or clear this site\'s browser data before leaving a shared device.');
        return;
      }
      location.replace('/');
    } catch {
      setSignOutError('Sign-out could not reach the account service. Please retry.');
    } finally {
      setSigningOut(false);
    }
  }

  if (loading) {
    return (
      <div className="apx-account-page">
        <main className="apx-account" aria-busy="true">
          <p className="apx-kicker" role="status" aria-live="polite">Loading account status…</p>
        </main>
      </div>
    );
  }

  const user = me?.user ?? null;

  return (
    <>
      <Head>
        <title>Account · Apocky</title>
        <meta name="description" content="Review your current Apocky identity and browser-local account settings." />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="robots" content="noindex,nofollow" />
      </Head>
      <div className="apx-account-page">
        <main className="apx-account">
          <header className="apx-account-head">
            <div>
              <p className="apx-kicker">Identity and boundaries</p>
              <h1>Your account</h1>
            </div>
            <div className="apx-actions" style={{ marginTop: 0 }}>
              <Link className="apx-button" href="/">Home</Link>
              {user && (
                <button className="apx-button" type="button" onClick={() => void handleSignOut()} disabled={signingOut}>
                  {signingOut ? 'Signing out…' : 'Sign out of this browser'}
                </button>
              )}
            </div>
          </header>

          {me?.stub && (
            <div className="apx-auth-warning" role="status">
              Authentication is not configured in this environment. No account identity can be loaded here.
            </div>
          )}
          {loadError && (
            <div className="apx-auth-warning" role="alert" aria-live="assertive">{loadError}</div>
          )}
          {signOutError && (
            <div className="apx-auth-warning" role="alert" aria-live="assertive">{signOutError}</div>
          )}

          <section className="apx-panel" aria-labelledby="identity-heading">
            <h2 id="identity-heading">Current identity</h2>
            {user ? (
              <div className="apx-data-list">
                <div className="apx-data-row">
                  <span className="apx-data-label">Email</span>
                  <span className="apx-data-value">{user.email}</span>
                </div>
                <div className="apx-data-row">
                  <span className="apx-data-label">Account ID</span>
                  <span className="apx-data-value">{user.id}</span>
                </div>
                <div className="apx-data-row">
                  <span className="apx-data-label">Current sign-in method</span>
                  <span className="apx-data-value">{displayProvider(user.provider)}</span>
                </div>
                <div className="apx-data-row">
                  <span className="apx-data-label">Account created</span>
                  <span className="apx-data-value">{displayDate(user.createdAt)}</span>
                </div>
                {user.emailConfirmedAt && (
                  <div className="apx-data-row">
                    <span className="apx-data-label">Email confirmed</span>
                    <span className="apx-data-value">{displayDate(user.emailConfirmedAt)}</span>
                  </div>
                )}
                {user.lastSignInAt && (
                  <div className="apx-data-row">
                    <span className="apx-data-label">Last browser sign-in</span>
                    <span className="apx-data-value">{displayDate(user.lastSignInAt)}</span>
                  </div>
                )}
              </div>
            ) : (
              <>
                <p className="apx-section-intro">{sessionReason ?? 'You are not signed in.'}</p>
                <div className="apx-actions">
                  <Link className="apx-button apx-button--primary" href="/login?next=%2Faccount">Sign in</Link>
                  <Link className="apx-button" href="/register?next=%2Faccount">Create account</Link>
                </div>
              </>
            )}
          </section>

          <section className="apx-panel" aria-labelledby="methods-heading">
            <h2 id="methods-heading">Sign-in methods</h2>
            {user ? (
              <>
                <p className="apx-section-intro">Observed on the current browser identity: {user.providers.map(displayProvider).join(', ')}.</p>
                <p className="apx-field-help">
                  This page reports the methods attached to the current identity. It does not currently offer
                  provider-management controls.
                </p>
              </>
            ) : (
              <p className="apx-section-intro">Sign in before reviewing the method attached to your current identity.</p>
            )}
          </section>

          <section className="apx-panel" aria-labelledby="account-data-heading">
            <h2 id="account-data-heading">Account data and deletion</h2>
            <p className="apx-section-intro">
              This page does not currently perform export or deletion. To make a privacy request, email{' '}
              <a href="mailto:apocky13@gmail.com?subject=%5Bprivacy%5D" style={{ color: 'var(--apx-sky)' }}>
                apocky13@gmail.com
              </a>{' '}
              with <code>[privacy]</code> in the subject.
            </p>
            <div className="apx-actions">
              <Link className="apx-button" href="/docs/sovereignty">Read about permissions and data sharing</Link>
              <Link className="apx-button" href="/legal/privacy">Read the privacy policy</Link>
            </div>
          </section>

          <section className="apx-panel" aria-labelledby="creator-links-heading">
            <h2 id="creator-links-heading">Official Apocky channels</h2>
            <div className="apx-actions" style={{ marginTop: 0 }}>
              {APOCKY_CHANNELS.map((channel) => (
                <a className="apx-button" key={channel.href} href={channel.href} target="_blank" rel="noopener noreferrer">
                  {channel.label} <span aria-hidden="true">↗</span>
                </a>
              ))}
            </div>
          </section>

          <p className="apx-auth-fine" style={{ marginTop: 36 }}>
            Account identity comes from the active authentication session. This page does not infer additional permissions from sign-in.
          </p>
        </main>
      </div>
    </>
  );
};

export default Account;
