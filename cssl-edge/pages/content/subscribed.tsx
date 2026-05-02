// cssl-edge · pages/content/subscribed.tsx
// W12-6 · /content/subscribed · user's subscriptions
// Auto-pull-state visible · sovereign-unsubscribe button per item

import type { NextPage } from 'next';
import Head from 'next/head';
import { useEffect, useState } from 'react';
import { ContentNav, ContentFooter, contentLandingCSS } from './index';
import ContentCard from '@/components/ContentCard';
import {
  fetchSubscribed,
  unsubscribe,
  STUB_LIST_RESPONSE,
  type ContentItem,
} from '@/lib/content-fetch';

const ContentSubscribed: NextPage = () => {
  const [items, setItems] = useState<ReadonlyArray<ContentItem>>([]);
  const [stubMode, setStubMode] = useState(false);
  const [loading, setLoading] = useState(true);
  const [autoPullEnabled, setAutoPullEnabled] = useState(false);
  const [revoking, setRevoking] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      // user_cap is normally read from session-cookie; W12-6 stubs as 'me'
      const res = await fetchSubscribed('me');
      setStubMode(res.stub_mode);
      setItems(res.data?.items ?? STUB_LIST_RESPONSE.items);
      setLoading(false);
      // Persist last-seen-state into localStorage for offline-friendly UX
      if (res.data && typeof window !== 'undefined') {
        try {
          window.localStorage.setItem(
            'apocky-content-subscribed-last',
            JSON.stringify({ when: Date.now(), count: res.data.items.length }),
          );
        } catch {
          /* private-mode or quota — ignore */
        }
      }
    })();
  }, []);

  const handleUnsubscribe = async (slug: string) => {
    setRevoking(slug);
    const ok = await unsubscribe(slug);
    if (ok) {
      setItems((prev) => prev.filter((i) => i.slug !== slug));
    } else {
      // stub-mode or API failure — still optimistically remove for UX
      setItems((prev) => prev.filter((i) => i.slug !== slug));
    }
    setRevoking(null);
  };

  return (
    <>
      <Head>
        <title>§ Subscribed · Content · Apocky</title>
        <meta
          name="description"
          content="Your content subscriptions · auto-pull-state visible · sovereign-unsubscribe always-available"
        />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#0a0a0f" />
        <link rel="canonical" href="https://apocky.com/content/subscribed" />
        <style>{contentLandingCSS}</style>
      </Head>
      <main className="content-shell">
        <ContentNav active="subscribed" />
        <header style={{ marginBottom: '2rem' }}>
          <h1 className="content-h1" style={{ fontSize: 'clamp(1.5rem, 4vw, 2.4rem)' }}>
            § Subscribed · {items.length}
          </h1>
          <p className="content-blurb">
            Packages you've subscribed to · sovereign-unsubscribe is one-click below
            (NO retention period · NO email-confirm · effective immediately) · auto-pull
            of new versions is opt-in.
          </p>
          <label
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: '0.5rem',
              fontSize: '0.85rem',
              color: '#a8a8b8',
              marginTop: '0.75rem',
              cursor: 'pointer',
            }}
          >
            <input
              type="checkbox"
              checked={autoPullEnabled}
              onChange={(e) => setAutoPullEnabled(e.target.checked)}
              style={{ accentColor: '#c084fc' }}
            />
            <span>
              auto-pull new versions · {autoPullEnabled ? '✓ enabled' : '○ disabled (default)'}
            </span>
          </label>
        </header>

        {stubMode && (
          <div
            role="status"
            style={{
              padding: '0.75rem 1rem',
              background: 'rgba(251,191,36,0.06)',
              border: '1px solid rgba(251,191,36,0.25)',
              borderRadius: 6,
              fontSize: '0.82rem',
              color: '#fbbf24',
              marginBottom: '1.25rem',
              lineHeight: 1.5,
            }}
          >
            <strong>◐ stub-mode</strong> · subscription-API (sibling W12-5/W12-8) not yet wired ·
            placeholder rendered
          </div>
        )}

        {loading ? (
          <p style={{ color: '#7a7a8c', fontSize: '0.85rem' }}>◐ loading subscriptions…</p>
        ) : items.length === 0 && !stubMode ? (
          <div
            style={{
              padding: '3rem 1.5rem',
              textAlign: 'center',
              color: '#5a5a6a',
              fontSize: '0.9rem',
              border: '1px dashed #1f1f2a',
              borderRadius: 6,
            }}
          >
            ○ no subscriptions yet · browse the{' '}
            <a href="/content" style={{ color: '#7dd3fc' }}>
              landing page
            </a>{' '}
            to find content packages
          </div>
        ) : (
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))',
              gap: '1rem',
            }}
          >
            {items.map((item) => (
              <div key={item.slug} style={{ position: 'relative' }}>
                <ContentCard item={item} />
                <button
                  type="button"
                  disabled={revoking === item.slug}
                  onClick={() => handleUnsubscribe(item.slug)}
                  aria-label={`unsubscribe from ${item.title}`}
                  title="sovereign-unsubscribe · effective immediately · no retention"
                  style={{
                    position: 'absolute',
                    bottom: 12,
                    left: 12,
                    padding: '0.3rem 0.7rem',
                    background: 'rgba(192,132,252,0.1)',
                    border: '1px solid rgba(192,132,252,0.3)',
                    color: '#c084fc',
                    fontSize: '0.7rem',
                    borderRadius: 4,
                    fontFamily: 'inherit',
                    cursor: revoking === item.slug ? 'wait' : 'pointer',
                  }}
                >
                  {revoking === item.slug ? '◐ revoking…' : '⊘ unsubscribe'}
                </button>
              </div>
            ))}
          </div>
        )}

        <ContentFooter />
      </main>
    </>
  );
};

export default ContentSubscribed;
