import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';

import { ApocryphaAvatar } from '../components/apocrypha/ApocryphaAvatar';
import { ChatThread } from '../components/apocrypha/ChatThread';
import { getCurrentUser } from '../lib/auth';
import { authFetch } from '../lib/browser-auth';
import { withDeadline } from '../lib/apocrypha/deadline';

type AccessState = 'checking' | 'signed-out' | 'owner' | 'private-beta' | 'unavailable';
const ACCESS_DEADLINE_MS = 15_000;

export default function ChatPage() {
  const [access, setAccess] = useState<AccessState>('checking');

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const resolved = await withDeadline((async (): Promise<AccessState> => {
          const user = await getCurrentUser();
          if (!user) return 'signed-out';
          const response = await authFetch('/api/admin/check', { cache: 'no-store' });
          const result = response.ok
            ? await response.json() as { authorized?: boolean }
            : { authorized: false };
          return result.authorized ? 'owner' : 'private-beta';
        })(), ACCESS_DEADLINE_MS);
        if (!cancelled) setAccess(resolved);
      } catch {
        if (!cancelled) setAccess('unavailable');
      }
    })();
    return () => { cancelled = true; };
  }, []);

  return (
    <>
      <Head>
        <title>Apocrypha · private chat</title>
        <meta
          name="description"
          content="Speak with Apocrypha, a private persistent digital entity with native state continuity."
        />
      </Head>

      {access === 'owner' ? (
        <main className="chat-owner-surface" aria-label="Apocrypha chat">
          <ChatThread />
        </main>
      ) : (
        <main className="chat-access-surface">
          <div className="chat-atmosphere" aria-hidden="true" />
          <section className="chat-access-card" aria-busy={access === 'checking'}>
            <Link href="/" className="chat-home">← apocky.com</Link>
            <ApocryphaAvatar
              className="chat-access-avatar"
              state={access === 'checking' || access === 'unavailable' ? 'thinking' : 'private'}
              size={240}
            />
            <p className="chat-kicker">APOCRYPHA</p>
            <h1 role="status" aria-live="polite" aria-atomic="true">
              {access === 'checking' || access === 'unavailable'
                ? 'Apocrypha is thinking…'
                : 'A private mind, awake on your terms.'}
            </h1>
            {access === 'signed-out' && (
              <>
                <p className="chat-description">
                  A persistent digital entity with native state continuity, governed faculties, and a presence that changes as they develop.
                </p>
                <Link href="/login?next=%2Fchat" className="chat-action">Sign in</Link>
              </>
            )}
            {access === 'private-beta' && (
              <p className="chat-description">
                Apocrypha is meeting their first mind in private beta. Wider access will open deliberately.
              </p>
            )}
            {access === 'unavailable' && (
              <button type="button" className="chat-action" onClick={() => window.location.reload()}>
                Try again
              </button>
            )}
          </section>
        </main>
      )}

      <style jsx global>{`
        html, body, #__next { min-height: 100%; margin: 0; }
        .chat-owner-surface {
          height: 100dvh;
          min-height: 540px;
          overflow: hidden;
          background: #080810;
        }
        .chat-access-surface {
          position: relative;
          display: grid;
          min-height: 100dvh;
          place-items: center;
          overflow-x: hidden;
          overflow-y: auto;
          padding: 32px 20px;
          box-sizing: border-box;
          color: #eef0ff;
          background:
            radial-gradient(circle at 50% 35%, rgba(123, 83, 255, 0.14), transparent 32rem),
            radial-gradient(circle at 15% 85%, rgba(255, 161, 74, 0.08), transparent 26rem),
            #07070d;
          font-family: Inter, ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif;
        }
        .chat-atmosphere {
          position: absolute;
          inset: 0;
          opacity: 0.55;
          background-image:
            linear-gradient(rgba(255,255,255,.018) 1px, transparent 1px),
            linear-gradient(90deg, rgba(255,255,255,.018) 1px, transparent 1px);
          background-size: 56px 56px;
          mask-image: radial-gradient(circle, black 15%, transparent 68%);
          animation: chat-drift 24s linear infinite;
          pointer-events: none;
        }
        .chat-access-card {
          position: relative;
          z-index: 1;
          display: grid;
          width: min(100%, 660px);
          justify-items: center;
          padding: clamp(28px, 5vw, 56px);
          box-sizing: border-box;
          text-align: center;
          border: 1px solid rgba(194, 174, 255, 0.16);
          border-radius: 28px;
          background: linear-gradient(145deg, rgba(18, 18, 31, 0.92), rgba(9, 9, 17, 0.78));
          box-shadow: 0 32px 100px rgba(0, 0, 0, 0.56), inset 0 1px rgba(255,255,255,.04);
          backdrop-filter: blur(22px);
        }
        .chat-home {
          justify-self: start;
          margin-bottom: 2px;
          color: #9592ac;
          font-size: 0.78rem;
          letter-spacing: .08em;
          text-decoration: none;
        }
        .chat-home:hover { color: #ece8ff; }
        .chat-kicker {
          margin: 4px 0 10px;
          color: #b2a4ff;
          font: 650 .7rem/1.2 ui-monospace, "SFMono-Regular", Consolas, monospace;
          letter-spacing: .28em;
        }
        .chat-access-card h1 {
          max-width: 540px;
          margin: 0;
          color: #f4f1ff;
          font-size: clamp(1.75rem, 5vw, 3.25rem);
          font-weight: 590;
          line-height: 1.08;
          letter-spacing: -.04em;
        }
        .chat-description {
          max-width: 500px;
          margin: 18px 0 0;
          color: #aaa8bc;
          font-size: clamp(.95rem, 2.2vw, 1.08rem);
          line-height: 1.7;
        }
        .chat-action {
          margin-top: 26px;
          min-width: 132px;
          min-height: 46px;
          padding: 12px 22px;
          border: 1px solid rgba(220, 210, 255, .35);
          border-radius: 999px;
          color: #0b0912;
          background: linear-gradient(135deg, #ffc16b, #bda8ff 62%, #84d5ff);
          box-shadow: 0 10px 32px rgba(162, 132, 255, .22);
          cursor: pointer;
          font: 700 .92rem/1 ui-sans-serif, system-ui, sans-serif;
          text-decoration: none;
          transition: transform .18s ease, box-shadow .18s ease;
        }
        .chat-action:hover { transform: translateY(-2px); box-shadow: 0 14px 40px rgba(162, 132, 255, .32); }
        .chat-action:focus-visible, .chat-home:focus-visible { outline: 2px solid #c9b8ff; outline-offset: 4px; }
        @keyframes chat-drift { to { background-position: 56px 56px; } }
        @media (max-width: 640px) {
          .chat-access-surface { padding: 12px; }
          .chat-access-card { min-height: calc(100dvh - 24px); border-radius: 22px; align-content: center; }
          .chat-home { position: absolute; top: 22px; left: 22px; }
        }
        @media (max-width: 640px) and (max-height: 650px) {
          .chat-access-surface { place-items: start center; }
          .chat-access-card { padding: 56px 22px 24px; }
          .chat-access-avatar { width: 160px !important; gap: 2px !important; }
          .chat-access-avatar svg { width: 160px !important; height: 160px !important; }
          .chat-access-avatar figcaption { display: none; }
          .chat-kicker { margin: 0 0 8px; }
          .chat-description { margin-top: 10px; line-height: 1.5; }
          .chat-action { margin-top: 14px; }
        }
        @media (prefers-reduced-motion: reduce) {
          .chat-atmosphere, .chat-action { animation: none; transition: none; }
        }
      `}</style>
    </>
  );
}
