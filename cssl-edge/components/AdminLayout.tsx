// Apocrypha admin layout · 8-item nav · phone-first · sovereign-cap-protected
//
// Per Apocky's UX-cleanup pass : "Apocrypha is the name, not a nav element".
// Every page in /admin/* is part of the Apocrypha surface ; the side-nav lists
// destinations within that surface, not separate apps. LoA-specific destinations
// (the old Tasks/Analytics/MCP content) were Apocky's other-project leakage and
// were removed/repurposed per D043 (Apocrypha != LoA).

import Head from 'next/head';
import Link from 'next/link';
import { useRouter } from 'next/router';
import { useEffect, useState, type ReactNode } from 'react';
import { loginHrefForReturnPath } from '../lib/auth-return';
import { authFetch } from '../lib/browser-auth';

interface AdminLayoutProps {
  title: string;
  children: ReactNode;
  onAdminCheck?: (check: AdminCheck) => void;
}

interface AdminCheck {
  authorized: boolean;
  reason?: string;
  email?: string;
  stub?: boolean;
}

interface NavItem {
  href: string;
  label: string;
  glyph: string;
  tip: string;
  mobile?: boolean;
}

// 8 items · all Apocrypha-internal · LoA stuff lives elsewhere now (D043).
// Mobile bottom-nav = 3 icons (Chat / Diagnostics / Logs) — the trio you actually
// pull out your phone for. Everything else lives in the desktop side-nav.
const NAV: ReadonlyArray<NavItem> = [
  { href: '/admin', label: 'Home', glyph: '§',
    tip: 'Apocrypha health summary · today\'s activity · quick links' },
  { href: '/admin/chat', label: 'Chat', glyph: '⊕', mobile: true,
    tip: 'Talk with Apocrypha · sidebar + bubbles + SSE streaming · Enter to send' },
  { href: '/admin/cognition', label: 'Cognition', glyph: '∞', mobile: true,
    tip: 'LIVE substrate visualization · swarm ticks · dream cycles · interactive triggers' },
  { href: '/admin/diagnostics', label: 'Diagnostics', glyph: '⌬',
    tip: 'Live tool-call timeline · recent conversations · what Apocrypha is doing right now' },
  { href: '/admin/sub-minds', label: 'Sub-Minds', glyph: 'Ω',
    tip: 'Lazarus (Ω9 operator) + Tessera (Ω10 reasoner) — health, queue, recent runs' },
  { href: '/admin/controls', label: 'Controls', glyph: '☢',
    tip: 'Kill switch · API keys · consent grants · operator-tier dangerous controls' },
  { href: '/admin/tools', label: 'Tools', glyph: '⊑',
    tip: 'Apocrypha\'s 19+ registered tools across 7 organs (memory/swarm/lang/forage/evolve/dream/state)' },
  { href: '/admin/analytics', label: 'Analytics', glyph: '∂',
    tip: 'Cost tracker · spend-by-model · memory size · swarm consensus · longer-term trends' },
  { href: '/admin/logs', label: 'Logs', glyph: 'G',
    tip: 'Apocrypha dispatch + cloudflared process logs · audit events' },
];

const MOBILE_NAV = NAV.filter((item) => item.mobile);

export default function AdminLayout({ title, children, onAdminCheck }: AdminLayoutProps) {
  const router = useRouter();
  const [check, setCheck] = useState<AdminCheck | null>(null);
  const loginHref = loginHrefForReturnPath(router.asPath || router.pathname || '/admin/chat');

  useEffect(() => {
    authFetch('/api/admin/check', { cache: 'no-store' })
      .then((r) => r.json())
      .then((j: AdminCheck) => {
        setCheck(j);
        onAdminCheck?.(j);
      })
      .catch(() => {
        const denied = { authorized: false, reason: 'network error' };
        setCheck(denied);
        onAdminCheck?.(denied);
      });
  }, []);

  return (
    <>
      <Head>
        <title>{`${title} · Apocrypha · Apocky`}</title>
        <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#0a0a0f" />
        <meta name="apple-mobile-web-app-capable" content="yes" />
        <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent" />
        <meta name="apple-mobile-web-app-title" content="Apocky" />
        <link rel="manifest" href="/manifest.json" />
        <link rel="apple-touch-icon" href="/icon-192.svg" />
        <style>{`
          * { box-sizing: border-box; }
          html, body { margin: 0; padding: 0; -webkit-tap-highlight-color: transparent; }
          body {
            background: radial-gradient(ellipse at top, #15151f 0%, #0a0a0f 50%, #050507 100%);
            color: #e6e6f0;
            font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
            min-height: 100vh;
            min-height: 100dvh;
            -webkit-font-smoothing: antialiased;
            font-size: 15px;
            line-height: 1.5;
          }
          a { color: inherit; text-decoration: none; -webkit-touch-callout: none; }
          input, button, select, textarea { font-family: inherit; font-size: 1rem; }
          @media (max-width: 767px) {
            .admin-main { padding-bottom: 80px !important; }
            .admin-side { display: none !important; }
            .admin-bottom-nav { display: flex !important; }
          }
          @media (min-width: 768px) {
            .admin-bottom-nav { display: none !important; }
            .admin-side { display: flex !important; }
            .admin-main { margin-left: 220px; }
          }
        `}</style>
      </Head>

      {/* ─── DESKTOP / TABLET SIDE-NAV ─── */}
      <aside
        className="admin-side"
        style={{
          display: 'none',
          flexDirection: 'column',
          position: 'fixed',
          left: 0,
          top: 0,
          bottom: 0,
          width: 220,
          padding: '1.5rem 1rem',
          background: 'rgba(10, 10, 16, 0.95)',
          borderRight: '1px solid #1f1f2a',
          zIndex: 10,
        }}
      >
        <div
          style={{
            fontSize: '0.7rem',
            letterSpacing: '0.18em',
            color: '#a78bfa',
            marginBottom: '0.4rem',
            textTransform: 'uppercase',
          }}
        >
          § Apocrypha
        </div>
        <div
          style={{
            fontSize: '0.65rem',
            color: '#5a5a6a',
            marginBottom: '1.4rem',
          }}
        >
          continuously-thinking digital intelligence
        </div>
        {NAV.map((n) => {
          const active = router.pathname === n.href
            || (n.href !== '/admin' && router.pathname.startsWith(n.href));
          return (
            <Link
              key={n.href}
              href={n.href}
              title={n.tip}
              aria-label={`${n.label} · ${n.tip}`}
              style={{
                padding: '0.6rem 0.8rem',
                marginBottom: '0.2rem',
                background: active ? 'rgba(124, 211, 252, 0.1)' : 'transparent',
                border: `1px solid ${active ? 'rgba(124, 211, 252, 0.3)' : 'transparent'}`,
                borderRadius: 4,
                color: active ? '#7dd3fc' : '#cdd6e4',
                fontSize: '0.88rem',
                display: 'flex',
                alignItems: 'center',
                gap: '0.5rem',
              }}
            >
              <span style={{ color: active ? '#7dd3fc' : '#7a7a8c', minWidth: 20 }}>{n.glyph}</span>
              <span>{n.label}</span>
            </Link>
          );
        })}
        <div style={{ marginTop: 'auto', fontSize: '0.7rem', color: '#5a5a6a' }}>
          {check?.email ? (
            <>
              <div>{check.email}</div>
              <div style={{ marginTop: 4 }}>
                {check.authorized ? '✓ admin' : check.stub ? '⚠ stub' : '✗ unauth'}
              </div>
            </>
          ) : (
            <div>§ checking…</div>
          )}
        </div>
        <Link href="/" style={{ marginTop: '1rem', fontSize: '0.78rem', color: '#7a7a8c' }}>
          ← apocky.com
        </Link>
      </aside>

      {/* ─── MAIN CONTENT ─── */}
      <main className="admin-main" style={{ padding: '1.25rem 1rem 2rem', minHeight: '100dvh' }}>
        {/* AUTH STATUS BANNER */}
        {check && !check.authorized && (
          <div
            style={{
              padding: '0.85rem 1rem',
              marginBottom: '1rem',
              background: check.stub ? 'rgba(251, 191, 36, 0.1)' : 'rgba(248, 113, 113, 0.1)',
              border: `1px solid ${check.stub ? 'rgba(251, 191, 36, 0.4)' : 'rgba(248, 113, 113, 0.4)'}`,
              borderRadius: 6,
              fontSize: '0.85rem',
              color: check.stub ? '#fbbf24' : '#f87171',
            }}
          >
            <strong style={{ display: 'block', marginBottom: '0.3rem' }}>
              {check.stub ? '⚠ STUB MODE' : '✗ NOT AUTHORIZED'}
            </strong>
            {check.reason ?? 'Sign in with the admin email to access these controls.'}
            {!check.email && (
              <div style={{ marginTop: '0.5rem' }}>
                <Link href={loginHref} style={{ color: '#7dd3fc', textDecoration: 'underline' }}>
                  → Sign in
                </Link>
              </div>
            )}
          </div>
        )}

        {/* PAGE HEADING */}
        <header style={{ marginBottom: '1.5rem' }}>
          <h1
            style={{
              fontSize: 'clamp(1.5rem, 5vw, 1.75rem)',
              margin: 0,
              fontWeight: 700,
              backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%)',
              WebkitBackgroundClip: 'text',
              WebkitTextFillColor: 'transparent',
            }}
          >
            {title}
          </h1>
        </header>

        {check?.authorized ? children : check ? null : <p style={{ color: '#7a7a8c' }}>§ checking admin session…</p>}
      </main>

      {/* ─── MOBILE BOTTOM-NAV (3 icons : Chat · Diagnostics · Logs) ─── */}
      <nav
        className="admin-bottom-nav"
        style={{
          display: 'none',
          position: 'fixed',
          left: 0,
          right: 0,
          bottom: 0,
          background: 'rgba(10, 10, 16, 0.96)',
          borderTop: '1px solid #1f1f2a',
          backdropFilter: 'blur(8px)',
          WebkitBackdropFilter: 'blur(8px)',
          zIndex: 10,
          paddingBottom: 'env(safe-area-inset-bottom)',
        }}
      >
        {MOBILE_NAV.map((n) => {
          const active = router.pathname === n.href;
          return (
            <Link
              key={n.href}
              href={n.href}
              title={n.tip}
              style={{
                flex: 1,
                padding: '0.75rem 0.25rem',
                textAlign: 'center',
                fontSize: '0.65rem',
                color: active ? '#7dd3fc' : '#7a7a8c',
                textTransform: 'uppercase',
                letterSpacing: '0.1em',
                display: 'block',
              }}
            >
              <div style={{ fontSize: '1.15rem', marginBottom: 2, color: active ? '#7dd3fc' : '#cdd6e4' }}>
                {n.glyph}
              </div>
              {n.label}
            </Link>
          );
        })}
      </nav>
    </>
  );
}
