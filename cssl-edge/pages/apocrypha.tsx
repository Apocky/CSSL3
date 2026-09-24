// /apocrypha -- the room. One lane, no account, no gate.
//
// WHAT THIS PAGE USED TO DO, and why none of it is here any more.
//
// It resolved the reader's account on the client, picked one of three lanes from the answer
// (owner, member, guest), ran a 4-second deadline against that resolution, and if the deadline
// expired it replaced the entire room with a sign-in wall -- "Account check took too long", "Sign
// in to chat", "Create an account". It also asked the server on every request whether the reader
// was the brain owner, which made the page uncacheable.
//
// All of that machinery existed to decide WHO you are before deciding WHETHER you may speak. The
// room does not need to know. The guest lane answers questions without an account, which this
// page's own description has said all along -- so the account check was gating a door that was
// already open, and when the check was slow it closed it.
//
// Apocky, 2026-09-20: "The entire flow is too complicated for now just exclude sign-in."
//
// So: one lane, no session hook, no deadline, no fallback state, no getServerSideProps.
// There is nothing left to stall on.
// Sign-in still exists elsewhere on the site; it is simply not in the way of talking.

import Head from 'next/head';
import Link from 'next/link';
import { useMemo } from 'react';

import ApocryphaChat from '@/components/apocrypha/ApocryphaChat';
import { guestLane } from '@/lib/apocrypha/chat-lanes';

export default function ApocryphaPage(): JSX.Element {
  // Stable for the life of the page, and built INSIDE the component. The first version put this
  // at module scope, which is stabler still and wrong: it calls guestLane() at import time, so a
  // test that mocks the lane module gets "guestLane is not a function" before its mock is even
  // installed. Import-time work is not free just because it looks like a constant.
  const lane = useMemo(() => guestLane(), []);

  return <>
    <Head>
      <title>Apocrypha · Apocky</title>
      <meta name="description" content="Talk to Apocrypha in your browser. No account, no sign-in: ask a question and it answers." />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      <meta name="robots" content="index,follow" />
      <meta name="referrer" content="no-referrer" />
      <meta name="theme-color" content="#05060b" />
    </Head>
    <p style={{ margin: 0, padding: '6px 16px', background: '#05060b', color: '#a9b5ffb0', fontSize: 13, textAlign: 'center' }}><Link href="/room" style={{ color: '#d2e6fa' }}>Enter the living room</Link> — it may speak first.</p>
    <ApocryphaChat lane={lane} signedIn={false} height="calc(100dvh - 33px)" />
  </>;
}
