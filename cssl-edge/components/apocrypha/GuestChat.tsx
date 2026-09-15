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
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import styles from '@/styles/GuestChat.module.css';

const STORE_KEY = 'apx.guest.thread.v1';
const MAX_STORED = 40;
const MAX_TEXT = 4_000;

interface Turn {
  readonly id: string;
  readonly role: 'you' | 'apocrypha';
  readonly text: string;
}

const OPENERS = [
  'What are you, and what are you for?',
  'Help me think through a decision I keep avoiding.',
  'Explain something you find genuinely difficult.',
];

/**
 * The guarantees this surface makes to a signed-out visitor, checked against what the server
 * actually returned. A mismatch is a refusal, not a warning: the point of a receipt is that it can
 * fail.
 */
function receiptFailure(result: Record<string, unknown>): string | null {
  if (result.training_consent !== false) return 'training_consent';
  if (result.effect_authority !== 'NONE') return 'effect_authority';
  if (result.tool_authority !== 'READ_ONLY_CONTEXT') return 'tool_authority';
  if (result.memory_scope !== 'public_safe_retrieval') return 'memory_scope';
  const identity = result.identity as Record<string, unknown> | undefined;
  if (!identity || identity.system_id !== 'apocrypha') return 'identity';
  return null;
}

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
  const sessionId = useMemo(
    () => (typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : `${Date.now()}`),
    [],
  );

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
      const response = await fetch('/api/apocrypha/chat', {
        method: 'POST',
        headers: { 'content-type': 'application/json', accept: 'application/x-ndjson' },
        body: JSON.stringify({
          text: mine.text,
          session_id: sessionId,
          request_id: typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : `${Date.now()}-r`,
        }),
      });

      if (response.status === 429) {
        const retry = Number(response.headers.get('retry-after') ?? 30);
        setNotice(`You have reached the short turn budget for right now. Try again in about ${Math.max(1, retry)} seconds, or sign in for more room.`);
        return;
      }
      if (!response.ok || !response.body) {
        setNotice('Apocrypha could not answer that one. Try again in a moment.');
        return;
      }

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      let verified = false;
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() ?? '';
        for (const line of lines) {
          if (!line.trim()) continue;
          let frame: Record<string, unknown>;
          try { frame = JSON.parse(line) as Record<string, unknown>; } catch { continue; }
          if (frame.type === 'delta' && typeof frame.text === 'string') {
            answer += frame.text;
            setStreaming(answer);
          } else if (frame.type === 'completed') {
            const result = (frame.result ?? {}) as Record<string, unknown>;
            const failure = receiptFailure(result);
            if (failure) {
              setNotice(`That answer did not carry the guarantees this page makes (${failure}), so it was discarded.`);
              answer = '';
              return;
            }
            verified = true;
            if (typeof result.text === 'string' && result.text) answer = result.text;
          } else if (frame.type === 'error') {
            setNotice('Apocrypha could not answer that one. Try again in a moment.');
            answer = '';
            return;
          }
        }
      }
      if (!verified) {
        // A stream that ended without a verified terminal frame is an unfinished turn, not an
        // answer. Showing the partial text as if it were complete is how a truncation becomes a
        // quote.
        setNotice('That answer was cut off before it finished. Nothing was saved.');
        answer = '';
      }
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
  }, [busy, sessionId]);

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
