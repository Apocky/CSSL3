// cssl-edge · /marketplace
// Server-rendered asset gallery. Loads /api/asset/search?q=&license=...
// server-side via getServerSideProps (the pages-router equivalent of an
// app-router server component). 24 cards per page · ?page=N pagination.

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

interface AssetResult {
  src: string;
  id: string;
  name: string;
  license: string;
  format: string;
  url: string;
  preview_url: string;
}

interface MarketplaceProps {
  results: AssetResult[];
  total: number;
  page: number;
  pageSize: number;
  q: string;
  licenseFilter: string;
  hasNextPage: boolean;
}

const PAGE_SIZE = 24;

// License-badge color : CC0 = green · CC-BY = blue · others = gray.
function licenseBadgeColor(license: string): { bg: string; fg: string } {
  const l = license.toLowerCase();
  if (l === 'cc0') return { bg: '#16a34a', fg: '#ffffff' };
  if (l === 'cc-by' || l === 'cc-by-4.0' || l === 'cc-by-3.0') return { bg: '#2563eb', fg: '#ffffff' };
  return { bg: '#6b7280', fg: '#ffffff' };
}

// Pretty license-string for badge text.
function licenseLabel(license: string): string {
  const l = license.toLowerCase();
  if (l === 'cc0') return 'CC0';
  if (l === 'cc-by' || l === 'cc-by-4.0') return 'CC-BY-4.0';
  if (l === 'cc-by-sa') return 'CC-BY-SA';
  if (l === 'public-domain') return 'Public Domain';
  return license.toUpperCase();
}

const Marketplace: NextPage<MarketplaceProps> = ({
  results,
  total,
  page,
  pageSize,
  q,
  licenseFilter,
  hasNextPage,
}) => {
  const empty = results.length === 0;
  return (
    <>
      <Head>
        <title>Marketplace · cssl-edge</title>
        <meta name="description" content="LoA-v13 asset marketplace · CC0 + CC-BY-4.0 only" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
      </Head>
      <main
        style={{
          fontFamily:
            'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
          maxWidth: 1200,
          margin: '0 auto',
          padding: '3rem 1.5rem',
          color: '#e6e6e6',
          background: '#0b0b10',
          minHeight: '100vh',
          lineHeight: 1.55,
        }}
      >
        <header style={{ marginBottom: '2rem' }}>
          <Link href="/" style={{ color: '#7dd3fc', textDecoration: 'none' }}>
            ← back
          </Link>
          <h1 style={{ fontSize: '1.75rem', marginTop: '0.5rem', marginBottom: '0.25rem' }}>
            Marketplace
          </h1>
          <p style={{ color: '#9aa0a6', marginTop: 0 }}>
            License-filtered asset gallery · {licenseFilter} · {total} result{total === 1 ? '' : 's'}
            {q.length > 0 ? ` · query "${q}"` : ''}
          </p>
        </header>

        {empty ? (
          <section
            style={{
              padding: '3rem 1rem',
              textAlign: 'center',
              border: '1px dashed #1f1f29',
              borderRadius: 8,
              color: '#9aa0a6',
            }}
          >
            <p style={{ fontSize: '1rem', margin: 0 }}>
              No assets matching filter — try CC0 or CC-BY-4.0
            </p>
          </section>
        ) : (
          <section
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fill, minmax(260px, 1fr))',
              gap: '1rem',
            }}
          >
            {results.map((asset) => {
              const badge = licenseBadgeColor(asset.license);
              return (
                <article
                  key={`${asset.src}-${asset.id}`}
                  style={{
                    background: '#13131a',
                    border: '1px solid #1f1f29',
                    borderRadius: 8,
                    padding: '1rem',
                    display: 'flex',
                    flexDirection: 'column',
                    gap: '0.5rem',
                  }}
                >
                  <div
                    style={{
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'center',
                    }}
                  >
                    <h3 style={{ fontSize: '1rem', margin: 0, color: '#e6e6e6' }}>
                      {asset.name}
                    </h3>
                    <span
                      style={{
                        fontSize: '0.7rem',
                        padding: '0.2rem 0.5rem',
                        borderRadius: 4,
                        background: badge.bg,
                        color: badge.fg,
                        fontWeight: 600,
                      }}
                    >
                      {licenseLabel(asset.license)}
                    </span>
                  </div>
                  <p style={{ margin: 0, fontSize: '0.85rem', color: '#9aa0a6' }}>
                    Author : {asset.src} · Format : {asset.format}
                  </p>
                  <div
                    style={{
                      display: 'flex',
                      gap: '0.5rem',
                      marginTop: '0.5rem',
                    }}
                  >
                    <Link
                      href={`/marketplace/${encodeURIComponent(asset.src)}--${encodeURIComponent(asset.id)}`}
                      style={{
                        flex: 1,
                        padding: '0.4rem 0.6rem',
                        background: '#1f1f29',
                        color: '#7dd3fc',
                        textAlign: 'center',
                        borderRadius: 4,
                        textDecoration: 'none',
                        fontSize: '0.85rem',
                      }}
                    >
                      View
                    </Link>
                    <a
                      href={asset.url}
                      target="_blank"
                      rel="noopener noreferrer"
                      style={{
                        flex: 1,
                        padding: '0.4rem 0.6rem',
                        background: '#0f4c81',
                        color: '#ffffff',
                        textAlign: 'center',
                        borderRadius: 4,
                        textDecoration: 'none',
                        fontSize: '0.85rem',
                      }}
                    >
                      Add to Scene
                    </a>
                  </div>
                </article>
              );
            })}
          </section>
        )}

        <nav
          style={{
            marginTop: '2rem',
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            color: '#9aa0a6',
          }}
        >
          <span>
            Page {page} · {pageSize} per page
          </span>
          <span style={{ display: 'flex', gap: '0.5rem' }}>
            {page > 1 && (
              <Link
                href={`/marketplace?page=${page - 1}${q ? `&q=${encodeURIComponent(q)}` : ''}`}
                style={{ color: '#7dd3fc', textDecoration: 'none' }}
              >
                ← prev
              </Link>
            )}
            {hasNextPage && (
              <Link
                href={`/marketplace?page=${page + 1}${q ? `&q=${encodeURIComponent(q)}` : ''}`}
                style={{ color: '#7dd3fc', textDecoration: 'none' }}
              >
                next →
              </Link>
            )}
          </span>
        </nav>
      </main>
    </>
  );
};

// Server-side data fetch — runs at request time, not in the browser. The
// /api/asset/search endpoint lives on the same deployment, so we call it via
// an absolute URL constructed from VERCEL_URL (or HOST during local dev).
function originFromReq(reqHeaders: Record<string, string | string[] | undefined>): string {
  if (process.env.VERCEL_URL) return `https://${process.env.VERCEL_URL}`;
  const host = reqHeaders['host'];
  const h = Array.isArray(host) ? host[0] : host;
  return h ? `http://${h}` : 'http://localhost:3000';
}

export const getServerSideProps: GetServerSideProps<MarketplaceProps> = async (
  ctx
) => {
  const qRaw = ctx.query['q'];
  const pageRaw = ctx.query['page'];
  const licenseRaw = ctx.query['license'];

  const q = (Array.isArray(qRaw) ? qRaw[0] : qRaw) ?? '';
  const pageStr = (Array.isArray(pageRaw) ? pageRaw[0] : pageRaw) ?? '1';
  const page = Math.max(1, parseInt(pageStr, 10) || 1);
  const licenseFilter = (Array.isArray(licenseRaw) ? licenseRaw[0] : licenseRaw) ?? 'CC0,CCBY40';

  const origin = originFromReq(ctx.req.headers as Record<string, string | string[] | undefined>);
  const params = new URLSearchParams();
  if (q) params.set('q', q);
  // /api/asset/search currently accepts a single license value; default CC0
  // is fine for stage-0 — we surface the multi-license preference in the UI.
  params.set('license', 'cc0');

  let results: AssetResult[] = [];
  let total = 0;
  try {
    const r = await fetch(`${origin}/api/asset/search?${params.toString()}`);
    if (r.ok) {
      const j = (await r.json()) as { results?: AssetResult[]; total?: number };
      results = Array.isArray(j.results) ? j.results : [];
      total = typeof j.total === 'number' ? j.total : results.length;
    }
  } catch {
    // Defensive : network errors during SSR shouldn't crash the page.
    results = [];
    total = 0;
  }

  // Client-side pagination over the combined result list.
  const start = (page - 1) * PAGE_SIZE;
  const slice = results.slice(start, start + PAGE_SIZE);
  const hasNextPage = start + PAGE_SIZE < results.length;

  return {
    props: {
      results: slice,
      total,
      page,
      pageSize: PAGE_SIZE,
      q,
      licenseFilter,
      hasNextPage,
    },
  };
};

export default Marketplace;
