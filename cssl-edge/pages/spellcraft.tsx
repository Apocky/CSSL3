import type { NextPage } from 'next';
import Head from 'next/head';

import ConversionBridge from '../components/spellcraft/ConversionBridge';
import SpellComposer from '../components/spellcraft/SpellComposer';
import neural from '../styles/NeuralPages.module.css';
import styles from '../styles/SymbolicStudio.module.css';

const SpellcraftPage: NextPage = () => (
  <>
    <Head>
      <title>Symbolic Spellcraft Engine · Compose, inspect, transform</title>
      <meta name="description" content="Build Haloic-derived symbolic spells in a transparent local engine. Inspect vocabulary, parse, effect graph, interpretation, confidence, and receipt." />
      <meta name="keywords" content="spell generator, custom spell engine, sigil maker, Haloic language, symbolic magic, chaos magic tools" />
      <meta property="og:title" content="Symbolic Spellcraft Engine · Apocky" />
      <meta property="og:description" content="Compose language. Inspect every transformation. Keep the result yours." />
      <meta property="og:url" content="https://www.apocky.com/spellcraft" />
      <link rel="canonical" href="https://www.apocky.com/spellcraft" />
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify({ '@context': 'https://schema.org', '@type': 'WebApplication', name: 'Apocky Symbolic Spellcraft Engine', url: 'https://www.apocky.com/spellcraft', applicationCategory: 'EntertainmentApplication', operatingSystem: 'Web', offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' } }) }} />
    </Head>
    <main className={`${neural.page} ${styles.studioPage}`}>
      <div className={neural.wrap}>
        <p className={neural.eyebrow}>Owner-authorized Haloic lineage · deterministic core</p>
        <h1 className={neural.title}>Words become structure. <em>You keep authority.</em></h1>
        <p className={neural.lead}>Compose with a versioned symbolic vocabulary, inspect the parser and non-executable graph, then decide whether the working deserves a sigil or a place in your local Spellbook.</p>
        <SpellComposer />
        <ConversionBridge source="spellcraft" />
        <p className={neural.truth}><strong>Effect boundary.</strong><span>The compiler creates immutable symbolic data for reflection. It executes no ritual, code, network request, notification, purchase, or real-world action and makes no claim of supernatural efficacy.</span></p>
      </div>
    </main>
  </>
);

export default SpellcraftPage;
