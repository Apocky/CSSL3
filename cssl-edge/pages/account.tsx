// apocky.com/account · profile · linked OAuth · social-media-linkage · spending · sign-out
// Per spec/22 · cross-project entitlements + per-event opt-in revocation

import type { NextPage } from 'next';
import Head from 'next/head';
import { useEffect, useState } from 'react';
import { APOCKY_CHANNELS, AUTH_PROVIDERS, PROFILE_LINKABLE, getAuthClient, persistSessionToCookie } from '../lib/auth';

interface MeResponse {
  user: {
    email: string;
    id: string;
    provider: string;
    createdAt: string;
  } | null;
  stub?: boolean;
}

interface ProfileLinks {
  [key: string]: string;
}

const Account: NextPage = () => {
  const [me, setMe] = useState<MeResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [stubMode, setStubMode] = useState(false);
  const [profileLinks, setProfileLinks] = useState<ProfileLinks>({});
  const [savedNotice, setSavedNotice] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      // First : if we have a Supabase client AND a localStorage session, mirror to cookie
      // so server-side /api/auth/me can resolve us. This handles the post-magic-link case.
      const client = getAuthClient();
      if (client) {
        try {
          const { data } = await client.auth.getSession();
          if (data?.session?.access_token) {
            persistSessionToCookie(
              data.session.access_token,
              data.session.refresh_token ?? undefined,
            );
          }
        } catch {
          // ignore · server-side fetch will report null
        }
      }
      // Then : ask server who we are
      try {
        const res = await fetch('/api/auth/me', { cache: 'no-store' });
        const j: MeResponse = await res.json();
        if (cancelled) return;
        setMe(j);
        setStubMode(!!j.stub);

        // If server says null but client has a session, fall back to client-side identity
        if (!j.user && client) {
          const { data } = await client.auth.getUser();
          if (cancelled) return;
          if (data?.user?.email) {
            setMe({
              user: {
                id: data.user.id,
                email: data.user.email,
                provider: data.user.app_metadata?.provider ?? 'email',
                createdAt: data.user.created_at ?? new Date().toISOString(),
              },
            });
          }
        }
        setLoading(false);
      } catch {
        if (cancelled) return;
        setLoading(false);
        setStubMode(true);
        setMe({ user: null, stub: true });
      }
    })();
    return () => {
      cancelled = true;
    };
    try {
      const stored = JSON.parse(localStorage.getItem('apocky-profile-links') ?? '{}');
      setProfileLinks(stored);
    } catch {
      // ignore
    }
  }, []);

  function setLink(id: string, value: string) {
    setProfileLinks((prev) => ({ ...prev, [id]: value }));
  }

  function saveLinks() {
    try {
      localStorage.setItem('apocky-profile-links', JSON.stringify(profileLinks));
      setSavedNotice('✓ Saved locally. Server-side sync activates when Apocky-Hub Supabase is configured.');
      setTimeout(() => setSavedNotice(null), 4000);
    } catch {
      setSavedNotice('✗ Could not save · localStorage blocked');
    }
  }

  async function handleSignOut() {
    await fetch('/api/auth/logout', { method: 'POST' });
    location.href = '/';
  }

  if (loading) {
    return (
      <main
        style={{
          minHeight: '100vh',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          background: '#0a0a0f',
          color: '#7a7a8c',
          fontFamily: 'monospace',
        }}
      >
        <p>§ loading…</p>
      </main>
    );
  }

  return (
    <>
      <Head>
        <title>Account · Apocky</title>
        <meta name="description" content="Your apocky.com account · linked providers · social-media · entitlements" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <style>{`
          * { box-sizing: border-box; }
          html, body { margin: 0; padding: 0; }
          body {
            background: radial-gradient(ellipse at top, #15151f 0%, #0a0a0f 50%, #050507 100%);
            color: #e6e6f0;
            font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
            min-height: 100vh;
          }
          a { color: inherit; text-decoration: none; }
          input { font-family: inherit; }
        `}</style>
      </Head>
      <main style={{ maxWidth: 720, margin: '0 auto', padding: '5rem 1.5rem 6rem' }}>
        <a href="/" style={{ fontSize: '0.85rem', color: '#7a7a8c', display: 'inline-block', marginBottom: '2rem' }}>
          ← apocky.com
        </a>

        {stubMode && (
          <div
            style={{
              padding: '1rem 1.25rem',
              background: 'rgba(251, 191, 36, 0.08)',
              border: '1px solid rgba(251, 191, 36, 0.3)',
              borderRadius: 6,
              marginBottom: '2rem',
              fontSize: '0.85rem',
              color: '#fbbf24',
            }}
          >
            <strong>⚠ stub-mode</strong> · Apocky-Hub Supabase project not yet configured. Account features show
            interface-only · server-side persistence activates once <code>APOCKY_HUB_SUPABASE_URL</code> env var is set.
            Profile-link customizations save locally for now.
          </div>
        )}

        <h1
          style={{
            fontSize: '2rem',
            margin: 0,
            fontWeight: 700,
            backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%)',
            WebkitBackgroundClip: 'text',
            WebkitTextFillColor: 'transparent',
          }}
        >
          Your Account
        </h1>

        {/* ─── IDENTITY ─── */}
        <section style={{ marginTop: '2rem' }}>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.18em', color: '#7a7a8c' }}>
            § Identity
          </h2>
          {me?.user ? (
            <div
              style={{
                padding: '1rem 1.25rem',
                background: 'rgba(20, 20, 30, 0.5)',
                border: '1px solid #1f1f2a',
                borderRadius: 6,
                fontSize: '0.9rem',
              }}
            >
              <div style={{ marginBottom: '0.4rem' }}>
                <span style={{ color: '#7a7a8c' }}>email :</span>{' '}
                <code style={{ color: '#7dd3fc' }}>{me.user.email}</code>
              </div>
              <div style={{ marginBottom: '0.4rem' }}>
                <span style={{ color: '#7a7a8c' }}>signed in via :</span>{' '}
                <code style={{ color: '#a78bfa' }}>{me.user.provider}</code>
              </div>
              <div style={{ fontSize: '0.78rem', color: '#7a7a8c' }}>
                joined {new Date(me.user.createdAt).toLocaleDateString()}
              </div>
            </div>
          ) : (
            <div
              style={{
                padding: '1rem 1.25rem',
                background: 'rgba(20, 20, 30, 0.5)',
                border: '1px solid #1f1f2a',
                borderRadius: 6,
                fontSize: '0.9rem',
              }}
            >
              <p style={{ margin: 0, color: '#cdd6e4' }}>You are not signed in.</p>
              <div style={{ marginTop: '0.75rem', display: 'flex', gap: '0.5rem' }}>
                <a
                  href="/login"
                  style={{
                    padding: '0.5rem 1rem',
                    background: 'linear-gradient(135deg, #c084fc 0%, #7dd3fc 100%)',
                    color: '#0a0a0f',
                    fontWeight: 600,
                    borderRadius: 4,
                    fontSize: '0.85rem',
                  }}
                >
                  Sign in
                </a>
                <a
                  href="/register"
                  style={{
                    padding: '0.5rem 1rem',
                    border: '1px solid #2a2a3a',
                    color: '#e6e6f0',
                    borderRadius: 4,
                    fontSize: '0.85rem',
                  }}
                >
                  Create account
                </a>
              </div>
            </div>
          )}
        </section>

        {/* ─── LINKED OAUTH ─── */}
        <section style={{ marginTop: '2.5rem' }}>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.18em', color: '#7a7a8c' }}>
            § Linked sign-in providers
          </h2>
          <p style={{ color: '#a8a8b8', fontSize: '0.85rem', margin: '0.4rem 0 0.8rem' }}>
            Link multiple providers · sign in with any of them · unlinking leaves at-least-one (sovereignty)
          </p>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))', gap: '0.5rem' }}>
            {AUTH_PROVIDERS.filter((p) => p.enabled).map((p) => {
              const linked = me?.user?.provider === p.id;
              return (
                <div
                  key={p.id}
                  style={{
                    padding: '0.6rem 0.9rem',
                    background: 'rgba(20, 20, 30, 0.5)',
                    border: `1px solid ${linked ? 'rgba(52, 211, 153, 0.4)' : '#1f1f2a'}`,
                    borderRadius: 4,
                    fontSize: '0.85rem',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                  }}
                >
                  <span>{p.label}</span>
                  <span style={{ fontSize: '0.7rem', color: linked ? '#34d399' : '#5a5a6a' }}>
                    {linked ? '✓ linked' : '+ link'}
                  </span>
                </div>
              );
            })}
          </div>
        </section>

        {/* ─── PROFILE SOCIAL LINKS ─── */}
        <section style={{ marginTop: '2.5rem' }}>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.18em', color: '#7a7a8c' }}>
            § Your social channels
          </h2>
          <p style={{ color: '#a8a8b8', fontSize: '0.85rem', margin: '0.4rem 0 0.8rem' }}>
            Display these on your public profile · enables creator-attribution on Akashic-imprints (per spec/18) ·
            opt-in always
          </p>
          <div style={{ display: 'grid', gap: '0.5rem' }}>
            {PROFILE_LINKABLE.map((p) => (
              <div key={p.id} style={{ display: 'flex', gap: '0.5rem', alignItems: 'center' }}>
                <label
                  style={{
                    minWidth: 110,
                    fontSize: '0.82rem',
                    color: '#cdd6e4',
                  }}
                >
                  {p.label}
                </label>
                <input
                  type="text"
                  value={profileLinks[p.id] ?? ''}
                  onChange={(e) => setLink(p.id, e.target.value)}
                  placeholder={p.placeholder}
                  style={{
                    flex: 1,
                    padding: '0.5rem 0.75rem',
                    background: 'rgba(20, 20, 30, 0.7)',
                    border: '1px solid #2a2a3a',
                    borderRadius: 4,
                    color: '#e6e6f0',
                    fontSize: '0.85rem',
                    outline: 'none',
                  }}
                />
              </div>
            ))}
          </div>
          <button
            onClick={saveLinks}
            style={{
              marginTop: '0.85rem',
              padding: '0.55rem 1.1rem',
              background: 'rgba(124, 211, 252, 0.15)',
              border: '1px solid rgba(124, 211, 252, 0.4)',
              color: '#7dd3fc',
              borderRadius: 4,
              cursor: 'pointer',
              fontSize: '0.85rem',
              fontFamily: 'inherit',
            }}
          >
            ↗ Save my channels
          </button>
          {savedNotice && (
            <p style={{ marginTop: '0.5rem', color: '#34d399', fontSize: '0.78rem' }}>{savedNotice}</p>
          )}
        </section>

        {/* ─── ENTITLEMENTS ─── */}
        <section style={{ marginTop: '2.5rem' }}>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.18em', color: '#7a7a8c' }}>
            § Entitlements (cross-project)
          </h2>
          <p style={{ color: '#a8a8b8', fontSize: '0.85rem', margin: '0.4rem 0 0.8rem' }}>
            Purchases follow your account across all Apocky-projects · refundable within 14-day window
          </p>
          <div
            style={{
              padding: '1rem 1.25rem',
              background: 'rgba(20, 20, 30, 0.5)',
              border: '1px solid #1f1f2a',
              borderRadius: 6,
              fontSize: '0.85rem',
              color: '#a8a8b8',
            }}
          >
            <em style={{ color: '#5a5a6a' }}>No purchases yet · Stripe-checkout activates at v1.0 launch</em>
          </div>
        </section>

        {/* ─── DATA SOVEREIGNTY ─── */}
        <section style={{ marginTop: '2.5rem' }}>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.18em', color: '#7a7a8c' }}>
            § Data sovereignty
          </h2>
          <div style={{ display: 'grid', gap: '0.5rem', marginTop: '0.5rem' }}>
            <a
              href="/transparency"
              style={{
                padding: '0.6rem 1rem',
                background: 'rgba(20, 20, 30, 0.5)',
                border: '1px solid #1f1f2a',
                borderRadius: 4,
                fontSize: '0.85rem',
                color: '#cdd6e4',
              }}
            >
              View transparency dashboard · sovereign-cap audit · cocreative-bias · spending
            </a>
            <button
              type="button"
              style={{
                padding: '0.6rem 1rem',
                background: 'rgba(20, 20, 30, 0.5)',
                border: '1px solid #1f1f2a',
                borderRadius: 4,
                fontSize: '0.85rem',
                color: '#cdd6e4',
                cursor: 'pointer',
                textAlign: 'left',
                fontFamily: 'inherit',
              }}
              onClick={() => alert('Data export will be ready when Apocky-Hub Supabase is configured.')}
            >
              ↓ Download your data (GDPR / CCPA full archive)
            </button>
            <button
              type="button"
              style={{
                padding: '0.6rem 1rem',
                background: 'rgba(20, 20, 30, 0.5)',
                border: '1px solid rgba(248, 113, 113, 0.3)',
                borderRadius: 4,
                fontSize: '0.85rem',
                color: '#f87171',
                cursor: 'pointer',
                textAlign: 'left',
                fontFamily: 'inherit',
              }}
              onClick={() =>
                confirm('Delete account? 30-day grace period before permanent deletion. Public Akashic-imprints will be anonymized.') &&
                alert('Account deletion will be processed once Apocky-Hub Supabase is configured.')
              }
            >
              ✗ Delete account (30-day grace · public imprints anonymized)
            </button>
          </div>
        </section>

        {/* ─── SIGN OUT ─── */}
        {me?.user && (
          <section style={{ marginTop: '2.5rem' }}>
            <button
              onClick={handleSignOut}
              style={{
                padding: '0.6rem 1.2rem',
                background: 'transparent',
                border: '1px solid #2a2a3a',
                color: '#cdd6e4',
                borderRadius: 4,
                cursor: 'pointer',
                fontSize: '0.85rem',
                fontFamily: 'inherit',
              }}
            >
              Sign out
            </button>
          </section>
        )}

        {/* ─── APOCKY CHANNELS ─── */}
        <section style={{ marginTop: '4rem' }}>
          <h2 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.18em', color: '#7a7a8c' }}>
            § Apocky · creator
          </h2>
          <div style={{ marginTop: '0.5rem', display: 'flex', flexWrap: 'wrap', gap: '0.5rem' }}>
            {APOCKY_CHANNELS.map((c) => (
              <a
                key={c.href}
                href={c.href}
                target="_blank"
                rel="noopener noreferrer"
                style={{
                  padding: '0.4rem 0.85rem',
                  border: '1px solid #2a2a3a',
                  borderRadius: 4,
                  fontSize: '0.78rem',
                  color: '#cdd6e4',
                }}
              >
                {c.label} ↗
              </a>
            ))}
          </div>
        </section>

        <footer style={{ marginTop: '4rem', color: '#5a5a6a', fontSize: '0.75rem', textAlign: 'center' }}>
          § ¬ data-sale · ¬ surveillance · ¬ password-storage · sovereignty preserved · t∞
        </footer>
      </main>
    </>
  );
};

export default Account;
