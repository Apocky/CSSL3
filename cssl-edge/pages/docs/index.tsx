// apocky.com/docs · table-of-contents landing page
// Replaces prior auto-snapshot listing · now drives /docs/<slug> pages.
// Specs auto-snapshot still surfaced via inline section linking to specs/grand-vision/*.csl.

import type { NextPage, GetStaticProps } from 'next';
import DocsLayout from '@/components/DocsLayout';
import { DOC_PAGES, getDocSections, statusBadge } from '@/lib/docs-content';
import { SPECS } from '@/lib/specs-snapshot';

interface DocsIndexProps {
  specEntries: ReadonlyArray<{ slug: string; title: string }>;
}

const DocsIndex: NextPage<DocsIndexProps> = ({ specEntries }) => {
  const sections = getDocSections();
  return (
    <DocsLayout
      activeSlug=""
      title="Docs · Apocky"
      description="Documentation for Labyrinth of Apocalypse · the CSSL language · the Substrate · sovereignty model · mycelium network · keyboard reference · troubleshooting."
    >
      <h1 className="docs-h1">Apocky Docs</h1>
      <p className="docs-blurb">
        § How to use the apps · what the language does · how the substrate works.
        Density = sovereignty · {DOC_PAGES.length} pages.
      </p>

      <p className="docs-p">
        These docs cover the substrate-native systems shipped under apocky.com today —
        primarily Labyrinth of Apocalypse (the first tenant), the CSSL language used to
        author it, and the Substrate primitives all Apocky projects share. Pick a topic
        from the sidebar or the sections below.
      </p>

      <p className="docs-p">
        Status legend ·{' '}
        <span style={{ color: '#34d399' }}>✓ available now</span> ·{' '}
        <span style={{ color: '#fbbf24' }}>◐ in progress</span> ·{' '}
        <span style={{ color: '#9aa0a6' }}>○ coming soon</span> ·{' '}
        <span style={{ color: '#f472b6' }}>‼ subject to change</span>.
      </p>

      {sections.map((s) => (
        <section key={s.name} style={{ marginTop: '2rem' }}>
          <h2 className="docs-h2">§ {s.name}</h2>
          <div style={{ display: 'grid', gap: '0.6rem' }}>
            {s.pages.map((p) => {
              const badge = statusBadge(p.status);
              return (
                <a
                  key={p.slug}
                  href={`/docs/${p.slug}`}
                  style={{
                    display: 'block',
                    padding: '0.8rem 1rem',
                    background: 'rgba(20, 20, 30, 0.5)',
                    border: '1px solid #1f1f2a',
                    borderRadius: 6,
                  }}
                >
                  <div style={{ display: 'flex', alignItems: 'baseline', gap: '0.55rem', flexWrap: 'wrap' }}>
                    <span style={{ color: badge.color, fontWeight: 700 }}>{badge.glyph}</span>
                    <span style={{ fontWeight: 600, color: '#e6e6f0' }}>{p.title}</span>
                    <span
                      className="docs-status-badge"
                      style={{ background: badge.color + '22', color: badge.color, border: `1px solid ${badge.color}33` }}
                    >
                      {badge.label}
                    </span>
                  </div>
                  <div style={{ fontSize: '0.85rem', color: '#a8a8b8', marginTop: '0.3rem' }}>{p.blurb}</div>
                </a>
              );
            })}
          </div>
        </section>
      ))}

      <section style={{ marginTop: '3rem' }}>
        <h2 className="docs-h2">§ Grand-Vision Specs</h2>
        <p className="docs-p">
          The CSL3-glyph-native architecture specs that drive every Apocky project. Auto-snapshotted from{' '}
          <code className="docs-ic">specs/grand-vision/*.csl</code> at build-time. {specEntries.length} documents.
        </p>
        <div style={{ display: 'grid', gap: '0.55rem', marginTop: '1rem' }}>
          {specEntries.map((e) => (
            <a
              key={e.slug}
              href={`/docs/${e.slug}`}
              style={{
                display: 'block',
                padding: '0.65rem 0.9rem',
                background: 'rgba(20, 20, 30, 0.4)',
                border: '1px solid #1f1f2a',
                borderRadius: 4,
              }}
            >
              <div style={{ fontSize: '0.7rem', color: '#7a7a8c', letterSpacing: '0.1em' }}>{e.slug}</div>
              <div style={{ fontSize: '0.9rem', color: '#cdd6e4', marginTop: '0.2rem' }}>{e.title}</div>
            </a>
          ))}
        </div>
      </section>

      <footer className="docs-footer">
        <p style={{ margin: 0 }}>§ ¬ harm in the making · sovereignty preserved · t∞</p>
        <p style={{ margin: '0.4rem 0 0' }}>
          Source: <code className="docs-ic">cssl-edge/lib/docs-content.ts</code> · static-site-generated.
        </p>
      </footer>
    </DocsLayout>
  );
};

export const getStaticProps: GetStaticProps<DocsIndexProps> = async () => {
  return {
    props: {
      specEntries: SPECS.map((s) => ({ slug: s.slug, title: s.title })),
    },
  };
};

export default DocsIndex;
