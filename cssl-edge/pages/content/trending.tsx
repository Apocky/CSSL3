// cssl-edge · pages/content/trending.tsx
// W12-6 · /content/trending · KAN-bias-weighted top picks
// "Why am I seeing this?" → ALWAYS shown on every card (sovereignty UX)
// Cosmetic-axiom-attestation visible per-card

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import { useEffect, useState } from 'react';
import ContentFeed from '@/components/ContentFeed';
import { ContentNav, ContentFooter, contentLandingCSS } from './index';
import {
  fetchContentList,
  EMPTY_LIST_RESPONSE,
  type ContentItem,
} from '@/lib/content-fetch';

const ContentTrending: NextPage = () => {
  const [items, setItems] = useState<ReadonlyArray<ContentItem>>([]);
  const [unavailable, setUnavailable] = useState(false);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    void (async () => {
      const res = await fetchContentList('trending', 24);
      setUnavailable(res.unavailable);
      setItems(res.data?.items ?? EMPTY_LIST_RESPONSE.items);
      setLoading(false);
    })();
  }, []);

  return (
    <>
      <Head>
        <title>Popular shared content · Apocky</title>
        <meta
          name="description"
          content="Popular shared content with a visible explanation of the ranking method."
        />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#000000" />
        <link rel="canonical" href="https://www.apocky.com/content/trending" />
        <style>{contentLandingCSS}</style>
      </Head>
      <main className="content-shell">
        <ContentNav active="trending" />
        <header style={{ marginBottom: '2rem' }}>
          <h1 className="content-h1" style={{ fontSize: 'clamp(1.5rem, 4vw, 2.4rem)' }}>
            Popular shared content
          </h1>
          <p className="content-blurb">
            When the service has enough real activity, this list uses the public factors explained below.
            Each item can also explain why it appears.
          </p>
          {/* Methodology disclosure — NEVER hide the algorithm */}
          <details
            style={{
              marginTop: '1rem',
              padding: '0.85rem 1rem',
              background: 'rgba(125,211,252,0.05)',
              border: '1px solid rgba(125,211,252,0.18)',
              borderRadius: 6,
              fontSize: '0.85rem',
              color: '#cdd6e4',
            }}
          >
            <summary
              style={{ cursor: 'pointer', color: '#7dd3fc', listStyle: 'none', fontWeight: 600 }}
            >
              How is “popular” calculated?
            </summary>
            <div style={{ marginTop: '0.6rem', lineHeight: 1.6, fontSize: '0.85rem' }}>
              <p style={{ margin: '0 0 0.5rem' }}>
                <strong style={{ color: '#c084fc' }}>Information used</strong>:
              </p>
              <ul style={{ margin: 0, paddingLeft: '1.25rem' }}>
                <li>Completed installs: 30 percent of the score.</li>
                <li>Four- and five-star ratings: 25 percent.</li>
                <li>New versions derived from the item: 25 percent.</li>
                <li>Publication time, with older items gradually reduced: 20 percent.</li>
              </ul>
              <p style={{ margin: '0.6rem 0 0' }}>
                <strong style={{ color: '#c084fc' }}>Information not intended for this score</strong>: how far
                you scroll, time spent on a page, click-through rate, experiment groups, or guesses about an
                individual’s behavior. This describes the ranking design and must be checked against the running
                service before launch.
              </p>
            </div>
          </details>
        </header>

        {loading ? (
          <p style={{ color: '#7a7a8c', fontSize: '0.85rem' }}>Loading popular content…</p>
        ) : (
          <ContentFeed
            items={items}
            unavailable={unavailable}
            showRationale={true}
            emptyMessage="There is not enough published activity to calculate this list."
          />
        )}

        <ContentFooter />
      </main>
    </>
  );
};

export const getServerSideProps: GetServerSideProps = async () => ({ notFound: true });

export default ContentTrending;
