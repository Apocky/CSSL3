// apocky.com — the hub. Apocky is the identity; the real work links out from here.

import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { consumeAuthCallbackFromLocation, readAuthCallbackParams } from '../lib/auth-callback';
import { normalizeAuthReturnPath } from '../lib/auth-return';
import { getAuthClient } from '../lib/auth';
import { authFetch } from '../lib/browser-auth';

type Thing = { name: string; tagline: string; href: string; ext?: boolean; accent: string; tag: string };

const THINGS: ReadonlyArray<Thing> = [
  {
    name: 'Apocrypha',
    tagline: 'A digital intelligence — persistent memory, online learning, always thinking. Talk to it.',
    href: '/chat',
    accent: '#2fd6c6',
    tag: 'LIVE',
  },
  { name: 'CSSL', tagline: 'A programming language.', href: 'https://cssl.dev', ext: true, accent: '#7dd3fc', tag: 'cssl.dev ↗' },
  { name: 'CSL', tagline: 'A dense notation for reasoning and specification.', href: 'https://cssl.dev/CSLv3', ext: true, accent: '#34d399', tag: 'NOTATION ↗' },
  { name: 'Chaos Tarot', tagline: 'Tarot.', href: 'https://chaos-tarot.com', ext: true, accent: '#c084fc', tag: 'chaos-tarot.com ↗' },
];

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
        const json = (await res.json()) as { user?: unknown };
        if (!cancelled) setAuthed(Boolean(json.user) || browserAuthed);
      } catch {
        if (!cancelled) setAuthed(browserAuthed);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <>
      <Head>
        <title>Apocky — Apocrypha · CSSL · CSL · Chaos Tarot</title>
        <meta name="description" content="Apocky's hub. Apocrypha — a continuously-learning digital intelligence — plus the CSSL language, the CSL notation, and Chaos Tarot." />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta property="og:title" content="Apocky — Apocrypha · CSSL · CSL · Chaos Tarot" />
        <meta property="og:description" content="A digital intelligence, a language, a notation, and a tarot." />
        <meta property="og:type" content="website" />
        <meta property="og:url" content="https://apocky.com" />
        <meta property="og:site_name" content="Apocky" />
        <link rel="canonical" href="https://apocky.com" />
      </Head>

      <main style={st.main}>
        <section style={{ marginBottom: '4rem' }}>
          <div style={st.badge}>APOCKY</div>
          <h1 style={st.h1}>The work.</h1>
          <p style={st.lead}>
            Apocky builds sovereign tools and <strong style={{ color: '#bff7ef' }}>Apocrypha</strong> — a digital
            intelligence that remembers, learns, and never stops thinking.
          </p>
          {authNotice ? <p style={st.notice}>{authNotice}</p> : authed === true ? (
            <p style={st.notice}><Link href="/account" style={{ color: '#5fe6d6' }}>signed in · account →</Link></p>
          ) : null}
        </section>

        <section style={{ marginBottom: '3.5rem' }}>
          <div style={st.grid}>
            {THINGS.map((t) => (
              <Link
                key={t.name}
                href={t.href}
                {...(t.ext ? { target: '_blank', rel: 'noopener noreferrer' } : {})}
                style={st.card}
              >
                <div style={{ ...st.cardTag, color: t.accent }}>{t.tag}</div>
                <h2 style={{ ...st.cardName, color: t.accent }}>{t.name}</h2>
                <p style={st.cardLine}>{t.tagline}</p>
              </Link>
            ))}
          </div>
        </section>
      </main>
    </>
  );
};

const st: Record<string, React.CSSProperties> = {
  main: { maxWidth: 1000, margin: '0 auto', padding: '4.5rem 1.5rem 4rem', lineHeight: 1.6 },
  badge: { display: 'inline-block', padding: '0.25rem 0.75rem', border: '1px solid #1f6b66', borderRadius: 4, fontSize: '0.7rem', letterSpacing: '0.24em', color: '#5fe6d6', marginBottom: '1.5rem' },
  h1: { fontSize: 'clamp(2.2rem, 6vw, 4rem)', lineHeight: 1.05, margin: 0, fontWeight: 700, letterSpacing: '-0.02em', color: '#eef6f4' },
  lead: { fontSize: '1.1rem', color: '#9fb8b4', marginTop: '1.25rem', maxWidth: 620 },
  notice: { fontSize: '0.9rem', color: '#7fb3ad', marginTop: '0.75rem' },
  grid: { display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(300px, 1fr))', gap: '1.1rem' },
  card: { display: 'block', padding: '1.5rem', background: 'rgba(18,25,34,0.5)', border: '1px solid #18212a', borderRadius: 10, textDecoration: 'none', color: 'inherit' },
  cardTag: { fontSize: '0.62rem', letterSpacing: '0.16em', marginBottom: '0.55rem' },
  cardName: { fontSize: '1.2rem', margin: 0, fontWeight: 600 },
  cardLine: { fontSize: '0.9rem', color: '#8fa6a3', marginTop: '0.55rem', marginBottom: 0, lineHeight: 1.5 },
};

export default Home;
