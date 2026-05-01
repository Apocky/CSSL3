// /admin · main dashboard · health · recent activity · quick-actions
// Phone-first responsive · sovereign-cap-protected

import type { NextPage } from 'next';
import { useEffect, useState } from 'react';
import AdminLayout from '../../components/AdminLayout';

interface HealthData {
  status: string;
  commit?: string;
  uptime_s?: number;
  stripe_configured?: boolean;
  supabase_configured?: boolean;
}

interface SystemCard {
  label: string;
  value: string;
  glyph: string;
  accent: string;
  href?: string;
}

const Dashboard: NextPage = () => {
  const [health, setHealth] = useState<HealthData | null>(null);
  const [now, setNow] = useState<string>('');

  useEffect(() => {
    fetch('/api/health')
      .then((r) => r.json())
      .then((j) => setHealth(j))
      .catch(() => setHealth({ status: 'unknown' }));
    setNow(new Date().toLocaleString());
    const t = setInterval(() => setNow(new Date().toLocaleString()), 1000);
    return () => clearInterval(t);
  }, []);

  const cards: SystemCard[] = [
    {
      label: 'apocky.com',
      value: health?.status === 'ok' ? '✓ live' : '◐ checking',
      glyph: '⊑',
      accent: '#34d399',
      href: '/',
    },
    {
      label: 'Stripe',
      value: health?.stripe_configured ? '✓ configured' : '◐ stub-mode',
      glyph: '$',
      accent: health?.stripe_configured ? '#34d399' : '#fbbf24',
      href: '/admin/payments',
    },
    {
      label: 'Supabase',
      value: health?.supabase_configured ? '✓ live' : '◐ stub-mode',
      glyph: '◇',
      accent: health?.supabase_configured ? '#34d399' : '#fbbf24',
    },
    {
      label: 'LoA-alpha',
      value: '✓ shipped',
      glyph: '※',
      accent: '#c084fc',
      href: '/download',
    },
    {
      label: 'Mycelium',
      value: '◐ W10 building',
      glyph: '⌬',
      accent: '#a78bfa',
    },
    {
      label: 'Σ-Chain',
      value: '○ planning',
      glyph: 'Σ',
      accent: '#7dd3fc',
    },
  ];

  const quickActions: Array<{ label: string; href: string; glyph: string }> = [
    { label: 'View scheduled tasks', href: '/admin/tasks', glyph: '◐' },
    { label: 'Approve pending Coder edits', href: '/admin/coder', glyph: 'W!' },
    { label: 'Invoke MCP tool', href: '/admin/mcp', glyph: '⊑' },
    { label: 'View audit logs', href: '/admin/logs', glyph: '✓' },
  ];

  return (
    <AdminLayout title="§ Apocky Console">
      <p style={{ color: '#7a7a8c', fontSize: '0.82rem', marginTop: 0 }}>
        {now} · phone + desktop · sovereign-cap protected · install as PWA from Share menu
      </p>

      {/* SYSTEM HEALTH GRID */}
      <section style={{ marginTop: '1.5rem' }}>
        <h2
          style={{
            fontSize: '0.65rem',
            textTransform: 'uppercase',
            letterSpacing: '0.18em',
            color: '#7a7a8c',
            marginBottom: '0.6rem',
          }}
        >
          § System health
        </h2>
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fill, minmax(140px, 1fr))',
            gap: '0.6rem',
          }}
        >
          {cards.map((c) =>
            c.href ? (
              <a
                key={c.label}
                href={c.href}
                style={{
                  padding: '0.85rem 0.85rem',
                  background: 'rgba(20, 20, 30, 0.5)',
                  border: '1px solid #1f1f2a',
                  borderRadius: 6,
                  textDecoration: 'none',
                  color: 'inherit',
                  display: 'block',
                }}
              >
                <div style={{ fontSize: '1.5rem', color: c.accent, marginBottom: 6 }}>{c.glyph}</div>
                <div style={{ fontSize: '0.7rem', color: '#7a7a8c', marginBottom: 2 }}>{c.label}</div>
                <div style={{ fontSize: '0.82rem', color: c.accent }}>{c.value}</div>
              </a>
            ) : (
              <div
                key={c.label}
                style={{
                  padding: '0.85rem',
                  background: 'rgba(20, 20, 30, 0.5)',
                  border: '1px solid #1f1f2a',
                  borderRadius: 6,
                }}
              >
                <div style={{ fontSize: '1.5rem', color: c.accent, marginBottom: 6 }}>{c.glyph}</div>
                <div style={{ fontSize: '0.7rem', color: '#7a7a8c', marginBottom: 2 }}>{c.label}</div>
                <div style={{ fontSize: '0.82rem', color: c.accent }}>{c.value}</div>
              </div>
            ),
          )}
        </div>
      </section>

      {/* QUICK ACTIONS */}
      <section style={{ marginTop: '2rem' }}>
        <h2
          style={{
            fontSize: '0.65rem',
            textTransform: 'uppercase',
            letterSpacing: '0.18em',
            color: '#7a7a8c',
            marginBottom: '0.6rem',
          }}
        >
          § Quick actions
        </h2>
        <div style={{ display: 'grid', gap: '0.5rem' }}>
          {quickActions.map((a) => (
            <a
              key={a.href}
              href={a.href}
              style={{
                padding: '0.85rem 1rem',
                background: 'rgba(20, 20, 30, 0.5)',
                border: '1px solid #1f1f2a',
                borderRadius: 6,
                fontSize: '0.92rem',
                color: '#cdd6e4',
                display: 'flex',
                alignItems: 'center',
                gap: '0.6rem',
                textDecoration: 'none',
                minHeight: 44, // touch target
              }}
            >
              <span style={{ fontSize: '1rem', color: '#a78bfa', minWidth: 24 }}>{a.glyph}</span>
              <span style={{ flex: 1 }}>{a.label}</span>
              <span style={{ color: '#5a5a6a' }}>→</span>
            </a>
          ))}
        </div>
      </section>

      {/* MONITORED PROJECTS */}
      <section style={{ marginTop: '2rem' }}>
        <h2
          style={{
            fontSize: '0.65rem',
            textTransform: 'uppercase',
            letterSpacing: '0.18em',
            color: '#7a7a8c',
            marginBottom: '0.6rem',
          }}
        >
          § External
        </h2>
        <div style={{ display: 'grid', gap: '0.4rem' }}>
          {[
            { label: 'Vercel dashboard', href: 'https://vercel.com/shawn-bakers-projects' },
            { label: 'Supabase dashboard', href: 'https://supabase.com/dashboard' },
            { label: 'Stripe dashboard', href: 'https://dashboard.stripe.com' },
            { label: 'GitHub repo', href: 'https://github.com/Apocky/CSSL3' },
            { label: 'github.com/Apocky', href: 'https://github.com/Apocky' },
          ].map((l) => (
            <a
              key={l.href}
              href={l.href}
              target="_blank"
              rel="noopener noreferrer"
              style={{
                padding: '0.7rem 1rem',
                background: 'rgba(20, 20, 30, 0.4)',
                border: '1px solid #1f1f2a',
                borderRadius: 4,
                fontSize: '0.85rem',
                color: '#cdd6e4',
                textDecoration: 'none',
                minHeight: 44,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
              }}
            >
              <span>{l.label}</span>
              <span style={{ color: '#7dd3fc' }}>↗</span>
            </a>
          ))}
        </div>
      </section>

      <footer
        style={{
          marginTop: '3rem',
          paddingTop: '1.5rem',
          borderTop: '1px solid #1f1f2a',
          fontSize: '0.7rem',
          color: '#5a5a6a',
          textAlign: 'center',
        }}
      >
        § sovereign-cap revoke : Ctrl+Shift+Alt+S · or sign-out
      </footer>
    </AdminLayout>
  );
};

export default Dashboard;
