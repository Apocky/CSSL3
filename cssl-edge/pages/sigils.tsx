import type { NextPage } from 'next';
import Head from 'next/head';

import ConversionBridge from '../components/spellcraft/ConversionBridge';
import SigilStudio from '../components/spellcraft/SigilStudio';
import styles from '../styles/SymbolicStudio.module.css';

const SigilsPage: NextPage = () => (
  <>
    <Head>
      <title>Make a sigil · Apocky</title>
      <meta name="description" content="Choose an intention, make a personal geometric sigil, and download the image. Free to use, right in your browser." />
      <meta name="keywords" content="sigil maker, sigil generator, chaos magic sigil, custom sigil, symbolic geometry, spell sigil" />
      <meta property="og:title" content="Make a sigil · Apocky" />
      <meta property="og:description" content="A meaning, made visible. Create a personal sigil and keep the image." />
      <meta property="og:url" content="https://www.apocky.com/sigils" />
      <link rel="canonical" href="https://www.apocky.com/sigils" />
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify({ '@context': 'https://schema.org', '@type': 'WebApplication', name: 'Apocky Deterministic Sigil Studio', url: 'https://www.apocky.com/sigils', applicationCategory: 'DesignApplication', operatingSystem: 'Web', offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' }, featureList: ['Deterministic SVG generation', 'Visible geometry controls', 'Device-local processing', 'SVG download'] }) }} />
    </Head>
    <main className={styles.studioPage}>
      <div className={styles.pageWrap}>
        <header className={styles.pageHeader}>
          <p className={styles.pageEyebrow}>Tools / Sigils</p>
          <h1 className={styles.pageTitle}>Make a mark that means something.</h1>
          <p className={styles.pageLead}>Choose a focus. Find a shape you like. Keep it as a reminder.</p>
        </header>
        <SigilStudio />
        <ConversionBridge source="sigils" />
      </div>
    </main>
  </>
);

export default SigilsPage;
