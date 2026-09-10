import type { NextPage } from 'next';
import Head from 'next/head';

const InfinityEngine: NextPage = () => (
  <>
    <Head>
      <title>Infinity Engine research · Apocky</title>
      <meta
        name="description"
        content="A plain-language explanation of the Infinity Engine name, the shared architecture research behind it, and its current status."
      />
      <link rel="canonical" href="https://www.apocky.com/infinity-engine" />
      <style>{`
        .ie-page {
          width: min(900px, calc(100% - 36px));
          margin: 0 auto;
          padding: clamp(36px, 5vw, 56px) 0 clamp(48px, 6vw, 72px);
        }
        .ie-page h1 { margin: 0; font-size: var(--apx-fs-h1); line-height: 1.05; letter-spacing: -.035em; text-wrap: balance; }
        .ie-page a:not([class]) { display: inline-flex; align-items: center; min-height: 40px; }
        .ie-lead { max-width: 660px; margin: 16px 0 0; color: var(--apx-copy); font-size: 1rem; line-height: 1.6; }
        .ie-note { margin: 34px 0; border: 1px solid rgba(255,196,125,.32); border-radius: 16px; background: rgba(255,196,125,.06); padding: 20px; color: #eed4b3; line-height: 1.65; }
        .ie-page section { margin-top: 54px; }
        .ie-page h2 { margin: 0 0 12px; color: var(--apx-mint-bright); font-size: 1.3rem; }
        .ie-page p, .ie-page li { color: var(--apx-copy); line-height: 1.75; }
        .ie-page ul { padding-left: 1.3rem; }
        .ie-page li { margin: 8px 0; }
        .ie-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; margin-top: 22px; }
        .ie-card { border: 1px solid var(--apx-line); border-radius: 16px; background: var(--apx-panel); padding: 22px; }
        .ie-card h3 { margin: 0; font-size: 1rem; }
        .ie-card p { margin: 8px 0 0; font-size: .88rem; }
        .ie-card a { color: var(--apx-sky); }
        @media (max-width: 650px) { .ie-grid { grid-template-columns: 1fr; } }
      `}</style>
    </Head>
    <main className="ie-page">
      <p className="apx-eyebrow">An idea in development</p>
      <h1>Infinity Engine</h1>
      <p className="ie-lead">
        What if a game, a simulation, and a creative tool could share useful building blocks?
        Infinity Engine explores that question. The research is unfinished; the links below lead to things
        you can try or read today.
      </p>

      <details className="ie-note">
        <summary>What the name means, and what is available</summary>
        <p>“Infinity Engine” is a project name for shared code and ideas. It is not a separate person,
          a claim of consciousness, or one finished program that is always running.</p>
        <p>
        Older copy blended plans, metaphors, and product claims. This page now separates them. The current
        evidence is a mixture of source code, tests, experiments, and architecture documents; each must be
        evaluated on its own.</p>
      </details>

      <section>
        <h2>What the research is trying to do</h2>
        <p>
          The practical goal is to avoid rebuilding the same low-level work for every project. Shared libraries
          could handle simulation state, permissions, compiling CSSL code, rendering, and carefully bounded
          experiments in learning. “Shared” does not mean every project must use every part.
        </p>
      </section>

      <details>
        <summary>Research, tests, and releases</summary>
        <ul>
          <li><strong>Source code:</strong> files and libraries that can be inspected and tested.</li>
          <li><strong>Tests:</strong> checks of particular behavior under stated conditions.</li>
          <li><strong>Experiments:</strong> work used to learn, without promising a public feature.</li>
          <li><strong>Specifications:</strong> technical plans written in CSLv3 notation.</li>
          <li><strong>Releases:</strong> software a visitor can actually download or use.</li>
        </ul>
        <p>
          One form does not automatically prove another. A specification is not a deployment, and a source
          module is not proof that a complete product is available.
        </p>
      </details>

      <section>
        <h2>Projects connected to the research</h2>
        <div className="ie-grid">
          <div className="ie-card">
            <h3>Labyrinth of Apocalypse</h3>
            <p>An early Windows game and engine test. <a href="/download">View the current download.</a></p>
          </div>
          <div className="ie-card">
            <h3>CSSL</h3>
            <p>A programming language under development. <a href="/docs/cssl-language">Read the local CSSL guide.</a></p>
          </div>
          <div className="ie-card">
            <h3>CSLv3</h3>
            <p>A compact notation used in technical specifications. <a href="/words#symbols">Read the local notation key.</a></p>
          </div>
          <div className="ie-card">
            <h3>Planned network research</h3>
            <p>Mycelium and related names describe proposed designs, not a public service. <a href="/docs/mycelium">Read the status.</a></p>
          </div>
        </div>
      </section>

      <section>
        <h2>Technical terms</h2>
        <p>
          The documentation introduces specialized names only after an ordinary explanation. See{' '}
          <a href="/words" style={{ color: 'var(--apx-sky)' }}>Words and symbols</a> or the{' '}
          <a href="/docs/substrate" style={{ color: 'var(--apx-sky)' }}>technical foundations guide</a>.
        </p>
      </section>
    </main>
  </>
);

export default InfinityEngine;
