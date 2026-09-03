// cssl-edge · pages/content/[slug].tsx
// W12-6 · /content/[slug] · per-package detail page
// SSR-fetch via getServerSideProps · never invents a placeholder item
// Renders ContentDetail component with full data

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import ContentDetail from '@/components/ContentDetail';
import { ContentNav, ContentFooter, contentLandingCSS } from './index';
import type { ContentDetail as ContentDetailType } from '@/lib/content-fetch';

interface ContentDetailPageProps {
  slug: string;
  detail: ContentDetailType | null;
  not_found: boolean;
}

const ContentDetailPage: NextPage<ContentDetailPageProps> = ({
  slug,
  detail,
  not_found,
}) => {
  const missing = not_found || detail === null;
  const titleText = missing
    ? `${slug} · not found`
    : `${detail.title} · Shared content · Apocky`;
  const descText = missing
    ? 'Shared item not found'
    : detail.blurb;

  return (
    <>
      <Head>
        <title>{titleText}</title>
        <meta name="description" content={descText} />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#0a0a0f" />
        <link rel="canonical" href={`https://www.apocky.com/content/${encodeURIComponent(slug)}`} />
        <meta property="og:title" content={titleText} />
        <meta property="og:description" content={descText} />
        <meta property="og:type" content="article" />
        <style>{contentLandingCSS}</style>
      </Head>
      <main className="content-shell">
        <ContentNav active="detail" />

        {missing ? (
          <div style={{ padding: '4rem 1.5rem', textAlign: 'center' }}>
            <h1 className="content-h1" style={{ fontSize: 'clamp(1.5rem, 4vw, 2.2rem)' }}>
              Not found
            </h1>
            <p className="content-blurb" style={{ margin: '1rem auto', maxWidth: 480 }}>
              The item <code style={{ color: '#fbbf24' }}>{slug}</code> was not found. It may never have been
              published, or its author may have withdrawn it.
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
          <ContentDetail detail={detail} />
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
    return { notFound: true };
  }

  try {
    const res = await fetch(
      `${baseURL}/api/content/detail/${encodeURIComponent(slug)}`,
      { headers: { Accept: 'application/json' } },
    );
    if (res.status === 404) {
      // A typed not-found response renders a clear page; an absent API returns 404.
      try {
        const body = await res.json();
        if (body && body.error === 'not_found') {
          return {
            props: {
              slug,
              detail: null,
              not_found: true,
            },
          };
        }
      } catch {
        /* fall through to framework 404 */
      }
      return { notFound: true };
    }
    if (!res.ok) {
      return { notFound: true };
    }
    const detail = (await res.json()) as ContentDetailType;
    return {
      props: { slug, detail, not_found: false },
    };
  } catch {
    return { notFound: true };
  }
};

export default ContentDetailPage;
