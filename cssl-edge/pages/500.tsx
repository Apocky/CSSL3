import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

import styles from '../styles/NeuralPages.module.css';

const ServerError: NextPage = () => (
  <>
    <Head>
      <title>Signal interrupted · Apocky</title>
      <meta name="robots" content="noindex,nofollow" />
    </Head>
    <main className={styles.page}>
      <div className={styles.wrap}>
        <p className={styles.eyebrow}>APX-SERVER-RENDER-FAILED · 500</p>
        <h1 className={styles.title}>The route exists. <em>The signal did not complete.</em></h1>
        <p className={styles.lead}>Refresh once. If the interruption remains, check the public observatory or continue through a static route while the failing connection recovers.</p>
        <div className={styles.actions}>
          <a className={styles.primary} href="">Refresh this route</a>
          <Link className={styles.secondary} href="/status">Check system status →</Link>
          <Link className={styles.secondary} href="/atlas">Use the Atlas →</Link>
        </div>
      </div>
    </main>
  </>
);

export default ServerError;
