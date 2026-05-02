// cssl-edge · pages/content/index.tsx
// W12-6 · UGC-Discover-Browse · /content landing
// 4 sections : Featured · Trending · New · Tagged-by-you
// Phone-first responsive · CSLv3-§-headers
// SSR + client-side fetch (stub-mode-aware)

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import { useEffect, useState } from 'react';
import ContentFeed from '@/components/ContentFeed';
import {
  fetchContentList,
  STUB_LIST_RESPONSE,
  type ContentItem,
} from '@/lib/content-fetch';

interface ContentLandingProps {
  /** SSR-fetched items per bucket. Empty arrays → client may retry. */
  featured: ReadonlyArray<ContentItem>;
  trending: ReadonlyArray<ContentItem>;
  fresh: ReadonlyArray<ContentItem>;
  tagged: ReadonlyArray<ContentItem>;
  initial_stub_mode: boolean;
}

const ContentLanding: NextPage<ContentLandingProps> = ({
  featured,
  trending,
  fresh,
  tagged,
  initial_stub_mode,
}) => {
  const [stubMode, setStubMode] = useState(initial_stub_mode);
  const [items, setItems] = useState({ featured, trending, fresh, tagged });

  // client-side retry · in case SSR ran during stub-mode but API is now live
  useEffect(() => {
    if (!initial_stub_mode) return;
    let cancelled = false;
    (async () => {
      const [f, t, n, g] = await Promise.all([
        fetchContentList('featured', 8),
        fetchContentList('trending', 8),
        fetchContentList('new', 8),
        fetchContentList('tagged', 8),
      ]);
      if (cancelled) return;
      const stillStub = f.stub_mode || t.stub_mode || n.stub_mode || g.stub_mode;
      setStubMode(stillStub);
      setItems({
        featured: f.data?.items ?? STUB_LIST_RESPONSE.items,
        trending: t.data?.items ?? STUB_LIST_RESPONSE.items,
        fresh: n.data?.items ?? STUB_LIST_RESPONSE.items,
        tagged: g.data?.items ?? STUB_LIST_RESPONSE.items,
      });
    })();
    return () => {
      cancelled = true;
    };
  }, [initial_stub_mode]);

  return (
    <>
      <Head>
        <title>§ Content · Apocky</title>
        <meta
          name="description"
          content="Browse · search · trending · subscribed UGC content packages · sovereign-cap revocable · cosmetic-axiom attested"
        />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#0a0a0f" />
        <meta name="author" content="Apocky" />
        <link rel="canonical" href="https://apocky.com/content" />
        <meta property="og:title" content="§ Content · Apocky" />
        <meta property="og:description" content="UGC content portal · sovereign-by-default · ¬ engagement-tracking" />
        <meta property="og:type" content="website" />
        <style>{contentLandingCSS}</style>
      </Head>
      <main className="content-shell">
        <ContentNav active="index" />
        <header className="content-hero">
          <div className="content-eyebrow">§ Apocky · Content Portal</div>
          <h1 className="content-h1">
            Browse the Mycelium
          </h1>
          <p className="content-blurb">
            Player-authored content packages · 4 lenses · sovereign-cap revocable ·
            cosmetic-axiom attested · ¬ engagement-tracking · ¬ scroll-depth · ¬ algorithmic
            black-box.
          </p>
          <div className="content-actions">
            <a href="/content/feed" className="content-btn-primary">↓ chronological feed →</a>
            <a href="/content/trending" className="content-btn-ghost">↑ trending</a>
            <a href="/content/search" className="content-btn-ghost">⌕ search</a>
            <a href="/content/subscribed" className="content-btn-ghost">★ subscribed</a>
          </div>
        </header>

        <ContentFeed
          heading="§ Featured · curator picks"
          subtitle="hand-picked by Apocky · cosmetic-axiom-pre-attested · gift-economy emphasized"
          items={items.featured}
          stubMode={stubMode}
        />

        <ContentFeed
          heading="§ Trending · KAN-bias-weighted"
          subtitle="weighted by collective-engagement-bias from Akashic-Records · click ◐ on any card to see why"
          items={items.trending}
          showRationale={true}
          stubMode={stubMode}
        />

        <ContentFeed
          heading="§ New · published this week"
          subtitle="freshly-attested packages · sorted reverse-chronological"
          items={items.fresh}
          stubMode={stubMode}
        />

        <ContentFeed
          heading="§ Tagged-by-you · matches your declared interests"
          subtitle="based on your own tag-subscriptions · ¬ tracking-derived · ¬ behavioral-inference"
          items={items.tagged}
          stubMode={stubMode}
        />

        <ContentFooter />
      </main>
    </>
  );
};

export const ContentNav = ({ active }: { active: string }) => (
  <nav aria-label="content portal" className="content-nav">
    <a href="/" className="content-nav-back">← apocky.com</a>
    <a href="/content" className={`content-nav-link ${active === 'index' ? 'is-active' : ''}`}>browse</a>
    <a href="/content/feed" className={`content-nav-link ${active === 'feed' ? 'is-active' : ''}`}>feed</a>
    <a href="/content/trending" className={`content-nav-link ${active === 'trending' ? 'is-active' : ''}`}>trending</a>
    <a href="/content/search" className={`content-nav-link ${active === 'search' ? 'is-active' : ''}`}>search</a>
    <a href="/content/subscribed" className={`content-nav-link ${active === 'subscribed' ? 'is-active' : ''}`}>subscribed</a>
  </nav>
);

export const ContentFooter = () => (
  <footer className="content-footer">
    <p style={{ margin: 0 }}>
      § ¬ engagement-tracking · ¬ scroll-depth · ¬ time-on-page · ⊘ all UGC capabilities revocable · t∞
    </p>
    <p style={{ margin: '0.4rem 0 0' }}>
      © {new Date().getFullYear()} Apocky · The Substrate is its own attestation.
    </p>
  </footer>
);

export const contentLandingCSS = `
  * { box-sizing: border-box; }
  html, body { margin: 0; padding: 0; }
  body {
    background: radial-gradient(ellipse at top, #15151f 0%, #0a0a0f 50%, #050507 100%);
    color: #e6e6f0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    min-height: 100vh;
    -webkit-font-smoothing: antialiased;
  }
  a { color: inherit; text-decoration: none; }
  a:hover { opacity: 0.9; }
  .content-shell {
    max-width: 1140px;
    margin: 0 auto;
    padding: 3rem 1.5rem 6rem;
    line-height: 1.6;
  }
  @media (max-width: 640px) {
    .content-shell { padding: 1.75rem 1rem 4rem; }
  }
  .content-nav {
    display: flex;
    flex-wrap: wrap;
    gap: 1.25rem;
    padding-bottom: 1.5rem;
    margin-bottom: 2rem;
    border-bottom: 1px solid #1f1f2a;
    font-size: 0.82rem;
    color: #a8a8b8;
  }
  .content-nav-back { color: #7a7a8c; margin-right: auto; }
  .content-nav-link { color: #a8a8b8; }
  .content-nav-link.is-active { color: #c084fc; }
  .content-hero { margin-bottom: 3rem; }
  .content-eyebrow {
    display: inline-block;
    padding: 0.25rem 0.75rem;
    border: 1px solid #2a2a3a;
    border-radius: 4px;
    font-size: 0.7rem;
    letter-spacing: 0.15em;
    color: #a78bfa;
    margin-bottom: 1.25rem;
    text-transform: uppercase;
  }
  .content-h1 {
    font-size: clamp(1.75rem, 5vw, 3rem);
    line-height: 1.1;
    margin: 0;
    font-weight: 700;
    letter-spacing: -0.02em;
    background-image: linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
  }
  .content-blurb {
    font-size: 0.95rem;
    color: #a8a8b8;
    margin-top: 1rem;
    max-width: 640px;
    line-height: 1.6;
  }
  .content-actions {
    margin-top: 1.75rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem;
  }
  .content-btn-primary {
    padding: 0.65rem 1.25rem;
    background: linear-gradient(135deg, #c084fc 0%, #7dd3fc 100%);
    color: #0a0a0f;
    font-weight: 600;
    border-radius: 4px;
    font-size: 0.88rem;
  }
  .content-btn-ghost {
    padding: 0.65rem 1.25rem;
    border: 1px solid #2a2a3a;
    color: #cdd6e4;
    border-radius: 4px;
    font-size: 0.88rem;
  }
  .content-btn-ghost:hover { border-color: #c084fc; }
  .content-footer {
    margin-top: 4rem;
    padding-top: 2.5rem;
    border-top: 1px solid #1f1f2a;
    color: #5a5a6a;
    font-size: 0.78rem;
  }
`;

export const getServerSideProps: GetServerSideProps<ContentLandingProps> = async () => {
  // SSR-fetch all 4 buckets in parallel · stub-fallback on any 404
  const baseURL = process.env.VERCEL_URL ? `https://${process.env.VERCEL_URL}` : '';
  const fetchBucket = async (bucket: string): Promise<{ items: ContentItem[]; stub: boolean }> => {
    if (!baseURL) return { items: [...STUB_LIST_RESPONSE.items], stub: true };
    try {
      const res = await fetch(`${baseURL}/api/content/list?bucket=${bucket}&limit=8`, {
        headers: { Accept: 'application/json' },
      });
      if (res.status === 404) return { items: [...STUB_LIST_RESPONSE.items], stub: true };
      if (!res.ok) return { items: [...STUB_LIST_RESPONSE.items], stub: true };
      const json = await res.json();
      return { items: json.items ?? [], stub: false };
    } catch {
      return { items: [...STUB_LIST_RESPONSE.items], stub: true };
    }
  };
  const [f, t, n, g] = await Promise.all([
    fetchBucket('featured'),
    fetchBucket('trending'),
    fetchBucket('new'),
    fetchBucket('tagged'),
  ]);
  return {
    props: {
      featured: f.items,
      trending: t.items,
      fresh: n.items,
      tagged: g.items,
      initial_stub_mode: f.stub || t.stub || n.stub || g.stub,
    },
  };
};

export default ContentLanding;
