import type { NextPage } from 'next';
import Head from 'next/head';

import ConversionBridge from '../components/spellcraft/ConversionBridge';
import SpellComposer from '../components/spellcraft/SpellComposer';
import styles from '../styles/SymbolicStudio.module.css';

const SpellcraftPage: NextPage = () => (
  <>
    <Head>
      <title>Write a spell · Apocky</title>
      <meta name="description" content="Choose a meaning, create a personal reflection, and turn it into a sigil or save it in your private spellbook." />
      <meta name="keywords" content="spell generator, custom spell engine, sigil maker, Haloic language, symbolic magic, chaos magic tools" />
      <meta property="og:title" content="Write a spell · Apocky" />
      <meta property="og:description" content="Compose language. Inspect every transformation. Keep the result yours." />
      <meta property="og:url" content="https://www.apocky.com/spellcraft" />
      <link rel="canonical" href="https://www.apocky.com/spellcraft" />
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify({ '@context': 'https://schema.org', '@type': 'WebApplication', name: 'Apocky Symbolic Spellcraft Engine', url: 'https://www.apocky.com/spellcraft', applicationCategory: 'EntertainmentApplication', operatingSystem: 'Web', offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' } }) }} />
    </Head>
    <main className={styles.studioPage}>
      <div className={styles.pageWrap}>
        <header className={styles.pageHeader}>
          <p className={styles.pageEyebrow}>Tools / Spellcraft</p>
          <h1 className={styles.pageTitle}>A spell of your own.</h1>
          <p className={styles.pageLead}>Choose what matters. Make a reflection to keep, or a symbol to carry with you.</p>
        </header>
        <SpellComposer />
        <ConversionBridge source="spellcraft" />
      </div>
    </main>
  </>
);

export default SpellcraftPage;
