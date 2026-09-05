import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

import MemoryExperience from '../components/memory/MemoryExperience';
import styles from '../styles/NeuralPages.module.css';

const structuredData = {
  '@context': 'https://schema.org',
  '@type': 'CollectionPage',
  name: 'Memory banks and tools',
  url: 'https://www.apocky.com/memory-tools',
  description: 'Read public writing, return to saved symbols, and manage your own notes.',
  hasPart: [
    { '@type': 'WebPage', name: 'Akashic Records', url: 'https://www.apocky.com/akashic-records' },
    { '@type': 'WebPage', name: 'Constellation Atlas', url: 'https://www.apocky.com/atlas' },
    { '@type': 'WebPage', name: 'Public quests', url: 'https://www.apocky.com/quests' },
    { '@type': 'WebPage', name: 'System status', url: 'https://www.apocky.com/status' },
    { '@type': 'WebApplication', name: 'Symbolic Spellcraft Engine', url: 'https://www.apocky.com/spellcraft' },
    { '@type': 'WebPage', name: 'Local Spellbook', url: 'https://www.apocky.com/spellbook' },
  ],
};

const MemoryTools: NextPage = () => (
  <>
    <Head>
      <title>Your notes & saved work · Apocky</title>
      <meta
        name="description"
        content="Read public writing, return to the symbols saved in this browser, and manage your own private notes."
      />
      <meta property="og:title" content="Your notes & saved work · Apocky" />
      <meta property="og:description" content="Find something worth keeping: essays, saved symbols, and your own notes." />
      <meta property="og:url" content="https://www.apocky.com/memory-tools" />
      <link rel="canonical" href="https://www.apocky.com/memory-tools" />
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }} />
    </Head>

    <main className={styles.page}>
      <div className={styles.wrap}>
        <p className={styles.eyebrow}>Your notes &amp; saved work</p>
        <h1 className={styles.title}>Find something worth <em>keeping.</em></h1>
        <p className={styles.lead}>
          Browse the essays, return to your saved symbols, or manage your own notes.
          Saved work in this browser stays on this device.
        </p>

        <MemoryExperience />

        <section className={styles.section} aria-labelledby="tools-title">
          <div className={styles.sectionHead}>
            <h2 id="tools-title">More things to use</h2>
            <p>Make a symbol, find a meaning, or explore a question. Search is always nearby.</p>
          </div>
          <div className={styles.grid3}>
            <article className={styles.card}>
              <span className={styles.tag}>BROWSE</span>
              <h3>Find a tool or idea</h3>
              <p>Search the tools, stories, and ideas here, then go straight to what you find.</p>
              <Link className={styles.cardLink} href="/atlas">Browse everything →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>HELP</span>
              <h3>Service status</h3>
              <p>Check whether the site is responding and find help if something fails.</p>
              <Link className={styles.cardLink} href="/status">Check availability →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>REFLECT</span>
              <h3>Compare readings</h3>
              <p>Compare symbolic systems while keeping reflection separate from evidence and guaranteed prediction.</p>
              <Link className={styles.cardLink} href="/divination">Compare the systems →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>GUIDE</span>
              <h3>Public quests</h3>
              <p>Choose a small activity and keep track of your progress in this browser.</p>
              <Link className={styles.cardLink} href="/quests">Begin a quest →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>COMMUNITY</span>
              <h3>The Clearing</h3>
              <p>Read the shared room publicly; sign in only when you intentionally choose to contribute.</p>
              <Link className={styles.cardLink} href="/clearing">Enter the room →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>ON CHAOS TAROT</span>
              <h3>Chaos Tarot</h3>
              <p>Continue into the independent reading and study system. Its account and payment boundary remains separate.</p>
              <a className={styles.cardLink} href="https://chaos-tarot.com/free-reading?source=apocky-memory-tools" target="_blank" rel="noopener noreferrer">Begin a free reading ↗</a>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>ON CHAOS TAROT</span>
              <h3>Yes / No reading</h3>
              <p>Ask a focused question on Chaos Tarot. Its yes/no reading currently requires a separate sign-in.</p>
              <a className={styles.cardLink} href="https://chaos-tarot.com/yes-no" target="_blank" rel="noopener noreferrer">Open Chaos Tarot ↗</a>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>MAKE SOMETHING</span>
              <h3>Spellcraft and Sigils</h3>
              <p>Compose a symbolic phrase, see what its words mean, and turn it into a sigil.</p>
              <Link className={styles.cardLink} href="/spellcraft">Make a phrase or sigil →</Link>
            </article>
          </div>
        </section>

        <p className={styles.truth}>
          <strong>Private stays private.</strong>
          <span>
            Named Mneme profiles remain unreachable. The member route accepts only a verified session and derives
            its profile on the server; if that profile or a dependency is absent, the page stays locked with a real
            recovery step. Apocky and Chaos Tarot still do not share an account.
          </span>
        </p>

        <div className={styles.actions}>
          <Link className={styles.primary} href="/atlas?node=memory-tools">Find more to explore →</Link>
          <Link className={styles.secondary} href="/labs">Try an experiment →</Link>
        </div>
      </div>
    </main>
  </>
);

export default MemoryTools;
