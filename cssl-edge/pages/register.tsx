// apocky.com/register · create new account
// Per spec/22 · same magic-link / OAuth flow as /login · just framed for new users

import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useState } from 'react';
import { AUTH_PROVIDERS } from '../lib/auth';

const Register: NextPage = () => {
  const [email, setEmail] = useState('');
  const [agreed, setAgreed] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [stubMode, setStubMode] = useState<boolean | null>(null);

  async function handleMagicLink(e: React.FormEvent) {
    e.preventDefault();
    if (!email || !agreed || submitting) return;
    setSubmitting(true);
    setMessage(null);
    try {
      const res = await fetch('/api/auth/magic-link', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          email,
          redirectTo: `${location.origin}/account`,
          isRegistration: true,
        }),
      });
      const json = await res.json();
      if (json.stub) setStubMode(true);
      setMessage(json.message ?? (json.ok ? '✓ Check your email for the verification link' : '✗ Failed'));
    } catch (err) {
      setMessage('✗ Network error');
    } finally {
      setSubmitting(false);
    }
  }

  async function handleOAuth(provider: string) {
    setMessage(null);
    if (!agreed) {
      setMessage('Please agree to the terms below first');
      return;
    }
    try {
      const res = await fetch('/api/auth/oauth', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ provider, redirectTo: `${location.origin}/account` }),
      });
      const json = await res.json();
      if (json.stub) {
        setStubMode(true);
        setMessage('Stub mode · OAuth not connected yet · pending Apocky-Hub Supabase signup');
        return;
      }
      if (json.url) location.href = json.url;
      else setMessage('✗ OAuth init failed');
    } catch (err) {
      setMessage('✗ Network error');
    }
  }

  return (
    <>
      <Head>
        <title>Create account · Apocky</title>
        <meta name="description" content="Create an account on apocky.com · single sign-on across all Apocky-projects" />
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
      <main style={{ maxWidth: 480, margin: '0 auto', padding: '5rem 1.5rem' }}>
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
          Create your account
        </h1>
        <p style={{ color: '#a8a8b8', marginTop: '0.5rem', fontSize: '0.92rem' }}>
          One identity · access to all Apocky-projects (LoA · CSSL · DGI · etc.) · sovereign-revocable
        </p>

        {/* ─── WHAT YOU GET ─── */}
        <section
          style={{
            marginTop: '2rem',
            padding: '1rem 1.25rem',
            background: 'rgba(20, 20, 30, 0.5)',
            border: '1px solid #1f1f2a',
            borderRadius: 6,
          }}
        >
          <h3 style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.15em', color: '#7a7a8c', margin: 0 }}>
            § What you get
          </h3>
          <ul style={{ margin: '0.6rem 0 0', paddingLeft: '1.2rem', color: '#cdd6e4', fontSize: '0.85rem', lineHeight: 1.7 }}>
            <li>Single sign-on across all Apocky-projects</li>
            <li>Cloud-saved progress when you opt-in (TIER-2 cross-device sync)</li>
            <li>Akashic-Records · Bazaar · Multiplayer · Mycelium when you opt-in</li>
            <li>Cross-device entitlement-tracking (purchases follow your account)</li>
            <li>Per-event opt-in granularity · revoke any cap at any time</li>
            <li>Full data-export + delete-on-request (GDPR / CCPA)</li>
          </ul>
        </section>

        {/* ─── EMAIL MAGIC-LINK ─── */}
        <form onSubmit={handleMagicLink} style={{ marginTop: '2rem' }}>
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
            § Email
          </label>
          <input
            type="email"
            required
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="you@example.com"
            style={{
              width: '100%',
              padding: '0.7rem 0.9rem',
              background: 'rgba(20, 20, 30, 0.7)',
              border: '1px solid #2a2a3a',
              borderRadius: 4,
              color: '#e6e6f0',
              fontSize: '0.95rem',
              outline: 'none',
            }}
          />

          <label
            style={{
              display: 'flex',
              alignItems: 'flex-start',
              marginTop: '1rem',
              gap: '0.5rem',
              fontSize: '0.82rem',
              color: '#cdd6e4',
              cursor: 'pointer',
            }}
          >
            <input
              type="checkbox"
              checked={agreed}
              onChange={(e) => setAgreed(e.target.checked)}
              style={{ marginTop: '0.2rem', accentColor: '#c084fc' }}
            />
            <span>
              I agree to the{' '}
              <a href="/legal/terms" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>
                Terms of Service
              </a>
              ,{' '}
              <a href="/legal/privacy" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>
                Privacy Policy
              </a>
              , and confirm I am 13 or older (18+ for paid features).
            </span>
          </label>

          <button
            type="submit"
            disabled={submitting || !email || !agreed}
            style={{
              marginTop: '1.25rem',
              width: '100%',
              padding: '0.8rem 1.2rem',
              background: 'linear-gradient(135deg, #c084fc 0%, #7dd3fc 100%)',
              color: '#0a0a0f',
              fontWeight: 700,
              border: 'none',
              borderRadius: 4,
              cursor: submitting || !email || !agreed ? 'not-allowed' : 'pointer',
              opacity: submitting || !email || !agreed ? 0.5 : 1,
              fontSize: '0.95rem',
              fontFamily: 'inherit',
            }}
          >
            {submitting ? '…' : '→ Send verification link'}
          </button>
        </form>

        {/* ─── OR ─── */}
        <div style={{ display: 'flex', alignItems: 'center', gap: '0.75rem', margin: '2rem 0 1rem' }}>
          <div style={{ flex: 1, height: 1, background: '#1f1f2a' }} />
          <span style={{ color: '#5a5a6a', fontSize: '0.75rem' }}>OR</span>
          <div style={{ flex: 1, height: 1, background: '#1f1f2a' }} />
        </div>

        {/* ─── OAUTH ─── */}
        <div style={{ display: 'grid', gap: '0.5rem' }}>
          {AUTH_PROVIDERS.filter((p) => p.enabled).map((p) => (
            <button
              key={p.id}
              onClick={() => handleOAuth(p.id)}
              disabled={!agreed}
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                padding: '0.7rem 1rem',
                background: 'rgba(20, 20, 30, 0.7)',
                border: '1px solid #2a2a3a',
                borderRadius: 4,
                color: '#e6e6f0',
                cursor: !agreed ? 'not-allowed' : 'pointer',
                opacity: !agreed ? 0.5 : 1,
                fontSize: '0.92rem',
                fontFamily: 'inherit',
              }}
            >
              <span>Sign up with {p.label}</span>
              <span style={{ color: '#5a5a6a' }}>→</span>
            </button>
          ))}
        </div>

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

        <p style={{ marginTop: '2.5rem', fontSize: '0.85rem', color: '#7a7a8c', textAlign: 'center' }}>
          Already have an account?{' '}
          <a href="/login" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>
            Sign in
          </a>
        </p>
      </main>
    </>
  );
};

export default Register;
