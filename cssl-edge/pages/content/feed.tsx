// cssl-edge · pages/content/feed.tsx
// W12-6 · /content/feed · chronological reverse-time feed
// Infinite-scroll via explicit "load more" button (NO scroll-tracking)
// Auto-refresh : 60s polling toggle (default off)
// Phone-first responsive

import type { NextPage } from 'next';
import Head from 'next/head';
import { useEffect, useRef, useState } from 'react';
import ContentFeed from '@/components/ContentFeed';
import { ContentNav, ContentFooter, contentLandingCSS } from './index';
import { fetchContentList, type ContentItem } from '@/lib/content-fetch';

const PAGE_SIZE = 12;
const REFRESH_INTERVAL_MS = 60_000;

const ContentFeedPage: NextPage = () => {
  const [items, setItems] = useState<ReadonlyArray<ContentItem>>([]);
  const [cursor, setCursor] = useState<string | undefined>(undefined);
  const [unavailable, setUnavailable] = useState(false);
  const [loading, setLoading] = useState(false);
  const [autoRefresh, setAutoRefresh] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const refreshTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const loadInitial = async () => {
    setLoading(true);
    const res = await fetchContentList('new', PAGE_SIZE);
    setUnavailable(res.unavailable);
    setItems(res.data?.items ?? []);
    setCursor(res.data?.next_cursor);
    setHasMore(Boolean(res.data?.next_cursor));
    setLoading(false);
  };

  const loadMore = async () => {
    if (loading || !hasMore || !cursor) return;
    setLoading(true);
    const res = await fetchContentList('new', PAGE_SIZE, cursor);
    if (res.data) {
      setItems((prev) => [...prev, ...res.data!.items]);
      setCursor(res.data.next_cursor);
      setHasMore(Boolean(res.data.next_cursor));
    }
    setLoading(false);
  };

  useEffect(() => {
    void loadInitial();
  }, []);

  // auto-refresh toggle · respects user-consent (default off · explicit opt-in)
  useEffect(() => {
    if (!autoRefresh) {
      if (refreshTimerRef.current) {
        clearInterval(refreshTimerRef.current);
        refreshTimerRef.current = null;
      }
      return;
    }
    refreshTimerRef.current = setInterval(() => {
      void (async () => {
        const res = await fetchContentList('new', PAGE_SIZE);
        if (res.data) {
          setItems(res.data.items);
          setUnavailable(res.unavailable);
          setCursor(res.data.next_cursor);
        }
      })();
    }, REFRESH_INTERVAL_MS);
    return () => {
      if (refreshTimerRef.current) {
        clearInterval(refreshTimerRef.current);
        refreshTimerRef.current = null;
      }
    };
  }, [autoRefresh]);

  return (
    <>
      <Head>
        <title>§ Feed · Content · Apocky</title>
        <meta name="description" content="Chronological feed of all UGC content packages · reverse-time order" />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#0a0a0f" />
        <link rel="canonical" href="https://apocky.com/content/feed" />
        <style>{contentLandingCSS}</style>
      </Head>
      <main className="content-shell">
        <ContentNav active="feed" />
        <header style={{ marginBottom: '2rem' }}>
          <h1 className="content-h1" style={{ fontSize: 'clamp(1.5rem, 4vw, 2.4rem)' }}>
            § Chronological Feed
          </h1>
          <p className="content-blurb">
            Every published package · reverse-time · ¬ algorithmic-curation · ¬ engagement-tracking.
            Auto-refresh is opt-in below.
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
              checked={autoRefresh}
              disabled={unavailable}
              onChange={(e) => setAutoRefresh(e.target.checked)}
              style={{ accentColor: '#c084fc' }}
            />
            <span>auto-refresh every 60s · {unavailable ? 'temporarily unavailable' : 'explicit opt-in'}</span>
          </label>
        </header>

        <ContentFeed
          items={items}
          unavailable={unavailable}
          onLoadMore={!unavailable && hasMore ? loadMore : undefined}
          loading={loading}
          emptyMessage="○ no published packages yet · check back soon"
        />

        <ContentFooter />
      </main>
    </>
  );
};

export default ContentFeedPage;
