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
      title="Documentation · Apocky"
      description="Plain-language help for Labyrinth of Apocalypse and CSSL, followed by optional technical references."
    >
      <h1 className="docs-h1">Documentation</h1>
      <p className="docs-blurb">
        Start with ordinary explanations. Open the technical specifications only when you need that level of detail.
      </p>

      <p className="docs-p">
        These {DOC_PAGES.length} guides cover Labyrinth of Apocalypse and CSSL. Pick a topic
        below. If a page needs a specialized word or symbol, it should explain it before relying
        on it. The shared <a href="/words" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>
          words and symbols page
        </a> is always available.
      </p>

      <p className="docs-p">
        Pages are labeled <span style={{ color: '#34d399' }}>Available now</span>,{' '}
        <span style={{ color: '#fbbf24' }}>In progress</span>,{' '}
        <span style={{ color: '#9aa0a6' }}>Coming soon</span>, or{' '}
        <span style={{ color: '#f472b6' }}>Subject to change</span>.
      </p>

      {sections.map((s) => (
        <section key={s.name} style={{ marginTop: '2rem' }}>
          <h2 className="docs-h2">{s.name}</h2>
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

      <section className="docs-section">
        <h2 className="docs-h2">Technical specifications</h2>
        <p className="docs-p">
          These {specEntries.length} source documents describe architecture and plans in compact CSLv3
          notation. They are reference material, not the starting point. Read the{' '}
          <a href="/words#symbols">
            symbol key
          </a>{' '}
          first. A symbol in these documents is technical notation, not decoration.
        </p>
        <div className="docs-spec-list">
          {specEntries.map((e) => (
            <a key={e.slug} href={`/docs/${e.slug}`} className="docs-spec-link">
              <div className="docs-spec-slug">{e.slug}</div>
              <div className="docs-spec-title">{e.title}</div>
            </a>
          ))}
        </div>
      </section>

      <footer className="docs-footer">
        <p style={{ margin: 0 }}>Plain language first. Technical detail when it helps.</p>
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
