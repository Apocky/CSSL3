// apocky.com/docs/[slug] · render single grand-vision spec
// SSG with getStaticPaths · CSL3-glyph-aware <pre> rendering.

import type { NextPage, GetStaticPaths, GetStaticProps } from 'next';
import Head from 'next/head';
import { SPECS, findSpec, type SpecEntry } from '@/lib/specs-snapshot';

interface DocsPageProps {
  spec: SpecEntry;
  prevSlug: string | null;
  nextSlug: string | null;
}

const DocsPage: NextPage<DocsPageProps> = ({ spec, prevSlug, nextSlug }) => {
  const isLegacyAkashicSpecification = spec.slug === '18_AKASHIC_RECORDS';
  const displayTitle = isLegacyAkashicSpecification
    ? 'Akashic Records — legacy Labyrinth technical specification'
    : spec.title;
  const canonicalUrl = `https://www.apocky.com/docs/${spec.slug}`;
  return (
    <>
      <Head>
        <title>{displayTitle} · Apocky docs</title>
        <meta
          name="description"
          content={isLegacyAkashicSpecification
            ? 'Preserved legacy Labyrinth of Apocalypse technical specification. This is distinct from Shawn Apocky’s public works archive.'
            : `${spec.slug}: a technical architecture specification written in CSLv3 notation.`}
        />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#000000" />
        <link rel="canonical" href={canonicalUrl} />
        <style>{`
          .csl-reference, .csl-reference * { box-sizing: border-box; }
          .csl-reference {
            background: radial-gradient(ellipse at top, #15151f 0%, #0a0a0f 50%, #050507 100%);
            color: #e6e6f0;
            font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
            min-height: 100vh;
            -webkit-font-smoothing: antialiased;
          }
          .csl-reference a { color: inherit; text-decoration: none; }
          .csl-reference a:hover { opacity: 0.85; }
          .csl-reference pre.csl-spec {
            background: rgba(15, 15, 25, 0.6);
            border: 1px solid #1f1f2a;
            border-radius: 8px;
            padding: 1.25rem 1.5rem;
            font-size: 0.82rem;
            line-height: 1.55;
            color: #cdd6e4;
            white-space: pre-wrap;
            overflow-x: auto;
            tab-size: 2;
          }
          .csl-reference pre.csl-spec span.glyph { color: #a78bfa; font-weight: 600; }
          .csl-reference pre.csl-spec span.modal { color: #fbbf24; }
          .csl-reference pre.csl-spec span.evidence { color: #34d399; }
          .csl-reference pre.csl-spec span.heading { color: #c084fc; font-weight: 600; }
        `}</style>
      </Head>
      <main
        className="csl-reference"
        style={{
          maxWidth: 920,
          margin: '0 auto',
          padding: '2rem 1.5rem 4rem',
          lineHeight: 1.65,
        }}
      >
        <a href="/docs" style={{ fontSize: '0.85rem', color: '#a8a8b8', display: 'inline-flex', alignItems: 'center', minHeight: 44, marginBottom: '1rem' }}>
          ← Guides & answers
        </a>

        {isLegacyAkashicSpecification ? (
          <div
            style={{
              marginBottom: '1.5rem',
              padding: '1rem 1.1rem',
              border: '1px solid rgba(251, 191, 36, 0.3)',
              borderLeft: '3px solid #fbbf24',
              borderRadius: 6,
              color: '#cdd6e4',
              background: 'rgba(251, 191, 36, 0.05)',
            }}
          >
            <strong>Legacy Labyrinth technical specification.</strong>{' '}
            This preserved document is distinct from the{' '}
            <a href="/akashic-records" style={{ color: '#fbbf24', textDecoration: 'underline' }}>
              Akashic Records public works archive
            </a>.
          </div>
        ) : null}

        <div style={{ fontSize: '0.7rem', color: '#7a7a8c', letterSpacing: '0.1em', textTransform: 'uppercase' }}>
          {spec.filename}
        </div>
        <h1
          style={{
            fontSize: 'clamp(1.4rem, 3vw, 2rem)',
            margin: '0.4rem 0 0',
            fontWeight: 700,
            color: '#e6e6f0',
            letterSpacing: '-0.01em',
          }}
        >
          {displayTitle}
        </h1>

        <div
          style={{
            marginTop: '1.5rem',
            padding: '1rem 1.1rem',
            border: '1px solid rgba(125, 211, 252, 0.25)',
            borderLeft: '3px solid #7dd3fc',
            borderRadius: 6,
            color: '#cdd6e4',
            background: 'rgba(125, 211, 252, 0.05)',
          }}
        >
          Technical reference in CSLv3, a compact notation system. Keep the{' '}
          <a href="/words#symbols" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>
            words and symbols key
          </a>{' '}nearby, or return to the <a href="/docs" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>practical guides</a>.
        </div>

        <pre className="csl-spec" style={{ marginTop: '2rem' }}>
          {renderCslWithGlyphs(spec.body)}
        </pre>

        <nav
          style={{
            marginTop: '2.5rem',
            display: 'flex',
            justifyContent: 'space-between',
            gap: '1rem',
            flexWrap: 'wrap',
          }}
        >
          {prevSlug !== null ? (
            <a href={`/docs/${prevSlug}`} style={{ fontSize: '0.85rem', color: '#7dd3fc' }}>← {prevSlug}</a>
          ) : <span />}
          {nextSlug !== null ? (
            <a href={`/docs/${nextSlug}`} style={{ fontSize: '0.85rem', color: '#7dd3fc' }}>{nextSlug} →</a>
          ) : <span />}
        </nav>

        <footer
          style={{
            marginTop: '4rem',
            paddingTop: '2.5rem',
            borderTop: '1px solid #1f1f2a',
            color: '#a8a2b3',
            fontSize: '0.78rem',
          }}
        >
          <p style={{ margin: 0 }}>Technical source material. Return to the documentation index for plain-language guides.</p>
        </footer>
      </main>
    </>
  );
};

// Lightweight CSL3-glyph aware highlighter. Tokenizes the body into spans
// so glyph-prefixes pop visually. Returns React-renderable array.
function renderCslWithGlyphs(body: string): JSX.Element[] {
  const out: JSX.Element[] = [];
  const lines = body.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i] as string;
    const tokens: JSX.Element[] = [];
    // Tokenize the line by glyphs/modals/evidence
    const re = /(§|I>|W!|R!|N!|M\?|Q\?|✓|◐|○|✗|⊘|△|▽|‼|⟨|⟩|⌈|⌉|⟦|⟧|«|»|⟪|⟫)/g;
    let lastIdx = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(line)) !== null) {
      if (m.index > lastIdx) tokens.push(<span key={`t-${i}-${lastIdx}`}>{line.slice(lastIdx, m.index)}</span>);
      const tok = m[1] as string;
      const kind =
        tok === '§' ? 'heading' :
        ['I>', 'W!', 'R!', 'N!', 'M?', 'Q?'].includes(tok) ? 'modal' :
        ['✓', '◐', '○', '✗', '⊘', '△', '▽', '‼'].includes(tok) ? 'evidence' :
        'glyph';
      tokens.push(<span key={`g-${i}-${m.index}`} className={kind}>{tok}</span>);
      lastIdx = m.index + tok.length;
    }
    if (lastIdx < line.length) tokens.push(<span key={`r-${i}`}>{line.slice(lastIdx)}</span>);
    out.push(<span key={`l-${i}`}>{tokens}{i < lines.length - 1 ? '\n' : ''}</span>);
  }
  return out;
}

export const getStaticPaths: GetStaticPaths = () => {
  return {
    paths: SPECS.map((s) => ({ params: { slug: s.slug } })),
    fallback: false,
  };
};

export const getStaticProps: GetStaticProps<DocsPageProps> = async (ctx) => {
  const slug = typeof ctx.params?.['slug'] === 'string' ? (ctx.params['slug'] as string) : '';
  const spec = findSpec(slug);
  if (spec === null) return { notFound: true };
  const idx = SPECS.findIndex((s) => s.slug === slug);
  const prev = idx > 0 ? SPECS[idx - 1] : null;
  const next = idx < SPECS.length - 1 ? SPECS[idx + 1] : null;
  return {
    props: {
      spec,
      prevSlug: prev !== null ? (prev as SpecEntry).slug : null,
      nextSlug: next !== null ? (next as SpecEntry).slug : null,
    },
  };
};

export default DocsPage;
