// /auth/callback · landing page after Supabase magic-link / OAuth redirect
// Exchanges Supabase OAuth / magic-link callback params into a browser session,
// writes server-readable cookies, then redirects to /account.

import type { NextPage } from 'next';
import Head from 'next/head';
import { useEffect, useState } from 'react';
import { consumeAuthCallbackFromLocation } from '../../lib/auth-callback';

const AuthCallback: NextPage = () => {
  const [message, setMessage] = useState<string>('§ verifying your sign-in…');
  const [stub, setStub] = useState<boolean>(false);

  const [debugInfo, setDebugInfo] = useState<string | null>(null);

  useEffect(() => {
    if (typeof location !== 'undefined' && location.hostname === 'localhost') {
      const q = new URLSearchParams(location.search);
      const h = new URLSearchParams(location.hash.replace(/^#/, ''));
      const parts: string[] = [`origin: ${location.origin}`];
      const code = q.get('code');
      const err = q.get('error') ?? h.get('error');
      const accessToken = h.get('access_token');
      if (code) parts.push(`code: ${code.slice(0, 12)}…`);
      if (accessToken) parts.push('hash: access_token present');
      if (err) parts.push(`error: ${err}`);
      if (!code && !accessToken && !err) parts.push('⚠ no code / token / error in URL — likely Supabase redirect URL not whitelisted');
      setDebugInfo(parts.join(' · '));
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const result = await consumeAuthCallbackFromLocation();
      if (cancelled) return;
      if (result.ok) {
        if (!cancelled) setMessage('✓ signed in · redirecting…');
        setTimeout(() => { location.replace('/account'); }, 600);
        return;
      }
      setStub(Boolean(result.stub));
      setMessage(`✗ sign-in failed · ${result.reason ?? 'no session found'} · check Supabase Auth redirect URLs and Google provider credentials.`);
    })();
    return () => { cancelled = true; };
  }, []);

  return (
    <>
      <Head>
        <title>Signing you in… · Apocky</title>
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      </Head>
      <main
        style={{
          minHeight: '100dvh',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          padding: '2rem',
          textAlign: 'center',
          background: 'radial-gradient(ellipse at top, #15151f 0%, #0a0a0f 50%, #050507 100%)',
          color: '#e6e6f0',
          fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
        }}
      >
        <div style={{ maxWidth: 400 }}>
          <h1
            style={{
              fontSize: '1.4rem',
              margin: '0 0 1rem',
              fontWeight: 700,
              backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%)',
              WebkitBackgroundClip: 'text',
              WebkitTextFillColor: 'transparent',
            }}
          >
            § signing you in
          </h1>
          <p
            style={{
              fontSize: '0.92rem',
              color: stub ? '#fbbf24' : '#cdd6e4',
              margin: 0,
            }}
          >
            {message}
          </p>
          {debugInfo && (
            <p
              style={{
                marginTop: '1rem',
                padding: '0.5rem 0.75rem',
                background: 'rgba(251, 191, 36, 0.07)',
                border: '1px solid rgba(251, 191, 36, 0.3)',
                borderRadius: 4,
                fontSize: '0.72rem',
                color: '#fbbf24',
                textAlign: 'left',
                wordBreak: 'break-all',
              }}
            >
              {debugInfo}
            </p>
          )}
          <a
            href="/login"
            style={{
              display: 'inline-block',
              marginTop: '1.5rem',
              fontSize: '0.85rem',
              color: '#7dd3fc',
              textDecoration: 'underline',
            }}
          >
            ← back to /login
          </a>
        </div>
      </main>
    </>
  );
};

export default AuthCallback;
