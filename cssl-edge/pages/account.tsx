import type { User } from '@supabase/supabase-js';
import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { APOCKY_CHANNELS, PROFILE_LINKABLE, getAuthClient, persistSessionToCookie } from '../lib/auth';

interface ServerAccountUser {
  email: string;
  id: string;
  provider: string;
  createdAt: string;
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

interface ProfileLinks {
  [key: string]: string;
}

type SaveNotice = {
  tone: 'success' | 'error';
  text: string;
};

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

function readLocalProfileLinks(raw: string | null): ProfileLinks {
  if (!raw) return {};
  const parsed: unknown = JSON.parse(raw);
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};

  const links: ProfileLinks = {};
  for (const field of PROFILE_LINKABLE) {
    const value = (parsed as Record<string, unknown>)[field.id];
    if (typeof value === 'string') links[field.id] = value;
  }
  return links;
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
  const [profileLinks, setProfileLinks] = useState<ProfileLinks>({});
  const [storageAvailable, setStorageAvailable] = useState(true);
  const [savedNotice, setSavedNotice] = useState<SaveNotice | null>(null);
  const [signingOut, setSigningOut] = useState(false);
  const [signOutError, setSignOutError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    try {
      setProfileLinks(readLocalProfileLinks(localStorage.getItem('apocky-profile-links')));
    } catch {
      setStorageAvailable(false);
    }

    void (async () => {
      const client = getAuthClient();
      let browserUser: AccountUser | null = null;

      if (client) {
        try {
          const sessionResult = await Promise.race([
            client.auth.getSession(),
            new Promise<never>((_, reject) => setTimeout(() => reject(new Error('timeout')), 5000)),
          ]) as Awaited<ReturnType<typeof client.auth.getSession>>;
          if (sessionResult.data.session?.access_token) {
            await persistSessionToCookie(sessionResult.data.session.access_token);
          }

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

  function setLink(id: string, value: string): void {
    setProfileLinks((previous) => ({ ...previous, [id]: value }));
    setSavedNotice(null);
  }

  function saveLinks(event: React.FormEvent): void {
    event.preventDefault();
    if (!storageAvailable) {
      setSavedNotice({ tone: 'error', text: 'Local browser storage is unavailable, so these drafts cannot be saved.' });
      return;
    }

    try {
      localStorage.setItem('apocky-profile-links', JSON.stringify(profileLinks));
      setSavedNotice({ tone: 'success', text: 'Saved in this browser only. Nothing was uploaded or published.' });
    } catch {
      setStorageAvailable(false);
      setSavedNotice({ tone: 'error', text: 'This browser blocked local storage. No settings were saved.' });
    }
  }

  async function handleSignOut(): Promise<void> {
    if (signingOut) return;
    setSigningOut(true);
    setSignOutError(null);

    try {
      const client = getAuthClient();
      let browserCleared = true;
      if (client) {
        const { error } = await client.auth.signOut({ scope: 'local' });
        browserCleared = !error;
      }

      const response = await fetch('/api/auth/logout', {
        method: 'POST',
        credentials: 'same-origin',
        cache: 'no-store',
      });
      if (!response.ok || !browserCleared) {
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
        <main id="main-content" className="apx-account" aria-busy="true">
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
        <main id="main-content" className="apx-account">
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
                <button className="apx-button" type="button" disabled aria-describedby="provider-management-note">
                  Link or unlink providers — unavailable
                </button>
                <p className="apx-field-help" id="provider-management-note">
                  This page can report observed identity methods, but it cannot add or remove providers yet.
                </p>
              </>
            ) : (
              <p className="apx-section-intro">Sign in before reviewing the method attached to your current identity.</p>
            )}
          </section>

          <section className="apx-panel" aria-labelledby="local-settings-heading">
            <h2 id="local-settings-heading">Local-only profile drafts</h2>
            <div className="apx-auth-warning" role="note">
              These optional channel values stay in this browser's local storage. They are not uploaded, synced, published, or connected to your account.
            </div>
            <form onSubmit={saveLinks} style={{ marginTop: 24 }}>
              <div className="apx-data-list">
                {PROFILE_LINKABLE.map((field) => {
                  const inputId = `profile-link-${field.id}`;
                  return (
                    <div key={field.id}>
                      <label className="apx-label" htmlFor={inputId}>{field.label}</label>
                      <input
                        id={inputId}
                        className="apx-input"
                        type="text"
                        value={profileLinks[field.id] ?? ''}
                        onChange={(event) => setLink(field.id, event.target.value)}
                        placeholder={field.placeholder}
                        autoComplete="off"
                        disabled={!storageAvailable}
                      />
                    </div>
                  );
                })}
              </div>
              <button className="apx-button apx-button--primary" type="submit" disabled={!storageAvailable} style={{ marginTop: 20 }}>
                Save in this browser
              </button>
              <p
                role={savedNotice?.tone === 'error' ? 'alert' : 'status'}
                aria-live={savedNotice?.tone === 'error' ? 'assertive' : 'polite'}
                aria-atomic="true"
                className={savedNotice?.tone === 'error' ? 'apx-auth-warning' : 'apx-field-help'}
              >
                {savedNotice?.text ?? (storageAvailable
                  ? 'No server copy will be created.'
                  : 'Local storage is unavailable; these fields are disabled.')}
              </p>
            </form>
          </section>

          <section className="apx-panel" aria-labelledby="entitlements-heading">
            <h2 id="entitlements-heading">Entitlements</h2>
            <p className="apx-section-intro"><strong>Status: unavailable.</strong> This page does not currently load purchase or subscription records, so it makes no entitlement claim.</p>
          </section>

          <section className="apx-panel" aria-labelledby="account-data-heading">
            <h2 id="account-data-heading">Account data and deletion</h2>
            <p className="apx-section-intro" id="account-operations-note">
              Export and account-deletion workflows are not implemented on this page. The controls remain disabled so no request is implied or lost.
            </p>
            <div className="apx-actions">
              <button className="apx-button" type="button" disabled aria-describedby="account-operations-note">
                Export account data — unavailable
              </button>
              <button className="apx-button apx-button--danger" type="button" disabled aria-describedby="account-operations-note">
                Delete account — unavailable
              </button>
              <Link className="apx-button" href="/docs/sovereignty">Read data sovereignty documentation</Link>
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
            Account identity comes from the active authentication session. Profile drafts remain local unless a future, explicit sync is implemented.
          </p>
        </main>
      </div>
    </>
  );
};

export default Account;
