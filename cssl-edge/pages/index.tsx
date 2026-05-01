// cssl-edge · pages/index.tsx
// Static landing page. No client-side data-fetch — pure SSG-friendly.

import type { NextPage } from 'next';
import Head from 'next/head';

const ENDPOINTS: ReadonlyArray<{ path: string; desc: string }> = [
  { path: 'GET  /api/health', desc: 'Liveness ping · returns commit SHA' },
  { path: 'POST /api/intent', desc: 'text → scene-graph (LLM-backed when configured)' },
  { path: 'GET  /api/asset/search?q=...', desc: 'license-filtered asset search across free upstreams' },
  { path: 'GET  /api/asset/<src>/<id>/glb', desc: 'cached binary proxy for a specific asset' },
  { path: 'POST /api/generate/3d', desc: 'neural-3D gateway · provider fan-out' },
];

const Home: NextPage = () => {
  return (
    <>
      <Head>
        <title>cssl-edge · LoA-v13 Edge</title>
        <meta name="description" content="LoA-v13 Edge · public · MCP gateway for CSSL/LoA scenes" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
      </Head>
      <main
        style={{
          fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
          maxWidth: 760,
          margin: '0 auto',
          padding: '4rem 1.5rem',
          color: '#e6e6e6',
          background: '#0b0b10',
          minHeight: '100vh',
          lineHeight: 1.55,
        }}
      >
        <h1 style={{ fontSize: '1.75rem', marginBottom: '0.25rem' }}>
          cssl-edge
        </h1>
        <p style={{ color: '#9aa0a6', marginTop: 0 }}>
          LoA-v13 Edge · public · MCP gateway
        </p>

        <section style={{ marginTop: '2.5rem' }}>
          <h2 style={{ fontSize: '1rem', textTransform: 'uppercase', letterSpacing: '0.08em', color: '#9aa0a6' }}>
            Endpoints
          </h2>
          <ul style={{ listStyle: 'none', padding: 0 }}>
            {ENDPOINTS.map((ep) => (
              <li key={ep.path} style={{ padding: '0.6rem 0', borderBottom: '1px solid #1f1f29' }}>
                <code style={{ color: '#7dd3fc', display: 'block' }}>{ep.path}</code>
                <span style={{ color: '#cdd6e4', fontSize: '0.92rem' }}>{ep.desc}</span>
              </li>
            ))}
          </ul>
        </section>

        <section style={{ marginTop: '2.5rem' }}>
          <h2 style={{ fontSize: '1rem', textTransform: 'uppercase', letterSpacing: '0.08em', color: '#9aa0a6' }}>
            Status
          </h2>
          <p>
            Stage-0 scaffold · all endpoints return well-shaped stub responses. Provide
            <code style={{ color: '#fbbf24' }}> CLAUDE_API_KEY</code>,
            <code style={{ color: '#fbbf24' }}> SUPABASE_URL</code>, and provider keys to
            unlock real responses. See <code>.env.example</code> for the full list.
          </p>
        </section>

        <footer style={{ marginTop: '4rem', color: '#6b7280', fontSize: '0.85rem' }}>
          <p>
            Source : CSSLv3 · branch <code>cssl/session-15/W-WAVE3-vercel-stub</code>
          </p>
          <p style={{ marginTop: '0.25rem' }}>
            There was no hurt nor harm in the making of this, to anyone/anything/anybody.
          </p>
        </footer>
      </main>
    </>
  );
};

export default Home;
