import Link from 'next/link';
import { useCallback, useEffect, useRef, useState } from 'react';

import { authFetch } from '@/lib/browser-auth';
import { useSiteSession } from '@/components/hub/SiteSession';
import CyberDreamField from '@/components/cyber/CyberDreamField';
import styles from '@/styles/PublicApocrypha.module.css';

type MessageRole = 'user' | 'apocrypha';

interface TurnReceipt {
  modelId: string;
  responseId: string;
  responseDigest: string;
  servingProfileDigest: string;
  memoryScope: string;
  conversationHistory: string;
  identity: IdentityReceipt | null;
  context: ContextReceipt | null;
}

interface IdentityReceipt {
  schema_version: string;
  system_id: string;
  architecture: string;
  compiler_version: string;
  identity_digest: string;
  learned_model_role: string;
  lineage: string;
}

interface CapabilityReceipt {
  id: string;
  status: string;
  authority: string;
  evidence: string;
}

interface ContextReceipt {
  frame_id: string;
  frame_digest: string;
  provenance_spine_digest: string;
  retrieval: { status: string; count: number; refs: unknown[] };
  memory: { provider: string; status: string; records_used: number; receipt_digest: string | null; refs: unknown[] };
  capabilities: CapabilityReceipt[];
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
  model_id?: unknown;
  response_id?: unknown;
  response_digest?: unknown;
  serving_profile_digest?: unknown;
  effect_authority?: unknown;
  tool_authority?: unknown;
  outcome?: unknown;
  learned_faculty_used?: unknown;
  memory_scope?: unknown;
  conversation_history?: unknown;
  training_consent?: unknown;
  duplicate_effect_protection?: unknown;
  retry_after_seconds?: unknown;
  identity?: unknown;
  context?: unknown;
}

type GenerativeMode = 'general' | 'code' | 'analyze' | 'write' | 'explain';

const GENERATIVE_MODES: ReadonlyArray<{
  id: GenerativeMode;
  label: string;
  starter: string;
  placeholder: string;
}> = [
  { id: 'general', label: 'Ask', starter: '', placeholder: 'Ask Apocrypha anything…' },
  { id: 'code', label: 'Code', starter: 'Help me implement this:\n', placeholder: 'Describe what you want to build or repair…' },
  { id: 'analyze', label: 'Analyze', starter: 'Analyze this rigorously:\n', placeholder: 'Paste or describe what should be analyzed…' },
  { id: 'write', label: 'Write', starter: 'Draft this for me:\n', placeholder: 'Describe the document or content to generate…' },
  { id: 'explain', label: 'Explain', starter: 'Explain this clearly and precisely:\n', placeholder: 'What should Apocrypha explain?…' },
];

const CHAT_BROWSER_DEADLINE_MS = 28_000;
const MAX_TEXT_BYTES = 16_384;
const SHA256_PATTERN = /^[0-9a-f]{64}$/;

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

function recordValue(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function identityReceipt(value: unknown): IdentityReceipt | null {
  const record = recordValue(value);
  if (!record) return null;
  const receipt = {
    schema_version: stringValue(record.schema_version),
    system_id: stringValue(record.system_id),
    architecture: stringValue(record.architecture),
    compiler_version: stringValue(record.compiler_version),
    identity_digest: stringValue(record.identity_digest),
    learned_model_role: stringValue(record.learned_model_role),
    lineage: stringValue(record.lineage),
  };
  return receipt.schema_version === 'apocv4.identity.v1'
    && receipt.system_id === 'apocrypha'
    && receipt.architecture === 'governed_hybrid_digital_intelligence'
    && receipt.identity_digest !== null
    && SHA256_PATTERN.test(receipt.identity_digest)
    && receipt.compiler_version !== null
    && receipt.learned_model_role === 'replaceable_faculty_not_system_identity'
    && receipt.lineage !== null
    ? receipt as IdentityReceipt
    : null;
}

function contextReceipt(value: unknown): ContextReceipt | null {
  const record = recordValue(value);
  const retrieval = recordValue(record?.retrieval);
  const memory = recordValue(record?.memory);
  if (
    !record || !retrieval || !memory
    || !stringValue(record.frame_id)
    || typeof record.frame_digest !== 'string'
    || !SHA256_PATTERN.test(record.frame_digest)
    || typeof record.provenance_spine_digest !== 'string'
    || !SHA256_PATTERN.test(record.provenance_spine_digest)
    || !stringValue(retrieval.status)
    || !Number.isInteger(retrieval.count)
    || !Array.isArray(retrieval.refs)
    || !stringValue(memory.provider)
    || !stringValue(memory.status)
    || !Number.isInteger(memory.records_used)
    || !Array.isArray(memory.refs)
    || !Array.isArray(record.capabilities)
  ) return null;
  const capabilities: CapabilityReceipt[] = [];
  for (const candidate of record.capabilities) {
    const capability = recordValue(candidate);
    if (!capability) return null;
    const id = stringValue(capability.id);
    const status = stringValue(capability.status);
    const authority = stringValue(capability.authority);
    const evidence = stringValue(capability.evidence);
    if (!id || !status || !authority || !evidence) return null;
    capabilities.push({ id, status, authority, evidence });
  }
  const memoryReceipt = memory.receipt_digest;
  if (memoryReceipt !== null && (
    typeof memoryReceipt !== 'string' || !SHA256_PATTERN.test(memoryReceipt)
  )) return null;
  return {
    frame_id: String(record.frame_id),
    frame_digest: record.frame_digest,
    provenance_spine_digest: record.provenance_spine_digest,
    retrieval: {
      status: String(retrieval.status),
      count: Number(retrieval.count),
      refs: retrieval.refs,
    },
    memory: {
      provider: String(memory.provider),
      status: String(memory.status),
      records_used: Number(memory.records_used),
      receipt_digest: memoryReceipt as string | null,
      refs: memory.refs,
    },
    capabilities,
  };
}

function isExactTurn(
  body: TurnResponse,
  conversationId: string,
  requestId: string,
): body is TurnResponse & {
  text: string;
  model_id: string;
  response_id: string;
  response_digest: string;
  serving_profile_digest: string;
} {
  const legacyBoundary = body.tool_authority === 'NONE'
    && body.memory_scope === 'ephemeral'
    && body.conversation_history === 'not_retained_by_public_interface'
    && body.identity === null
    && body.context === null;
  const governedBoundary = body.tool_authority === 'READ_ONLY_CONTEXT'
    && (body.memory_scope === 'owner_partitioned_retrieval'
      || body.memory_scope === 'public_safe_retrieval')
    && body.conversation_history === 'session_bounded'
    && identityReceipt(body.identity) !== null
    && contextReceipt(body.context) !== null;
  return body.conversation_id === conversationId
    && body.request_id === requestId
    && Boolean(stringValue(body.text))
    && Boolean(stringValue(body.model_id))
    && Boolean(stringValue(body.response_id))
    && typeof body.response_digest === 'string'
    && SHA256_PATTERN.test(body.response_digest)
    && typeof body.serving_profile_digest === 'string'
    && SHA256_PATTERN.test(body.serving_profile_digest)
    && body.effect_authority === 'NONE'
    && body.outcome === 'completed'
    && body.learned_faculty_used === true
    && (legacyBoundary || governedBoundary)
    && body.training_consent === false
    && body.duplicate_effect_protection === 'not_applicable_no_effect_authority';
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
  const [lastModel, setLastModel] = useState<string | null>(null);
  const [mode, setMode] = useState<GenerativeMode>('general');
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
    setLastModel(null);
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
      const identity = identityReceipt(body.identity);
      const context = contextReceipt(body.context);
      setLastModel(body.model_id);
      setMessages((current) => [
        ...current,
        {
          id: `reply-${requestId}`,
          role: 'apocrypha',
          text: responseText,
          receipt: {
            modelId: body.model_id,
            responseId: body.response_id,
            responseDigest: body.response_digest,
            servingProfileDigest: body.serving_profile_digest,
            memoryScope: String(body.memory_scope),
            conversationHistory: String(body.conversation_history),
            identity,
            context,
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
  const selectedMode = GENERATIVE_MODES.find((candidate) => candidate.id === mode) ?? GENERATIVE_MODES[0]!;
  const sessionLabel = access === 'checking'
    ? 'Checking sign-in'
    : authenticated
      ? 'Signed in'
      : access === 'unavailable'
        ? 'Verification unavailable'
        : 'Sign in required';

  return (
    <div className={styles.page} data-public-apocrypha="apocv4-chat-v1">
      <CyberDreamField variant="relay" activity={waiting ? 'thinking' : draft.trim() ? 'listening' : 'idle'} density={0.78} viewport />
      <a className={styles.skipLink} href="#apocrypha-conversation">Skip to conversation</a>

      <header className={styles.header}>
        <Link href="/" className={styles.brand} aria-label="Apocky home">
          <span className={styles.brandMark} aria-hidden="true">A</span>
          <span>APOCKY</span>
        </Link>
        <div className={styles.identity}>
          <span className={styles.identityName}>Apocrypha</span>
          <span className={styles.identityMeta}>Digital intelligence workspace</span>
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
              <p className={styles.eyebrow}>APOCRYPHA</p>
              <h1>Intelligence workspace</h1>
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
                New chat
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
                <div className={styles.emptyKicker} aria-hidden="true">A</div>
                <h2>What are we working on?</h2>
                <p>
                  Ask, analyze, write, explain, or generate code. Each response
                  is validated and carries an inspectable receipt.
                </p>
                <dl className={styles.contract}>
                  <div>
                    <dt>Chat history</dt>
                    <dd>This session</dd>
                  </div>
                  <div>
                    <dt>Training consent</dt>
                    <dd>Off</dd>
                  </div>
                  <div>
                    <dt>Effect authority</dt>
                    <dd>None</dd>
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
                    <summary>Verified response receipt</summary>
                    <dl>
                      <div><dt>Model</dt><dd>{message.receipt.modelId}</dd></div>
                      <div><dt>Response</dt><dd>{message.receipt.responseId}</dd></div>
                      <div><dt>Digest</dt><dd>{message.receipt.responseDigest.slice(0, 16)}…</dd></div>
                      <div><dt>Serving profile</dt><dd>{message.receipt.servingProfileDigest.slice(0, 16)}…</dd></div>
                      <div><dt>Memory</dt><dd>{message.receipt.memoryScope}</dd></div>
                      <div><dt>History</dt><dd>{message.receipt.conversationHistory}</dd></div>
                      {message.receipt.identity && (
                        <>
                          <div><dt>Identity</dt><dd>Apocrypha · governed hybrid</dd></div>
                          <div><dt>Compiler</dt><dd>{message.receipt.identity.compiler_version}</dd></div>
                          <div><dt>Identity digest</dt><dd>{message.receipt.identity.identity_digest.slice(0, 16)}…</dd></div>
                        </>
                      )}
                      {message.receipt.context && (
                        <>
                          <div><dt>Context frame</dt><dd>{message.receipt.context.frame_id}</dd></div>
                          <div><dt>Provenance</dt><dd>{message.receipt.context.provenance_spine_digest.slice(0, 16)}…</dd></div>
                          <div><dt>Retrieval</dt><dd>{message.receipt.context.retrieval.status} · {message.receipt.context.retrieval.count}</dd></div>
                          <div><dt>Memory bank</dt><dd>{message.receipt.context.memory.provider} · {message.receipt.context.memory.status} · {message.receipt.context.memory.records_used} used</dd></div>
                          <div><dt>Capabilities</dt><dd>{message.receipt.context.capabilities.map((capability) => `${capability.id}:${capability.status}`).join(' · ') || 'none'}</dd></div>
                        </>
                      )}
                    </dl>
                  </details>
                )}
              </article>
            ))}

            {waiting && (
              <div className={styles.waiting} role="status">
                <span className={styles.waitingMark} aria-hidden="true" />
                Apocrypha is thinking…
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
              <div className={styles.toolDock} aria-label="Prompt starters">
                <span>Prompt starters</span>
                {GENERATIVE_MODES.map((candidate) => (
                  <button
                    key={candidate.id}
                    type="button"
                    aria-pressed={candidate.id === mode}
                    onClick={() => {
                      setMode(candidate.id);
                      if (candidate.starter) {
                        setDraft((current) => current.startsWith(candidate.starter)
                          ? current
                          : `${candidate.starter}${current}`);
                      }
                      requestAnimationFrame(() => composerRef.current?.focus());
                    }}
                    disabled={waiting}
                  >
                    {candidate.label}
                  </button>
                ))}
              </div>
              <div className={styles.composerField}>
                <textarea
                  id="public-apocrypha-message"
                  ref={composerRef}
                  value={draft}
                  rows={2}
                  maxLength={MAX_TEXT_BYTES}
                  placeholder={selectedMode.placeholder}
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
                  Governed retrieval and read-only context may be used · workspace and external effects require separate owner confirmation.
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

        <aside className={styles.truthRail} aria-label="Privacy and response details">
          <details>
            <summary>Privacy &amp; response details</summary>
            <div className={styles.truthIntro}>
              <h2>Your current chat</h2>
              <p>This transcript stays in this view. Refreshing starts a new conversation.</p>
            </div>
            <dl className={styles.truthList}>
              <div><dt>Connection</dt><dd>Signed-in member → Apocrypha</dd></div>
              <div><dt>Response model</dt><dd>{lastModel ?? 'Verified with each response'}</dd></div>
              <div><dt>Retry</dt><dd>Same request ID; effects remain disabled</dd></div>
              <div><dt>Effects</dt><dd>No external effects; read-only context may be used</dd></div>
              <div><dt>History</dt><dd>Bounded to this session; not retained across sessions</dd></div>
            </dl>
          </details>
        </aside>
      </main>
    </div>
  );
}
