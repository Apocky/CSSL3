// cssl-edge · /marketplace/recommended
// Server-rendered "recommended for you" panel · 24-card grid keyed off
// /api/asset/recommend. License-badge + score + why text on each card.
// Style mirrors /marketplace/index.tsx so the gallery feels of-a-piece.

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

interface AssetSummary {
  asset_id: string;
  source: string;
  name: string;
  license: string;
  license_short: string;
  score: number;
  why: string;
}

interface RecommendedProps {
  recommendations: AssetSummary[];
  reason: string;
  player_id: string;
  total: number;
}

// License-badge color : CC0 = green · CC-BY = blue · others = gray.
function licenseBadgeColor(license: string): { bg: string; fg: string } {
  const l = license.toLowerCase();
  if (l === 'cc0') return { bg: '#16a34a', fg: '#ffffff' };
  if (l === 'cc-by' || l === 'cc-by-4.0' || l === 'cc-by-3.0') {
    return { bg: '#2563eb', fg: '#ffffff' };
  }
  return { bg: '#6b7280', fg: '#ffffff' };
}

const Recommended: NextPage<RecommendedProps> = ({
  recommendations,
  reason,
  player_id,
  total,
}) => {
  const empty = recommendations.length === 0;
  return (
    <>
      <Head>
        <title>Recommended · cssl-edge</title>
        <meta
          name="description"
          content="Personalized asset recommendations · CC0 + CC-BY-4.0"
        />
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
          <Link
            href="/marketplace"
            style={{ color: '#7dd3fc', textDecoration: 'none' }}
          >
            ← marketplace
          </Link>
          <h1
            style={{
              fontSize: '1.75rem',
              marginTop: '0.5rem',
              marginBottom: '0.25rem',
            }}
          >
            Recommended for you
          </h1>
          <p style={{ color: '#9aa0a6', marginTop: 0 }}>
            {total} result{total === 1 ? '' : 's'} · player_id : {player_id}
          </p>
          <p
            style={{
              color: '#9aa0a6',
              marginTop: '0.25rem',
              fontSize: '0.85rem',
            }}
          >
            {reason}
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
              No recommendations yet — interact with a few assets and check
              back.
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
            {recommendations.map((rec) => {
              const badge = licenseBadgeColor(rec.license);
              return (
                <article
                  key={rec.asset_id}
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
                      gap: '0.5rem',
                    }}
                  >
                    <h3
                      style={{
                        fontSize: '1rem',
                        margin: 0,
                        color: '#e6e6e6',
                        flex: 1,
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                      }}
                    >
                      {rec.name}
                    </h3>
                    <span
                      style={{
                        fontSize: '0.7rem',
                        padding: '0.2rem 0.5rem',
                        borderRadius: 4,
                        background: badge.bg,
                        color: badge.fg,
                        fontWeight: 600,
                        flexShrink: 0,
                      }}
                    >
                      {rec.license_short}
                    </span>
                  </div>
                  <p
                    style={{
                      margin: 0,
                      fontSize: '0.85rem',
                      color: '#9aa0a6',
                    }}
                  >
                    Source : {rec.source}
                  </p>
                  <p
                    style={{
                      margin: 0,
                      fontSize: '0.8rem',
                      color: '#7dd3fc',
                    }}
                  >
                    score : {rec.score.toFixed(3)}
                  </p>
                  <p
                    style={{
                      margin: 0,
                      fontSize: '0.8rem',
                      color: '#9aa0a6',
                      fontStyle: 'italic',
                    }}
                  >
                    {rec.why}
                  </p>
                  <Link
                    href={`/marketplace/${encodeURIComponent(rec.asset_id)}`}
                    style={{
                      marginTop: '0.5rem',
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
                </article>
              );
            })}
          </section>
        )}
      </main>
    </>
  );
};

// Server-side data fetch — calls /api/asset/recommend on the same deployment.
function originFromReq(
  reqHeaders: Record<string, string | string[] | undefined>
): string {
  if (process.env.VERCEL_URL) return `https://${process.env.VERCEL_URL}`;
  const host = reqHeaders['host'];
  const h = Array.isArray(host) ? host[0] : host;
  return h ? `http://${h}` : 'http://localhost:3000';
}

export const getServerSideProps: GetServerSideProps<RecommendedProps> = async (
  ctx
) => {
  const playerRaw = ctx.query['player_id'];
  const seedRaw = ctx.query['seed_features'];

  const player_id =
    (Array.isArray(playerRaw) ? playerRaw[0] : playerRaw) ?? 'anonymous';
  const seedFeatures =
    (Array.isArray(seedRaw) ? seedRaw[0] : seedRaw) ?? '';

  const origin = originFromReq(
    ctx.req.headers as Record<string, string | string[] | undefined>
  );

  const params = new URLSearchParams();
  params.set('player_id', player_id);
  if (seedFeatures.length > 0) params.set('seed_features', seedFeatures);
  params.set('limit', '24');

  let recommendations: AssetSummary[] = [];
  let reason = '';
  let total = 0;
  try {
    const r = await fetch(
      `${origin}/api/asset/recommend?${params.toString()}`
    );
    if (r.ok) {
      const j = (await r.json()) as {
        recommendations?: AssetSummary[];
        reason?: string;
        total?: number;
      };
      recommendations = Array.isArray(j.recommendations) ? j.recommendations : [];
      reason = typeof j.reason === 'string' ? j.reason : '';
      total = typeof j.total === 'number' ? j.total : recommendations.length;
    }
  } catch {
    recommendations = [];
    reason = '';
    total = 0;
  }

  return {
    props: {
      recommendations,
      reason,
      player_id,
      total,
    },
  };
};

export default Recommended;
