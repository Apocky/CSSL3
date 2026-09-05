import type { NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';

import styles from '../styles/RecoveryPages.module.css';

const NotFound: NextPage = () => (
  <>
    <Head>
      <title>Page not found · Apocky</title>
      <meta name="robots" content="noindex,follow" />
    </Head>
    <main className={styles.page}>
      <div className={styles.wrap}>
        <p className={styles.eyebrow}>Page not found · 404</p>
        <h1 className={styles.title}>Let’s find your <em>next page.</em></h1>
        <p className={styles.lead}>This address doesn’t lead to an available page. Check the link, or start again from Home.</p>
        <div className={styles.actions}>
          <Link className={styles.primary} href="/">Back to Home →</Link>
          <Link className={styles.secondary} href="/tools">Find a useful tool →</Link>
        </div>
        <nav className={styles.related} aria-label="Other places to explore">
          <Link href="/words">Look up a word</Link>
          <Link href="/conversations">Explore thoughts</Link>
          <Link href="/codex-apockalypsis">Read the Codex</Link>
        </nav>
      </div>
    </main>
  </>
);

export default NotFound;
