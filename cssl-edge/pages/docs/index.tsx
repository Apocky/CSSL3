// apocky.com/docs · index of grand-vision specs
// SSG · reads SPECS from build-time snapshot · zero runtime fs.

import type { NextPage, GetStaticProps } from 'next';
import Head from 'next/head';
import { SPECS } from '@/lib/specs-snapshot';

interface DocsIndexProps {
  entries: ReadonlyArray<{ slug: string; title: string }>;
}

const DocsIndex: NextPage<DocsIndexProps> = ({ entries }) => {
  return (
    <>
      <Head>
        <title>Docs · grand-vision specs · Apocky</title>
        <meta name="description" content="CSL3-glyph-native architecture specs for the Substrate, Σ-Chain, Mycelial Network, Akashic Records, distribution strategy, and the apocky.com portfolio hub." />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#0a0a0f" />
        <link rel="canonical" href="https://apocky.com/docs" />
        <style>{`
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
          a:hover { opacity: 0.85; }
        `}</style>
      </Head>
      <main
        style={{
          maxWidth: 880,
          margin: '0 auto',
          padding: '4rem 1.5rem 6rem',
          lineHeight: 1.65,
        }}
      >
        <a href="/" style={{ fontSize: '0.85rem', color: '#7a7a8c', display: 'inline-block', marginBottom: '2rem' }}>
          ← apocky.com
        </a>

        <h1
          style={{
            fontSize: 'clamp(1.75rem, 4vw, 2.5rem)',
            margin: 0,
            fontWeight: 700,
            letterSpacing: '-0.02em',
            backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%)',
            WebkitBackgroundClip: 'text',
            WebkitTextFillColor: 'transparent',
          }}
        >
          Grand-Vision Specs
        </h1>
        <p style={{ color: '#a8a8b8', marginTop: '0.5rem', fontSize: '0.95rem' }}>
          § CSL3-glyph-native architecture specs · density = sovereignty · {entries.length} documents
        </p>

        <section style={{ marginTop: '2.5rem', display: 'grid', gap: '0.75rem' }}>
          {entries.map((e) => (
            <a
              key={e.slug}
              href={`/docs/${e.slug}`}
              style={{
                display: 'block',
                padding: '1rem 1.2rem',
                background: 'rgba(20, 20, 30, 0.5)',
                border: '1px solid #1f1f2a',
                borderRadius: 6,
              }}
            >
              <div style={{ fontSize: '0.7rem', color: '#7a7a8c', letterSpacing: '0.1em' }}>{e.slug}</div>
              <div style={{ fontSize: '0.95rem', color: '#cdd6e4', marginTop: '0.25rem' }}>{e.title}</div>
            </a>
          ))}
        </section>

        <footer
          style={{
            marginTop: '4rem',
            paddingTop: '2.5rem',
            borderTop: '1px solid #1f1f2a',
            color: '#5a5a6a',
            fontSize: '0.78rem',
          }}
        >
          <p style={{ margin: 0 }}>§ ¬ harm in the making · sovereignty preserved · t∞</p>
          <p style={{ margin: '0.4rem 0 0' }}>
            Specs auto-snapshot from <code style={{ color: '#a78bfa' }}>specs/grand-vision/*.csl</code> at build-time.
          </p>
        </footer>
      </main>
    </>
  );
};

export const getStaticProps: GetStaticProps<DocsIndexProps> = async () => {
  return {
    props: {
      entries: SPECS.map((s) => ({ slug: s.slug, title: s.title })),
    },
  };
};

export default DocsIndex;
