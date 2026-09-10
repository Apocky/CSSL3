import type { NextPage } from 'next';
import Head from 'next/head';

import ConversionBridge from '../components/spellcraft/ConversionBridge';
import SpellbookPanel from '../components/spellcraft/SpellbookPanel';
import styles from '../styles/SymbolicStudio.module.css';

const SpellbookPage: NextPage = () => (
  <>
    <Head>
      <title>Your spellbook · Apocky</title>
      <meta name="description" content="Keep your saved reflections together. Open, export, or remove the spells stored in this browser." />
      <meta name="keywords" content="digital spellbook, private spell journal, sigil collection, symbolic spell archive" />
      <meta property="og:title" content="Local Spellbook · Apocky" />
      <meta property="og:description" content="Your explicitly saved symbolic workings, sealed and kept in this browser." />
      <meta property="og:url" content="https://www.apocky.com/spellbook" />
      <link rel="canonical" href="https://www.apocky.com/spellbook" />
      <meta name="robots" content="index,follow,max-image-preview:large" />
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify({ '@context': 'https://schema.org', '@type': 'WebApplication', name: 'Apocky Local Spellbook', url: 'https://www.apocky.com/spellbook', applicationCategory: 'ProductivityApplication', operatingSystem: 'Web', offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' }, featureList: ['Explicit local save', 'Integrity-checked import', 'Portable JSON export', 'Local deletion controls'] }) }} />
    </Head>
    <main className={styles.studioPage}>
      <div className={styles.pageWrap}>
        <header className={styles.pageHeader}>
          <p className={styles.pageEyebrow}>Tools / Spellbook</p>
          <h1 className={styles.pageTitle}>Your own small collection.</h1>
          <p className={styles.pageLead}>The reflections you chose to keep, saved in this browser. Export a copy to take them with you.</p>
        </header>
        <SpellbookPanel />
        <ConversionBridge source="spellbook" />
      </div>
    </main>
  </>
);

export default SpellbookPage;
