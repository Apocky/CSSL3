// cssl-edge · /marketplace/[id]
// Asset detail page. The composite id arrives as `<src>--<asset_id>` so a
// single dynamic segment is sufficient. Server-side fetches HEAD info from
// /api/asset/<src>/<id>/glb to surface attribution + size hint.

import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

interface DetailProps {
  src: string;
  id: string;
  cached: boolean;
  upstreamHint: string | null;
  attribution: string;
  format: 'glb';
  exists: boolean;
}

const Detail: NextPage<DetailProps> = ({
  src,
  id,
  cached,
  upstreamHint,
  attribution,
  format,
  exists,
}) => {
  return (
    <>
      <Head>
        <title>
          {id} · {src} · cssl-edge marketplace
        </title>
        <meta name="viewport" content="width=device-width, initial-scale=1" />
      </Head>
      <main
        style={{
          fontFamily:
            'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
          maxWidth: 760,
          margin: '0 auto',
          padding: '3rem 1.5rem',
          color: '#e6e6e6',
          background: '#0b0b10',
          minHeight: '100vh',
          lineHeight: 1.55,
        }}
      >
        <Link href="/marketplace" style={{ color: '#7dd3fc', textDecoration: 'none' }}>
          ← back to marketplace
        </Link>

        <h1 style={{ fontSize: '1.5rem', marginTop: '1rem', marginBottom: '0.5rem' }}>
          {id}
        </h1>
        <p style={{ color: '#9aa0a6', marginTop: 0 }}>
          source : <code style={{ color: '#7dd3fc' }}>{src}</code> · format :{' '}
          <code style={{ color: '#fbbf24' }}>{format}</code> · cached :{' '}
          <code style={{ color: cached ? '#16a34a' : '#9aa0a6' }}>
            {cached ? 'yes' : 'no'}
          </code>
        </p>

        {!exists ? (
          <section style={{ marginTop: '2rem', padding: '1rem', border: '1px dashed #1f1f29', borderRadius: 8 }}>
            <p style={{ margin: 0, color: '#fbbf24' }}>
              Asset descriptor unavailable from edge. The asset may still resolve via the upstream link below.
            </p>
          </section>
        ) : null}

        <section style={{ marginTop: '2rem' }}>
          <h2
            style={{
              fontSize: '0.9rem',
              textTransform: 'uppercase',
              letterSpacing: '0.08em',
              color: '#9aa0a6',
            }}
          >
            Attribution
          </h2>
          <p style={{ background: '#13131a', padding: '0.75rem', borderRadius: 6 }}>
            {attribution}
          </p>
        </section>

        <section style={{ marginTop: '2rem', display: 'flex', gap: '0.75rem' }}>
          <a
            href={`/api/asset/${encodeURIComponent(src)}/${encodeURIComponent(id)}/glb`}
            style={{
              padding: '0.6rem 1rem',
              background: '#0f4c81',
              color: '#ffffff',
              borderRadius: 4,
              textDecoration: 'none',
            }}
          >
            Download GLB
          </a>
          <button
            type="button"
            disabled
            style={{
              padding: '0.6rem 1rem',
              background: '#1f1f29',
              color: '#9aa0a6',
              border: 'none',
              borderRadius: 4,
              cursor: 'not-allowed',
            }}
            title="3D viewer wires up in wave-5"
          >
            View 3D (wave-5)
          </button>
        </section>

        {upstreamHint ? (
          <section style={{ marginTop: '2rem' }}>
            <h2
              style={{
                fontSize: '0.9rem',
                textTransform: 'uppercase',
                letterSpacing: '0.08em',
                color: '#9aa0a6',
              }}
            >
              Upstream
            </h2>
            <a
              href={upstreamHint}
              target="_blank"
              rel="noopener noreferrer"
              style={{ color: '#7dd3fc', wordBreak: 'break-all' }}
            >
              {upstreamHint}
            </a>
          </section>
        ) : null}
      </main>
    </>
  );
};

function originFromReq(reqHeaders: Record<string, string | string[] | undefined>): string {
  if (process.env.VERCEL_URL) return `https://${process.env.VERCEL_URL}`;
  const host = reqHeaders['host'];
  const h = Array.isArray(host) ? host[0] : host;
  return h ? `http://${h}` : 'http://localhost:3000';
}

// Parse `/marketplace/<src>--<id>` composite param. The double-dash separator
// avoids collisions with id-internal dashes (which are common in Polyhaven /
// Quaternius identifiers).
function splitCompositeId(raw: string): { src: string; id: string } {
  const i = raw.indexOf('--');
  if (i < 0) return { src: 'unknown', id: raw };
  return { src: raw.slice(0, i), id: raw.slice(i + 2) };
}

export const getServerSideProps: GetServerSideProps<DetailProps> = async (ctx) => {
  const idRaw = ctx.params?.['id'];
  const composite = (Array.isArray(idRaw) ? idRaw[0] : idRaw) ?? '';
  const { src, id } = splitCompositeId(composite);

  const origin = originFromReq(ctx.req.headers as Record<string, string | string[] | undefined>);
  let cached = false;
  let upstreamHint: string | null = null;
  let exists = false;
  try {
    const r = await fetch(`${origin}/api/asset/${encodeURIComponent(src)}/${encodeURIComponent(id)}/glb`);
    if (r.ok) {
      const j = (await r.json()) as { cached?: unknown; upstream_hint?: unknown };
      cached = j.cached === true;
      upstreamHint = typeof j.upstream_hint === 'string' ? j.upstream_hint : null;
      exists = true;
    }
  } catch {
    exists = false;
  }

  // Stage-0 attribution string. Real impl reads from cached license-row.
  const attribution = `${id} · sourced from ${src} · CC0 / CC-BY-4.0 (filtered)`;

  return {
    props: {
      src,
      id,
      cached,
      upstreamHint,
      attribution,
      format: 'glb',
      exists,
    },
  };
};

export default Detail;
