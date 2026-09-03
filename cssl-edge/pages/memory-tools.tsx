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
  description: 'A truthful routing map for Apocky’s public memory banks, interactive tools, and guarded private systems.',
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
      <title>Memory banks and tools · Apocky</title>
      <meta
        name="description"
        content="Navigate Apocky’s connected public memory banks and tools, with private Mneme, account, and effect boundaries kept explicit."
      />
      <meta property="og:title" content="Memory banks and tools · Apocky" />
      <meta property="og:description" content="Public memory, usable instruments, and guarded synapses—connected without pretending every rail is open." />
      <meta property="og:url" content="https://www.apocky.com/memory-tools" />
      <link rel="canonical" href="https://www.apocky.com/memory-tools" />
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }} />
    </Head>

    <main className={styles.page}>
      <div className={styles.wrap}>
        <p className={styles.eyebrow}>Memory and tools · start here</p>
        <h1 className={styles.title}>Find it. Remember it. <em>Know where it lives.</em></h1>
        <p className={styles.lead}>
          Read the public library, inspect what this browser keeps, or open your signed-in memory when the full
          identity chain is ready. Every layer says who can see it, what persists, and how to leave with your data.
        </p>

        <MemoryExperience />

        <section className={styles.section} aria-labelledby="tools-title">
          <div className={styles.sectionHead}>
            <h2 id="tools-title">Tools with real handles</h2>
            <p>These interfaces do something observable now. Use the command palette from any page with Ctrl/⌘ K to jump between them.</p>
          </div>
          <div className={styles.grid3}>
            <article className={styles.card}>
              <span className={styles.tag}>ROUTE</span>
              <h3>Atlas explorer</h3>
              <p>Filter by dimension, kind, access state, or language, then share the resulting URL.</p>
              <Link className={styles.cardLink} href="/atlas">Map the system →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>PROBE</span>
              <h3>Status observatory</h3>
              <p>Check the bounded public health endpoint and receive a stable recovery path when it cannot answer.</p>
              <Link className={styles.cardLink} href="/status">Run a health check →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>LENS</span>
              <h3>Divination comparator</h3>
              <p>Compare symbolic systems while keeping reflection separate from evidence and guaranteed prediction.</p>
              <Link className={styles.cardLink} href="/divination">Compare the systems →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>GUIDE</span>
              <h3>Public quests</h3>
              <p>Turn exploration into an eleven-step, device-local journey across the constellation.</p>
              <Link className={styles.cardLink} href="/quests">Begin a quest →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>COMMUNITY</span>
              <h3>The Clearing</h3>
              <p>Read the shared room publicly; sign in only when you intentionally choose to contribute.</p>
              <Link className={styles.cardLink} href="/clearing">Enter the room →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>EXTERNAL · LIVE</span>
              <h3>Chaos Tarot</h3>
              <p>Continue into the independent reading and study system. Its account and payment boundary remains separate.</p>
              <a className={styles.cardLink} href="https://chaos-tarot.com/free-reading?source=apocky-memory-tools" target="_blank" rel="noopener noreferrer">Begin a free reading ↗</a>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>SIGNAL · LOCAL</span>
              <h3>Yes / No Oracle</h3>
              <p>Generate a bounded two-state signal and counter-question from a 128-bit local seed.</p>
              <Link className={styles.cardLink} href="/oracle">Ask one question →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>COMPILER · LOCAL</span>
              <h3>Spellcraft and Sigils</h3>
              <p>Parse an owner-authorized Haloic-derived vocabulary into an inspectable non-executable graph, then render its deterministic geometry.</p>
              <Link className={styles.cardLink} href="/spellcraft">Operate the engine →</Link>
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
          <Link className={styles.primary} href="/atlas?node=memory-tools">Trace these connections →</Link>
          <Link className={styles.secondary} href="/labs">Operate the public labs →</Link>
        </div>
      </div>
    </main>
  </>
);

export default MemoryTools;
