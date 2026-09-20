// /apocrypha — the room.
//
// This page used to choose between three chat components and carried a good deal of machinery to
// do it: a pending-turn probe, a deadline, a ref holding which surface had won, and a button to
// climb back out of the one you had been dropped into. All of that existed to decide WHICH chat to
// render. There is one now, so the decision — and its machinery — is gone.
//
// What remains is the only thing that genuinely varies: which lane the reader is entitled to.

import Head from 'next/head';
import Link from 'next/link';
import type { GetServerSideProps, NextApiRequest } from 'next';
import { useEffect, useMemo, useState } from 'react';

import ApocryphaChat from '@/components/apocrypha/ApocryphaChat';
import { useSiteSession } from '@/components/hub/SiteSession';
import { guestLane, memberLane, ownerLane } from '@/lib/apocrypha/chat-lanes';
import { authFetch } from '@/lib/browser-auth';
import { requireBrainOwner } from '@/lib/brain/owner';
import { usesOwnerRuntime } from '@/lib/mobile/owner-runtime';
import styles from '@/styles/AccountChat.module.css';

export const ACCOUNT_SESSION_VISIBLE_DEADLINE_MS = 4_000;

interface ApocryphaPageProps { readonly ownerConversation: boolean }

export const getServerSideProps: GetServerSideProps<ApocryphaPageProps> = async ({ req, res }) => {
  res.setHeader('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Vary', 'Cookie, Authorization');
  res.setHeader('Referrer-Policy', 'no-referrer');
  // Indexable now that the room is open. A page nobody can find is not publicly functional.
  // The response stays private/no-store because the SIGNED-IN view is per-account; only the
  // signed-out room is meant to be discoverable.
  res.setHeader('X-Robots-Tag', 'index, follow');
  const owner = await requireBrainOwner(req as NextApiRequest);
  return { props: { ownerConversation: owner.ok && usesOwnerRuntime(owner.user) } };
};

// The account check stalling is not a reason to demand an account. This used to render a wall --
// "Sign in to chat" and "Create an account" -- which was the opposite of what the room is for: the
// guest lane needs no account and the page's own description says so. A reader who arrives while
// session resolution is slow got told to go and authenticate to reach something that was already
// free. Now the slow path drops into the guest room and the sign-in offer sits beside it as an
// offer, which is what it always should have been.
function AccountCheckSlowNotice(): JSX.Element {
  return <p className={styles.phoneLink} role="status">
    Still checking your account. You can start talking now without one --{' '}
    <Link href="/login?next=%2Fapocrypha">sign in</Link> whenever you want your conversations kept
    across devices.
  </p>;
}

export default function ApocryphaPage({ ownerConversation }: ApocryphaPageProps): JSX.Element {
  const session = useSiteSession();
  const [sessionTimedOut, setSessionTimedOut] = useState(false);

  useEffect(() => {
    if (session.access !== 'checking') { setSessionTimedOut(false); return undefined; }
    const deadline = setTimeout(() => { setSessionTimedOut(true); }, ACCOUNT_SESSION_VISIBLE_DEADLINE_MS);
    return () => { clearTimeout(deadline); };
  }, [session.access]);

  const owner = session.ownerConversation === true && (ownerConversation || session.access === 'owner');
  const account = session.authenticated ? session.subjectKey : null;

  // Rebuilt only when the entitlement actually changes. A fresh lane object on every render would
  // restart the poll loop that follows it.
  const lane = useMemo(
    () => (owner ? ownerLane(authFetch) : account ? memberLane(authFetch) : guestLane()),
    [account, owner],
  );

  return <>
    <Head>
      <title>Apocrypha · Apocky</title>
      <meta name="description" content="Talk to Apocrypha in your browser. No account needed to ask a question; sign in to keep your conversations across devices." />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      <meta name="robots" content="index,follow" />
      <meta name="referrer" content="no-referrer" />
      <meta name="theme-color" content="#05060b" />
    </Head>
    {session.access === 'checking' && sessionTimedOut ? <AccountCheckSlowNotice /> : null}
    <ApocryphaChat lane={lane} signedIn={session.authenticated} />
  </>;
}
