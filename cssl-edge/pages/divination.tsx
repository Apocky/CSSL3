import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

import styles from '../styles/NeuralPages.module.css';

const SYSTEMS = [
  ['Chaos Tarot', 'A transformed tarot language built for reflection, pattern, and authored interpretation.'],
  ['Elder Futhark runes', 'Twenty-four symbols read as forces, tensions, inheritances, and possible movement.'],
  ['I Ching', 'Hexagrams and changing lines for examining a situation as a process in motion.'],
  ['Ogham', 'A symbolic alphabet often approached through trees, thresholds, memory, and relation.'],
  ['Lenormand', 'Compact combinations that reward concrete questions and relational reading.'],
  ['Geomancy', 'Figures generated through structured chance and interpreted through elemental patterns.'],
  ['Astrology', 'Timing and correspondence explored through planets, signs, houses, and cycles.'],
  ['Cross-system synthesis', 'Multiple symbolic traditions placed beside one another without pretending they are interchangeable.'],
] as const;

const Divination: NextPage = () => {
  const faq = [
    {
      question: 'What is Chaos Tarot?',
      answer: 'Chaos Tarot is an interactive symbolic-reading project with tarot, runes, I Ching, Ogham, Lenormand, geomancy, astrology, and cross-system synthesis.',
    },
    {
      question: 'Does a divination reading predict a guaranteed future?',
      answer: 'No. A reading can structure reflection and surface patterns, but it does not establish certainty, remove agency, or replace medical, legal, financial, or mental-health expertise.',
    },
    {
      question: 'Can I try Chaos Tarot for free?',
      answer: 'Yes. Chaos Tarot publishes a free-reading route and current plan terms before any paid checkout.',
    },
  ];

  const structuredData = {
    '@context': 'https://schema.org',
    '@graph': [
      {
        '@type': 'Article',
        headline: 'Chaos, Tarot, and Divination: a practical map',
        description: 'A grounded guide to symbolic divination and the interconnected Chaos Tarot experience.',
        author: { '@type': 'Person', name: 'Shawn Apocky' },
        publisher: { '@type': 'Organization', name: 'Apocky', url: 'https://www.apocky.com/' },
        mainEntityOfPage: 'https://www.apocky.com/divination',
      },
      {
        '@type': 'FAQPage',
        mainEntity: faq.map((item) => ({
          '@type': 'Question',
          name: item.question,
          acceptedAnswer: { '@type': 'Answer', text: item.answer },
        })),
      },
    ],
  };

  return (
    <>
      <Head>
        <title>Chaos, Tarot, and Divination · A practical map</title>
        <meta
          name="description"
          content="Explore Chaos Tarot and seven divination traditions plus cross-system synthesis—what symbolic readings can do, what they cannot prove, and where to begin free."
        />
        <meta name="keywords" content="tarot, chaos tarot, divination, runes, I Ching, Ogham, Lenormand, geomancy, astrology, symbolic systems" />
        <meta property="og:title" content="Chaos, Tarot, and Divination · A practical map" />
        <meta property="og:description" content="Seven traditions, cross-system synthesis, and one honest route into symbolic reflection." />
        <meta property="og:type" content="article" />
        <meta property="og:url" content="https://www.apocky.com/divination" />
        <link rel="canonical" href="https://www.apocky.com/divination" />
        <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }} />
      </Head>

      <main className={styles.page}>
        <div className={styles.wrap}>
          <p className={styles.eyebrow}>Tarot & symbolic reflection</p>
          <h1 className={styles.title}>Look at your question <em>from another angle.</em></h1>
          <p className={styles.lead}>
            Tarot and other divination systems can interrupt a rehearsed story, reveal a neglected angle,
            and give intuition something concrete to work against. Their value does not require pretending
            that a random draw is a guaranteed command from the future.
          </p>
          <div className={styles.actions}>
            <a className={styles.secondary} href="https://chaos-tarot.com/yes-no" target="_blank" rel="noopener noreferrer">
              Yes / No · Chaos Tarot sign-in <span aria-hidden="true">↗</span>
            </a>
            <a className={styles.primary} href="https://chaos-tarot.com/free-reading?source=apocky-divination" target="_blank" rel="noopener noreferrer">
              Try a free reading <span aria-hidden="true">↗</span>
            </a>
            <a className={styles.secondary} href="https://chaos-tarot.com/system-quiz?source=apocky-divination" target="_blank" rel="noopener noreferrer">
              Find your system <span aria-hidden="true">↗</span>
            </a>
            <Link className={styles.secondary} href="/spellcraft">Compose a symbolic phrase →</Link>
          </div>

          <section className={styles.section} aria-labelledby="systems-title">
            <div className={styles.sectionHead}>
              <h2 id="systems-title">Choose a way to explore.</h2>
              <p>Each symbolic grammar keeps its own identity. Cross-system synthesis compares patterns; it does not flatten differences.</p>
            </div>
            <div className={styles.grid4}>
              {SYSTEMS.map(([name, description], index) => (
                <article className={styles.card} key={name}>
                  <span className={styles.tag}>{String(index + 1).padStart(2, '0')}</span>
                  <h3>{name}</h3>
                  <p>{description}</p>
                </article>
              ))}
            </div>
          </section>

          <section className={styles.section} aria-labelledby="process-title">
            <div className={styles.sectionHead}>
              <h2 id="process-title">Put a reading to use.</h2>
              <p>Ask a clear question. Notice what a symbol brings to mind. Choose one response you can actually try, then reflect on what happened.</p>
            </div>
            <div className={styles.diagram}>
              <svg viewBox="0 0 760 350" role="img" aria-labelledby="div-loop-title div-loop-desc">
                <title id="div-loop-title">Reflective divination loop</title>
                <desc id="div-loop-desc">Question leads to symbol, symbol to interpretation, interpretation to choice, and choice to reflection.</desc>
                <defs>
                  <linearGradient id="div-line" x1="0" x2="1"><stop stopColor="#78e7ff" /><stop offset=".5" stopColor="#7b8fff" /><stop offset="1" stopColor="#c28cff" /></linearGradient>
                </defs>
                <path d="M125 175 C125 54 635 54 635 175 C635 296 125 296 125 175Z" fill="none" stroke="url(#div-line)" strokeWidth="2" opacity=".55" />
                {[
                  [125, 175, 'QUESTION'],
                  [300, 72, 'SYMBOL'],
                  [500, 72, 'INTERPRET'],
                  [635, 175, 'CHOOSE'],
                  [380, 278, 'REFLECT'],
                ].map(([x, y, label]) => (
                  <g key={String(label)}>
                    <circle cx={Number(x)} cy={Number(y)} r="46" fill="#07091f" stroke="url(#div-line)" />
                    <text x={Number(x)} y={Number(y) + 4} fill="#f5f3ff" fontFamily="ui-monospace, monospace" fontSize="11" fontWeight="700" textAnchor="middle">{label}</text>
                  </g>
                ))}
              </svg>
            </div>
            <p className={styles.truth}>
              <strong>Keep your judgment.</strong>
              <span>A symbolic reading can generate perspective. It cannot establish a medical diagnosis, legal conclusion, financial guarantee, another person’s hidden thoughts, or an unavoidable future.</span>
            </p>
          </section>

          <section className={styles.section} aria-labelledby="faq-title">
            <div className={styles.sectionHead}>
              <h2 id="faq-title">Common questions.</h2>
              <p>Direct answers, with room to go deeper.</p>
            </div>
            <div className={styles.grid3}>
              {faq.map((item) => (
                <article className={styles.card} key={item.question}><h3>{item.question}</h3><p>{item.answer}</p></article>
              ))}
            </div>
          </section>

          <section className={styles.section} aria-labelledby="continue-title">
            <div className={styles.sectionHead}>
              <h2 id="continue-title">Try a reading or learn a symbol.</h2>
              <p>Readings open on Chaos Tarot. You can also create a phrase or sigil here.</p>
            </div>
            <div className={styles.actions}>
              <a className={styles.primary} href="https://chaos-tarot.com/free-reading?source=apocky-divination-end" target="_blank" rel="noopener noreferrer">Begin free <span aria-hidden="true">↗</span></a>
              <a className={styles.secondary} href="https://chaos-tarot.com/glossary?source=apocky-divination" target="_blank" rel="noopener noreferrer">Open the divination glossary <span aria-hidden="true">↗</span></a>
              <Link className={styles.secondary} href="/atlas">Find another tool →</Link>
              <Link className={styles.secondary} href="/sigils">Make a sigil →</Link>
            </div>
          </section>
        </div>
      </main>
    </>
  );
};

export default Divination;
