import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

import styles from '../styles/NeuralPages.module.css';

const Start: NextPage = () => (
  <>
    <Head>
      <title>Start here · Apocky</title>
      <meta
        name="description"
        content="Choose a path through Apocky: get a Chaos Tarot reading, explore the Atlas, read the archive, join the Clearing, or support the work."
      />
      <meta property="og:title" content="Start here · Apocky" />
      <meta property="og:description" content="One clear launch point for Apocky’s interconnected worlds, tools, and ideas." />
      <meta property="og:url" content="https://www.apocky.com/start" />
      <link rel="canonical" href="https://www.apocky.com/start" />
    </Head>

    <main className={styles.page}>
      <div className={styles.wrap}>
        <p className={styles.eyebrow}>Start here</p>
        <h1 className={styles.title}>What would you <em>like to do?</em></h1>
        <p className={styles.lead}>
          Make something, find a meaning, or lose yourself in a story. Choose what brings you here.
        </p>

        <section className={styles.section} aria-labelledby="choose-title">
          <div className={styles.sectionHead}>
            <h2 id="choose-title">What brought you here?</h2>
            <p>A story to read, a tool to try, or a thought to keep.</p>
          </div>
          <div className={styles.grid2}>
            <article className={`${styles.card} ${styles.tierFeatured}`}>
              <span className={styles.tag}>I want a story</span>
              <h3>Codex Apockalypsis</h3>
              <p>Dark fantasy, dark comedy, and the Good Book. Begin with creation, then explore the world and its sources.</p>
              <Link className={styles.cardLink} href="/codex-apockalypsis/library/novel-volume-01-01-before-anyone-asked">Begin the story →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>I want a meaning</span>
              <h3>Find the word for it.</h3>
              <p>Look up a word or symbol, see an example, and use it in your own writing.</p>
              <Link className={styles.cardLink} href="/words">Find a definition →</Link>
            </article>
            <article className={`${styles.card} ${styles.tierFeatured}`}>
              <span className={styles.tag}>I want an experience</span>
              <h3>Read the pattern in front of you.</h3>
              <p>Enter Chaos Tarot for interactive readings, multiple divination systems, learning tools, journaling, and progression.</p>
              <a className={styles.cardLink} href="https://chaos-tarot.com/free-reading?source=apocky-start" target="_blank" rel="noopener noreferrer">
                Begin a free reading <span aria-hidden="true">↗</span>
              </a>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>I want the big picture</span>
              <h3>See how the pieces connect.</h3>
              <p>Use the Constellation Atlas as a visual map, multidimensional index, and dictionary.</p>
              <Link className={styles.cardLink} href="/atlas">Open the Atlas →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>I want the ideas</span>
              <h3>Find a thought to keep.</h3>
              <p>Explore the Akashic Records, the Omnoid Singularity cosmology, and the words and symbols that hold the system together.</p>
              <Link className={styles.cardLink} href="/akashic-records">Read essays →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>I want to participate</span>
              <h3>Join a conversation.</h3>
              <p>Read The Clearing, join a public conversation, or choose a short discovery quest.</p>
              <div className={styles.actions}>
                <Link className={styles.cardLink} href="/clearing">Enter the Clearing →</Link>
                <Link className={styles.cardLink} href="/quests">Choose a quest →</Link>
              </div>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>I want to make something</span>
              <h3>Make a symbol of your own.</h3>
              <p>Compose a symbolic phrase, turn it into a sigil, and save what you want to keep.</p>
              <div className={styles.actions}>
                <Link className={styles.cardLink} href="/sigils">Make a sigil →</Link>
                <Link className={styles.cardLink} href="/spellcraft">Open Spellcraft →</Link>
              </div>
            </article>
          </div>
        </section>

        <details className={styles.section}>
          <summary>How these paths connect</summary>
          <div className={styles.sectionHead}>
            <h2 id="route-title">A route, not a funnel-shaped dead end.</h2>
            <p>
              The recommended path starts with value, then context, then participation, then support.
              Leave at any point or follow the connections deeper.
            </p>
          </div>
          <div className={styles.diagram}>
            <svg viewBox="0 0 760 360" role="img" aria-labelledby="route-map-title route-map-desc">
              <title id="route-map-title">Recommended route through Apocky</title>
              <desc id="route-map-desc">A path from Chaos Tarot through the Atlas and public archive to community and optional membership.</desc>
              <defs>
                <linearGradient id="route-line" x1="0" x2="1">
                  <stop offset="0" stopColor="#78e7ff" />
                  <stop offset="0.55" stopColor="#7c8fff" />
                  <stop offset="1" stopColor="#c28cff" />
                </linearGradient>
                <filter id="route-glow"><feGaussianBlur stdDeviation="3" result="b" /><feMerge><feMergeNode in="b" /><feMergeNode in="SourceGraphic" /></feMerge></filter>
              </defs>
              <path d="M84 180 C180 78 274 78 370 180 S560 282 676 180" fill="none" stroke="url(#route-line)" strokeWidth="3" opacity=".6" />
              {[
                { x: 84, y: 180, n: '01', title: 'EXPERIENCE' },
                { x: 225, y: 107, n: '02', title: 'ORIENT' },
                { x: 370, y: 180, n: '03', title: 'UNDERSTAND' },
                { x: 520, y: 253, n: '04', title: 'PARTICIPATE' },
                { x: 676, y: 180, n: '05', title: 'SUSTAIN' },
              ].map((node) => (
                <g key={node.n} filter="url(#route-glow)">
                  <circle cx={node.x} cy={node.y} r="43" fill="#06081d" stroke="url(#route-line)" />
                  <text x={node.x} y={node.y - 5} fill="#78e7ff" fontFamily="ui-monospace, monospace" fontSize="11" textAnchor="middle">{node.n}</text>
                  <text x={node.x} y={node.y + 13} fill="#f5f3ff" fontFamily="ui-monospace, monospace" fontSize="9" fontWeight="700" textAnchor="middle">{node.title}</text>
                </g>
              ))}
            </svg>
          </div>
        </details>

        <section className={styles.section} aria-labelledby="next-title">
          <div className={styles.sectionHead}>
            <h2 id="next-title">Ready to move?</h2>
            <p>Choose the tool, story, or idea that interests you. You can always come back for another.</p>
          </div>
          <div className={styles.actions}>
            <a className={styles.primary} href="https://chaos-tarot.com/free-reading?source=apocky-start-end" target="_blank" rel="noopener noreferrer">
              Draw your first cards <span aria-hidden="true">↗</span>
            </a>
            <Link className={styles.secondary} href="/atlas">Explore the constellation →</Link>
            <Link className={styles.secondary} href="/spellcraft">Craft a symbolic working →</Link>
            <Link className={styles.secondary} href="/membership">Help power the work →</Link>
          </div>
        </section>
      </div>
    </main>
  </>
);

export default Start;
