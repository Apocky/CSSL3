// cssl-edge · pages/content/subscribed.tsx
// W12-6 · /content/subscribed · user's subscriptions
// Auto-pull-state visible · sovereign-unsubscribe button per item

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import { useEffect, useState } from 'react';
import { ContentNav, ContentFooter, contentLandingCSS } from './index';
import ContentCard from '@/components/ContentCard';
import {
  fetchSubscribed,
  unsubscribe,
  EMPTY_LIST_RESPONSE,
  type ContentItem,
} from '@/lib/content-fetch';

const ContentSubscribed: NextPage = () => {
  const [items, setItems] = useState<ReadonlyArray<ContentItem>>([]);
  const [unavailable, setUnavailable] = useState(false);
  const [loading, setLoading] = useState(true);
  const [revoking, setRevoking] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      // The server derives the signed-in identity; the client does not assert one.
      const res = await fetchSubscribed('me');
      setUnavailable(res.unavailable);
      setItems(res.data?.items ?? EMPTY_LIST_RESPONSE.items);
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
    setActionError(null);
    const ok = await unsubscribe(slug);
    if (ok) {
      setItems((prev) => prev.filter((i) => i.slug !== slug));
    } else {
      setActionError('The subscription could not be changed. Nothing was removed.');
    }
    setRevoking(null);
  };

  return (
    <>
      <Head>
        <title>Shared-content subscriptions · Apocky</title>
        <meta
          name="description"
          content="Content packages you chose to follow, when the subscription service is available."
        />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#000000" />
        <link rel="canonical" href="https://www.apocky.com/content/subscribed" />
        <style>{contentLandingCSS}</style>
      </Head>
      <main className="content-shell">
        <ContentNav active="subscribed" />
        <header style={{ marginBottom: '2rem' }}>
          <h1 className="content-h1" style={{ fontSize: 'clamp(1.5rem, 4vw, 2.4rem)' }}>
            Subscriptions · {items.length}
          </h1>
          <p className="content-blurb">
            Content packages you chose to follow. This page does not claim a change succeeded until the server
            confirms it.
          </p>
        </header>

        {actionError ? <p role="alert" style={{ color: '#ffb0b8' }}>{actionError}</p> : null}

        {unavailable && (
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
            <strong>Subscriptions are not available yet.</strong> No placeholder subscription is shown.
          </div>
        )}

        {loading ? (
          <p style={{ color: '#7a7a8c', fontSize: '0.85rem' }}>Loading subscriptions…</p>
        ) : items.length === 0 && !unavailable ? (
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
            No subscriptions yet. Browse the{' '}
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
                  title="Stop following this item"
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
                  {revoking === item.slug ? 'Removing…' : 'Unsubscribe'}
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

export const getServerSideProps: GetServerSideProps = async () => ({ notFound: true });

export default ContentSubscribed;
