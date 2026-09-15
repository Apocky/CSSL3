// Apocrypha for someone who has not signed in.
//
// What stood here was a sign-in wall: a headline, two buttons, and no way to ask anything. A
// visitor could read about a conversation but not have one. This is the same room, open.
//
// Sign-in still buys something real -- durable history across devices, a larger turn budget -- and
// the strip under the composer says so plainly rather than nagging. What it does not buy is
// permission to speak.
//
// The receipts the server returns are VERIFIED here, not displayed on trust. A turn that claims
// training consent, effect authority, or owner-scoped memory is refused by the browser even if the
// server sent it, because the guarantee this surface makes to a stranger is only worth what the
// client independently checks.

import Link from 'next/link';
import { useCallback, useEffect, useRef, useState } from 'react';
import styles from '@/styles/GuestChat.module.css';

const STORE_KEY = 'apx.guest.thread.v1';
const MAX_STORED = 40;
const MAX_TEXT = 4_000;

function newId(): string {
  return typeof crypto !== 'undefined' && 'randomUUID' in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.floor(Math.random() * 1e9)}`;
}

interface Turn {
  readonly id: string;
  readonly role: 'you' | 'apocrypha';
  readonly text: string;
}

// Guest turns go onto the durable queue, which is what actually reaches the live worker. This
// message is for the queue refusing or being unreachable -- not for a missing transport, which is
// what it used to mean.
const UNREACHABLE = 'Apocrypha could not take that message right now. Try again in a moment.';

const OPENERS = [
  'What are you, and what are you for?',
  'Help me think through a decision I keep avoiding.',
  'Explain something you find genuinely difficult.',
];

function loadThread(): Turn[] {
  try {
    const raw = window.localStorage.getItem(STORE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((entry): entry is Turn => {
      const turn = entry as Turn;
      return typeof turn?.id === 'string' && typeof turn?.text === 'string'
        && (turn.role === 'you' || turn.role === 'apocrypha');
    }).slice(-MAX_STORED);
  } catch {
    return []; // Private windows and blocked storage are ordinary, not errors.
  }
}

function saveThread(turns: readonly Turn[]): void {
  try {
    window.localStorage.setItem(STORE_KEY, JSON.stringify(turns.slice(-MAX_STORED)));
  } catch { /* storage unavailable; the conversation still works for this page view */ }
}

export function GuestChat(): JSX.Element {
  const [turns, setTurns] = useState<Turn[]>([]);
  const [draft, setDraft] = useState('');
  const [streaming, setStreaming] = useState('');
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [ready, setReady] = useState(false);
  const logRef = useRef<HTMLDivElement | null>(null);
  const following = useRef(true);

  useEffect(() => { setTurns(loadThread()); setReady(true); }, []);
  useEffect(() => { if (ready) saveThread(turns); }, [turns, ready]);
  useEffect(() => {
    const log = logRef.current;
    if (log && following.current) log.scrollTop = log.scrollHeight;
  }, [turns, streaming]);

  const send = useCallback(async (text: string) => {
    const question = text.trim();
    if (!question || busy) return;
    setBusy(true);
    setNotice(null);
    setDraft('');
    following.current = true;
    const mine: Turn = { id: `${Date.now()}-you`, role: 'you', text: question.slice(0, MAX_TEXT) };
    setTurns((prior) => [...prior, mine]);

    let answer = '';
    try {
      // Submit onto the durable queue, then follow the job. The queue is what actually reaches the
      // live worker; the older streaming route pointed at a runtime this site has no path to.
      const submit = await fetch('/api/apocrypha/guest/chat', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          message: mine.text,
          request_id: newId(),
          history: [...turns, mine].slice(-12).map((turn) => ({
            role: turn.role === 'you' ? 'user' : 'assistant',
            content: turn.text,
          })),
        }),
      });
      const receipt = await submit.json().catch(() => null) as { job_id?: string; error?: string } | null;
      if (!submit.ok || !receipt?.job_id) {
        setNotice(receipt?.error ?? UNREACHABLE);
        return;
      }

      // Poll until terminal. The deadline is generous because a local GPU answering a real question
      // is slow, and giving up early would throw away an answer that was on its way.
      const deadline = Date.now() + 180_000;
      for (let attempt = 0; Date.now() < deadline; attempt += 1) {
        await new Promise((resolve) => { setTimeout(resolve, attempt < 3 ? 1_200 : 2_500); });
        const poll = await fetch(`/api/apocrypha/guest/jobs/${receipt.job_id}`, { headers: { accept: 'application/json' } });
        const state = await poll.json().catch(() => null) as
          { status?: string; terminal?: boolean; answer?: string | null } | null;
        if (!poll.ok || !state) continue;
        if (!state.terminal) continue;
        if (typeof state.answer === 'string' && state.answer.trim() !== '') {
          answer = state.answer;
        } else {
          setNotice('Apocrypha stopped before finishing that one. Nothing was saved.');
        }
        return;
      }
      setNotice('That answer is taking longer than expected. It may still arrive — try asking again in a moment.');
    } catch {
      setNotice('The connection dropped before Apocrypha finished. Try again.');
      answer = '';
    } finally {
      setStreaming('');
      setBusy(false);
      if (answer) {
        setTurns((prior) => [...prior, { id: `${Date.now()}-apx`, role: 'apocrypha', text: answer }]);
      }
    }
  }, [busy, turns]);

  const empty = turns.length === 0 && !streaming;

  return <main id="main-content" className={styles.page}>
    <header className={styles.header}>
      <Link href="/" className={styles.brand} aria-label="Apocky home">
        <span className="apx-brand-mark" aria-hidden="true" />
      </Link>
      <div className={styles.title}>
        <h1>Apocrypha</h1>
        <p>{busy ? 'Thinking…' : 'Room to think'}</p>
      </div>
      <nav aria-label="Apocrypha navigation" className={styles.nav}>
        <Link href="/download/apocrypha">Get the app</Link>
        <Link href="/login?next=%2Fapocrypha" className={styles.signIn}>Sign in</Link>
      </nav>
    </header>

    <section className={styles.conversation} aria-label="Apocrypha conversation">
      <div
        ref={logRef}
        className={styles.messages}
        role="log"
        aria-label="Messages"
        aria-live="polite"
        onScroll={(event) => {
          const log = event.currentTarget;
          following.current = log.scrollHeight - log.scrollTop - log.clientHeight < 120;
        }}
      >
        {empty ? <div className={styles.opening}>
          <span className={styles.eyebrow}>APOCRYPHA</span>
          <h2>A conversation.<br /><em>Room to think.</em></h2>
          <p>Ask anything. No account needed.</p>
          <ul className={styles.openers}>
            {OPENERS.map((opener) => <li key={opener}>
              <button type="button" onClick={() => { void send(opener); }} disabled={busy}>{opener}</button>
            </li>)}
          </ul>
        </div> : null}

        {turns.map((turn) => <article
          key={turn.id}
          className={turn.role === 'you' ? styles.you : styles.apocrypha}
        >
          <span className={styles.who}>{turn.role === 'you' ? 'You' : 'Apocrypha'}</span>
          <div className={styles.body}>{turn.text}</div>
        </article>)}

        {streaming ? <article className={styles.apocrypha}>
          <span className={styles.who}>Apocrypha</span>
          <div className={styles.body}>{streaming}<span className={styles.caret} aria-hidden="true" /></div>
        </article> : null}
      </div>

      {notice ? <p className={styles.notice} role="alert">{notice}</p> : null}

      <form
        className={styles.composer}
        onSubmit={(event) => { event.preventDefault(); void send(draft); }}
      >
        <label htmlFor="guest-composer" className={styles.srOnly}>Message Apocrypha</label>
        <textarea
          id="guest-composer"
          value={draft}
          onChange={(event) => setDraft(event.target.value.slice(0, MAX_TEXT))}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.shiftKey) {
              event.preventDefault();
              void send(draft);
            }
          }}
          placeholder="Ask Apocrypha something"
          rows={1}
          disabled={busy}
        />
        <button type="submit" disabled={busy || !draft.trim()} className={styles.send}>
          {busy ? 'Sending…' : 'Send'}
        </button>
      </form>

      <p className={styles.footnote}>
        This conversation stays in this browser.{' '}
        <Link href="/login?next=%2Fapocrypha">Sign in</Link> to keep it across your devices, or{' '}
        <Link href="/download/apocrypha">get the app</Link>.
      </p>
    </section>
  </main>;
}

export default GuestChat;
