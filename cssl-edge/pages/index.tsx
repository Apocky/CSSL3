import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { useSiteSession } from '../components/hub/SiteSession';
import SiteDirectory from '../components/site/SiteDirectory';
import { consumeAuthCallbackFromLocation, readAuthCallbackParams } from '../lib/auth-callback';
import { normalizeAuthReturnPath } from '../lib/auth-return';
import styles from '../styles/UsefulHub.module.css';

const structuredData = {
  '@context': 'https://schema.org',
  '@graph': [
    { '@type': 'WebSite', '@id': 'https://www.apocky.com/#website', name: 'Apocky', url: 'https://www.apocky.com/',
      description: 'Useful tools, words, thoughts, and stories from Shawn Apocky.',
      creator: { '@id': 'https://www.apocky.com/#shawn-apocky' },
      potentialAction: { '@type': 'SearchAction', target: 'https://www.apocky.com/atlas?q={search_term_string}', 'query-input': 'required name=search_term_string' } },
    { '@type': 'Person', '@id': 'https://www.apocky.com/#shawn-apocky', name: 'Shawn Apocky', url: 'https://www.apocky.com/' },
  ],
};

export default function Home(): JSX.Element {
  const [authNotice, setAuthNotice] = useState<string | null>(null);
  const { refresh } = useSiteSession();

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const callbackParams = readAuthCallbackParams(location.search, location.hash);
      if (!callbackParams.hasCallback) return;
      const returnTo = normalizeAuthReturnPath(new URLSearchParams(location.search).get('next'), '');
      setAuthNotice('Finishing your sign-in…');
      const callbackResult = await consumeAuthCallbackFromLocation();
      if (cancelled) return;
      if (callbackResult.ok) {
        if (returnTo) { location.replace(returnTo); return; }
        setAuthNotice('You are signed in.');
        await refresh();
      } else {
        setAuthNotice(`Sign-in failed: ${callbackResult.reason ?? 'please try again'}`);
      }
    })();
    return () => { cancelled = true; };
  }, [refresh]);

  return <>
    <Head>
      <title>Apocky · Tools, thoughts, and stories</title>
      <meta name="description" content="Make a sigil, find a meaning, explore an idea, or read Codex Apockalypsis. Useful tools and unusual thoughts from Shawn Apocky." />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      <meta name="theme-color" content="#101116" />
      <meta property="og:title" content="Apocky · Curiosity, put to use." />
      <meta property="og:description" content="Tools to try. Words to understand. Thoughts and stories to get lost in." />
      <meta property="og:type" content="website" />
      <meta property="og:url" content="https://www.apocky.com/" />
      <meta property="og:site_name" content="Apocky" />
      <meta name="twitter:card" content="summary_large_image" />
      <link rel="canonical" href="https://www.apocky.com/" />
      <link rel="alternate" type="text/plain" href="/llms.txt" title="Apocky for language models and digital intelligences" />
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }} />
    </Head>
    <main className={styles.home}>
      <section className={styles.welcome} aria-labelledby="home-title">
        <p className={styles.overline}>Shawn Apocky</p>
        <h1 id="home-title">Curiosity, <em>put to use.</em></h1>
        <p>Make a symbol. Find a meaning. Follow a thought somewhere new.</p>
        <p role="status" hidden={!authNotice}>{authNotice}</p>
      </section>
      <SiteDirectory />
      <aside className={styles.support}><p>If something here gave you a little wonder, you can help me make more.</p><Link href="/membership">Support the work →</Link></aside>
    </main>
  </>;
}
