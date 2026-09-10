import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

import styles from '../styles/RecoveryPages.module.css';

const ServerError: NextPage = () => (
  <>
    <Head>
      <title>Something went wrong · Apocky</title>
      <meta name="robots" content="noindex,nofollow" />
    </Head>
    <main className={styles.page}>
      <div className={styles.wrap}>
        <p className={styles.eyebrow}>Page unavailable · 500</p>
        <h1 className={styles.title}>Something went wrong <em>loading this page.</em></h1>
        <p className={styles.lead}>Try loading it again. If that doesn’t help, check the site’s status or return to Home.</p>
        <div className={styles.actions}>
          <a className={styles.primary} href="">Try again ↻</a>
          <Link className={styles.secondary} href="/status">Check site status →</Link>
        </div>
        <nav className={styles.related} aria-label="Other ways to continue">
          <Link href="/">Back to Home</Link>
          <Link href="/codex-apockalypsis">Read the Codex</Link>
        </nav>
      </div>
    </main>
  </>
);

export default ServerError;
