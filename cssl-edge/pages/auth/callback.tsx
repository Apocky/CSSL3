// /auth/callback · landing page after Supabase magic-link / OAuth redirect
// Picks up session from URL hash (detectSessionInUrl=true does this on first getSession)
// then writes cookies and redirects to /account

import type { NextPage } from 'next';
import Head from 'next/head';
import { useEffect, useState } from 'react';
import { getAuthClient, persistSessionToCookie } from '../../lib/auth';

const AuthCallback: NextPage = () => {
  const [message, setMessage] = useState<string>('§ verifying your sign-in…');
  const [stub, setStub] = useState<boolean>(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const client = getAuthClient();
      if (!client) {
        if (!cancelled) {
          setStub(true);
          setMessage('⚠ stub-mode · APOCKY_HUB_SUPABASE_URL not set on this deploy.');
        }
        return;
      }

      // Give detectSessionInUrl a moment to consume the hash, then read session.
      await new Promise((r) => setTimeout(r, 250));
      const { data, error } = await client.auth.getSession();

      if (error || !data?.session) {
        if (!cancelled) {
          setMessage(`✗ no session detected in callback · ${error?.message ?? 'click the magic-link in your email again, or sign in fresh.'}`);
        }
        return;
      }

      // Persist as cookie so server-side /api/auth/me + /api/admin/check can resolve.
      persistSessionToCookie(data.session.access_token, data.session.refresh_token ?? undefined);

      if (!cancelled) {
        setMessage('✓ signed in · redirecting to your account…');
      }
      setTimeout(() => {
        location.replace('/account');
      }, 600);
    })();
    return () => {
      cancelled = true;
    };
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
