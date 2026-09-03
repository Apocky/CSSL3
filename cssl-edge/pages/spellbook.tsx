import type { NextPage } from 'next';
import Head from 'next/head';

import ConversionBridge from '../components/spellcraft/ConversionBridge';
import SpellbookPanel from '../components/spellcraft/SpellbookPanel';
import neural from '../styles/NeuralPages.module.css';
import styles from '../styles/SymbolicStudio.module.css';

const SpellbookPage: NextPage = () => (
  <>
    <Head>
      <title>Local Spellbook · Save and export symbolic workings</title>
      <meta name="description" content="A private browser-local spellbook for explicitly saved symbolic workings, with integrity receipts, verified JSON import, export, and deletion." />
      <meta name="keywords" content="digital spellbook, private spell journal, sigil collection, symbolic spell archive" />
      <meta property="og:title" content="Local Spellbook · Apocky" />
      <meta property="og:description" content="Your explicitly saved symbolic workings, sealed and kept in this browser." />
      <meta property="og:url" content="https://www.apocky.com/spellbook" />
      <link rel="canonical" href="https://www.apocky.com/spellbook" />
      <meta name="robots" content="index,follow,max-image-preview:large" />
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify({ '@context': 'https://schema.org', '@type': 'WebApplication', name: 'Apocky Local Spellbook', url: 'https://www.apocky.com/spellbook', applicationCategory: 'ProductivityApplication', operatingSystem: 'Web', offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' }, featureList: ['Explicit local save', 'Integrity-checked import', 'Portable JSON export', 'Local deletion controls'] }) }} />
    </Head>
    <main className={`${neural.page} ${styles.studioPage}`}>
      <div className={neural.wrap}>
        <p className={neural.eyebrow}>Device-local memory · explicit actions only</p>
        <h1 className={neural.title}>Keep the working. <em>Keep the keys.</em></h1>
        <p className={neural.lead}>This Spellbook is a private shelf in the current browser. Save deliberately, verify lineage, export a portable copy, and delete any or all entries without asking permission.</p>
        <SpellbookPanel />
        <ConversionBridge source="spellbook" />
      </div>
    </main>
  </>
);

export default SpellbookPage;
