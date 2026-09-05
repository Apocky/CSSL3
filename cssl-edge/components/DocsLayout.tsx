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
  const navigation = <>

          <a href="/" className="docs-back">
            ← apocky.com
          </a>
          <a
            href="/docs"
            className={`docs-nav-link ${activeSlug === '' ? 'is-active' : ''}`}
            style={{ fontWeight: 600 }}
          >
            Documentation
          </a>
          <a href="/words" className="docs-nav-link">
            Words and symbols
          </a>
          {sections.map((s) => (
            <div key={s.name}>
              <div className="docs-section-title">{s.name}</div>
              {s.pages.map((p) => {
                const badge = statusBadge(p.status);
                return (
                  <a
                    key={p.slug}
                    href={`/docs/${p.slug}`}
                    className={`docs-nav-link ${activeSlug === p.slug ? 'is-active' : ''}`}
                  >
                    {p.title}
                    <span className="docs-nav-status" style={{ color: badge.color }}>
                      {badge.label}
                    </span>
                  </a>
                );
              })}
            </div>
          ))}
          <div className="docs-sidebar-foot">
            {DOC_PAGES.length} pages. Technical terms are explained before use.
          </div>

  </>;
  return (
    <>
      <Head>
        <title>{title}</title>
        <meta name="description" content={description} />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta name="theme-color" content="#000000" />
        <meta name="author" content="Apocky" />
        <link rel="canonical" href={`https://www.apocky.com${canonicalPath}`} />
        <meta property="og:title" content={title} />
        <meta property="og:description" content={description} />
        <meta property="og:type" content="article" />
        <meta property="og:site_name" content="Apocky Docs" />
        <meta property="og:url" content={`https://www.apocky.com${canonicalPath}`} />
        <meta name="twitter:card" content="summary" />
        <meta name="twitter:title" content={title} />
        <meta name="twitter:description" content={description} />
        <style>{`
          /* Scoped to the docs surface: the shared shell (nav/footer) keeps the site system. */
          .docs-shell {
            display: grid;
            grid-template-columns: 220px minmax(0, 1fr);
            gap: 2rem;
            max-width: 1140px;
            margin: 0 auto;
            padding: 1.75rem 1.5rem 3.5rem;
            color: var(--apx-copy, #d8dcf4);
            font-family: var(--apx-sans, Inter, ui-sans-serif, system-ui, sans-serif);
            line-height: 1.6;
          }
          .docs-shell a { color: inherit; text-decoration: none; }
          @media (max-width: 900px) {
            .docs-shell { grid-template-columns: 1fr; gap: 1rem; padding: 1.25rem 1rem 3rem; }
            .docs-sidebar { position: static; max-height: none; }
          }
          .docs-sidebar {
            position: sticky;
            top: 80px;
            align-self: start;
            max-height: calc(100vh - 96px);
            overflow-y: auto;
            font-size: 0.85rem;
            color: var(--apx-copy, #d8dcf4);
            scrollbar-color: rgba(124, 136, 255, 0.3) transparent;
          }
          .docs-back {
            display: inline-flex;
            align-items: center;
            min-height: 40px;
            margin-bottom: 0.5rem;
            color: var(--apx-muted, #9ca6cc);
            font: 600 var(--apx-fs-micro, 0.72rem)/1 var(--apx-mono, ui-monospace, monospace);
            letter-spacing: 0.06em;
          }
          .docs-section-title {
            color: var(--apx-dim, #7580aa);
            font: 750 var(--apx-fs-micro, 0.72rem)/1.2 var(--apx-mono, ui-monospace, monospace);
            letter-spacing: 0.12em;
            text-transform: uppercase;
            margin: 1rem 0 0.25rem;
          }
          .docs-section-title:first-child { margin-top: 0; }
          .docs-nav-link {
            display: flex;
            flex-direction: column;
            justify-content: center;
            min-height: 40px;
            padding: 0.3rem 0.6rem;
            margin: 0 -0.6rem;
            border-left: 2px solid transparent;
            border-radius: 8px;
            color: var(--apx-muted, #9ca6cc);
            line-height: 1.3;
          }
          .docs-nav-link:hover { background: rgba(79, 124, 255, 0.09); color: var(--apx-ink, #f5f3ff); }
          .docs-nav-link.is-active { background: rgba(109, 93, 252, 0.14); color: var(--apx-ink, #f5f3ff); border-left-color: var(--apx-violet, #b998ff); border-radius: 0 8px 8px 0; }
          .docs-nav-status { display: block; margin-top: 0.05rem; font: 600 0.68rem/1.2 var(--apx-mono, ui-monospace, monospace); }
          .docs-sidebar-foot { margin-top: 1.25rem; color: var(--apx-dim, #7580aa); font-size: 0.72rem; line-height: 1.5; }
          .docs-main {
            min-width: 0;
            line-height: 1.65;
            font-size: 0.95rem;
          }
          .docs-h1 {
            margin: 0 0 0.4rem;
            font-family: var(--apx-display, Georgia, serif);
            font-size: var(--apx-fs-h1, clamp(2rem, 3.6vw, 3.1rem));
            font-weight: 600;
            letter-spacing: -0.03em;
            line-height: 1.05;
            color: transparent;
            background: linear-gradient(112deg, var(--apx-ink, #f5f3ff) 0%, var(--apx-violet, #b998ff) 55%, var(--apx-sky, #7ddcff) 100%);
            -webkit-background-clip: text;
            background-clip: text;
          }
          .docs-blurb { color: var(--apx-muted, #9ca6cc); font-size: 0.95rem; margin: 0.3rem 0 1.4rem; max-width: 62ch; }
          .docs-h2 { margin: 1.8rem 0 0.5rem; color: var(--apx-violet, #b998ff); font-size: 1.2rem; font-weight: 650; letter-spacing: -0.01em; }
          .docs-h3 { margin: 1.3rem 0 0.35rem; color: var(--apx-sky, #7ddcff); font-size: 1.02rem; font-weight: 650; }
          .docs-p { margin: 0.6rem 0; color: var(--apx-copy, #d8dcf4); }
          .docs-ul, .docs-ol { margin: 0.6rem 0; padding-left: 1.3rem; color: var(--apx-copy, #d8dcf4); }
          .docs-ul li, .docs-ol li { margin: 0.25rem 0; }
          .docs-p a, .docs-blurb a, .docs-ul a, .docs-ol a, .docs-table a { color: var(--apx-sky, #7ddcff); text-decoration: underline; text-underline-offset: 0.18em; }
          .docs-ic { background: rgba(125, 220, 255, 0.1); padding: 0.08rem 0.32rem; border-radius: 4px; color: var(--apx-sky, #7ddcff); font: 0.86em var(--apx-mono, ui-monospace, monospace); }
          .docs-kbd { display: inline-block; background: rgba(8, 10, 27, 0.9); border: 1px solid var(--apx-line, rgba(169, 181, 255, 0.17)); border-bottom-width: 2px; padding: 0.05rem 0.45rem; border-radius: 5px; font: 0.8em var(--apx-mono, ui-monospace, monospace); color: var(--apx-ink, #f5f3ff); margin: 0 0.1em; }
          .docs-table { border-collapse: collapse; width: 100%; margin: 0.9rem 0; font-size: 0.88rem; display: block; overflow-x: auto; }
          .docs-table th, .docs-table td { border-bottom: 1px solid var(--apx-line, rgba(169, 181, 255, 0.17)); padding: 0.5rem 0.65rem; text-align: left; vertical-align: top; }
          .docs-table th { color: var(--apx-violet, #b998ff); font: 750 0.72rem/1.3 var(--apx-mono, ui-monospace, monospace); letter-spacing: 0.06em; text-transform: uppercase; }
          .docs-status-badge { display: inline-block; padding: 0.1rem 0.5rem; border-radius: 999px; font: 600 0.7rem/1.5 var(--apx-mono, ui-monospace, monospace); letter-spacing: 0.04em; }
          .docs-section { margin-top: 2rem; }
          .docs-spec-list { display: grid; grid-template-columns: minmax(0, 1fr); gap: 0.5rem; margin-top: 0.9rem; }
          .docs-spec-link { display: block; min-width: 0; padding: 0.6rem 0.85rem; border: 1px solid var(--apx-line, rgba(169, 181, 255, 0.17)); border-radius: 10px; background: rgba(8, 10, 27, 0.6); overflow-wrap: anywhere; }
          .docs-spec-link:hover { border-color: var(--apx-line-strong, rgba(124, 143, 255, 0.58)); }
          .docs-spec-slug { color: var(--apx-dim, #7580aa); font: 600 0.7rem/1.3 var(--apx-mono, ui-monospace, monospace); letter-spacing: 0.08em; }
          .docs-spec-title { margin-top: 0.15rem; color: var(--apx-copy, #d8dcf4); font-size: 0.9rem; line-height: 1.4; }
          .docs-mobile-navigation { display:none; }
          @media(max-width:900px) { .docs-sidebar { display:none; } .docs-mobile-navigation { display:block; border-bottom:1px solid var(--apx-line); padding-bottom:12px; } .docs-mobile-navigation summary { min-height:44px; display:flex; align-items:center; cursor:pointer; color:var(--apx-violet); } .docs-mobile-navigation summary::after{content:" +";margin-left:auto} .docs-mobile-navigation[open] summary::after{content:" −"} .docs-mobile-navigation nav{padding:12px} }
          .docs-footer { margin-top: 2.5rem; padding-top: 1.25rem; border-top: 1px solid var(--apx-line, rgba(169, 181, 255, 0.17)); color: var(--apx-dim, #7580aa); font-size: 0.8rem; }
        `}</style>
      </Head>
      <main className="docs-shell">
        <details className="docs-mobile-navigation"><summary>Browse guides</summary><nav aria-label="Choose a guide">{navigation}</nav></details>
        <aside className="docs-sidebar" aria-label="Docs navigation">{navigation}</aside>
        <article className="docs-main">{children}</article>
      </main>
    </>
  );
};

export default DocsLayout;
