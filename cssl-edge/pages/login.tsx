// apocky.com/login · sign-in with email-magic-link or OAuth provider
// Per spec/22 · single SSO across all Apocky-projects via hub-Supabase JWT

import type { NextPage } from 'next';
import Head from 'next/head';
import { useState } from 'react';
import { AUTH_PROVIDERS } from '../lib/auth';

const Login: NextPage = () => {
  const [email, setEmail] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [stubMode, setStubMode] = useState<boolean | null>(null);

  async function handleMagicLink(e: React.FormEvent) {
    e.preventDefault();
    if (!email || submitting) return;
    setSubmitting(true);
    setMessage(null);
    try {
      const res = await fetch('/api/auth/magic-link', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email, redirectTo: `${location.origin}/auth/callback` }),
      });
      const json = await res.json();
      if (json.stub) setStubMode(true);
      setMessage(json.message ?? (json.ok ? '✓ Check your email for the sign-in link' : '✗ Failed · check the email'));
    } catch (err) {
      setMessage('✗ Network error · try again');
    } finally {
      setSubmitting(false);
    }
  }

  async function handleOAuth(provider: string) {
    setMessage(null);
    try {
      const res = await fetch('/api/auth/oauth', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ provider, redirectTo: `${location.origin}/auth/callback` }),
      });
      const json = await res.json();
      if (json.stub) {
        setStubMode(true);
        setMessage('Stub mode · OAuth not connected yet · pending Apocky-Hub Supabase signup');
        return;
      }
      if (json.url) {
        location.href = json.url;
      } else {
        setMessage('✗ OAuth init failed');
      }
    } catch (err) {
      setMessage('✗ Network error');
    }
  }

  return (
    <>
      <Head>
        <title>Sign in · Apocky</title>
        <meta name="description" content="Sign in to apocky.com · single account across all Apocky-projects" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <style>{`
          * { box-sizing: border-box; }
          html, body { margin: 0; padding: 0; }
          body {
            background: radial-gradient(ellipse at top, #15151f 0%, #0a0a0f 50%, #050507 100%);
            color: #e6e6f0;
            font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
            min-height: 100vh;
            -webkit-font-smoothing: antialiased;
          }
          a { color: inherit; text-decoration: none; }
          input { font-family: inherit; }
        `}</style>
      </Head>
      <main style={{ maxWidth: 440, margin: '0 auto', padding: '5rem 1.5rem' }}>
        <a href="/" style={{ fontSize: '0.85rem', color: '#7a7a8c', display: 'inline-block', marginBottom: '2rem' }}>
          ← apocky.com
        </a>

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
          Sign in
        </h1>
        <p style={{ color: '#a8a8b8', marginTop: '0.5rem', fontSize: '0.92rem' }}>
          One account · all Apocky-projects · sovereign-revocable
        </p>

        {/* ─── EMAIL MAGIC-LINK ─── */}
        <form onSubmit={handleMagicLink} style={{ marginTop: '2.5rem' }}>
          <label
            style={{
              display: 'block',
              fontSize: '0.7rem',
              textTransform: 'uppercase',
              letterSpacing: '0.18em',
              color: '#7a7a8c',
              marginBottom: '0.5rem',
            }}
          >
            § Email magic-link
          </label>
          <div style={{ display: 'flex', gap: '0.5rem' }}>
            <input
              type="email"
              required
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="you@example.com"
              style={{
                flex: 1,
                padding: '0.7rem 0.9rem',
                background: 'rgba(20, 20, 30, 0.7)',
                border: '1px solid #2a2a3a',
                borderRadius: 4,
                color: '#e6e6f0',
                fontSize: '0.95rem',
                outline: 'none',
              }}
            />
            <button
              type="submit"
              disabled={submitting || !email}
              style={{
                padding: '0.7rem 1.2rem',
                background: 'linear-gradient(135deg, #c084fc 0%, #7dd3fc 100%)',
                color: '#0a0a0f',
                fontWeight: 600,
                border: 'none',
                borderRadius: 4,
                cursor: submitting || !email ? 'not-allowed' : 'pointer',
                opacity: submitting || !email ? 0.5 : 1,
                fontSize: '0.92rem',
              }}
            >
              {submitting ? '…' : '→ Send link'}
            </button>
          </div>
          <p style={{ color: '#7a7a8c', fontSize: '0.78rem', marginTop: '0.5rem' }}>
            We'll email you a secure one-tap link · no password to manage
          </p>
        </form>

        {/* ─── OR ─── */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '0.75rem', margin: '2rem 0' }}>
          <div style={{ flex: 1, height: 1, background: '#1f1f2a' }} />
          <span style={{ color: '#5a5a6a', fontSize: '0.75rem' }}>OR</span>
          <div style={{ flex: 1, height: 1, background: '#1f1f2a' }} />
        </div>

        {/* ─── OAUTH PROVIDERS ─── */}
        <div style={{ display: 'grid', gap: '0.5rem' }}>
          {AUTH_PROVIDERS.filter((p) => p.enabled).map((p) => (
            <button
              key={p.id}
              onClick={() => handleOAuth(p.id)}
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                padding: '0.7rem 1rem',
                background: 'rgba(20, 20, 30, 0.7)',
                border: '1px solid #2a2a3a',
                borderRadius: 4,
                color: '#e6e6f0',
                cursor: 'pointer',
                fontSize: '0.92rem',
                fontFamily: 'inherit',
              }}
            >
              <span>Continue with {p.label}</span>
              <span style={{ color: '#5a5a6a' }}>→</span>
            </button>
          ))}
        </div>

        {/* ─── MESSAGE ─── */}
        {message && (
          <div
            style={{
              marginTop: '1.5rem',
              padding: '0.75rem 1rem',
              background: stubMode ? 'rgba(251, 191, 36, 0.08)' : 'rgba(124, 211, 252, 0.08)',
              border: `1px solid ${stubMode ? 'rgba(251, 191, 36, 0.3)' : 'rgba(124, 211, 252, 0.3)'}`,
              borderRadius: 4,
              fontSize: '0.85rem',
              color: stubMode ? '#fbbf24' : '#cdd6e4',
            }}
          >
            {stubMode && <strong style={{ display: 'block', marginBottom: '0.3rem' }}>⚠ stub-mode</strong>}
            {message}
          </div>
        )}

        {/* ─── REGISTER LINK ─── */}
        <p style={{ marginTop: '2.5rem', fontSize: '0.85rem', color: '#7a7a8c', textAlign: 'center' }}>
          Don't have an account?{' '}
          <a href="/register" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>
            Create one
          </a>
        </p>

        <footer style={{ marginTop: '4rem', color: '#5a5a6a', fontSize: '0.75rem', textAlign: 'center' }}>
          § sovereignty-respecting · ¬ password-storage · ¬ tracking · ¬ data-sale
        </footer>
      </main>
    </>
  );
};

export default Login;
