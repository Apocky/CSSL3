// cssl-edge · pages/content/trending.tsx
// W12-6 · /content/trending · KAN-bias-weighted top picks
// "Why am I seeing this?" → ALWAYS shown on every card (sovereignty UX)
// Cosmetic-axiom-attestation visible per-card

import type { NextPage } from 'next';
import Head from 'next/head';
import { useEffect, useState } from 'react';
import ContentFeed from '@/components/ContentFeed';
import { ContentNav, ContentFooter, contentLandingCSS } from './index';
import {
  fetchContentList,
  STUB_LIST_RESPONSE,
  type ContentItem,
} from '@/lib/content-fetch';

const ContentTrending: NextPage = () => {
  const [items, setItems] = useState<ReadonlyArray<ContentItem>>([]);
  const [stubMode, setStubMode] = useState(false);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    void (async () => {
      const res = await fetchContentList('trending', 24);
      setStubMode(res.stub_mode);
      setItems(res.data?.items ?? STUB_LIST_RESPONSE.items);
      setLoading(false);
    })();
  }, []);

  return (
    <>
      <Head>
        <title>§ Trending · Content · Apocky</title>
        <meta
          name="description"
          content="KAN-bias-weighted trending content · explainable rationale on every card · ¬ algorithmic black-box"
        />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#0a0a0f" />
        <link rel="canonical" href="https://apocky.com/content/trending" />
        <style>{contentLandingCSS}</style>
      </Head>
      <main className="content-shell">
        <ContentNav active="trending" />
        <header style={{ marginBottom: '2rem' }}>
          <h1 className="content-h1" style={{ fontSize: 'clamp(1.5rem, 4vw, 2.4rem)' }}>
            § Trending · KAN-bias-weighted
          </h1>
          <p className="content-blurb">
            Weighted by collective-engagement-bias from Akashic-Records · ALWAYS-explainable ·
            ¬ algorithmic-black-box · click ◐ on any card to see its rationale.
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
              ◐ how is "trending" computed?
            </summary>
            <div style={{ marginTop: '0.6rem', lineHeight: 1.6, fontSize: '0.85rem' }}>
              <p style={{ margin: '0 0 0.5rem' }}>
                <strong style={{ color: '#c084fc' }}>signals weighted</strong> :
              </p>
              <ul style={{ margin: 0, paddingLeft: '1.25rem' }}>
                <li>install-completions (count) · weight 0.3</li>
                <li>positive-rating count (4-5★) · weight 0.25</li>
                <li>remix-count (downstream descendants) · weight 0.25</li>
                <li>recency decay (e^(-Δt/14d)) · weight 0.2</li>
              </ul>
              <p style={{ margin: '0.6rem 0 0' }}>
                <strong style={{ color: '#c084fc' }}>signals NOT used</strong> : scroll-depth ·
                time-on-page · click-through-rate · A/B-test bucketing · per-user
                behavioral-inference. Sovereignty-default.
              </p>
            </div>
          </details>
        </header>

        {loading ? (
          <p style={{ color: '#7a7a8c', fontSize: '0.85rem' }}>◐ loading trending feed…</p>
        ) : (
          <ContentFeed
            items={items}
            stubMode={stubMode}
            showRationale={true}
            emptyMessage="○ trending pool empty · weights need at least 24h of data"
          />
        )}

        <ContentFooter />
      </main>
    </>
  );
};

export default ContentTrending;
