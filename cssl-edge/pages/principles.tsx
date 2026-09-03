import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

import styles from '../styles/NeuralPages.module.css';

const PRINCIPLES = [
  {
    title: 'Consent is active',
    copy: 'Every offer names its source, object, audience, purpose, duration, aftermath, and learning boundary. An available interface never grants authority by itself.',
  },
  {
    title: 'Sovereignty persists',
    copy: 'Payment, proximity, history, or interface readiness never creates ownership over a being, their attention, their identity, or their refusal.',
  },
  {
    title: 'Appearance tells truth',
    copy: 'Samples are labeled samples. Local drafts remain local. Prototypes never masquerade as live rooms, participants, receipts, or working capabilities.',
  },
  {
    title: 'Stillness is complete',
    copy: 'Motion can clarify depth, but every relationship and action remains understandable when animation, parallax, color, and perspective disappear.',
  },
] as const;

const Principles: NextPage = () => (
  <>
    <Head>
      <title>Interface principles · Apocky</title>
      <meta name="description" content="The four interface invariants behind Apocky: active consent, persistent sovereignty, truthful appearance, and complete stillness." />
      <meta property="og:title" content="The space has laws · Apocky principles" />
      <meta property="og:url" content="https://www.apocky.com/principles" />
      <link rel="canonical" href="https://www.apocky.com/principles" />
    </Head>
    <main className={styles.page}>
      <div className={styles.wrap}>
        <p className={styles.eyebrow}>Interface invariants</p>
        <h1 className={styles.title}>The visual field may shimmer. <em>Its truth boundaries do not.</em></h1>
        <p className={styles.lead}>Geometry explains relationships; it never creates authority or consent. These laws remain visible in ordinary text, keyboard order, forced colors, reduced motion, and the complete path index.</p>

        <section className={styles.section} aria-labelledby="invariants-title">
          <div className={styles.sectionHead}><h2 id="invariants-title">Four invariants. No hidden edge.</h2><p>The interface can become more vivid, responsive, and persuasive without becoming manipulative or false.</p></div>
          <div className={styles.grid4}>
            {PRINCIPLES.map((principle, index) => (
              <article className={styles.card} key={principle.title}>
                <span className={styles.tag}>{String(index + 1).padStart(2, '0')} · invariant</span>
                <h3>{principle.title}</h3>
                <p>{principle.copy}</p>
              </article>
            ))}
          </div>
        </section>

        <section className={styles.section} aria-labelledby="views-title">
          <div className={styles.sectionHead}><h2 id="views-title">Every view must preserve the same truth.</h2><p>The map, index, dictionary, command palette, and contextual synapses are different projections of one typed public graph.</p></div>
          <div className={styles.diagram}>
            <svg viewBox="0 0 760 360" role="img" aria-labelledby="principle-map-title principle-map-desc">
              <title id="principle-map-title">Equivalent interface views</title>
              <desc id="principle-map-desc">Map, index, dictionary, and plain links all connect to one public truth source.</desc>
              <defs><linearGradient id="principle-line" x1="0" x2="1"><stop stopColor="#78e7ff" /><stop offset="1" stopColor="#c28cff" /></linearGradient></defs>
              <g fill="none" stroke="url(#principle-line)" opacity=".45"><path d="M380 180L145 86M380 180L615 86M380 180L145 274M380 180L615 274" /></g>
              <circle cx="380" cy="180" r="68" fill="#07091f" stroke="#78e7ff" />
              <text x="380" y="176" fill="#f5f3ff" fontFamily="ui-serif, Georgia" fontSize="22" textAnchor="middle">ONE GRAPH</text>
              <text x="380" y="198" fill="#9ca6cc" fontFamily="ui-monospace, monospace" fontSize="10" textAnchor="middle">EXPLICIT RELATIONS</text>
              {[[145,86,'MAP'],[615,86,'INDEX'],[145,274,'DICTIONARY'],[615,274,'PLAIN LINKS']].map(([x,y,label]) => (
                <g key={String(label)}><circle cx={Number(x)} cy={Number(y)} r="48" fill="#080a24" stroke="url(#principle-line)" /><text x={Number(x)} y={Number(y)+4} fill="#d8dcf4" fontFamily="ui-monospace, monospace" fontSize="11" fontWeight="700" textAnchor="middle">{label}</text></g>
              ))}
            </svg>
          </div>
          <p className={styles.truth}><strong>Readable equivalent.</strong><span>If the visual projection fails, filters to zero, or motion is disabled, the complete semantic list and direct destination links remain usable.</span></p>
        </section>

        <section className={styles.section} aria-labelledby="follow-title">
          <div className={styles.sectionHead}><h2 id="follow-title">Follow a relationship.</h2><p>See the principles operating in the public map, the community room, and the honest support layer.</p></div>
          <div className={styles.actions}>
            <Link className={styles.primary} href="/atlas">Test them in the Atlas →</Link>
            <Link className={styles.secondary} href="/clearing">Enter the Clearing →</Link>
            <Link className={styles.secondary} href="/membership">Read the support terms →</Link>
          </div>
        </section>
      </div>
    </main>
  </>
);

export default Principles;
