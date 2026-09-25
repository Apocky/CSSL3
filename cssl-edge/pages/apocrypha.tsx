import Head from 'next/head';
import Link from 'next/link';
import type { GetServerSideProps, NextApiRequest } from 'next';
import { useEffect, useState } from 'react';
import { useSiteSession } from '@/components/hub/SiteSession';
import AccountChat from '@/components/apocrypha/AccountChat';
import { ChatThread } from '@/components/apocrypha/ChatThread';
import { requireBrainOwner } from '@/lib/brain/owner';
import { usesOwnerRuntime } from '@/lib/mobile/owner-runtime';
import styles from '@/styles/AccountChat.module.css';

export const ACCOUNT_SESSION_VISIBLE_DEADLINE_MS = 4_000;

interface ApocryphaPageProps { readonly ownerConversation: boolean; readonly frontDoor?: boolean }

export const getServerSideProps: GetServerSideProps<ApocryphaPageProps> = async ({ req, res }) => {
  res.setHeader('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Vary', 'Cookie, Authorization');
  res.setHeader('Referrer-Policy', 'no-referrer');
  res.setHeader('X-Robots-Tag', 'noindex, nofollow, noarchive, nosnippet');
  const owner = await requireBrainOwner(req as NextApiRequest);
  return { props: { ownerConversation: owner.ok && usesOwnerRuntime(owner.user) } };
};

// The same page as apocky.com's front door (pages/index.tsx): the conversation is the site, so
// the front door is indexable and self-canonical while /apocrypha stays a private alias.
export const getFrontDoorServerSideProps: GetServerSideProps<ApocryphaPageProps> = async (context) => {
  const result = await getServerSideProps(context);
  context.res.setHeader('X-Robots-Tag', 'index, follow');
  if ('props' in result) {
    const props = await result.props;
    return { props: { ...props, frontDoor: true } };
  }
  return result;
};

function AccountResolutionUnavailable(): JSX.Element {
  return <main id="main-content" className={styles.page}>
    <header className={styles.header}>
      <Link href="/hub" className={styles.brand} aria-label="Apocky hub"><span className="apx-brand-mark" aria-hidden="true" /></Link>
      <div className={styles.roomTitle}><h1>Apocrypha</h1><p>Room to think</p></div>
      <nav aria-label="Apocrypha navigation"><Link href="/download/apocrypha">Get the app</Link><Link href="/login?next=%2Fapocrypha">Sign in</Link></nav>
    </header>
    <section className={styles.welcome} role="alert" aria-labelledby="account-check-title">
      <span className={styles.eyebrow}>APOCRYPHA</span>
      <h2 id="account-check-title">Account check took too long.</h2>
      <p>Your conversation is still safe. Sign in again to reconnect, or create an account to begin.</p>
      <div className={styles.welcomeActions}>
        <Link href="/login?next=%2Fapocrypha" className={styles.primary}>Sign in to chat</Link>
        <Link href="/register?next=%2Fapocrypha" className={styles.secondary}>Create an account</Link>
      </div>
      <Link className={styles.phoneLink} href="/download/apocrypha">Apocrypha for iPhone and Android →</Link>
    </section>
  </main>;
}

export default function ApocryphaPage({ ownerConversation, frontDoor = false }: ApocryphaPageProps): JSX.Element {
  const session = useSiteSession();
  const [sessionTimedOut, setSessionTimedOut] = useState(false);
  // Owner decision 2026-09-25: the verified owner always gets the realtime ChatThread. A saved
  // member-chat draft in this browser no longer flips the owner onto the account surface.
  const displayOwner = session.ownerConversation === true
    && (ownerConversation || session.access === 'owner');
  useEffect(() => {
    if (session.access !== 'checking') {
      setSessionTimedOut(false);
      return undefined;
    }
    const deadline = setTimeout(() => { setSessionTimedOut(true); }, ACCOUNT_SESSION_VISIBLE_DEADLINE_MS);
    return () => { clearTimeout(deadline); };
  }, [session.access]);
  const showOwner = displayOwner;
  return <>
    <Head>
      {frontDoor ? <title>Apocky · Apocrypha</title> : <title>Apocrypha · Apocky</title>}
      <meta name="description" content="Chat with Apocrypha from your browser. Sign in to your Apocky account to keep your own conversations together." />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      {frontDoor
        ? <>
          <meta name="robots" content="index,follow" />
          <link rel="canonical" href="https://www.apocky.com/" />
          <meta property="og:title" content="Apocky · Apocrypha" />
          <meta property="og:description" content="A conversation. Room to think. Chat with Apocrypha, then explore the rest of Apocky at /hub." />
          <meta property="og:type" content="website" />
          <meta property="og:url" content="https://www.apocky.com/" />
          <meta property="og:site_name" content="Apocky" />
          <link rel="alternate" type="text/plain" href="/llms.txt" title="Apocky for language models and digital intelligences" />
        </>
        : <meta name="robots" content="noindex,nofollow,noarchive,nosnippet" />}
      <meta name="referrer" content="no-referrer" />
      <meta name="theme-color" content="#05060b" />
    </Head>
    {session.access === 'checking' && sessionTimedOut ? <AccountResolutionUnavailable />
      : showOwner ? <main id="main-content" aria-label="Apocrypha owner conversation" style={{ height: '100dvh', minHeight: 480, overflow: 'hidden' }}><ChatThread /></main> : <AccountChat onPendingChange={() => undefined} />}
  </>;
}
