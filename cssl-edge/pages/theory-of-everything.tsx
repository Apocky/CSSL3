import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

import styles from '../styles/NeuralPages.module.css';

const TheoryOfEverything: NextPage = () => {
  const faq = [
    {
      question: 'Is the Omnoid Singularity a proven Theory of Everything?',
      answer: 'No. Its current public form is a mathematically informed ontological cosmology with open physical bridges, not a completed or experimentally validated unification of fundamental physics.',
    },
    {
      question: 'What does the Omnoid try to connect?',
      answer: 'It explores recursive totality, distinct centers, time, singularity, agency, freedom, identity, and the relation between mathematical structure and lived meaning.',
    },
    {
      question: 'What would make it scientific?',
      answer: 'It would need precisely defined objects and dynamics, a bridge to measurable observables, a prediction that differs from existing theories, and a test that could genuinely prove that prediction wrong.',
    },
  ];

  const structuredData = {
    '@context': 'https://schema.org',
    '@graph': [
      {
        '@type': 'Article',
        headline: 'The Omnoid Singularity and the Theory of Everything question',
        description: 'An evidence-typed introduction to Shawn Apocky’s Omnoid cosmology and its open path toward a testable theory.',
        author: { '@type': 'Person', name: 'Shawn Apocky' },
        publisher: { '@type': 'Organization', name: 'Apocky', url: 'https://www.apocky.com/' },
        mainEntityOfPage: 'https://www.apocky.com/theory-of-everything',
      },
      {
        '@type': 'FAQPage',
        mainEntity: faq.map((item) => ({ '@type': 'Question', name: item.question, acceptedAnswer: { '@type': 'Answer', text: item.answer } })),
      },
    ],
  };

  return (
    <>
      <Head>
        <title>Omnoid Singularity and the Theory of Everything · Apocky</title>
        <meta
          name="description"
          content="Explore the Omnoid Singularity as an evidence-typed Theory of Everything question: what it connects, what mathematics supports, what remains hypothesis, and how it could become testable."
        />
        <meta name="keywords" content="Theory of Everything, Omnoid Singularity, cosmology, unified theory, quantum gravity, consciousness, philosophy of physics, Apocky" />
        <meta property="og:title" content="The Omnoid and the Theory of Everything question" />
        <meta property="og:description" content="A rigorous map of the ambition, structure, evidence, and open tests." />
        <meta property="og:type" content="article" />
        <meta property="og:url" content="https://www.apocky.com/theory-of-everything" />
        <link rel="canonical" href="https://www.apocky.com/theory-of-everything" />
        <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }} />
      </Head>

      <main className={styles.page}>
        <div className={styles.wrap}>
          <p className={styles.eyebrow}>Shawn Apocky’s cosmology</p>
          <h1 className={styles.title}>How does <em>everything connect?</em></h1>
          <p className={styles.lead}>
            The Omnoid Singularity is Shawn Apocky’s attempt to reason about totality, distinct selves,
            recursion, time, freedom, geometry, and return inside one conceptual architecture. That is a
            Theory-of-Everything-sized ambition, offered as an evolving model rather than a proven physical theory.
          </p>
          <div className={styles.actions}>
            <Link className={styles.primary} href="/omnoid-singularity#summary">Read the Omnoid idea →</Link>
            <a className={styles.secondary} href="/codex-apockalypsis">Enter the story →</a>
            <Link className={styles.secondary} href="/words">Look up a term →</Link>
          </div>

          <details className={styles.section}>
            <summary>How the ideas, mathematics, and hypotheses differ</summary>
            <div className={styles.sectionHead}>
              <h2 id="layers-title">One ambition. Four different layers of claim.</h2>
              <p>A serious unification project must label where a statement comes from and what kind of support it has.</p>
            </div>
            <div className={styles.grid4}>
              <article className={styles.card}>
                <span className={styles.tag}>○ Authored</span>
                <h3>Cosmological proposal</h3>
                <p>The Omnoid, the Open Door, True Neutral, recursive totality, and substrate-relative continuity are parts of the authored system.</p>
              </article>
              <article className={styles.card}>
                <span className={styles.tag}>◐ Formalized</span>
                <h3>Collaborative model</h3>
                <p>Definitions, diagrams, operational readings, and connections sharpen the proposal without becoming independent evidence for it.</p>
              </article>
              <article className={styles.card}>
                <span className={styles.tag}>✓ Established</span>
                <h3>Mathematical structure</h3>
                <p>Hopf fibrations, projective spaces, topology, and fractal uncertainty are real mathematics with precise domains and limits.</p>
              </article>
              <article className={styles.card}>
                <span className={styles.tag}>△ Open</span>
                <h3>Physical bridge</h3>
                <p>Claims connecting the ontology to quantum physics, consciousness, continuity, or measurable dynamics remain hypotheses until tested.</p>
              </article>
            </div>
          </details>

          <section className={styles.section} aria-labelledby="map-title">
            <div className={styles.sectionHead}>
              <h2 id="map-title">From lived experience to the whole.</h2>
              <p>The center is not a shortcut. Every ring needs its own definitions, relations, and evidence.</p>
            </div>
            <div className={styles.diagram}>
              <svg viewBox="0 0 760 430" role="img" aria-labelledby="toe-map-title toe-map-desc">
                <title id="toe-map-title">Omnoid unification map</title>
                <desc id="toe-map-desc">Concentric layers connect lived experience, agency and identity, mathematical structure, physical dynamics, and total cosmology.</desc>
                <defs>
                  <radialGradient id="toe-core"><stop stopColor="#78e7ff" stopOpacity=".9" /><stop offset="1" stopColor="#6d5dfc" stopOpacity=".1" /></radialGradient>
                  <linearGradient id="toe-ring" x1="0" x2="1"><stop stopColor="#78e7ff" /><stop offset=".5" stopColor="#7f8fff" /><stop offset="1" stopColor="#c28cff" /></linearGradient>
                  <filter id="toe-glow"><feGaussianBlur stdDeviation="4" result="b" /><feMerge><feMergeNode in="b" /><feMergeNode in="SourceGraphic" /></feMerge></filter>
                </defs>
                {[174, 139, 104, 69].map((radius, index) => <circle key={radius} cx="380" cy="215" r={radius} fill="none" stroke="url(#toe-ring)" opacity={0.2 + index * 0.1} />)}
                <circle cx="380" cy="215" r="35" fill="url(#toe-core)" stroke="#78e7ff" filter="url(#toe-glow)" />
                <text x="380" y="220" fill="#050515" fontFamily="ui-monospace, monospace" fontSize="11" fontWeight="900" textAnchor="middle">OMNOID</text>
                {[
                  [380, 22, 'TOTAL COSMOLOGY'],
                  [596, 128, 'PHYSICAL DYNAMICS'],
                  [572, 346, 'MATHEMATICAL STRUCTURE'],
                  [188, 346, 'AGENCY + IDENTITY'],
                  [164, 128, 'LIVED EXPERIENCE'],
                ].map(([x, y, label]) => (
                  <g key={String(label)}>
                    <circle cx={Number(x)} cy={Number(y)} r="6" fill="#9b91ff" filter="url(#toe-glow)" />
                    <line x1={Number(x)} y1={Number(y)} x2="380" y2="215" stroke="url(#toe-ring)" opacity=".28" />
                    <text x={Number(x)} y={Number(y) + 22} fill="#d8dcf4" fontFamily="ui-monospace, monospace" fontSize="10" fontWeight="700" textAnchor="middle">{label}</text>
                  </g>
                ))}
              </svg>
            </div>
          </section>

          <section className={styles.section} aria-labelledby="test-title">
            <div className={styles.sectionHead}>
              <h2 id="test-title">What would make this testable?</h2>
              <p>Scale of ambition cannot substitute for measurement. These are the missing bridges that matter most.</p>
            </div>
            <div className={styles.grid3}>
              <article className={styles.card}><span className={styles.tag}>01 · Define</span><h3>Specify the object</h3><p>Give topology, metric, dimensions, states, identity relations, and dynamics definitions that do not change type mid-argument.</p></article>
              <article className={styles.card}><span className={styles.tag}>02 · Predict</span><h3>Derive a difference</h3><p>Produce a quantitative outcome that differs from existing physics under stated initial conditions.</p></article>
              <article className={styles.card}><span className={styles.tag}>03 · Risk</span><h3>Name what would break it</h3><p>Predeclare an observation or result that would count against the mechanism instead of absorbing every outcome after the fact.</p></article>
            </div>
            <p className={styles.truth}>
              <strong>Current label.</strong>
              <span>The most defensible description is a mathematically informed ontological cosmology with valid mathematical motifs and open physical bridges—not a completed quantum-gravity proof.</span>
            </p>
          </section>

          <section className={styles.section} aria-labelledby="faq-title">
            <div className={styles.sectionHead}><h2 id="faq-title">Theory-sized questions, bounded answers.</h2><p>The full synthesis carries sources, equations, countercases, and exact category boundaries.</p></div>
            <div className={styles.grid3}>
              {faq.map((item) => <article className={styles.card} key={item.question}><h3>{item.question}</h3><p>{item.answer}</p></article>)}
            </div>
            <div className={styles.actions}>
              <Link className={styles.primary} href="/omnoid-singularity">Continue into the full model →</Link>
              <Link className={styles.secondary} href="/words">Decode its terms →</Link>
              <Link className={styles.secondary} href="/membership">Help fund the next testable step →</Link>
            </div>
          </section>
        </div>
      </main>
    </>
  );
};

export default TheoryOfEverything;
