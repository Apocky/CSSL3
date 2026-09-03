import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

import styles from '../styles/NeuralPages.module.css';

const NotFound: NextPage = () => (
  <>
    <Head>
      <title>Signal not found · Apocky</title>
      <meta name="robots" content="noindex,follow" />
    </Head>
    <main className={styles.page}>
      <div className={styles.wrap}>
        <p className={styles.eyebrow}>APX-ROUTE-NOT-FOUND · 404</p>
        <h1 className={styles.title}>That coordinate is not in the <em>public constellation.</em></h1>
        <p className={styles.lead}>The address may have moved, never existed, or belong to a route that is intentionally unavailable. Use a known public path instead.</p>
        <div className={styles.actions}>
          <Link className={styles.primary} href="/atlas">Search the Atlas →</Link>
          <Link className={styles.secondary} href="/start">Choose a path →</Link>
          <a className={styles.secondary} href="https://chaos-tarot.com/free-reading?source=apocky-404" target="_blank" rel="noopener noreferrer">Try Chaos Tarot ↗</a>
        </div>
      </div>
    </main>
  </>
);

export default NotFound;
