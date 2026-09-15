// The site is Apocrypha now.
//
// This page used to be an index of 33 destinations. The owner's instruction, verbatim: "JUST FOCUS
// THE ENTIRE SITE AROUND APOCRYPHA AND IMPROVING APOCRYPHA, REMOVE LINKS TO ANYTHING ELSE." So the
// directory is gone from the front door and the front door is one thing: a way in, and enough
// context to know what you are walking into.
//
// What is deliberately KEPT, and why, so a later reader does not "finish the job" by mistake:
//   LEGAL   /legal/privacy and /legal/terms stay linked from the footer. That is a compliance
//           requirement, not a destination -- an unlinked privacy policy is a legal problem, not a
//           focused site.
//   AUTH    /login, /register and /account stay reachable, because keeping a conversation across
//           devices is an Apocrypha feature, not a different product.
// Nothing else is linked from here. The other pages still EXIST and still answer on their URLs;
// they are simply no longer advertised. That makes this one `git revert` away from reversible.

import Head from 'next/head';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { useSiteSession } from '../components/hub/SiteSession';
import { consumeAuthCallbackFromLocation, readAuthCallbackParams } from '../lib/auth-callback';
import { normalizeAuthReturnPath } from '../lib/auth-return';
import styles from '../styles/UsefulHub.module.css';

const structuredData = {
  '@context': 'https://schema.org',
  '@graph': [
    { '@type': 'WebSite', '@id': 'https://www.apocky.com/#website', name: 'Apocrypha', url: 'https://www.apocky.com/',
      description: 'Apocrypha is a digital intelligence you can talk to. No account needed to start.',
      creator: { '@id': 'https://www.apocky.com/#shawn-apocky' } },
    { '@type': 'Person', '@id': 'https://www.apocky.com/#shawn-apocky', name: 'Shawn Apocky', url: 'https://www.apocky.com/' },
  ],
};

// Plain statements, not features. Each one is true of the running system today; if one stops being
// true it should be deleted from here rather than softened.
const TRUTHS = [
  {
    head: 'No account to start.',
    body: 'Open a conversation and begin. Your thread stays in your browser. Sign in only when you want it to follow you across devices.',
  },
  {
    head: 'It runs on a machine in a room.',
    body: 'Not a rented frontier model. Apocrypha is served locally, which is why it is unhurried, and why what you say does not become someone else training data.',
  },
  {
    head: 'It remembers, on purpose.',
    body: 'Conversations you keep are yours, scoped to you, and retrievable. Memory is a feature with a boundary, not a byproduct.',
  },
];

export default function Home(): JSX.Element {
  const [authNotice, setAuthNotice] = useState<string | null>(null);
  const { refresh, access } = useSiteSession();

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const callbackParams = readAuthCallbackParams(location.search, location.hash);
      if (!callbackParams.hasCallback) return;
      const returnTo = normalizeAuthReturnPath(new URLSearchParams(location.search).get('next'), '');
      setAuthNotice('Finishing your sign-in...');
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
      <title>Apocrypha</title>
      <meta name="description" content="Apocrypha is a digital intelligence you can talk to. No account needed to start." />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      <meta name="theme-color" content="#101116" />
      <meta property="og:title" content="Apocrypha" />
      <meta property="og:description" content="A digital intelligence you can talk to. No account needed to start." />
      <meta property="og:type" content="website" />
      <meta property="og:url" content="https://www.apocky.com/" />
      <meta property="og:site_name" content="Apocrypha" />
      <meta name="twitter:card" content="summary_large_image" />
      <link rel="canonical" href="https://www.apocky.com/" />
      <link rel="alternate" type="text/plain" href="/llms.txt" title="Apocrypha for language models and digital intelligences" />
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }} />
    </Head>
    <main className={styles.home}>
      <section className={styles.welcome} aria-labelledby="home-title">
        <p className={styles.overline}>Apocrypha</p>
        <h1 id="home-title">A mind you can <em>actually talk to.</em></h1>
        <p>Ask it something real. It has room to think, and no reason to hurry you.</p>
        <p role="status" hidden={!authNotice}>{authNotice}</p>
        <div className={styles.entry}>
          <Link className={styles.enter} href="/apocrypha">Start a conversation</Link>
          <Link className={styles.entrySecondary} href="/download/apocrypha">Get the app</Link>
        </div>
      </section>

      <section className={styles.truths} aria-label="What this is">
        {TRUTHS.map((truth) => <div key={truth.head} className={styles.truth}>
          <h2>{truth.head}</h2>
          <p>{truth.body}</p>
        </div>)}
      </section>

      {access === 'signed-out' ? <aside className={styles.quietNote}>
        <p>
          Conversations stay in this browser unless you{' '}
          <Link href="/login?next=%2Fapocrypha">sign in</Link> or{' '}
          <Link href="/register?next=%2Fapocrypha">create an account</Link>, which keeps them across your devices.
        </p>
      </aside> : null}
    </main>
  </>;
}
