// cssl-edge · pages/content/feed.tsx
// W12-6 · /content/feed · chronological reverse-time feed
// Infinite-scroll via explicit "load more" button (NO scroll-tracking)
// Auto-refresh : 60s polling toggle (default off)
// Phone-first responsive

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import { useEffect, useRef, useState } from 'react';
import ContentFeed from '@/components/ContentFeed';
import { ContentNav, ContentFooter, contentLandingCSS } from './index';
import {
  fetchContentList,
  EMPTY_LIST_RESPONSE,
  type ContentItem,
} from '@/lib/content-fetch';

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
    setItems(res.data?.items ?? EMPTY_LIST_RESPONSE.items);
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
        <title>Newest shared content · Apocky</title>
        <meta name="description" content="Shared content listed by publication time, newest first." />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#000000" />
        <link rel="canonical" href="https://www.apocky.com/content/feed" />
        <style>{contentLandingCSS}</style>
      </Head>
      <main className="content-shell">
        <ContentNav active="feed" />
        <header style={{ marginBottom: '2rem' }}>
          <h1 className="content-h1" style={{ fontSize: 'clamp(1.5rem, 4vw, 2.4rem)' }}>
            Newest shared content
          </h1>
          <p className="content-blurb">
            Published items are listed newest first. Automatic refresh is off unless you turn it on.
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
              onChange={(e) => setAutoRefresh(e.target.checked)}
              style={{ accentColor: '#c084fc' }}
            />
            <span>Refresh this list every 60 seconds</span>
          </label>
        </header>

        <ContentFeed
          items={items}
          unavailable={unavailable}
          onLoadMore={hasMore ? loadMore : undefined}
          loading={loading}
          emptyMessage="No published packages are available."
        />

        <ContentFooter />
      </main>
    </>
  );
};

export const getServerSideProps: GetServerSideProps = async () => ({ notFound: true });

export default ContentFeedPage;
