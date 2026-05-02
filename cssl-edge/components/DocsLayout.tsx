// cssl-edge · components/DocsLayout.tsx
// Shared layout: <Head> meta + sidebar + main column + footer.
// Phone-first responsive · sidebar collapses to top-of-page nav <900px.

import Head from 'next/head';
import type { ReactNode } from 'react';
import { DOC_PAGES, getDocSections, statusBadge } from '@/lib/docs-content';

interface DocsLayoutProps {
  /** Slug of the active page (for sidebar highlight). '' on the index page. */
  activeSlug: string;
  /** <title> tag value. */
  title: string;
  /** <meta description> content. */
  description: string;
  /** Page body. */
  children: ReactNode;
}

const DocsLayout = ({ activeSlug, title, description, children }: DocsLayoutProps) => {
  const sections = getDocSections();
  const canonicalPath = activeSlug === '' ? '/docs' : `/docs/${activeSlug}`;
  return (
    <>
      <Head>
        <title>{title}</title>
        <meta name="description" content={description} />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#0a0a0f" />
        <meta name="author" content="Apocky" />
        <link rel="canonical" href={`https://apocky.com${canonicalPath}`} />
        <meta property="og:title" content={title} />
        <meta property="og:description" content={description} />
        <meta property="og:type" content="article" />
        <meta property="og:site_name" content="Apocky Docs" />
        <meta property="og:url" content={`https://apocky.com${canonicalPath}`} />
        <meta name="twitter:card" content="summary" />
        <meta name="twitter:title" content={title} />
        <meta name="twitter:description" content={description} />
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
          .docs-shell {
            display: grid;
            grid-template-columns: 240px minmax(0, 1fr);
            gap: 2.5rem;
            max-width: 1140px;
            margin: 0 auto;
            padding: 3rem 1.5rem 6rem;
          }
          @media (max-width: 900px) {
            .docs-shell { grid-template-columns: 1fr; gap: 1.5rem; padding: 2rem 1rem 4rem; }
            .docs-sidebar { position: static; max-height: none; }
          }
          .docs-sidebar {
            position: sticky;
            top: 1.5rem;
            align-self: start;
            max-height: calc(100vh - 3rem);
            overflow-y: auto;
            font-size: 0.83rem;
            color: #cdd6e4;
          }
          .docs-section-title {
            color: #7a7a8c;
            font-size: 0.7rem;
            letter-spacing: 0.12em;
            text-transform: uppercase;
            margin: 1.4rem 0 0.4rem;
          }
          .docs-section-title:first-child { margin-top: 0; }
          .docs-nav-link {
            display: block;
            padding: 0.32rem 0.6rem;
            margin: 0.1rem -0.6rem;
            border-radius: 4px;
            color: #a8a8b8;
          }
          .docs-nav-link:hover { background: rgba(124, 211, 252, 0.06); color: #e6e6f0; }
          .docs-nav-link.is-active { background: rgba(192, 132, 252, 0.12); color: #e6e6f0; border-left: 2px solid #c084fc; padding-left: calc(0.6rem - 2px); }
          .docs-main {
            min-width: 0;
            line-height: 1.7;
            font-size: 0.95rem;
          }
          .docs-h1 {
            font-size: clamp(1.75rem, 4vw, 2.4rem);
            margin: 0 0 0.5rem;
            font-weight: 700;
            letter-spacing: -0.02em;
            background-image: linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            background-clip: text;
          }
          .docs-blurb { color: #a8a8b8; font-size: 0.95rem; margin: 0.4rem 0 2rem; }
          .docs-h2 { font-size: 1.25rem; margin: 2.2rem 0 0.6rem; color: #c084fc; font-weight: 600; }
          .docs-h3 { font-size: 1.05rem; margin: 1.6rem 0 0.4rem; color: #7dd3fc; font-weight: 600; }
          .docs-p { margin: 0.7rem 0; color: #cdd6e4; }
          .docs-ul, .docs-ol { margin: 0.7rem 0; padding-left: 1.3rem; color: #cdd6e4; }
          .docs-ul li, .docs-ol li { margin: 0.25rem 0; }
          .docs-ic { background: rgba(124, 211, 252, 0.08); padding: 0.1rem 0.3rem; border-radius: 3px; color: #7dd3fc; font-size: 0.88em; }
          .docs-kbd { display: inline-block; background: #1a1a26; border: 1px solid #2a2a3a; border-bottom-width: 2px; padding: 0.05rem 0.45rem; border-radius: 4px; font-size: 0.82em; color: #e6e6f0; margin: 0 0.1em; }
          .docs-table { border-collapse: collapse; width: 100%; margin: 1rem 0; font-size: 0.85rem; }
          .docs-table th, .docs-table td { border-bottom: 1px solid #1f1f2a; padding: 0.55rem 0.7rem; text-align: left; vertical-align: top; }
          .docs-table th { color: #c084fc; font-weight: 600; font-size: 0.78rem; letter-spacing: 0.06em; text-transform: uppercase; }
          .docs-status-badge { display: inline-block; padding: 0.1rem 0.45rem; border-radius: 999px; font-size: 0.7rem; letter-spacing: 0.04em; }
          .docs-footer { margin-top: 4rem; padding-top: 2.5rem; border-top: 1px solid #1f1f2a; color: #5a5a6a; font-size: 0.78rem; }
        `}</style>
      </Head>
      <main className="docs-shell">
        <aside className="docs-sidebar" aria-label="Docs navigation">
          <a href="/" style={{ fontSize: '0.78rem', color: '#7a7a8c', display: 'inline-block', marginBottom: '1.2rem' }}>
            ← apocky.com
          </a>
          <a
            href="/docs"
            className={`docs-nav-link ${activeSlug === '' ? 'is-active' : ''}`}
            style={{ fontWeight: 600, marginBottom: '0.4rem' }}
          >
            Docs · index
          </a>
          {sections.map((s) => (
            <div key={s.name}>
              <div className="docs-section-title">§ {s.name}</div>
              {s.pages.map((p) => {
                const badge = statusBadge(p.status);
                return (
                  <a
                    key={p.slug}
                    href={`/docs/${p.slug}`}
                    className={`docs-nav-link ${activeSlug === p.slug ? 'is-active' : ''}`}
                  >
                    <span style={{ color: badge.color, marginRight: '0.4rem' }}>{badge.glyph}</span>
                    {p.title}
                  </a>
                );
              })}
            </div>
          ))}
          <div style={{ marginTop: '2rem', fontSize: '0.7rem', color: '#5a5a6a' }}>
            {DOC_PAGES.length} pages · sovereign-by-default
          </div>
        </aside>
        <article className="docs-main">{children}</article>
      </main>
    </>
  );
};

export default DocsLayout;
