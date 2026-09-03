import type { NextPage } from 'next';
import Head from 'next/head';

import ConversionBridge from '../components/spellcraft/ConversionBridge';
import SigilStudio from '../components/spellcraft/SigilStudio';
import neural from '../styles/NeuralPages.module.css';
import styles from '../styles/SymbolicStudio.module.css';

const SigilsPage: NextPage = () => (
  <>
    <Head>
      <title>Deterministic Sigil Maker · Visible symbolic geometry</title>
      <meta name="description" content="Create downloadable sigil SVGs from validated symbolic forms. Every variant is deterministic, bounded, visible, and generated on your device." />
      <meta name="keywords" content="sigil maker, sigil generator, chaos magic sigil, custom sigil, symbolic geometry, spell sigil" />
      <meta property="og:title" content="Deterministic Sigil Studio · Apocky" />
      <meta property="og:description" content="Validated language becomes reproducible visible geometry—without hidden payloads." />
      <meta property="og:url" content="https://www.apocky.com/sigils" />
      <link rel="canonical" href="https://www.apocky.com/sigils" />
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify({ '@context': 'https://schema.org', '@type': 'WebApplication', name: 'Apocky Deterministic Sigil Studio', url: 'https://www.apocky.com/sigils', applicationCategory: 'DesignApplication', operatingSystem: 'Web', offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' }, featureList: ['Deterministic SVG generation', 'Visible geometry controls', 'Device-local processing', 'SVG download'] }) }} />
    </Head>
    <main className={`${neural.page} ${styles.studioPage}`}>
      <div className={neural.wrap}>
        <p className={neural.eyebrow}>Sigil studio · local SVG generator</p>
        <h1 className={neural.title}>Make a mark. <em>See how it was made.</em></h1>
        <p className={neural.lead}>The studio turns a validated symbolic program into a reproducible visual fingerprint. Change the visible variant, inspect its geometry contract, and download the SVG without sending the source anywhere.</p>
        <SigilStudio />
        <ConversionBridge source="sigils" />
      </div>
    </main>
  </>
);

export default SigilsPage;
