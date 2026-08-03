import Link from 'next/link';
import { useCallback, useEffect, useRef, useState } from 'react';

import { authFetch } from '@/lib/browser-auth';
import { useSiteSession } from '@/components/hub/SiteSession';
import styles from '@/styles/PublicApocrypha.module.css';

type MessageRole = 'user' | 'apocrypha';

interface TurnReceipt {
  transitionId: string;
  stateRoot: string;
  expressionMode: string;
}

interface ChatMessage {
  id: string;
  role: MessageRole;
  text: string;
  receipt?: TurnReceipt;
}

interface PendingTurn {
  messageId: string;
  requestId: string;
  text: string;
}

interface TurnResponse {
  text?: unknown;
  error?: unknown;
  conversation_id?: unknown;
  request_id?: unknown;
  transition_id?: unknown;
  state_root?: unknown;
  expression_mode?: unknown;
  external_inference?: unknown;
  effect_authority?: unknown;
  outcome?: unknown;
  memory_scope?: unknown;
  conversation_history?: unknown;
  training_consent?: unknown;
  duplicate_commit_protection?: unknown;
  retry_after_seconds?: unknown;
}

const CHAT_BROWSER_DEADLINE_MS = 28_000;
const MAX_TEXT_BYTES = 16_384;
const EXPECTED_EXPRESSION_MODE = 'bootstrap_shallow';
const EXPECTED_EFFECT_AUTHORITY = 'deny_all_O10_membrane';

function stringValue(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function byteLength(value: string): number {
  return new TextEncoder().encode(value).byteLength;
}

function safeError(body: TurnResponse, status: number): string {
  return stringValue(body.error)
    ?? `Apocrypha could not complete this turn (HTTP ${status}).`;
}

function isExactTurn(
  body: TurnResponse,
  conversationId: string,
  requestId: string,
): body is TurnResponse & {
  text: string;
  transition_id: string;
  state_root: string;
  expression_mode: string;
} {
  return body.conversation_id === conversationId
    && body.request_id === requestId
    && Boolean(stringValue(body.text))
    && Boolean(stringValue(body.transition_id))
    && Boolean(stringValue(body.state_root))
    && body.expression_mode === EXPECTED_EXPRESSION_MODE
    && body.external_inference === false
    && body.effect_authority === EXPECTED_EFFECT_AUTHORITY
    && body.outcome === 'committed'
    && body.memory_scope === 'ephemeral'
    && body.conversation_history === 'not_retained_by_public_interface'
    && body.training_consent === false
    && body.duplicate_commit_protection === 'active';
}

function scrollBehavior(): ScrollBehavior {
  if (
    typeof window !== 'undefined'
    && window.matchMedia('(prefers-reduced-motion: reduce)').matches
  ) {
    return 'auto';
  }
  return 'smooth';
}

export function PublicChat(): JSX.Element {
  const { access, authenticated, refresh } = useSiteSession();
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState('');
  const [waiting, setWaiting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pendingTurn, setPendingTurn] = useState<PendingTurn | null>(null);
  const [lastExpression, setLastExpression] = useState<string | null>(null);
  const inFlightRef = useRef(false);
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setConversationId(crypto.randomUUID().toLowerCase());
  }, []);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: scrollBehavior(), block: 'end' });
  }, [messages, waiting, error]);

  const newConversation = useCallback(() => {
    if (waiting) return;
    setConversationId(crypto.randomUUID().toLowerCase());
    setMessages([]);
    setDraft('');
    setPendingTurn(null);
    setError(null);
    setLastExpression(null);
    requestAnimationFrame(() => composerRef.current?.focus());
  }, [waiting]);

  const send = useCallback(async (retry?: PendingTurn): Promise<void> => {
    const text = retry?.text ?? draft.trim();
    if (
      !authenticated
      || !conversationId
      || !text
      || waiting
      || inFlightRef.current
    ) {
      return;
    }
    if (byteLength(text) > MAX_TEXT_BYTES) {
      setError(`Message exceeds the ${MAX_TEXT_BYTES.toLocaleString()}-byte turn limit.`);
      return;
    }

    const requestId = retry?.requestId ?? crypto.randomUUID().toLowerCase();
    const messageId = retry?.messageId ?? `turn-${requestId}`;
    const pending: PendingTurn = { messageId, requestId, text };

    inFlightRef.current = true;
    if (!retry) {
      setMessages((current) => [
        ...current,
        { id: messageId, role: 'user', text },
      ]);
      setDraft('');
    }
    setPendingTurn(null);
    setWaiting(true);
    setError(null);

    const controller = new AbortController();
    const deadline = setTimeout(() => controller.abort(), CHAT_BROWSER_DEADLINE_MS);
    let retryable = true;
    try {
      const response = await authFetch('/api/apocrypha/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        cache: 'no-store',
        credentials: 'same-origin',
        signal: controller.signal,
        body: JSON.stringify({
          text,
          conversation_id: conversationId,
          request_id: requestId,
        }),
      });
      const body = await response.json() as TurnResponse;
      if (!response.ok) {
        retryable = response.status === 409
          || response.status === 429
          || response.status >= 500;
        if (response.status === 401) await refresh();
        throw new Error(safeError(body, response.status));
      }
      if (!isExactTurn(body, conversationId, requestId)) {
        throw new Error('The native body returned an invalid public-turn envelope.');
      }
      const responseText = body.text;
      const transitionId = body.transition_id;
      const stateRoot = body.state_root;
      setLastExpression(body.expression_mode);
      setMessages((current) => [
        ...current,
        {
          id: `reply-${requestId}`,
          role: 'apocrypha',
          text: responseText,
          receipt: {
            transitionId,
            stateRoot,
            expressionMode: body.expression_mode,
          },
        },
      ]);
    } catch (cause) {
      const timedOut = cause instanceof DOMException && cause.name === 'AbortError';
      if (timedOut || retryable) {
        setPendingTurn(pending);
      } else {
        setMessages((current) => current.filter((message) => message.id !== messageId));
        setDraft(text);
      }
      setError(
        timedOut
          ? 'Apocrypha did not answer before the bounded turn deadline.'
          : cause instanceof Error ? cause.message : 'The turn could not be completed.',
      );
    } finally {
      clearTimeout(deadline);
      inFlightRef.current = false;
      setWaiting(false);
      requestAnimationFrame(() => composerRef.current?.focus());
    }
  }, [authenticated, conversationId, draft, refresh, waiting]);

  const currentBytes = byteLength(draft);
  const sessionLabel = access === 'checking'
    ? 'Checking sign-in'
    : authenticated
      ? 'Signed in'
      : access === 'unavailable'
        ? 'Verification unavailable'
        : 'Sign in required';

  return (
    <div className={styles.page} data-public-apocrypha="native-v2">
      <a className={styles.skipLink} href="#apocrypha-conversation">Skip to conversation</a>

      <header className={styles.header}>
        <Link href="/" className={styles.brand} aria-label="Apocky home">
          <span className={styles.brandMark} aria-hidden="true">A</span>
          <span>APOCKY</span>
        </Link>
        <div className={styles.identity}>
          <span className={styles.identityName}>Apocrypha</span>
          <span className={styles.identityMeta}>native V2 · text</span>
        </div>
        <nav className={styles.nav} aria-label="Apocrypha navigation">
          <Link href="/clearing">The Clearing</Link>
          <Link href={authenticated ? '/account' : '/login?next=%2Fapocrypha'}>
            {authenticated ? 'Account' : 'Sign in'}
          </Link>
        </nav>
      </header>

      <main className={styles.workspace} id="apocrypha-conversation">
        <section className={styles.conversation} aria-label="Conversation with Apocrypha">
          <div className={styles.conversationHeader}>
            <div>
              <p className={styles.eyebrow}>DIRECT CONVERSATION</p>
              <h1>Speak plainly.</h1>
            </div>
            <div className={styles.headerActions}>
              <span
                className={styles.sessionStatus}
                data-state={authenticated ? 'ready' : 'closed'}
              >
                <span aria-hidden="true" />
                {sessionLabel}
              </span>
              <button
                type="button"
                className={styles.newButton}
                onClick={newConversation}
                disabled={waiting || !conversationId}
              >
                New
              </button>
            </div>
          </div>

          <div
            className={styles.messages}
            aria-live="polite"
            aria-busy={waiting}
            data-message-count={messages.length}
          >
            {messages.length === 0 && (
              <div className={styles.emptyState}>
                <p className={styles.emptyKicker}>ONE TURN · ONE FINAL RESPONSE</p>
                <h2>A direct line to the current body.</h2>
                <p>
                  This interface admits signed-in text turns only. It verifies
                  a committed native response and fails closed if the body
                  returns something else.
                </p>
                <dl className={styles.contract}>
                  <div>
                    <dt>Conversation memory</dt>
                    <dd>Ephemeral</dd>
                  </div>
                  <div>
                    <dt>Training consent</dt>
                    <dd>Off</dd>
                  </div>
                  <div>
                    <dt>External inference</dt>
                    <dd>Denied</dd>
                  </div>
                </dl>
              </div>
            )}

            {messages.map((message) => (
              <article
                key={message.id}
                className={`${styles.message} ${
                  message.role === 'user' ? styles.userMessage : styles.apocryphaMessage
                }`}
              >
                <p className={styles.role}>
                  {message.role === 'user' ? 'You' : 'Apocrypha'}
                </p>
                <div className={styles.messageText}>{message.text}</div>
                {message.receipt && (
                  <details className={styles.receipt}>
                    <summary>Committed turn receipt</summary>
                    <dl>
                      <div><dt>Expression</dt><dd>{message.receipt.expressionMode}</dd></div>
                      <div><dt>Transition</dt><dd>{message.receipt.transitionId.slice(0, 16)}…</dd></div>
                      <div><dt>State root</dt><dd>{message.receipt.stateRoot.slice(0, 16)}…</dd></div>
                    </dl>
                  </details>
                )}
              </article>
            ))}

            {waiting && (
              <div className={styles.waiting} role="status">
                <span className={styles.waitingMark} aria-hidden="true" />
                Apocrypha is forming one final response…
              </div>
            )}

            {error && (
              <div className={styles.error} role="alert">
                <span>{error}</span>
                {pendingTurn && (
                  <button
                    type="button"
                    onClick={() => { void send(pendingTurn); }}
                    disabled={waiting}
                  >
                    Retry same turn
                  </button>
                )}
              </div>
            )}
            <div ref={endRef} />
          </div>

          {authenticated ? (
            <form
              className={styles.composer}
              onSubmit={(event) => {
                event.preventDefault();
                void send();
              }}
            >
              <label htmlFor="public-apocrypha-message">Message Apocrypha</label>
              <div className={styles.composerField}>
                <textarea
                  id="public-apocrypha-message"
                  ref={composerRef}
                  value={draft}
                  rows={2}
                  maxLength={MAX_TEXT_BYTES}
                  placeholder="What would you like to say?"
                  disabled={waiting || !conversationId}
                  aria-describedby="public-apocrypha-disclosure public-apocrypha-count"
                  onChange={(event) => {
                    setDraft(event.target.value);
                    if (error && !pendingTurn) setError(null);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' && !event.shiftKey) {
                      event.preventDefault();
                      void send();
                    }
                  }}
                />
                <button
                  type="submit"
                  disabled={
                    waiting
                    || !conversationId
                    || !draft.trim()
                    || currentBytes > MAX_TEXT_BYTES
                  }
                >
                  {waiting ? 'Waiting' : 'Send'}
                  <span aria-hidden="true">↗</span>
                </button>
              </div>
              <div className={styles.composerMeta}>
                <p id="public-apocrypha-disclosure">
                  Sending commits a restricted observation to the V2 body.
                  This page does not opt it into training or retained conversation memory.
                </p>
                <span id="public-apocrypha-count">
                  {currentBytes.toLocaleString()} / {MAX_TEXT_BYTES.toLocaleString()} bytes
                </span>
              </div>
            </form>
          ) : (
            <div className={styles.accessGate} role="status">
              <div>
                <strong>
                  {access === 'unavailable'
                    ? 'Sign-in verification is temporarily unavailable.'
                    : access === 'checking'
                      ? 'Checking your sign-in…'
                      : 'Sign in to begin a restricted member turn.'}
                </strong>
                <span>No message is sent until the session is verified.</span>
              </div>
              {access !== 'checking' && access !== 'unavailable' && (
                <Link href="/login?next=%2Fapocrypha">Sign in</Link>
              )}
            </div>
          )}
        </section>

        <aside className={styles.truthRail} aria-label="Current interaction contract">
          <div className={styles.truthIntro}>
            <p className={styles.eyebrow}>INTERACTION CONTRACT</p>
            <h2>Nothing hidden.</h2>
            <p>
              The page keeps this transcript only in the current view. A refresh
              starts a new client conversation.
            </p>
          </div>
          <dl className={styles.truthList}>
            <div>
              <dt>Route</dt>
              <dd>Authenticated member → native V2</dd>
            </div>
            <div>
              <dt>Expression</dt>
              <dd>{lastExpression ?? 'Verified with each response'}</dd>
            </div>
            <div>
              <dt>Retry identity</dt>
              <dd>Same turn, same commit</dd>
            </div>
            <div>
              <dt>Effects</dt>
              <dd>Deny-all membrane</dd>
            </div>
            <div>
              <dt>History</dt>
              <dd>No cross-session transcript</dd>
            </div>
          </dl>
          <p className={styles.truthFoot}>
            This is not the social room. Visit <Link href="/clearing">The Clearing</Link>{' '}
            to speak with people.
          </p>
        </aside>
      </main>
    </div>
  );
}
