// apocky.com · Portfolio Hub landing page
// Public project hub · auth-aware navigation + Lazarus entrypoint.

import type { NextPage } from 'next';
import Head from 'next/head';
import { useEffect, useState } from 'react';
import { consumeAuthCallbackFromLocation, readAuthCallbackParams } from '../lib/auth-callback';
import { normalizeAuthReturnPath } from '../lib/auth-return';
import { getAuthClient } from '../lib/auth';
import { authFetch } from '../lib/browser-auth';

type ProjectStatus = 'active' | 'docs' | 'private' | 'console';

interface Project {
  id: string;
  name: string;
  tagline: string;
  status: ProjectStatus;
  href: string;
  external?: boolean;
  accent: string;
  requiresAuth?: boolean;
}

const PROJECTS: ReadonlyArray<Project> = [
  {
    id: 'loa',
    name: 'LoA',
    tagline: 'Primary game project · active development · playable builds and project updates.',
    status: 'active',
    href: '/download',
    accent: '#c084fc',
  },
  {
    id: 'cssl',
    name: 'CSSL',
    tagline: 'Implementation language surface · public docs where useful · internals stay private.',
    status: 'docs',
    href: '/docs/cssl-language',
    accent: '#7dd3fc',
  },
  {
    id: 'csl',
    name: 'CSL',
    tagline: 'Dense specification notation · reasoning layer · project documentation format.',
    status: 'docs',
    href: '/docs',
    accent: '#34d399',
  },
  {
    id: 'apocrypha',
    name: 'Apocrypha',
    tagline: 'Continuously-thinking digital intelligence · chat + cockpit · Lazarus (Ω9) + Tessera (Ω10) absorbed.',
    status: 'console',
    href: '/admin/apocrypha',
    accent: '#ffaa55',
    requiresAuth: true,
  },
];

const STATUS_LABEL: Record<ProjectStatus, string> = {
  active: 'ACTIVE',
  docs: 'DOCS',
  private: 'PRIVATE',
  console: 'CONSOLE',
};

const STATUS_COLOR: Record<ProjectStatus, string> = {
  active: '#c084fc',
  docs: '#7dd3fc',
  private: '#9aa0a6',
  console: '#fbbf24',
};

const Home: NextPage = () => {
  const [authed, setAuthed] = useState<boolean | null>(null);
  const [authNotice, setAuthNotice] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const callbackParams = readAuthCallbackParams(location.search, location.hash);
      if (callbackParams.hasCallback) {
        const returnTo = normalizeAuthReturnPath(new URLSearchParams(location.search).get('next'), '');
        setAuthNotice('finishing sign-in…');
        const callbackResult = await consumeAuthCallbackFromLocation();
        if (cancelled) return;
        if (callbackResult.ok) {
          if (returnTo) {
            location.replace(returnTo);
            return;
          }
          setAuthNotice('signed in · session saved');
          setAuthed(true);
        } else {
          setAuthNotice(`sign-in failed · ${callbackResult.reason ?? 'try again from /login'}`);
          setAuthed(false);
          return;
        }
      }

      let browserAuthed = false;
      const client = getAuthClient();
      if (client) {
        try {
          const { data } = await client.auth.getSession();
          browserAuthed = !!data.session;
        } catch {
          browserAuthed = false;
        }
      }
      try {
        const res = await authFetch('/api/auth/me', { cache: 'no-store' });
        const json = await res.json() as { user?: unknown };
        if (!cancelled) setAuthed(Boolean(json.user) || browserAuthed);
      } catch {
        if (!cancelled) setAuthed(browserAuthed);
      }
    })();
    return () => { cancelled = true; };
  }, []);

  return (
    <>
      <Head>
        <title>Apocky · LoA · CSSL · CSL · Apocrypha</title>
        <meta name="description" content="Apocky project hub for LoA, CSSL, CSL, and Apocrypha (cognitive substrate with Lazarus + Tessera absorbed as sub-minds)." />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="author" content="Apocky" />
        <meta name="theme-color" content="#0a0a0f" />
        <meta name="mobile-web-app-capable" content="yes" />
        <meta name="apple-mobile-web-app-capable" content="yes" />
        <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent" />
        <meta property="og:title" content="Apocky · LoA · CSSL · CSL · Apocrypha" />
        <meta property="og:description" content="Project hub for LoA, CSSL, CSL, and Apocrypha." />
        <meta property="og:type" content="website" />
        <meta property="og:url" content="https://apocky.com" />
        <meta property="og:site_name" content="Apocky" />
        <meta name="twitter:card" content="summary_large_image" />
        <meta name="twitter:title" content="Apocky · Projects" />
        <meta name="twitter:description" content="LoA, CSSL, CSL, and Apocrypha." />
        <link rel="canonical" href="https://apocky.com" />
        <style>{`
          * { box-sizing: border-box; }
          html { margin: 0; padding: 0; background-color: #0a0a0f; min-height: 100%; }
          body { margin: 0; padding: 0; background-color: #0a0a0f; }
          body {
            background-image: radial-gradient(ellipse at top, #15151f 0%, #0a0a0f 50%, #050507 100%);
            color: #e6e6f0;
            font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, 'Liberation Mono', monospace;
            min-height: 100vh;
            min-height: 100dvh;
            -webkit-font-smoothing: antialiased;
            -webkit-text-size-adjust: 100%;
          }
          a { color: inherit; text-decoration: none; }
          a:hover { opacity: 0.85; }
          @keyframes pulse-spore {
            0%, 100% { opacity: 0.3; transform: scale(1); }
            50% { opacity: 0.7; transform: scale(1.1); }
          }
        `}</style>
      </Head>
      <main
        style={{
          maxWidth: 1080,
          margin: '0 auto',
          padding: '5rem 1.5rem 6rem',
          lineHeight: 1.6,
        }}
      >
        {/* ─── NAV-BAR ─── */}
        <nav
          aria-label="primary"
          style={{
            display: 'flex',
            flexWrap: 'wrap',
            gap: '1.25rem',
            paddingBottom: '2rem',
            marginBottom: '2.5rem',
            borderBottom: '1px solid #1f1f2a',
            fontSize: '0.82rem',
            color: '#a8a8b8',
          }}
        >
          <a href="/download">LoA</a>
          <a href="/docs/cssl-language">CSSL</a>
          <a href="/docs">CSL</a>
          {authed ? <a href="/admin/apocrypha" style={{ color: '#ffaa55' }}>Apocrypha</a> : null}
          <span style={{ flexGrow: 1 }} />
          {authed ? (
            <a href="/account" style={{ color: '#34d399' }}>Account ✓</a>
          ) : authed === null ? (
            <span style={{ color: '#5a5a6a' }}>checking session…</span>
          ) : (
            <a href="/login">Sign in</a>
          )}
        </nav>

        {/* ─── HERO ─── */}
        <section style={{ marginBottom: '5rem' }}>
          <div
            style={{
              display: 'inline-block',
              padding: '0.25rem 0.75rem',
              border: '1px solid #2a2a3a',
              borderRadius: 4,
              fontSize: '0.7rem',
              letterSpacing: '0.15em',
              color: '#a78bfa',
              marginBottom: '1.5rem',
              textTransform: 'uppercase',
            }}
          >
            § Apocky · Project Hub
          </div>
          <h1
            style={{
              fontSize: 'clamp(2rem, 5vw, 3.5rem)',
              lineHeight: 1.1,
              margin: 0,
              fontWeight: 700,
              letterSpacing: '-0.02em',
              backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%)',
              WebkitBackgroundClip: 'text',
              WebkitTextFillColor: 'transparent',
            }}
          >
            LoA · CSSL · CSL
            <br />
            Apocrypha.
          </h1>
          <p
            style={{
              fontSize: '1.05rem',
              color: '#a8a8b8',
              marginTop: '1.25rem',
              maxWidth: 640,
            }}
          >
            Public entry points for the actual active projects. Details stay sparse by design:
            enough to navigate, not enough to leak internals.
          </p>
          <p
            style={{
              fontSize: '0.95rem',
              color: '#cdd6e4',
              marginTop: '0.75rem',
              maxWidth: 640,
            }}
          >
            {authed === true
              ? 'signed in · Apocrypha cockpit available'
              : authed === false
                ? authNotice ?? 'sign in to access private project controls'
                : 'checking session…'}
          </p>
          <div style={{ marginTop: '2rem', display: 'flex', flexWrap: 'wrap', gap: '0.75rem' }}>
            <a
              href={authed ? '/admin/apocrypha' : '#projects'}
              style={{
                padding: '0.75rem 1.5rem',
                background: 'linear-gradient(135deg, #c084fc 0%, #7dd3fc 100%)',
                color: '#0a0a0f',
                fontWeight: 600,
                borderRadius: 4,
                fontSize: '0.95rem',
              }}
            >
              {authed ? 'Open Apocrypha →' : 'Explore projects →'}
            </a>
            {authed === true ? (
              <a
                href="/account"
                style={{
                  padding: '0.75rem 1.5rem',
                  border: '1px solid #2a2a3a',
                  color: '#e6e6f0',
                  borderRadius: 4,
                  fontSize: '0.95rem',
                }}
              >
                Account ✓
              </a>
            ) : authed === false ? (
              <a
                href="/login"
                style={{
                  padding: '0.75rem 1.5rem',
                  border: '1px solid #2a2a3a',
                  color: '#e6e6f0',
                  borderRadius: 4,
                  fontSize: '0.95rem',
                }}
              >
                Sign in
              </a>
            ) : (
              <span
                style={{
                  padding: '0.75rem 1.5rem',
                  border: '1px solid #2a2a3a',
                  color: '#7a7a8c',
                  borderRadius: 4,
                  fontSize: '0.95rem',
                }}
              >
                checking session…
              </span>
            )}
            <a
              href="https://github.com/Apocky"
              style={{
                padding: '0.75rem 1.5rem',
                border: '1px solid #2a2a3a',
                color: '#e6e6f0',
                borderRadius: 4,
                fontSize: '0.95rem',
              }}
            >
              GitHub ↗
            </a>
          </div>
        </section>

        {/* ─── PROJECTS GRID ─── */}
        <section id="projects" style={{ marginBottom: '5rem' }}>
          <h2
            style={{
              fontSize: '0.75rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '1.5rem',
            }}
          >
            § Projects
          </h2>
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))',
              gap: '1.25rem',
            }}
          >
            {PROJECTS.map((p) => {
              const clickable = !p.requiresAuth || authed === true;
              const cardStyle: React.CSSProperties = {
                display: 'block',
                padding: '1.5rem',
                background: 'rgba(20, 20, 30, 0.5)',
                border: '1px solid #1f1f2a',
                borderRadius: 8,
                transition: 'border-color 150ms, background 150ms',
                position: 'relative',
                opacity: clickable ? 1 : 0.85,
                cursor: clickable ? 'pointer' : 'default',
              };
              const inner = (
                <>
                  <div
                    aria-hidden="true"
                    style={{
                      position: 'absolute',
                      top: 12,
                      right: 12,
                      width: 8,
                      height: 8,
                      borderRadius: '50%',
                      background: STATUS_COLOR[p.status],
                      animation: p.status === 'active' || p.status === 'console' ? 'pulse-spore 2.5s ease-in-out infinite' : 'none',
                    }}
                  />
                  <div
                    style={{
                      fontSize: '0.65rem',
                      letterSpacing: '0.15em',
                      color: STATUS_COLOR[p.status],
                      marginBottom: '0.5rem',
                    }}
                  >
                    {STATUS_LABEL[p.status]}
                  </div>
                  <h3
                    style={{
                      fontSize: '1.15rem',
                      margin: 0,
                      color: p.accent,
                      fontWeight: 600,
                    }}
                  >
                    {p.name}
                    {clickable && p.external ? <span style={{ color: '#5a5a6a', fontSize: '0.85rem', marginLeft: 6 }}>↗</span> : null}
                  </h3>
                  <p
                    style={{
                      fontSize: '0.88rem',
                      color: '#a0a0b0',
                      marginTop: '0.6rem',
                      marginBottom: 0,
                      lineHeight: 1.5,
                    }}
                  >
                    {p.tagline}
                  </p>
                </>
              );
              if (clickable) {
                return (
                  <a
                    key={p.id}
                    href={p.href}
                    target={p.external ? '_blank' : undefined}
                    rel={p.external ? 'noopener noreferrer' : undefined}
                    style={cardStyle}
                  >
                    {inner}
                  </a>
                );
              }
              return (
                <div key={p.id} style={cardStyle}>
                  {inner}
                </div>
              );
            })}
          </div>
        </section>

        {/* ─── ABOUT ─── */}
        <section style={{ marginBottom: '5rem' }}>
          <h2
            style={{
              fontSize: '0.75rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '1.5rem',
            }}
          >
            § About
          </h2>
          <p style={{ color: '#cdd6e4', fontSize: '0.95rem' }}>
            Apocky builds LoA and its supporting language, notation, and autonomous development tooling.
            Public pages stay intentionally concise; private controls live behind signed-in admin access.
          </p>
        </section>

        {/* ─── FOOTER ─── */}
        <footer
          style={{
            paddingTop: '2.5rem',
            borderTop: '1px solid #1f1f2a',
            color: '#5a5a6a',
            fontSize: '0.78rem',
          }}
        >
          <div
            style={{
              display: 'flex',
              flexWrap: 'wrap',
              gap: '1rem 1.5rem',
              marginBottom: '1.25rem',
            }}
          >
            <a href="/docs">Docs</a>
            <a href="/devblog">Devblog</a>
            <a href="/press">Press</a>
            <a href="/download">Download</a>
            {authed ? <a href="/admin/apocrypha">Apocrypha</a> : null}
            <span style={{ color: '#2a2a3a' }}>|</span>
            <a href="/legal/privacy">Privacy</a>
            <a href="/legal/terms">Terms</a>
            <a href="/legal/eula">EULA</a>
            <a href="/legal/privacy">Privacy Controls</a>
            <a href="/api/health">Status</a>
            <a href="mailto:apocky13@gmail.com">Contact</a>
          </div>
          <p style={{ margin: 0 }}>
            § ¬ harm in the making · sovereignty preserved · t∞
          </p>
          <p style={{ margin: '0.4rem 0 0' }}>
            © {new Date().getFullYear()} Apocky.
          </p>
        </footer>
      </main>
    </>
  );
};

export default Home;
