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
        content="Try a creative tool, follow a small challenge, explore symbolic readings, or discover an idea in development."
      />
      <meta property="og:title" content="Public labs and experiments · Apocky" />
      <meta property="og:description" content="Tools to try and ideas to explore, with a clear explanation of what is available." />
      <meta property="og:url" content="https://www.apocky.com/labs" />
      <link rel="canonical" href="https://www.apocky.com/labs" />
    </Head>

    <main className={styles.page}>
      <div className={styles.wrap}>
        <p className={styles.eyebrow}>Tools &amp; experiments</p>
        <h1 className={styles.title}>Try something. <em>See what it opens.</em></h1>
        <p className={styles.lead}>
          Make a symbol, take a small quest, compare readings, or explore an idea.
          Research pages are marked so you can choose a tool you can use today.
        </p>

        <section className={styles.section} aria-labelledby="lab-title">
          <div className={styles.sectionHead}>
            <h2 id="lab-title">Things to try</h2>
            <p>Choose an activity or browse an idea.</p>
          </div>
          <div className={styles.grid2}>
            <article className={`${styles.card} ${styles.tierFeatured}`}>
              <span className={styles.tag}>FIND SOMETHING</span>
              <h3>Constellation Atlas</h3>
              <p>Switch between an explorable relationship map, sortable index, and shared dictionary.</p>
              <Link className={styles.cardLink} href="/atlas">Find a tool or idea →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>TRY A CHALLENGE</span>
              <h3>Discovery quests</h3>
              <p>Complete a forgiving eight-node expedition. Progress is saved only in the current browser and can be reset.</p>
              <Link className={styles.cardLink} href="/quests">Take a quest →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>HELP</span>
              <h3>Service status</h3>
              <p>Check whether the site is responding and find help if something fails.</p>
              <Link className={styles.cardLink} href="/status">Check availability →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>LEARN</span>
              <h3>Documentation library</h3>
              <p>Move from ordinary-language guides into technical specifications only when that level of detail is useful.</p>
              <Link className={styles.cardLink} href="/docs">Read the documentation →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>REFLECT</span>
              <h3>Divination systems</h3>
              <p>Compare seven symbolic traditions and keep reflective use separate from empirical or predictive claims.</p>
              <Link className={styles.cardLink} href="/divination">Compare readings →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>COSMOLOGY</span>
              <h3>The Omnoid cosmology</h3>
              <p>Explore Shawn’s ideas about selves, freedom, and a connected universe. This is an authored cosmology.</p>
              <Link className={styles.cardLink} href="/omnoid-singularity">Enter the model →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>IN DEVELOPMENT</span>
              <h3>Infinity Engine research</h3>
              <p>Read an unfinished research idea about how projects can share useful capabilities.</p>
              <Link className={styles.cardLink} href="/infinity-engine">Read the study →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>ON CHAOS TAROT</span>
              <h3>Chaos Tarot</h3>
              <p>Use the constellation’s independent live reading and study system. It keeps its own account, data, and payment boundary.</p>
              <a className={styles.cardLink} href="https://chaos-tarot.com/free-reading?source=apocky-labs" target="_blank" rel="noopener noreferrer">Begin a free reading ↗</a>
            </article>
            <article className={`${styles.card} ${styles.tierFeatured}`}>
              <span className={styles.tag}>ON CHAOS TAROT</span>
              <h3>Yes / No reading</h3>
              <p>Ask a focused question on Chaos Tarot. Its yes/no reading currently requires a separate sign-in.</p>
              <a className={styles.cardLink} href="https://chaos-tarot.com/yes-no" target="_blank" rel="noopener noreferrer">Open Chaos Tarot ↗</a>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>MAKE SOMETHING</span>
              <h3>Compose a symbolic phrase</h3>
              <p>Combine symbolic words, see an interpretation, and turn your phrase into a sigil.</p>
              <Link className={styles.cardLink} href="/spellcraft">Write a phrase →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>MAKE A SYMBOL</span>
              <h3>Sigil studio</h3>
              <p>Turn a symbolic phrase into a visible design, adjust it, and download the image.</p>
              <Link className={styles.cardLink} href="/sigils">Generate a sigil →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>SAVED ON THIS DEVICE</span>
              <h3>Local Spellbook</h3>
              <p>Return to your saved phrases and symbols, download a copy, or bring a saved collection back into this browser.</p>
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
          <Link className={styles.primary} href="/atlas?kind=study">Explore more ideas →</Link>
          <Link className={styles.secondary} href="/memory-tools">Find your notes &amp; saved work →</Link>
        </div>
      </div>
    </main>
  </>
);

export default Labs;
