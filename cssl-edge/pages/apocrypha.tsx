import Head from 'next/head';
import { useEffect, useRef, useState } from 'react';
import { openAccountPendingJournal } from '@/lib/mobile/chat-contract';
import { withDeadline } from '@/lib/apocrypha/deadline';
import { useSiteSession } from '@/components/hub/SiteSession';
import AccountChat from '@/components/apocrypha/AccountChat';
import { ChatThread } from '@/components/apocrypha/ChatThread';

export const ACCOUNT_JOURNAL_RESOLUTION_DEADLINE_MS = 4_000;

function loadPendingAccountTurn(account: string): Promise<unknown> {
  return withDeadline(
    openAccountPendingJournal().then(store => store.load(account)),
    ACCOUNT_JOURNAL_RESOLUTION_DEADLINE_MS,
  );
}

export default function ApocryphaPage(): JSX.Element {
  const session = useSiteSession();
  const displayOwner = session.ownerConversation === true;
  const account = session.authenticated ? session.subjectKey : null;
  const [pendingCheck, setPendingCheck] = useState<{ account: string; status: 'clear' | 'pending' | 'unavailable' } | null>(null);
  const controller = useRef<{ account: string; choice: 'owner' | 'account' } | null>(null);
  const [, redraw] = useState(0);
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
    {checkingSaved ? <main id="main-content" role="status"><p>Opening your saved conversation…</p></main>
      : showOwner ? <main id="main-content" aria-label="Apocrypha owner conversation" style={{ height: '100dvh', minHeight: 480, overflow: 'hidden' }}><ChatThread /></main> : <AccountChat onPendingChange={pending => {
        if (account && controller.current?.account === account) setPendingCheck({ account, status: pending ? 'pending' : 'clear' });
      }} />}
    {displayOwner && account && !checkingSaved && !showOwner ? <p><button type="button" disabled={pendingCheck?.status !== 'clear'}
      onClick={() => { void returnToOwner(); }}>Open your main conversation</button></p> : null}
  </>;
}
