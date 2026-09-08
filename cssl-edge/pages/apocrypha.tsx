import Head from 'next/head';
import Link from 'next/link';
import type { GetServerSideProps, NextApiRequest } from 'next';
import { useEffect, useRef, useState } from 'react';
import { withDeadline } from '@/lib/apocrypha/deadline';
import { readMemberChatPending } from '@/lib/apocrypha/member-chat-client';
import { useSiteSession } from '@/components/hub/SiteSession';
import AccountChat from '@/components/apocrypha/AccountChat';
import { ChatThread } from '@/components/apocrypha/ChatThread';
import { requireBrainOwner } from '@/lib/brain/owner';
import { usesOwnerRuntime } from '@/lib/mobile/owner-runtime';
import styles from '@/styles/AccountChat.module.css';

export const ACCOUNT_JOURNAL_RESOLUTION_DEADLINE_MS = 4_000;
export const ACCOUNT_SESSION_VISIBLE_DEADLINE_MS = 4_000;

interface ApocryphaPageProps { readonly ownerConversation: boolean }

export const getServerSideProps: GetServerSideProps<ApocryphaPageProps> = async ({ req, res }) => {
  res.setHeader('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Vary', 'Cookie, Authorization');
  res.setHeader('Referrer-Policy', 'no-referrer');
  res.setHeader('X-Robots-Tag', 'noindex, nofollow, noarchive, nosnippet');
  const owner = await requireBrainOwner(req as NextApiRequest);
  return { props: { ownerConversation: owner.ok && usesOwnerRuntime(owner.user) } };
};

function loadPendingAccountTurn(account: string): Promise<unknown> {
  return withDeadline(
    Promise.resolve().then(() => readMemberChatPending(account, window.localStorage)),
    ACCOUNT_JOURNAL_RESOLUTION_DEADLINE_MS,
  );
}

function AccountResolutionUnavailable(): JSX.Element {
  return <main id="main-content" className={styles.page}>
    <header className={styles.header}>
      <Link href="/" className={styles.brand} aria-label="Apocky home"><span className="apx-brand-mark" aria-hidden="true" /></Link>
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

export default function ApocryphaPage({ ownerConversation }: ApocryphaPageProps): JSX.Element {
  const session = useSiteSession();
  const [sessionTimedOut, setSessionTimedOut] = useState(false);
  const displayOwner = session.ownerConversation === true
    && (ownerConversation || session.access === 'owner');
  const account = session.authenticated ? session.subjectKey : null;
  const [pendingCheck, setPendingCheck] = useState<{ account: string; status: 'clear' | 'pending' | 'unavailable' } | null>(null);
  const controller = useRef<{ account: string; choice: 'owner' | 'account' } | null>(null);
  const [, redraw] = useState(0);
  useEffect(() => {
    if (session.access !== 'checking') {
      setSessionTimedOut(false);
      return undefined;
    }
    const deadline = setTimeout(() => { setSessionTimedOut(true); }, ACCOUNT_SESSION_VISIBLE_DEADLINE_MS);
    return () => { clearTimeout(deadline); };
  }, [session.access]);
  if (!account) controller.current = null;
  else if (controller.current?.account !== account) controller.current = { account, choice: displayOwner ? 'owner' : 'account' };
  useEffect(() => {
    let active = true;
    if (!account) return;
    void loadPendingAccountTurn(account).then(pending => {
      if (active) setPendingCheck({ account, status: pending ? 'pending' : 'clear' });
    }, () => { if (active) setPendingCheck({ account, status: 'unavailable' }); });
    return () => { active = false; };
  }, [account]);
  const checked = account !== null && pendingCheck?.account === account;
  if (checked && pendingCheck?.status !== 'clear' && controller.current) controller.current.choice = 'account';
  const showOwner = displayOwner && checked && pendingCheck?.status === 'clear' && controller.current?.choice === 'owner';
  const checkingSaved = displayOwner && (!account || !checked);
  const returnToOwner = async () => {
    if (!account || !displayOwner) return;
    try {
      const pending = await loadPendingAccountTurn(account);
      if (controller.current?.account !== account) return;
      setPendingCheck({ account, status: pending ? 'pending' : 'clear' });
      if (!pending) { controller.current.choice = 'owner'; redraw(value => value + 1); }
    } catch { if (controller.current?.account === account) setPendingCheck({ account, status: 'unavailable' }); }
  };
  return <>
    <Head>
      <title>Apocrypha · Apocky</title>
      <meta name="description" content="Chat with Apocrypha from your browser. Sign in to your Apocky account to keep your own conversations together." />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      <meta name="robots" content="noindex,nofollow,noarchive,nosnippet" />
      <meta name="referrer" content="no-referrer" />
      <meta name="theme-color" content="#05060b" />
    </Head>
    {session.access === 'checking' && sessionTimedOut ? <AccountResolutionUnavailable />
      : checkingSaved ? <main id="main-content" role="status"><p>Opening your saved conversation…</p></main>
      : showOwner ? <main id="main-content" aria-label="Apocrypha owner conversation" style={{ height: '100dvh', minHeight: 480, overflow: 'hidden' }}><ChatThread /></main> : <AccountChat onPendingChange={pending => {
        if (account && controller.current?.account === account) setPendingCheck({ account, status: pending ? 'pending' : 'clear' });
      }} />}
    {displayOwner && account && !checkingSaved && !showOwner ? <p><button type="button" disabled={pendingCheck?.status !== 'clear'}
      onClick={() => { void returnToOwner(); }}>Open your main conversation</button></p> : null}
  </>;
}
