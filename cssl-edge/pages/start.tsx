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
        <p className={styles.eyebrow}>Orientation protocol</p>
        <h1 className={styles.title}>Pick the signal you want. <em>The system will meet you there.</em></h1>
        <p className={styles.lead}>
          You do not need to understand the whole constellation before entering it. Choose what you want
          right now; every path offers a route onward when you are ready.
        </p>

        <section className={styles.section} aria-labelledby="choose-title">
          <div className={styles.sectionHead}>
            <h2 id="choose-title">What brought you here?</h2>
            <p>Four useful beginnings. No quiz gate, no email wall, no invented recommendation engine.</p>
          </div>
          <div className={styles.grid2}>
            <article className={`${styles.card} ${styles.tierFeatured}`}>
              <span className={styles.tag}>I want an experience</span>
              <h3>Read the pattern in front of you.</h3>
              <p>Enter Chaos Tarot for interactive readings, multiple divination systems, learning tools, journaling, and progression.</p>
              <a className={styles.cardLink} href="https://chaos-tarot.com/free-reading" target="_blank" rel="noopener noreferrer">
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
              <h3>Read the source material.</h3>
              <p>Explore the Akashic Records, the Omnoid Singularity cosmology, and the words and symbols that hold the system together.</p>
              <Link className={styles.cardLink} href="/akashic-records">Search the archive →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>I want to participate</span>
              <h3>Move from audience to signal.</h3>
              <p>Enter The Clearing for public conversation, take a self-directed quest, or join the sustaining layer.</p>
              <div className={styles.actions}>
                <Link className={styles.cardLink} href="/clearing">Enter the Clearing →</Link>
                <Link className={styles.cardLink} href="/quests">Choose a quest →</Link>
              </div>
            </article>
          </div>
        </section>

        <section className={styles.section} aria-labelledby="route-title">
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
        </section>

        <section className={styles.section} aria-labelledby="next-title">
          <div className={styles.sectionHead}>
            <h2 id="next-title">Ready to move?</h2>
            <p>Chaos Tarot is the most complete interactive doorway. The Atlas is the best doorway if you want to understand the whole.</p>
          </div>
          <div className={styles.actions}>
            <a className={styles.primary} href="https://chaos-tarot.com/free-reading" target="_blank" rel="noopener noreferrer">
              Draw your first cards <span aria-hidden="true">↗</span>
            </a>
            <Link className={styles.secondary} href="/atlas">Explore the constellation →</Link>
            <Link className={styles.secondary} href="/membership">Help power the work →</Link>
          </div>
        </section>
      </div>
    </main>
  </>
);

export default Start;
