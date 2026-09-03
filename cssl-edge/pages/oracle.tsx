import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

import YesNoOracle from '../components/oracle/YesNoOracle';
import neural from '../styles/NeuralPages.module.css';
import styles from '../styles/SymbolicStudio.module.css';

const OraclePage: NextPage = () => {
  const structuredData = {
    '@context': 'https://schema.org',
    '@type': 'WebApplication',
    name: 'Apocky Yes / No Oracle',
    url: 'https://www.apocky.com/oracle',
    applicationCategory: 'EntertainmentApplication',
    operatingSystem: 'Any modern web browser',
    offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
    description: 'A private, device-local yes or no reflection tool with reproducible receipts and explicit limits.',
  };

  return (
    <>
      <Head>
        <title>Yes / No Oracle · Ask one clear question</title>
        <meta name="description" content="Ask a quick yes or no question and receive a private, device-local symbolic signal plus a useful counter-question." />
        <meta name="keywords" content="yes no oracle, yes or no tarot, quick divination, decision reflection tool, symbolic oracle" />
        <meta property="og:title" content="Yes / No Oracle · Apocky" />
        <meta property="og:description" content="One question. One symbolic signal. Your judgment stays in charge." />
        <meta property="og:url" content="https://www.apocky.com/oracle" />
        <link rel="canonical" href="https://www.apocky.com/oracle" />
        <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }} />
      </Head>
      <main className={`${neural.page} ${styles.studioPage}`}>
        <div className={neural.wrap}>
          <p className={neural.eyebrow}>Zero-account oracle · private by default</p>
          <h1 className={neural.title}>Ask quickly. <em>Keep your agency.</em></h1>
          <p className={neural.lead}>
            When a decision is looping, a clean yes or no can expose your reaction. The signal is symbolic,
            generated in this browser, and deliberately paired with a counterweight—not presented as fate.
          </p>
          <YesNoOracle />

          <section className={neural.section} aria-labelledby="oracle-next-title">
            <div className={neural.sectionHead}>
              <h2 id="oracle-next-title">A signal is the beginning.</h2>
              <p>Turn the reaction into language, a sigil, or a deeper multi-system reading.</p>
            </div>
            <div className={neural.grid3}>
              <article className={neural.card}><span className={neural.tag}>COMPOSE</span><h3>Spellcraft</h3><p>Build a bounded symbolic intention and inspect every morpheme before saving it.</p><Link className={neural.cardLink} href="/spellcraft">Open the engine →</Link></article>
              <article className={neural.card}><span className={neural.tag}>DRAW</span><h3>Sigil studio</h3><p>Generate deterministic visible geometry from validated symbolic structure.</p><Link className={neural.cardLink} href="/sigils">Craft a sigil →</Link></article>
              <article className={neural.card}><span className={neural.tag}>DEEPEN</span><h3>Chaos Tarot</h3><p>Move from one bit to a richer reading across several symbolic systems.</p><a className={neural.cardLink} href="https://chaos-tarot.com/free-reading?source=apocky-oracle" target="_blank" rel="noopener noreferrer">Begin free ↗</a></article>
            </div>
          </section>

          <p className={neural.truth}><strong>Reality boundary.</strong><span>This is a reflective creative tool. It cannot predict a guaranteed future or replace medical, legal, financial, safety, or mental-health expertise.</span></p>
        </div>
      </main>
    </>
  );
};

export default OraclePage;
