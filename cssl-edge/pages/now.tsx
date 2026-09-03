import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

import styles from '../styles/NeuralPages.module.css';

const Now: NextPage = () => (
  <>
    <Head>
      <title>What is alive now · Apocky</title>
      <meta
        name="description"
        content="A candid map of what people can use on Apocky now, what remains experimental, and which connections still require verification."
      />
      <meta property="og:title" content="What is alive now · Apocky" />
      <meta property="og:description" content="Available, experimental, and still unwired—without roadmap theater." />
      <meta property="og:url" content="https://www.apocky.com/now" />
      <link rel="canonical" href="https://www.apocky.com/now" />
    </Head>

    <main className={styles.page}>
      <div className={styles.wrap}>
        <p className={styles.eyebrow}>Living-system ledger</p>
        <h1 className={styles.title}>What is alive. <em>What is becoming.</em></h1>
        <p className={styles.lead}>
          This is the shortest honest route through the current public system. “Available” means there is a
          page or interaction you can use. “Experimental” means the interface exists but its supporting service
          may be empty or degraded. “Unwired” means you should not be sold the promise yet.
        </p>
        <p className={styles.note}><time dateTime="2026-09-03">Last reviewed September 3, 2026.</time> Runtime status may change between reviews.</p>

        <section className={styles.section} aria-labelledby="alive-title">
          <div className={styles.sectionHead}>
            <h2 id="alive-title">Alive now</h2>
            <p>These paths deliver public value before requiring payment or participation.</p>
          </div>
          <div className={styles.grid3}>
            <article className={`${styles.card} ${styles.tierFeatured}`}>
              <span className={styles.tag}>Interactive</span>
              <h3>Constellation Atlas</h3>
              <p>Relationship map, kind × access matrix, index, dictionary, filters, shareable state, and explicit links across the public work.</p>
              <Link className={styles.cardLink} href="/atlas">Explore the whole map →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>Public memory</span>
              <h3>Akashic Records</h3>
              <p>Approved public writing and public-safe conversation records with stable reader pages and provenance.</p>
              <Link className={styles.cardLink} href="/akashic-records">Search the record →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>Independent live product</span>
              <h3>Chaos Tarot</h3>
              <p>Free readings, symbolic systems, study tools, journals, and a separate optional Oracle membership.</p>
              <a className={styles.cardLink} href="https://chaos-tarot.com/free-reading?source=apocky-now" target="_blank" rel="noopener noreferrer">Begin free on Chaos Tarot ↗</a>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>Public room</span>
              <h3>The Clearing</h3>
              <p>Read without an account. Sign in only if you decide to speak or react.</p>
              <Link className={styles.cardLink} href="/clearing">Enter the room →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>Playable alpha</span>
              <h3>Labyrinth of Apocalypse</h3>
              <p>An unfinished Windows test build with version, checksum, limitations, and download terms shown first.</p>
              <Link className={styles.cardLink} href="/download">Inspect the build →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>Operational truth</span>
              <h3>Public status</h3>
              <p>A same-origin health probe with typed states and fallback routes, bounded away from secrets and private logs.</p>
              <Link className={styles.cardLink} href="/status">Check the system →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>Routed nervous system</span>
              <h3>Memory banks and tools</h3>
              <p>One directory connects public memory, working instruments, chosen handoffs, and the private rails that remain closed.</p>
              <Link className={styles.cardLink} href="/memory-tools">Trace the signal →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>Private symbolic tools</span>
              <h3>Oracle, Spellcraft, Sigils, Spellbook</h3>
              <p>A fast Yes / No prompt, fail-closed language compiler, deterministic SVG studio, and explicit device-local collection.</p>
              <Link className={styles.cardLink} href="/spellcraft">Operate the studio →</Link>
            </article>
          </div>
        </section>

        <section className={styles.section} aria-labelledby="becoming-title">
          <div className={styles.sectionHead}>
            <h2 id="becoming-title">Becoming, without pretending.</h2>
            <p>The lab collects usable experiments and design studies without relabeling a prototype as a finished service.</p>
          </div>
          <div className={styles.grid2}>
            <article className={styles.card}>
              <h3>Public experiments</h3>
              <p>Device-local quests, live diagnostics, cross-system divination, symbolic compilation, visual cosmology, and architecture research.</p>
              <Link className={styles.cardLink} href="/labs">Enter the labs →</Link>
            </article>
            <article className={styles.card}>
              <h3>Documentation</h3>
              <p>Plain-language guides lead; technical specifications remain available when their detail helps.</p>
              <Link className={styles.cardLink} href="/docs">Read the documentation →</Link>
            </article>
          </div>
        </section>

        <p className={styles.truth}>
          <strong>Still unwired.</strong>
          <span>
            Apocky and Chaos Tarot do not yet share one account or entitlement system. First-party Apocky paid
            membership is not offered as live. A public route, configured flag, or plan is not proof that an
            end-to-end account, payment, or provider journey succeeded.
          </span>
        </p>

        <div className={styles.actions}>
          <Link className={styles.primary} href="/start">Choose your path →</Link>
          <Link className={styles.secondary} href="/membership">Fund what should become real →</Link>
        </div>
      </div>
    </main>
  </>
);

export default Now;
