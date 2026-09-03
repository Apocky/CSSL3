import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

import styles from '../styles/NeuralPages.module.css';

const Labs: NextPage = () => (
  <>
    <Head>
      <title>Public labs and experiments · Apocky</title>
      <meta
        name="description"
        content="Use Apocky’s public experiments: knowledge maps, local quests, system diagnostics, comparative divination, visual cosmology, and architecture studies."
      />
      <meta property="og:title" content="Public labs and experiments · Apocky" />
      <meta property="og:description" content="Interfaces you can test, inspect, and traverse—with maturity labels attached." />
      <meta property="og:url" content="https://www.apocky.com/labs" />
      <link rel="canonical" href="https://www.apocky.com/labs" />
    </Head>

    <main className={styles.page}>
      <div className={styles.wrap}>
        <p className={styles.eyebrow}>Public experiment deck</p>
        <h1 className={styles.title}>Touch the machinery. <em>Keep the labels attached.</em></h1>
        <p className={styles.lead}>
          The lab is not a coming-soon graveyard. Every card below reaches an inspectable public surface.
          Maturity labels distinguish a dependable public route from a design study or a service-backed
          experiment that may truthfully report no data.
        </p>

        <section className={styles.section} aria-labelledby="lab-title">
          <div className={styles.sectionHead}>
            <h2 id="lab-title">Twelve connected public surfaces</h2>
            <p>Start anywhere; the contextual synapses beneath each mapped page provide a route onward.</p>
          </div>
          <div className={styles.grid2}>
            <article className={`${styles.card} ${styles.tierFeatured}`}>
              <span className={styles.tag}>PUBLIC · INTERACTIVE</span>
              <h3>Constellation Atlas</h3>
              <p>Switch between an explorable relationship map, sortable index, and shared dictionary.</p>
              <Link className={styles.cardLink} href="/atlas">Operate the Atlas →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>PUBLIC · DEVICE-LOCAL</span>
              <h3>Quest engine</h3>
              <p>Complete a forgiving eight-node expedition. Progress is saved only in the current browser and can be reset.</p>
              <Link className={styles.cardLink} href="/quests">Take a quest →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>PUBLIC · LIVE PROBE</span>
              <h3>Status observatory</h3>
              <p>Ask the same-origin health endpoint what it can currently prove, then use a typed fallback if it cannot answer.</p>
              <Link className={styles.cardLink} href="/status">Run the probe →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>PUBLIC · STATUS-LABELED</span>
              <h3>Documentation library</h3>
              <p>Move from ordinary-language guides into technical specifications only when that level of detail is useful.</p>
              <Link className={styles.cardLink} href="/docs">Read the documentation →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>PUBLIC · COMPARATIVE LENS</span>
              <h3>Divination systems</h3>
              <p>Compare seven symbolic traditions and keep reflective use separate from empirical or predictive claims.</p>
              <Link className={styles.cardLink} href="/divination">Rotate the lens →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>PUBLIC · EVIDENCE-TYPED</span>
              <h3>Omnoid visual model</h3>
              <p>Traverse an authored cosmology while its claims, mathematical motifs, hypotheses, and falsifiers remain visibly distinct.</p>
              <Link className={styles.cardLink} href="/omnoid-singularity">Enter the model →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>DESIGN STUDY</span>
              <h3>Infinity Engine research</h3>
              <p>Follow the source, test, experiment, specification, and release distinction behind shared architecture research.</p>
              <Link className={styles.cardLink} href="/infinity-engine">Read the study →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>INDEPENDENT LIVE PRODUCT</span>
              <h3>Chaos Tarot</h3>
              <p>Use the constellation’s independent live reading and study system. It keeps its own account, data, and payment boundary.</p>
              <a className={styles.cardLink} href="https://chaos-tarot.com/free-reading?source=apocky-labs" target="_blank" rel="noopener noreferrer">Begin a free reading ↗</a>
            </article>
            <article className={`${styles.card} ${styles.tierFeatured}`}>
              <span className={styles.tag}>PUBLIC · DEVICE-LOCAL</span>
              <h3>Yes / No Oracle</h3>
              <p>Ask one bounded question and use a reproducible symbolic signal to inspect your own reaction.</p>
              <Link className={styles.cardLink} href="/oracle">Reveal a signal →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>PUBLIC · DETERMINISTIC</span>
              <h3>Spellcraft engine</h3>
              <p>Compile Haloic-derived language into an inspectable, authority-none symbolic graph and interpretation.</p>
              <Link className={styles.cardLink} href="/spellcraft">Open the compiler →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>PUBLIC · EXPORTABLE SVG</span>
              <h3>Sigil studio</h3>
              <p>Render a validated working as visible, bounded geometry and download the reproducible artifact.</p>
              <Link className={styles.cardLink} href="/sigils">Generate a sigil →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>PUBLIC ROUTE · PRIVATE DATA</span>
              <h3>Local Spellbook</h3>
              <p>Explicitly save, verify, export, import, and delete private workings in the current browser.</p>
              <Link className={styles.cardLink} href="/spellbook">Open the shelf →</Link>
            </article>
          </div>
        </section>

        <p className={styles.truth}>
          <strong>Lab contract.</strong>
          <span>
            “Experimental” can mean missing data, changing behavior, or an unavailable dependency. It never
            means permission to transmit private material, spend money, or act on your behalf. External tools
            are contacted only after you choose their link.
          </span>
        </p>

        <div className={styles.actions}>
          <Link className={styles.primary} href="/atlas?kind=study">Map the studies →</Link>
          <Link className={styles.secondary} href="/memory-tools">Connect memory and tools →</Link>
        </div>
      </div>
    </main>
  </>
);

export default Labs;
