// Minimal V2 chat: one authenticated REST turn, one client-owned UUID, and no
// predecessor history or simulated streaming.

import React, { useCallback, useEffect, useRef, useState } from 'react';

import { authFetch } from '../../lib/browser-auth';
import { VisionPanel } from './VisionPanel';

type MessageRole = 'user' | 'apocrypha';

interface ChatMessage {
  id: string;
  role: MessageRole;
  text: string;
}

interface RetryTurn {
  text: string;
  requestId: string;
}

interface PersistedRetryTurn extends RetryTurn {
  conversationId: string;
}

interface TurnResponse {
  text?: unknown;
  error?: unknown;
  detail?: unknown;
  conversation_id?: unknown;
  conversation_ref?: unknown;
  request_id?: unknown;
  request_ref?: unknown;
  transition_id?: unknown;
  state_root?: unknown;
  expression_mode?: unknown;
  external_inference?: unknown;
  outcome?: unknown;
  duplicate_commit_protection?: unknown;
}

interface Capabilities {
  chat?: {
    expression?: unknown;
    learned_native_whole_field?: unknown;
    public_release_gate?: unknown;
  };
  voice?: {
    audio_input?: unknown;
    speech_to_text?: unknown;
    text_to_speech?: unknown;
    realtime_duplex?: unknown;
  };
}

const CONVERSATION_STORAGE_KEY = 'apocrypha.v2.owner-conversation-id';
const PENDING_TURN_STORAGE_KEY = 'apocrypha.v2.owner-pending-turn';
const CHAT_BROWSER_DEADLINE_MS = 28_000;
const MAX_TEXT_BYTES = 16_384;
const EXPECTED_EXPRESSION_CAPABILITY = 'bootstrap_shallow';
const LEARNED_FIELD_ABSENT = 'absent';
const AUDIO_UNAVAILABLE = 'unavailable';
const DUPLICATE_COMMIT_PROTECTION = 'active';

function isUuidV4(value: unknown): value is string {
  return typeof value === 'string'
    && /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);
}

function mintConversationId(): string {
  return crypto.randomUUID().toLowerCase();
}

function mintRequestId(): string {
  return crypto.randomUUID().toLowerCase();
}

function messageIdForRequest(requestId: string): string {
  return `turn-${requestId}`;
}

function clearPendingTurn(): void {
  try {
    sessionStorage.removeItem(PENDING_TURN_STORAGE_KEY);
  } catch {
    // Session persistence is a retry-safety aid; callers still fail closed
    // before dispatch when a new pending turn cannot be retained.
  }
}

function readPendingTurn(conversationId: string): RetryTurn | null {
  try {
    const raw = sessionStorage.getItem(PENDING_TURN_STORAGE_KEY);
    if (!raw) return null;
    const pending = JSON.parse(raw) as Partial<PersistedRetryTurn>;
    if (
      pending.conversationId !== conversationId
      || !isUuidV4(pending.requestId)
      || typeof pending.text !== 'string'
      || !pending.text.trim()
      || new TextEncoder().encode(pending.text).byteLength > MAX_TEXT_BYTES
    ) {
      clearPendingTurn();
      return null;
    }
    return { text: pending.text, requestId: pending.requestId };
  } catch {
    clearPendingTurn();
    return null;
  }
}

function writePendingTurn(conversationId: string, turn: RetryTurn): boolean {
  try {
    const pending: PersistedRetryTurn = { conversationId, ...turn };
    sessionStorage.setItem(PENDING_TURN_STORAGE_KEY, JSON.stringify(pending));
    return true;
  } catch {
    return false;
  }
}

function stringValue(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function safeError(body: TurnResponse, status: number): string {
  return stringValue(body.error)
    ?? (typeof body.detail === 'string' ? body.detail : null)
    ?? `Apocrypha could not complete this turn (HTTP ${status}).`;
}

export function ChatThread() {
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [conversationRef, setConversationRef] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState('');
  const [waiting, setWaiting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [retryTurn, setRetryTurn] = useState<RetryTurn | null>(null);
  const [capabilities, setCapabilities] = useState<Capabilities | null>(null);
  const inFlightRef = useRef(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const retained = sessionStorage.getItem(CONVERSATION_STORAGE_KEY);
    const resolved = isUuidV4(retained) ? retained : mintConversationId();
    sessionStorage.setItem(CONVERSATION_STORAGE_KEY, resolved);
    setConversationId(resolved);
    const pending = readPendingTurn(resolved);
    if (pending) {
      setRetryTurn(pending);
      setMessages([{
        id: messageIdForRequest(pending.requestId),
        role: 'user',
        text: pending.text,
      }]);
      setError('A prior turn may have completed. Retry the same turn to recover its protected response.');
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    void authFetch('/api/admin/apocrypha/capabilities', { cache: 'no-store' })
      .then(async (response) => {
        const body = await response.json() as { data?: unknown };
        if (!cancelled && response.ok && body.data && typeof body.data === 'object') {
          setCapabilities(body.data as Capabilities);
        }
      })
      .catch(() => undefined);
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' });
  }, [messages, waiting]);

  const startNewConversation = useCallback(() => {
    clearPendingTurn();
    const next = mintConversationId();
    sessionStorage.setItem(CONVERSATION_STORAGE_KEY, next);
    setConversationId(next);
    setConversationRef(null);
    setMessages([]);
    setError(null);
    setRetryTurn(null);
    requestAnimationFrame(() => textareaRef.current?.focus());
  }, []);

  const send = useCallback(async (retry?: RetryTurn) => {
    const text = retry?.text ?? draft.trim();
    if (!text || waiting || inFlightRef.current || !conversationId) return;
    const requestId = retry?.requestId ?? mintRequestId();
    const localMessageId = messageIdForRequest(requestId);
    if (!writePendingTurn(conversationId, { text, requestId })) {
      setError('This browser could not retain a safe retry identity, so the turn was not sent.');
      return;
    }
    inFlightRef.current = true;
    if (!retry) {
      const localMessage: ChatMessage = {
        id: localMessageId,
        role: 'user',
        text,
      };
      setMessages((current) => [...current, localMessage]);
      setDraft('');
    }
    setWaiting(true);
    setError(null);
    setRetryTurn(null);

    const controller = new AbortController();
    const deadline = setTimeout(() => controller.abort(), CHAT_BROWSER_DEADLINE_MS);
    let retryable = true;
    try {
      const response = await authFetch('/api/admin/apocrypha/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        cache: 'no-store',
        signal: controller.signal,
        body: JSON.stringify({
          text,
          conversation_id: conversationId,
          request_id: requestId,
        }),
      });
      const body = await response.json() as TurnResponse;
      if (!response.ok) {
        retryable = response.status === 409 || response.status >= 500;
        throw new Error(safeError(body, response.status));
      }
      const echoedConversationId = stringValue(body.conversation_id);
      const echoedRequestId = stringValue(body.request_id);
      const nextConversationRef = stringValue(body.conversation_ref);
      const requestRef = stringValue(body.request_ref);
      const transitionId = stringValue(body.transition_id);
      const stateRoot = stringValue(body.state_root);
      const responseText = stringValue(body.text);
      if (
        echoedConversationId !== conversationId
        || echoedRequestId !== requestId
        || !nextConversationRef
        || !requestRef
        || !transitionId
        || !stateRoot
        || !responseText
        || body.outcome !== 'committed'
        || body.external_inference !== false
        || body.expression_mode !== EXPECTED_EXPRESSION_CAPABILITY
        || body.duplicate_commit_protection !== DUPLICATE_COMMIT_PROTECTION
      ) {
        throw new Error('The V2 turn returned an invalid conversation-continuity envelope.');
      }
      clearPendingTurn();
      setConversationRef(nextConversationRef);
      setMessages((current) => [
        ...current,
        { id: crypto.randomUUID(), role: 'apocrypha', text: responseText },
      ]);
    } catch (cause) {
      const timedOut = cause instanceof DOMException && cause.name === 'AbortError';
      if (timedOut || retryable) {
        setRetryTurn({ text, requestId });
      } else {
        clearPendingTurn();
        setMessages((current) => current.filter((message) => message.id !== localMessageId));
        if (!retry || !draft.trim()) setDraft(text);
      }
      setError(timedOut
        ? 'Apocrypha did not answer before this turn’s bounded deadline.'
        : cause instanceof Error ? cause.message : String(cause));
    } finally {
      clearTimeout(deadline);
      inFlightRef.current = false;
      setWaiting(false);
      requestAnimationFrame(() => textareaRef.current?.focus());
    }
  }, [conversationId, draft, waiting]);

  const expression = stringValue(capabilities?.chat?.expression) ?? 'unverified';
  const learnedField = stringValue(capabilities?.chat?.learned_native_whole_field) ?? 'unverified';
  const audioLive = capabilities?.voice
    ? Object.values(capabilities.voice).some((value) => value === 'live')
    : false;
  const audioLabel = capabilities ? (audioLive ? 'live' : AUDIO_UNAVAILABLE) : 'unverified';

  return (
    <section className="v2-chat" aria-label="Apocrypha V2 chat">
      <header className="v2-chat-header">
        <div>
          <p className="v2-eyebrow">APOCRYPHA V2</p>
          <h1>Private conversation</h1>
        </div>
        <button type="button" className="v2-new" onClick={startNewConversation} disabled={waiting}>
          New conversation
        </button>
      </header>

      <div className="v2-capabilities" aria-label="Current capability boundary">
        <span data-capability-expression={expression}>
          Expression · {expression === EXPECTED_EXPRESSION_CAPABILITY ? 'bootstrap_shallow' : expression}
        </span>
        <span data-capability-learned-field={learnedField}>
          Learned field · {learnedField === LEARNED_FIELD_ABSENT ? 'absent' : learnedField}
        </span>
        <span data-capability-audio={audioLabel}>Audio · {audioLabel}</span>
        <span data-capability-retry-dedupe="active">Retry dedupe · active</span>
      </div>

      <div className="v2-continuity" aria-live="polite">
        <span>Client conversation · {conversationId ? conversationId.slice(0, 8) : 'minting…'}</span>
        <span>Body reference · {conversationRef ? conversationRef.slice(0, 12) : 'not established'}</span>
        <span>History · native V2 projection not yet available</span>
      </div>

      <VisionPanel />

      <div className="v2-messages" aria-live="polite" aria-busy={waiting}>
        {messages.length === 0 && (
          <div className="v2-empty">
            <h2>Say what you mean.</h2>
            <p>This surface sends one governed text turn and shows one final response. It does not simulate streaming or import predecessor history.</p>
          </div>
        )}
        {messages.map((message) => (
          <article key={message.id} className={`v2-message v2-message-${message.role}`}>
            <p className="v2-role">{message.role === 'user' ? 'You' : 'Apocrypha'}</p>
            <div>{message.text}</div>
          </article>
        ))}
        {waiting && <p className="v2-waiting" role="status">Apocrypha is forming one final response…</p>}
        {error && (
          <div className="v2-error" role="alert">
            <span>{error}</span>
            {retryTurn && (
              <button type="button" onClick={() => { void send(retryTurn); }} disabled={waiting}>
                Retry same turn
              </button>
            )}
          </div>
        )}
        <div ref={endRef} />
      </div>

      <form
        className="v2-composer"
        onSubmit={(event) => {
          event.preventDefault();
          void send();
        }}
      >
        <label className="sr-only" htmlFor="apocrypha-v2-message">Message Apocrypha</label>
        <textarea
          id="apocrypha-v2-message"
          ref={textareaRef}
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.shiftKey) {
              event.preventDefault();
              void send();
            }
          }}
          placeholder="Message Apocrypha…"
          rows={2}
          disabled={waiting || !conversationId}
        />
        <button type="submit" disabled={waiting || !conversationId || !draft.trim()}>
          {waiting ? 'Waiting…' : 'Send'}
        </button>
      </form>

      <style jsx>{`
        .v2-chat { display:grid; grid-template-rows:auto auto auto auto minmax(0,1fr) auto; height:100%; min-height:0; color:#ececf5; background:#080810; font-family:Inter,ui-sans-serif,system-ui,sans-serif; }
        .v2-chat-header { display:flex; align-items:center; justify-content:space-between; gap:16px; padding:18px clamp(16px,3vw,32px); border-bottom:1px solid #20202c; }
        .v2-eyebrow { margin:0 0 4px; color:#9e8cff; font:700 .68rem/1 ui-monospace,monospace; letter-spacing:.18em; }
        h1 { margin:0; font-size:clamp(1rem,2vw,1.3rem); font-weight:650; }
        .v2-new { min-height:44px; padding:0 15px; border:1px solid #343344; border-radius:999px; color:#d7d5e5; background:#12121c; cursor:pointer; }
        .v2-new:focus-visible, button:focus-visible, textarea:focus-visible { outline:2px solid #b9a8ff; outline-offset:3px; }
        .v2-capabilities, .v2-continuity { display:flex; flex-wrap:wrap; gap:7px 16px; padding:9px clamp(16px,3vw,32px); border-bottom:1px solid #191923; color:#a3a1b4; font:600 .7rem/1.4 ui-monospace,monospace; }
        .v2-capabilities span { padding:4px 8px; border:1px solid #292838; border-radius:999px; background:#101019; }
        .v2-continuity { color:#747286; font-weight:500; }
        .v2-messages { min-height:0; overflow-y:auto; padding:clamp(20px,4vw,44px) max(16px,calc((100% - 760px)/2)); }
        .v2-empty { max-width:620px; margin:10vh auto 0; text-align:center; color:#9290a2; }
        .v2-empty h2 { margin:0 0 12px; color:#eeeafc; font-size:clamp(1.5rem,4vw,2.4rem); }
        .v2-empty p { line-height:1.7; }
        .v2-message { max-width:min(82%,680px); margin:0 0 20px; padding:14px 17px; border-radius:18px; line-height:1.65; white-space:pre-wrap; overflow-wrap:anywhere; }
        .v2-message-user { margin-left:auto; background:#172332; border:1px solid #26415a; }
        .v2-message-apocrypha { margin-right:auto; background:#14131e; border:1px solid #29263b; }
        .v2-role { margin:0 0 6px; color:#8d88a6; font:700 .64rem/1 ui-monospace,monospace; letter-spacing:.1em; text-transform:uppercase; }
        .v2-waiting, .v2-error { max-width:680px; margin:12px auto; padding:12px 14px; border-radius:12px; }
        .v2-waiting { color:#b7accf; background:#12111a; }
        .v2-error { display:flex; align-items:center; justify-content:space-between; gap:12px; color:#ffb8bd; background:#2a151b; border:1px solid #5b2933; }
        .v2-error button { flex:0 0 auto; min-height:40px; padding:0 13px; border:1px solid #7d4650; border-radius:999px; color:#ffe7e9; background:#351b22; cursor:pointer; font-weight:700; }
        .v2-composer { display:grid; grid-template-columns:1fr auto; gap:10px; padding:14px max(16px,calc((100% - 800px)/2)) calc(14px + env(safe-area-inset-bottom)); border-top:1px solid #20202c; background:#0b0b13; }
        textarea { min-height:48px; max-height:180px; resize:vertical; box-sizing:border-box; padding:13px 15px; border:1px solid #2d2c3d; border-radius:15px; color:#f1f0f8; background:#12121b; font:inherit; line-height:1.45; }
        .v2-composer button { min-width:76px; min-height:48px; border:0; border-radius:14px; color:#100d19; background:linear-gradient(135deg,#ffc06c,#baa5ff); cursor:pointer; font-weight:750; }
        button:disabled, textarea:disabled { cursor:not-allowed; opacity:.55; }
        .sr-only { position:absolute; width:1px; height:1px; padding:0; margin:-1px; overflow:hidden; clip:rect(0,0,0,0); white-space:nowrap; border:0; }
        @media (max-width:640px) {
          .v2-chat-header { padding:12px; }
          .v2-capabilities,.v2-continuity { padding:8px 12px; }
          .v2-messages { padding:18px 12px; }
          .v2-message { max-width:90%; }
          .v2-composer { padding:10px 10px calc(10px + env(safe-area-inset-bottom)); }
        }
        @media (prefers-reduced-motion:reduce) { * { scroll-behavior:auto !important; } }
      `}</style>
    </section>
  );
}
