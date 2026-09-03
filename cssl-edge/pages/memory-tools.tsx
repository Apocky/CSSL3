import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

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
        <p className={styles.eyebrow}>Nervous-system directory</p>
        <h1 className={styles.title}>Memory banks. Tools. <em>One routed mind.</em></h1>
        <p className={styles.lead}>
          Apocky’s public memory and instruments now share one navigable routing layer. Each connection below
          names where information lives, what can act on it, and which synapses remain closed until identity,
          consent, and ownership can be proved end to end.
        </p>

        <section className={styles.section} aria-labelledby="system-map-title">
          <div className={styles.sectionHead}>
            <h2 id="system-map-title">The public signal path</h2>
            <p>The picture is an orientation aid. The same route remains available as ordinary links immediately below it.</p>
          </div>
          <div className={styles.diagram}>
            <svg viewBox="0 0 900 420" role="img" aria-labelledby="memory-map-title memory-map-desc">
              <title id="memory-map-title">Apocky public memory and tool routing map</title>
              <desc id="memory-map-desc">Visitor intent enters the Atlas, which routes to public memory banks, usable tools, and optional external experiences. Private memory and effect systems remain behind a closed broker.</desc>
              <defs>
                <linearGradient id="memory-node" x1="0" x2="1">
                  <stop offset="0" stopColor="#161944" />
                  <stop offset="1" stopColor="#26164d" />
                </linearGradient>
                <marker id="memory-arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
                  <path d="M0,0 L8,4 L0,8 Z" fill="#78e7ff" />
                </marker>
              </defs>
              <g fill="none" stroke="#6677ff" strokeOpacity="0.7" strokeWidth="2" markerEnd="url(#memory-arrow)">
                <path d="M170 105 H335" />
                <path d="M475 105 C560 105 550 82 625 82" />
                <path d="M475 105 C560 105 550 174 625 174" />
                <path d="M475 105 C560 105 550 266 625 266" />
                <path d="M695 116 V140" />
                <path d="M695 208 V232" />
              </g>
              <g fill="url(#memory-node)" stroke="#93a2ff" strokeWidth="2">
                <rect x="35" y="65" width="135" height="80" rx="20" />
                <rect x="335" y="65" width="140" height="80" rx="20" />
                <rect x="625" y="42" width="160" height="74" rx="20" />
                <rect x="625" y="140" width="160" height="68" rx="20" />
                <rect x="625" y="232" width="160" height="68" rx="20" />
              </g>
              <g fill="#f4f2ff" fontFamily="system-ui, sans-serif" fontWeight="700" textAnchor="middle">
                <text x="102" y="100">Visitor intent</text>
                <text x="102" y="122" fill="#a9b1d2" fontSize="13">choose · search · ask</text>
                <text x="405" y="100">Atlas router</text>
                <text x="405" y="122" fill="#a9b1d2" fontSize="13">map · index · dictionary</text>
                <text x="705" y="75">Public memory</text>
                <text x="705" y="96" fill="#a9b1d2" fontSize="13">Akashic · docs · words</text>
                <text x="705" y="168">Public tools</text>
                <text x="705" y="190" fill="#a9b1d2" fontSize="13">quests · status · lenses</text>
                <text x="705" y="260">Chosen handoffs</text>
                <text x="705" y="282" fill="#a9b1d2" fontSize="13">Chaos Tarot · support</text>
              </g>
              <g transform="translate(335 318)">
                <rect width="450" height="66" rx="18" fill="#0b0b18" stroke="#8f75d8" strokeDasharray="8 6" />
                <text x="225" y="28" fill="#d8ceff" fontFamily="system-ui, sans-serif" fontWeight="700" textAnchor="middle">Guarded private synapses</text>
                <text x="225" y="49" fill="#a9a6c5" fontFamily="system-ui, sans-serif" fontSize="13" textAnchor="middle">Mneme · account identity · tool/effect broker · private telemetry</text>
              </g>
            </svg>
          </div>
        </section>

        <section className={styles.section} aria-labelledby="memory-banks-title">
          <div className={styles.sectionHead}>
            <h2 id="memory-banks-title">Memory banks you can use</h2>
            <p>Each bank has a different persistence and visibility contract. “Memory” never silently means “we stored your behavior.”</p>
          </div>
          <div className={styles.grid2}>
            <article className={`${styles.card} ${styles.tierFeatured}`}>
              <span className={styles.tag}>PUBLIC · PUBLISHED</span>
              <h3>Akashic Records</h3>
              <p>Approved public writing and public-safe conversations, with stable records and provenance. It is a library, not a profile dossier.</p>
              <Link className={styles.cardLink} href="/akashic-records">Search public memory →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>PUBLIC · SEMANTIC</span>
              <h3>Atlas and dictionary</h3>
              <p>A shared relationship graph and vocabulary bank. Search state may appear in the URL so you can copy a view; it is not a personal memory.</p>
              <Link className={styles.cardLink} href="/atlas?view=dictionary">Open semantic memory →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>DEVICE-LOCAL · REVERSIBLE</span>
              <h3>Quest progress</h3>
              <p>Eight journey markers live only in this browser. They can be reset from the quest page and are not transmitted as an account history.</p>
              <Link className={styles.cardLink} href="/quests">Open local memory →</Link>
            </article>
            <article className={styles.card}>
              <span className={styles.tag}>PUBLIC · REFERENCE</span>
              <h3>Documentation</h3>
              <p>Guides, status labels, and technical specifications preserve how the systems are intended to work without claiming runtime proof.</p>
              <Link className={styles.cardLink} href="/docs">Browse reference memory →</Link>
            </article>
          </div>
        </section>

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
              <p>Turn exploration into an eight-step, device-local journey across the constellation.</p>
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
          </div>
        </section>

        <p className={styles.truth}>
          <strong>Private means closed.</strong>
          <span>
            Mneme profile memory, private telemetry, and effect-capable tools are not public amenities. The
            candidate runtime blocks the unbrokered Mneme API until a signed-in person can be bound to exactly
            one owned profile with export and forgetting controls. Apocky and Chaos Tarot still do not share an account.
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
