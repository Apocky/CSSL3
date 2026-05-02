// cssl-edge · pages/content/[slug].tsx
// W12-6 · /content/[slug] · per-package detail page
// SSR-fetch via getServerSideProps · stub-fallback on 404
// Renders ContentDetail component with full data

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import ContentDetail from '@/components/ContentDetail';
import { ContentNav, ContentFooter, contentLandingCSS } from './index';
import {
  STUB_DETAIL,
  type ContentDetail as ContentDetailType,
} from '@/lib/content-fetch';

interface ContentDetailPageProps {
  slug: string;
  detail: ContentDetailType;
  stub_mode: boolean;
  not_found: boolean;
}

const ContentDetailPage: NextPage<ContentDetailPageProps> = ({
  slug,
  detail,
  stub_mode,
  not_found,
}) => {
  const titleText = not_found
    ? `§ ${slug} · not found`
    : `§ ${detail.title} · Content · Apocky`;
  const descText = not_found
    ? 'package not found'
    : detail.blurb;

  return (
    <>
      <Head>
        <title>{titleText}</title>
        <meta name="description" content={descText} />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#0a0a0f" />
        <link rel="canonical" href={`https://apocky.com/content/${encodeURIComponent(slug)}`} />
        <meta property="og:title" content={titleText} />
        <meta property="og:description" content={descText} />
        <meta property="og:type" content="article" />
        <style>{contentLandingCSS}</style>
      </Head>
      <main className="content-shell">
        <ContentNav active="detail" />

        {not_found ? (
          <div style={{ padding: '4rem 1.5rem', textAlign: 'center' }}>
            <h1 className="content-h1" style={{ fontSize: 'clamp(1.5rem, 4vw, 2.2rem)' }}>
              § Not found
            </h1>
            <p className="content-blurb" style={{ margin: '1rem auto', maxWidth: 480 }}>
              ✗ package <code style={{ color: '#fbbf24' }}>{slug}</code> not found · it may have
              been unpublished or revoked-by-author (Σ-mask sovereign-cap)
            </p>
            <a
              href="/content"
              style={{
                display: 'inline-block',
                marginTop: '1rem',
                padding: '0.65rem 1.25rem',
                background: 'transparent',
                border: '1px solid #2a2a3a',
                color: '#cdd6e4',
                borderRadius: 4,
                fontSize: '0.88rem',
                textDecoration: 'none',
              }}
            >
              ← back to /content
            </a>
          </div>
        ) : (
          <ContentDetail detail={detail} stubMode={stub_mode} />
        )}

        <ContentFooter />
      </main>
    </>
  );
};

export const getServerSideProps: GetServerSideProps<ContentDetailPageProps> = async (ctx) => {
  const slug = typeof ctx.params?.slug === 'string' ? ctx.params.slug : '';
  if (!slug || slug.length === 0) {
    return { notFound: true };
  }

  const baseURL = process.env.VERCEL_URL ? `https://${process.env.VERCEL_URL}` : '';
  if (!baseURL) {
    // Local dev OR API not deployed → stub
    return {
      props: {
        slug,
        detail: { ...STUB_DETAIL, slug, title: `⟨ ${slug} · stub ⟩` },
        stub_mode: true,
        not_found: false,
      },
    };
  }

  try {
    const res = await fetch(
      `${baseURL}/api/content/detail/${encodeURIComponent(slug)}`,
      { headers: { Accept: 'application/json' } },
    );
    if (res.status === 404) {
      // Distinguish "package not found" (404 with body) vs "API not wired" (404 no body)
      // Pragmatic stub heuristic : try parse JSON · success-shape → not_found · else stub
      try {
        const body = await res.json();
        if (body && body.error === 'not_found') {
          return {
            props: {
              slug,
              detail: STUB_DETAIL,
              stub_mode: false,
              not_found: true,
            },
          };
        }
      } catch {
        /* fall through to stub */
      }
      return {
        props: {
          slug,
          detail: { ...STUB_DETAIL, slug, title: `⟨ ${slug} · stub ⟩` },
          stub_mode: true,
          not_found: false,
        },
      };
    }
    if (!res.ok) {
      return {
        props: {
          slug,
          detail: { ...STUB_DETAIL, slug, title: `⟨ ${slug} · stub ⟩` },
          stub_mode: true,
          not_found: false,
        },
      };
    }
    const detail = (await res.json()) as ContentDetailType;
    return {
      props: { slug, detail, stub_mode: false, not_found: false },
    };
  } catch {
    return {
      props: {
        slug,
        detail: { ...STUB_DETAIL, slug, title: `⟨ ${slug} · stub ⟩` },
        stub_mode: true,
        not_found: false,
      },
    };
  }
};

export default ContentDetailPage;
