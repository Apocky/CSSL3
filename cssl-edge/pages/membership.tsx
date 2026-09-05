import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

import { SUPPORT_LINKS } from '../lib/support-links';
import styles from '../styles/NeuralPages.module.css';

const Membership: NextPage = () => {
  const koFi = SUPPORT_LINKS.find((link) => link.name === 'Ko-fi');
  const patreon = SUPPORT_LINKS.find((link) => link.name === 'Patreon');

  return (
    <>
      <Head>
        <title>Membership and support · Apocky</title>
        <meta
          name="description"
          content="Become part of Apocky’s sustaining layer through Patreon, Ko-fi, or Chaos Tarot—and help fund interconnected tools, worlds, and public knowledge."
        />
        <meta property="og:title" content="Membership and support · Apocky" />
        <meta property="og:description" content="Help keep the constellation alive and the next strange thing possible." />
        <meta property="og:type" content="website" />
        <meta property="og:url" content="https://www.apocky.com/membership" />
        <link rel="canonical" href="https://www.apocky.com/membership" />
      </Head>

      <main className={styles.page}>
        <div className={styles.wrap}>
          <p className={styles.eyebrow}>The sustaining layer</p>
          <h1 className={styles.title}>Don’t just visit the frontier. <em>Help power it.</em></h1>
          <p className={styles.lead}>
            Apocky already contains years of tools, writing, games, visual systems, and living experiments.
            Membership turns attention into runway: more time to connect the pieces, publish what is ready,
            and keep independent work independent.
          </p>
          <div className={styles.actions} aria-label="Primary membership actions">
            {patreon ? (
              <a className={styles.primary} href={patreon.href} target="_blank" rel="noopener noreferrer">
                Join on Patreon <span aria-hidden="true">↗</span>
              </a>
            ) : null}
            {koFi ? (
              <a className={styles.secondary} href={koFi.href} target="_blank" rel="noopener noreferrer">
                Fuel it on Ko-fi <span aria-hidden="true">↗</span>
              </a>
            ) : null}
            <a className={styles.secondary} href="https://chaos-tarot.com/pricing" target="_blank" rel="noopener noreferrer">
              Unlock Chaos Tarot <span aria-hidden="true">↗</span>
            </a>
          </div>

          <p className={styles.truth}>
            <strong>Truth first.</strong>
            <span>
              Patreon, Ko-fi, and Chaos Tarot handle their own checkout and publish the current price,
              benefits, renewal, and cancellation terms. Apocky does not invent a countdown, hide a free
              public basic behind a surprise paywall, or claim a benefit that is not wired.
            </span>
          </p>

          <section className={styles.section} aria-labelledby="paths-title">
            <div className={styles.sectionHead}>
              <h2 id="paths-title">Choose how you enter the sustaining circuit.</h2>
              <p>
                Each path does something different. Use the living product, become a recurring patron,
                or send a direct pulse of support. All three feed the same independent creative ecosystem.
              </p>
            </div>

            <div className={styles.grid3}>
              <article className={`${styles.tier} ${styles.tierFeatured}`}>
                <span className={styles.tag}>Living product</span>
                <h3>Chaos Tarot</h3>
                <p>Use the deepest active experience: readings, divination systems, study tools, journals, patterns, and progression.</p>
                <ul>
                  <li>Start free before choosing a paid plan</li>
                  <li>Support a product you can actually use</li>
                  <li>Keep your Apocky exploration moving</li>
                </ul>
                <a className={styles.cardLink} href="https://chaos-tarot.com/pricing" target="_blank" rel="noopener noreferrer">
                  See current Chaos Tarot plans <span aria-hidden="true">↗</span>
                </a>
              </article>

              <article className={styles.tier}>
                <span className={styles.tag}>Recurring patron</span>
                <h3>Patreon</h3>
                <p>A direct recurring vote for more time spent building, connecting, documenting, and releasing the work.</p>
                <ul>
                  <li>Recurring support</li>
                  <li>Current benefits shown before checkout</li>
                  <li>Cancel through Patreon</li>
                </ul>
                {patreon ? (
                  <a className={styles.cardLink} href={patreon.href} target="_blank" rel="noopener noreferrer">
                    Become a patron <span aria-hidden="true">↗</span>
                  </a>
                ) : null}
              </article>

              <article className={styles.tier}>
                <span className={styles.tag}>Direct support</span>
                <h3>Ko-fi</h3>
                <p>The quickest route for a one-time contribution, with recurring support available if you prefer it.</p>
                <ul>
                  <li>One-time or recurring contribution</li>
                  <li>No Apocky account required</li>
                  <li>Handled by Ko-fi</li>
                </ul>
                {koFi ? (
                  <a className={styles.cardLink} href={koFi.href} target="_blank" rel="noopener noreferrer">
                    Send a signal on Ko-fi <span aria-hidden="true">↗</span>
                  </a>
                ) : null}
              </article>
            </div>
          </section>

          <section className={styles.section} aria-labelledby="free-title">
            <div className={styles.sectionHead}>
              <h2 id="free-title">The public commons remains a real place.</h2>
              <p>
                Support buys sustainability, not ownership of another person or authority over the work.
                These public paths remain available whether or not you pay.
              </p>
            </div>
            <div className={styles.grid3}>
              <article className={styles.card}>
                <h3>Map the system</h3>
                <p>Navigate projects, concepts, and relationships in the Constellation Atlas.</p>
                <Link className={styles.cardLink} href="/atlas">Open the Atlas →</Link>
              </article>
              <article className={styles.card}>
                <h3>Read the record</h3>
                <p>Search the approved, public-safe Akashic archive.</p>
                <Link className={styles.cardLink} href="/akashic-records">Enter the archive →</Link>
              </article>
              <article className={styles.card}>
                <h3>Join the room</h3>
                <p>Read The Clearing freely; sign in when you want to participate.</p>
                <Link className={styles.cardLink} href="/clearing">Enter the Clearing →</Link>
              </article>
              <article className={styles.card}>
                <h3>Understand the language</h3>
                <p>Use the words and symbols dictionary when the work gets dense.</p>
                <Link className={styles.cardLink} href="/words">Open the dictionary →</Link>
              </article>
              <article className={styles.card}>
                <h3>Ask one question</h3>
                <p>Use the private Yes / No Oracle and keep your own judgment in charge.</p>
                <Link className={styles.cardLink} href="/oracle">Reveal a signal →</Link>
              </article>
              <article className={styles.card}>
                <h3>Craft a working</h3>
                <p>Compile symbolic language, render a sigil, and save it locally without an account.</p>
                <Link className={styles.cardLink} href="/spellcraft">Open Spellcraft →</Link>
              </article>
            </div>
          </section>

          <section className={styles.section} aria-labelledby="loop-title">
            <div className={styles.sectionHead}>
              <h2 id="loop-title">Attention becomes a creative flywheel.</h2>
              <p>
                Discovery leads to use; use creates evidence about what matters; support buys focused time;
                focused time produces better tools and richer public work.
              </p>
            </div>
            <div className={styles.diagram}>
              <svg viewBox="0 0 760 360" role="img" aria-labelledby="flywheel-title flywheel-desc">
                <title id="flywheel-title">Apocky sustaining flywheel</title>
                <desc id="flywheel-desc">Discovery connects to use, use to support, support to building, and building back to discovery.</desc>
                <defs>
                  <linearGradient id="flywheel-line" x1="0" x2="1">
                    <stop offset="0" stopColor="#78e7ff" />
                    <stop offset="0.5" stopColor="#7c8fff" />
                    <stop offset="1" stopColor="#c28cff" />
                  </linearGradient>
                  <filter id="flywheel-glow"><feGaussianBlur stdDeviation="4" result="blur" /><feMerge><feMergeNode in="blur" /><feMergeNode in="SourceGraphic" /></feMerge></filter>
                </defs>
                <path d="M195 180 C195 72 565 72 565 180 C565 288 195 288 195 180Z" fill="none" stroke="url(#flywheel-line)" strokeWidth="2" opacity=".55" />
                <path d="M380 64 L395 76 L374 80Z" fill="#78e7ff" />
                <path d="M380 296 L365 284 L386 280Z" fill="#c28cff" />
                {[
                  { x: 145, y: 180, label: 'DISCOVER', fill: '#0b1434' },
                  { x: 380, y: 74, label: 'USE', fill: '#10143d' },
                  { x: 615, y: 180, label: 'SUPPORT', fill: '#17113b' },
                  { x: 380, y: 286, label: 'BUILD', fill: '#101338' },
                ].map((node) => (
                  <g key={node.label} filter="url(#flywheel-glow)">
                    <circle cx={node.x} cy={node.y} r="49" fill={node.fill} stroke="url(#flywheel-line)" />
                    <text x={node.x} y={node.y + 4} fill="#f5f3ff" fontFamily="ui-monospace, monospace" fontSize="13" fontWeight="700" textAnchor="middle">{node.label}</text>
                  </g>
                ))}
                <circle cx="380" cy="180" r="58" fill="#050718" stroke="#78e7ff" strokeWidth="2" filter="url(#flywheel-glow)" />
                <text x="380" y="176" fill="#f5f3ff" fontFamily="ui-serif, Georgia" fontSize="22" textAnchor="middle">APOCKY</text>
                <text x="380" y="198" fill="#9ca6cc" fontFamily="ui-monospace, monospace" fontSize="10" textAnchor="middle">LIVING SYSTEM</text>
              </svg>
            </div>
          </section>
        </div>
      </main>
    </>
  );
};

export default Membership;
