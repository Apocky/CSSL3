// cssl-edge · pages/content/index.tsx
// W12-6 · UGC-Discover-Browse · /content landing
// 4 sections : Featured · Trending · New · Tagged-by-you
// Phone-first responsive · SSR + client-side fetch

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import { useEffect, useState } from 'react';
import ContentFeed from '@/components/ContentFeed';
import {
  fetchContentList,
  EMPTY_LIST_RESPONSE,
  type ContentItem,
} from '@/lib/content-fetch';

interface ContentLandingProps {
  /** SSR-fetched items per bucket. Empty arrays → client may retry. */
  featured: ReadonlyArray<ContentItem>;
  trending: ReadonlyArray<ContentItem>;
  fresh: ReadonlyArray<ContentItem>;
  tagged: ReadonlyArray<ContentItem>;
  initial_unavailable: boolean;
}

const ContentLanding: NextPage<ContentLandingProps> = ({
  featured,
  trending,
  fresh,
  tagged,
  initial_unavailable,
}) => {
  const [unavailable, setUnavailable] = useState(initial_unavailable);
  const [items, setItems] = useState({ featured, trending, fresh, tagged });

  // Retry in the browser in case the service recovered after server rendering.
  useEffect(() => {
    if (!initial_unavailable) return;
    let cancelled = false;
    (async () => {
      const [f, t, n, g] = await Promise.all([
        fetchContentList('featured', 8),
        fetchContentList('trending', 8),
        fetchContentList('new', 8),
        fetchContentList('tagged', 8),
      ]);
      if (cancelled) return;
      const stillUnavailable = f.unavailable || t.unavailable || n.unavailable || g.unavailable;
      setUnavailable(stillUnavailable);
      setItems({
        featured: f.data?.items ?? EMPTY_LIST_RESPONSE.items,
        trending: t.data?.items ?? EMPTY_LIST_RESPONSE.items,
        fresh: n.data?.items ?? EMPTY_LIST_RESPONSE.items,
        tagged: g.data?.items ?? EMPTY_LIST_RESPONSE.items,
      });
    })();
    return () => {
      cancelled = true;
    };
  }, [initial_unavailable]);

  return (
    <>
      <Head>
        <title>Shared content · Apocky</title>
        <meta
          name="description"
          content="An experimental library for content people deliberately choose to share. Participation is optional."
        />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#0a0a0f" />
        <meta name="author" content="Apocky" />
        <link rel="canonical" href="https://apocky.com/content" />
        <meta property="og:title" content="Shared content · Apocky" />
        <meta property="og:description" content="An experimental, optional library for community-created content." />
        <meta property="og:type" content="website" />
        <style>{contentLandingCSS}</style>
      </Head>
      <main className="content-shell">
        <ContentNav active="index" />
        <header className="content-hero">
          <div className="content-eyebrow">Experimental shared library</div>
          <h1 className="content-h1">
            Shared content
          </h1>
          <p className="content-blurb">
            A place for packages that people deliberately choose to publish. Publishing, browsing, and
            contributing are optional. If the service is unavailable, the page shows an honest empty state
            instead of invented examples.
          </p>
          <div className="content-actions">
            <a href="/content/feed" className="content-btn-primary">Newest first</a>
            <a href="/content/trending" className="content-btn-ghost">Popular</a>
            <a href="/content/search" className="content-btn-ghost">Search</a>
            <a href="/content/subscribed" className="content-btn-ghost">Subscriptions</a>
          </div>
        </header>

        <ContentFeed
          heading="Featured"
          subtitle="Items selected by Apocky. Selection does not imply an independent security review."
          items={items.featured}
          unavailable={unavailable}
        />

        <ContentFeed
          heading="Popular now"
          subtitle="When data exists, each card can explain why it appears in this list."
          items={items.trending}
          showRationale={true}
          unavailable={unavailable}
        />

        <ContentFeed
          heading="New"
          subtitle="Recently published items, newest first."
          items={items.fresh}
          unavailable={unavailable}
        />

        <ContentFeed
          heading="Tags you chose"
          subtitle="Matches tags you deliberately followed, when that feature is available."
          items={items.tagged}
          unavailable={unavailable}
        />

        <ContentFooter />
      </main>
    </>
  );
};

export const ContentNav = ({ active }: { active: string }) => (
  <nav aria-label="content portal" className="content-nav">
    <a href="/" className="content-nav-back">← apocky.com</a>
    <a href="/content" className={`content-nav-link ${active === 'index' ? 'is-active' : ''}`}>Browse</a>
    <a href="/content/feed" className={`content-nav-link ${active === 'feed' ? 'is-active' : ''}`}>Newest</a>
    <a href="/content/trending" className={`content-nav-link ${active === 'trending' ? 'is-active' : ''}`}>Popular</a>
    <a href="/content/search" className={`content-nav-link ${active === 'search' ? 'is-active' : ''}`}>Search</a>
    <a href="/content/subscribed" className={`content-nav-link ${active === 'subscribed' ? 'is-active' : ''}`}>Subscriptions</a>
  </nav>
);

export const ContentFooter = () => (
  <footer className="content-footer">
    <p style={{ margin: 0 }}>
      Sharing is voluntary. A published item should identify its author and its current status.
    </p>
    <p style={{ margin: '0.4rem 0 0' }}>
      © {new Date().getFullYear()} Apocky · Technical claims require evidence; a label is not proof.
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

// The content service is not connected. Keep the route absent instead of
// publishing a portal whose controls cannot complete their advertised work.
export const getServerSideProps: GetServerSideProps<ContentLandingProps> = async () => ({
  notFound: true,
});

export default ContentLanding;
