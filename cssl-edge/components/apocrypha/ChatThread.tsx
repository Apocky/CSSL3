// Modern Apocrypha chat — durable job submission, recovery, and partial output.
//
// Per HANDOFF_v10 § TRACK-A polish-pass (replaces the cockpit-monospace draft).

import React, { useCallback, useEffect, useRef, useState } from 'react';
import Link from 'next/link';

import { authFetch } from '../../lib/browser-auth';
import { ApocryphaAvatar } from './ApocryphaAvatar';

// ─── Types ──────────────────────────────────────────────────────────

interface ToolCallChip {
  name: string;
  ok: boolean;
  elapsed_ms?: number;
  error?: string | null;
}

interface ChatMessage {
  id?: string;
  role: 'user' | 'apocrypha';
  text: string;
  ts: Date;
  toolCalls?: ToolCallChip[];
  halt?: string;
  elapsed_s?: number;
  cost_usd?: number;
}

type ConversationScope = 'active' | 'archived' | 'trash';
interface ConvSummary {
  id: string;
  title: string | null;
  last_active_iso: string;
  message_count?: number;
}

interface ConvMessagesResponse {
  conversation: { id: string; title: string | null; last_active_iso: string };
  messages: Array<{
    id: string;
    role: string;
    text: string;
    ts_iso: string;
    tool_trace: ToolCallChip[];
  }>;
}

interface ApocryphaEnvelope<T> {
  upstream_status: number;
  data: T;
}

interface JobSnapshotResponse {
  ok: boolean;
  job?: {
    id: string;
    status: 'queued' | 'leased' | 'running' | 'cancel_requested' | 'succeeded' | 'failed' | 'cancelled';
    request?: { conversation_id?: string | null };
    error_code?: string | null;
    error_detail?: string | null;
  };
  chunks?: Array<{ seq: number; delta: string }>;
  revisions?: Array<{
    content: string;
    provenance?: { tool_calls?: ToolCallChip[] };
    usage?: { elapsed_s?: number; total_cost_usd?: number };
  }>;
}

interface ActiveJobRecord {
  id: string;
  prompt: string;
  submittedAt: string;
  conversationId?: string;
}

const ACTIVE_JOB_KEY = 'apocky.apocrypha.active-job.v1';
// Set when the reader presses New chat, cleared the moment they actually send
// something. It exists because "open the most recent conversation on load" and
// "I asked for a blank one" are both correct behaviours and only the reader
// knows which applies - so the choice has to outlive the page, not just the
// React tree. Without it, New chat cannot survive a reload: the mount decides
// on its own to reopen the newest conversation, and every chat is the same chat.
const NEW_CHAT_KEY = 'apocky.apocrypha.new-chat.v1';
// Polling cadence. The job path now HAS a push transport - /api/admin/
// apocrypha/jobs/[id]/stream - and the loop below waits on whichever arrives
// first, a pushed frame or this timer. So these intervals are the fallback
// ceiling, not the latency: they are what the reader falls back to if the
// stream never opens or dies mid-answer.
//
// A flat 1500ms meant text landed in 1.5s jumps and every transition (queued ->
// leased -> first token -> done) cost up to a full interval of nothing. So the
// cadence follows the work: fast while the answer is actually growing, backing
// off when nothing is changing. That is strictly fewer requests than a flat
// interval when idle, and far more responsive when it matters.
const JOB_POLL_FAST_MS = 250;
const JOB_POLL_SLOW_MS = 1_500;
const COMPACT_CHAT_QUERY = '(max-width: 767px)';
const MUTED_TEXT = '#85859a';
const CONVERSATION_ID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

// Opens the job's push channel and calls `onAdvance` whenever the server says
// something moved. Reconnects across the server's bounded windows, resuming
// from the cursor it was handed.
//
// This deliberately does NOT parse the answer out of the stream. The snapshot
// fetch below remains the single source of truth for what the reader sees; the
// stream only says "now". Two code paths assembling the same text from two
// transports is how they drift, and the one that drifts silently is the one
// that is not authoritative.
function subscribeJobAdvance(
  jobId: string,
  signal: AbortSignal,
  onAdvance: () => void,
): void {
  void (async () => {
    let cursor = 0;
    while (!signal.aborted) {
      try {
        const response = await authFetch(
          `/api/admin/apocrypha/jobs/${encodeURIComponent(jobId)}/stream?after=${cursor}`,
          { cache: 'no-store', credentials: 'include', signal, headers: { Accept: 'text/event-stream' } },
        );
        if (!response.ok || !response.body) return;
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';
        let finished = false;
        while (!signal.aborted) {
          const { done, value } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, { stream: true });
          // Frames are separated by a blank line. Anything after the last one
          // is a partial frame and has to stay in the buffer.
          const frames = buffer.split('\n\n');
          buffer = frames.pop() ?? '';
          for (const frame of frames) {
            for (const line of frame.split('\n')) {
              if (line.startsWith('id: ')) {
                const parsed = Number(line.slice(4));
                if (Number.isSafeInteger(parsed) && parsed > cursor) cursor = parsed;
              } else if (line.startsWith('event: complete')) {
                finished = true;
              }
            }
            if (frame.trim()) onAdvance();
          }
        }
        if (finished || signal.aborted) return;
        // Window closed or the connection dropped. Reopen from the cursor.
      } catch {
        // Network fault or abort. The caller's poll is still running, so the
        // answer still arrives; retrying here is best-effort.
        if (signal.aborted) return;
        await waitForPoll(signal, 1_000);
      }
    }
  })();
}

function waitForPoll(signal: AbortSignal, ms: number): Promise<void> {
  return new Promise((resolve) => {
    if (signal.aborted) return resolve();
    const timer = window.setTimeout(resolve, ms);
    signal.addEventListener('abort', () => {
      window.clearTimeout(timer);
      resolve();
    }, { once: true });
  });
}

// ─── Component ─────────────────────────────────────────────────────

export function ChatThread() {
  const [convs, setConvs] = useState<ConvSummary[]>([]);
  const [scope, setScope] = useState<ConversationScope>('active');
  const [currentConv, setCurrentConv] = useState<string | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState('');
  const [streaming, setStreaming] = useState(false);
  const [streamingText, setStreamingText] = useState('');
  const [streamingPhase, setStreamingPhase] = useState('Preparing your place in the queue…');
  const [activeJob, setActiveJob] = useState<ActiveJobRecord | null>(null);
  const [streamingTools, setStreamingTools] = useState<ToolCallChip[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [compactViewport, setCompactViewport] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [showTrace, setShowTrace] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const newChatButtonRef = useRef<HTMLButtonElement>(null);
  const sidebarToggleRef = useRef<HTMLButtonElement>(null);
  const restoredInitialConversationRef = useRef(false);
  const hydratedActiveJobRef = useRef<string | null>(null);
  const activeHydrationRef = useRef<{ jobId: string; promise: Promise<boolean> } | null>(null);

  useEffect(() => {
    try {
      // Read BEFORE the active-job branch below, and outside its early return,
      // so a reader who pressed New chat keeps a blank one even on a load where
      // there is no active job to recover.
      if (window.localStorage.getItem(NEW_CHAT_KEY)) {
        restoredInitialConversationRef.current = true;
      }
    } catch {
      // Storage blocked: fall through to the default, which is to reopen the
      // most recent conversation. Losing the preference is a worse experience,
      // not a broken one.
    }
    try {
      const raw = window.localStorage.getItem(ACTIVE_JOB_KEY);
      if (!raw) return;
      const recovered = JSON.parse(raw) as ActiveJobRecord;
      if (!recovered?.id || !recovered?.prompt) return;
      setActiveJob(recovered);
      if (recovered.conversationId && CONVERSATION_ID.test(recovered.conversationId)) {
        setCurrentConv(recovered.conversationId.toLowerCase());
      }
      setMessages([{ role: 'user', text: recovered.prompt, ts: new Date(recovered.submittedAt) }]);
      setStreaming(true);
      setStreamingPhase('Reconnected. Apocrypha is continuing this answer…');
    } catch {
      window.localStorage.removeItem(ACTIVE_JOB_KEY);
    }
  }, []);

  useEffect(() => {
    const media = window.matchMedia(COMPACT_CHAT_QUERY);
    const syncViewport = (compact: boolean) => {
      setCompactViewport(compact);
      setSidebarOpen(!compact);
    };
    syncViewport(media.matches);
    const onChange = (event: MediaQueryListEvent) => syncViewport(event.matches);
    media.addEventListener('change', onChange);
    return () => media.removeEventListener('change', onChange);
  }, []);

  // ── data loading ──────────────────────────────────────────────

  const loadConvs = useCallback(async (requestedScope: ConversationScope = scope) => {
    try {
      const r = await authFetch(`/api/admin/apocrypha/conversations?scope=${requestedScope}`);
      if (!r.ok) throw new Error(`Conversation history returned ${r.status}.`);
      const env = (await r.json()) as ApocryphaEnvelope<{ conversations: ConvSummary[] }>;
      setConvs(env.data?.conversations ?? []);
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : 'Conversation history is unavailable.');
    }
  }, [scope]);

  const loadConv = useCallback(async (id: string, recoveringJob?: ActiveJobRecord) => {
    try {
      const r = await authFetch(`/api/admin/apocrypha/conversations?id=${id}`);
      if (!r.ok) throw new Error(`Conversation history returned ${r.status}.`);
      const env = (await r.json()) as ApocryphaEnvelope<ConvMessagesResponse>;
      const msgs: ChatMessage[] = (env.data?.messages ?? []).map((m) => ({
        id: m.id,
        role: m.role === 'apocrypha' ? ('apocrypha' as const) : ('user' as const),
        text: m.text,
        ts: new Date(m.ts_iso),
        toolCalls: m.tool_trace ?? [],
      }));
      const recoveredPromptId = recoveringJob ? `${recoveringJob.id}:user` : null;
      const hydratedMessages = recoveringJob && recoveredPromptId
        && !msgs.some((message) => message.id === recoveredPromptId)
        ? [...msgs, {
            id: recoveredPromptId,
            role: 'user' as const,
            text: recoveringJob.prompt,
            ts: new Date(recoveringJob.submittedAt),
          }]
        : msgs;
      setMessages((previous) => {
        if (!recoveringJob) return hydratedMessages;
        const activeIds = new Set([
          `${recoveringJob.id}:user`,
          `${recoveringJob.id}:apocrypha`,
        ]);
        const activeMessagesById = new Map(previous.flatMap((message) => (
          message.id && activeIds.has(message.id) ? [[message.id, message] as const] : []
        )));
        const mergedMessages = hydratedMessages.map((message) => (
          message.id ? activeMessagesById.get(message.id) ?? message : message
        ));
        const knownIds = new Set(mergedMessages.flatMap((message) => message.id ? [message.id] : []));
        const activeMessagesMissingFromSnapshot = previous.filter((message) => (
          Boolean(message.id) && activeIds.has(message.id!) && !knownIds.has(message.id!)
        ));
        return [...mergedMessages, ...activeMessagesMissingFromSnapshot];
      });
      setCurrentConv(id);
      setStreamingTools([]);
      setError(null);
      if (compactViewport) {
        setSidebarOpen(false);
        requestAnimationFrame(() => textareaRef.current?.focus());
      }
      return true;
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      return false;
    }
  }, [compactViewport]);

  const hydrateActiveConversation = useCallback((conversationId: string, job: ActiveJobRecord): Promise<boolean> => {
    if (hydratedActiveJobRef.current === job.id) return Promise.resolve(true);
    if (activeHydrationRef.current?.jobId === job.id) return activeHydrationRef.current.promise;
    const promise = loadConv(conversationId, { ...job, conversationId }).then((loaded) => {
      if (loaded) hydratedActiveJobRef.current = job.id;
      return loaded;
    }).finally(() => {
      if (activeHydrationRef.current?.jobId === job.id) activeHydrationRef.current = null;
    });
    activeHydrationRef.current = { jobId: job.id, promise };
    return promise;
  }, [loadConv]);

  useEffect(() => {
    void loadConvs();
  }, [loadConvs]);

  useEffect(() => {
    if (restoredInitialConversationRef.current || activeJob || currentConv || convs.length === 0) return;
    restoredInitialConversationRef.current = true;
    void loadConv(convs[0]!.id);
  }, [activeJob, convs, currentConv, loadConv]);

  useEffect(() => {
    if (!activeJob || hydratedActiveJobRef.current === activeJob.id) return;
    const conversationId = activeJob.conversationId;
    if (!conversationId || !CONVERSATION_ID.test(conversationId)) return;
    void hydrateActiveConversation(conversationId.toLowerCase(), activeJob);
  }, [activeJob, hydrateActiveConversation]);

  const newChat = useCallback(() => {
    setMessages([]);
    setCurrentConv(null);
    setStreamingTools([]);
    setError(null);
    // The persisted active-job record has to go too, and this is the whole bug
    // it fixes: clearing only React state left ACTIVE_JOB_KEY in localStorage,
    // so the mount effect above read it back on the next load and restored
    // currentConv from it. New chat appeared to work, and then reopening the
    // app put you straight back into the previous conversation - which reads,
    // correctly, as every chat being the same chat.
    //
    // Also stopping the stream and dropping activeJob, because a record that
    // is gone from storage but still in state would re-hydrate the same
    // conversation the moment anything touched that effect.
    setActiveJob(null);
    setStreaming(false);
    setStreamingText('');
    setStreamingPhase('');
    hydratedActiveJobRef.current = null;
    activeHydrationRef.current = null;
    restoredInitialConversationRef.current = true;
    try {
      window.localStorage.removeItem(ACTIVE_JOB_KEY);
      // Recorded so the choice survives a reload. The mount deliberately
      // reopens the most recent conversation, which is right when you are
      // coming back and wrong when you just asked for a blank one.
      window.localStorage.setItem(NEW_CHAT_KEY, '1');
    } catch {
      // A browser with storage blocked has nothing to clear, and failing to
      // clear what does not exist must not stop the new chat from opening.
    }
    if (compactViewport) setSidebarOpen(false);
    setTimeout(() => textareaRef.current?.focus(), 0);
  }, [compactViewport]);

  // ── auto-scroll + textarea auto-grow ──────────────────────────

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' });
  }, [messages, streamingTools, streaming]);

  useEffect(() => {
    const t = textareaRef.current;
    if (!t) return;
    t.style.height = 'auto';
    t.style.height = `${Math.min(t.scrollHeight, 200)}px`;
  }, [draft]);

  const closeCompactSidebar = useCallback(() => {
    setSidebarOpen(false);
    requestAnimationFrame(() => sidebarToggleRef.current?.focus());
  }, []);

  useEffect(() => {
    if (!compactViewport || !sidebarOpen) return;
    const focusFirstControl = requestAnimationFrame(() => newChatButtonRef.current?.focus());
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      closeCompactSidebar();
    };
    document.addEventListener('keydown', onKeyDown);
    return () => {
      cancelAnimationFrame(focusFirstControl);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [closeCompactSidebar, compactViewport, sidebarOpen]);

  const handleSidebarKeyDown = useCallback((event: React.KeyboardEvent<HTMLElement>) => {
    if (!compactViewport || event.key !== 'Tab') return;
    const controls = Array.from(event.currentTarget.querySelectorAll<HTMLElement>(
      'button:not([disabled]), a[href], input:not([disabled]), select:not([disabled]), textarea:not([disabled])',
    )).filter((control) => control.getClientRects().length > 0);
    if (controls.length === 0) return;
    const first = controls[0];
    const last = controls.at(-1);
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }, [compactViewport]);

  useEffect(() => {
    if (!activeJob) return;
    const controller = new AbortController();
    let disposed = false;
    // Seeded fast: the first tick after submitting is the one most worth
    // spending a request on.
    let pollDelay = JOB_POLL_FAST_MS;
    let lastSeen = '';
    // Resolved by the stream the moment the server reports movement. The loop
    // waits on this OR the backoff timer, whichever comes first, so a pushed
    // frame turns into a snapshot read immediately instead of at the next tick.
    let releaseAdvance: (() => void) | null = null;
    let advance = new Promise<void>((resolve) => { releaseAdvance = resolve; });
    subscribeJobAdvance(activeJob.id, controller.signal, () => {
      releaseAdvance?.();
      advance = new Promise<void>((resolve) => { releaseAdvance = resolve; });
    });
    void (async () => {
      while (!controller.signal.aborted) {
        let progressed = false;
        try {
          const response = await authFetch(`/api/admin/apocrypha/jobs/${encodeURIComponent(activeJob.id)}`, {
            cache: 'no-store',
            credentials: 'include',
            signal: controller.signal,
          });
          if (!response.ok) throw new Error(response.status === 404
            ? 'The accepted job could not be found.'
            : `Status service returned ${response.status}.`);
          const snapshot = await response.json() as JobSnapshotResponse;
          if (!snapshot.job) throw new Error('The job status response was incomplete.');
          const partial = [...(snapshot.chunks ?? [])]
            .sort((left, right) => left.seq - right.seq)
            .map((chunk) => chunk.delta)
            .join('');
          const revision = snapshot.revisions?.[0];
          const visibleText = revision?.content || partial;
          const snapshotConversationId = activeJob.conversationId
            ?? snapshot.job.request?.conversation_id
            ?? snapshot.job.id;
          const normalizedConversationId = snapshotConversationId && CONVERSATION_ID.test(snapshotConversationId)
            ? snapshotConversationId.toLowerCase()
            : null;
          if (!disposed) {
            setError(null);
            if (normalizedConversationId) {
              setCurrentConv(normalizedConversationId);
              if (snapshot.job.status !== 'succeeded') {
                void hydrateActiveConversation(normalizedConversationId, activeJob);
              }
            }
            setStreamingText(visibleText);
            // "Did anything move" is the whole input to the cadence below.
            // Status counts as movement as well as text: queued -> leased is a
            // transition the reader is waiting on even though no answer has
            // appeared yet.
            const mark = `${snapshot.job.status}:${visibleText.length}`;
            progressed = mark !== lastSeen;
            lastSeen = mark;
            setStreamingPhase(snapshot.job.status === 'queued'
              ? 'Accepted. Waiting for the local Apocrypha node…'
              : snapshot.job.status === 'leased'
                ? 'The Apocrypha node has claimed this thought…'
                : snapshot.job.status === 'cancel_requested'
                  ? 'Stopping after the current safe boundary…'
                  : 'Apocrypha is composing the answer…');
          }
          if (snapshot.job.status === 'succeeded') {
            if (normalizedConversationId && !disposed) {
              const hydrated = await hydrateActiveConversation(normalizedConversationId, activeJob);
              if (!hydrated) await hydrateActiveConversation(normalizedConversationId, activeJob);
            }
            if (!disposed) {
              setMessages((previous) => {
                const completed: ChatMessage = {
                  id: `${activeJob.id}:apocrypha`,
                  role: 'apocrypha',
                  text: visibleText || 'Apocrypha completed the thought without words.',
                  ts: new Date(),
                  toolCalls: revision?.provenance?.tool_calls ?? [],
                  elapsed_s: revision?.usage?.elapsed_s,
                  cost_usd: revision?.usage?.total_cost_usd,
                };
                const existingIndex = previous.findIndex((message) => message.id === completed.id);
                if (existingIndex < 0) return [...previous, completed];
                return previous.map((message, index) => index === existingIndex ? completed : message);
              });
              setStreamingText('');
              setStreamingTools([]);
              setStreaming(false);
              setActiveJob(null);
              window.localStorage.removeItem(ACTIVE_JOB_KEY);
              void loadConvs();
            }
            return;
          }
          if (snapshot.job.status === 'failed' || snapshot.job.status === 'cancelled') {
            if (!disposed) {
              setError(snapshot.job.status === 'cancelled'
                ? 'This answer was cancelled.'
                : 'The model attempt failed and was preserved. Retry sends a fresh attempt without losing this request.');
              setStreaming(false);
              setStreamingText(partial);
              setActiveJob(null);
              window.localStorage.removeItem(ACTIVE_JOB_KEY);
            }
            return;
          }
        } catch (pollError) {
          if (controller.signal.aborted) return;
          if (!disposed) {
            setStreamingPhase('Connection interrupted. The job is safe; reconnecting…');
            setError(pollError instanceof Error ? pollError.message : 'Connection interrupted.');
          }
        }
        // Something moved this tick, so the next answer is probably close:
        // ask again soon. Nothing moved, so widen towards the slow cadence
        // rather than hammering a job that is still sitting in a queue.
        pollDelay = progressed ? JOB_POLL_FAST_MS : Math.min(Math.round(pollDelay * 1.5), JOB_POLL_SLOW_MS);
        await Promise.race([waitForPoll(controller.signal, pollDelay), advance]);
      }
    })();
    return () => {
      disposed = true;
      controller.abort();
    };
  }, [activeJob, hydrateActiveConversation, loadConvs]);

  const handleSend = useCallback(async () => {
    const text = draft.trim();
    if (!text || streaming) return;
    setDraft('');
    setError(null);
    setStreamingTools([]);
    setStreamingText('');
    setMessages((previous) => [...previous, { role: 'user', text, ts: new Date() }]);
    setStreaming(true);
    setStreamingPhase('Saving your message…');
    try {
      const conversationId = currentConv ?? window.crypto.randomUUID();
      const idempotencyKey = window.crypto.randomUUID();
      const response = await authFetch('/api/admin/apocrypha/jobs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'include',
        body: JSON.stringify({
          prompt: text,
          conversation_id: conversationId,
          output_budget: 2048,
          response_mode: text.length > 1200 ? 'deep' : 'standard',
          idempotency_key: idempotencyKey,
        }),
      });
      const payload = await response.json().catch(() => null) as {
        conversation_id?: string;
        job?: { id?: string };
        error?: string;
      } | null;
      if (!response.ok || !payload?.job?.id || payload.conversation_id !== conversationId) {
        throw new Error(payload?.error ?? `Request was not accepted (${response.status}).`);
      }
      const record: ActiveJobRecord = {
        id: payload.job.id,
        prompt: text,
        submittedAt: new Date().toISOString(),
        conversationId,
      };
      window.localStorage.setItem(ACTIVE_JOB_KEY, JSON.stringify(record));
      // The blank chat has been used, so the preference is spent. From here on
      // a reload should reopen THIS conversation, which is what the default
      // already does.
      window.localStorage.removeItem(NEW_CHAT_KEY);
      setCurrentConv(conversationId);
      setActiveJob(record);
      setStreamingPhase('Accepted. Waiting for the local Apocrypha node…');
    } catch (sendError) {
      setStreaming(false);
      setError(sendError instanceof Error ? sendError.message : String(sendError));
    }
  }, [currentConv, draft, streaming]);

  const cancelActiveJob = useCallback(async () => {
    if (!activeJob) return;
    try {
      await authFetch(`/api/admin/apocrypha/jobs/${encodeURIComponent(activeJob.id)}`, {
        method: 'DELETE',
        credentials: 'include',
      });
      setStreamingPhase('Cancellation requested…');
    } catch {
      setError('Cancellation could not be delivered. The job remains recoverable.');
    }
  }, [activeJob]);

  const handleKey = useCallback((e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void handleSend();
    }
  }, [handleSend]);

  // ─── render ─────────────────────────────────────────────────

  return (
    <div className="chat-shell" style={{
      display: 'flex',
      height: '100%',
      background: '#0a0a10',
      color: '#e6e6f0',
      fontFamily: 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif',
    }}>
      {compactViewport && sidebarOpen && (
        <button
          type="button"
          className="chat-sidebar-backdrop"
          aria-label="Close conversations"
          tabIndex={-1}
          onClick={closeCompactSidebar}
        />
      )}
      {/* SIDEBAR */}
      {sidebarOpen && (
        <aside
          id="apocrypha-conversations"
          className="chat-sidebar"
          role={compactViewport ? 'dialog' : undefined}
          aria-modal={compactViewport || undefined}
          aria-label="Conversations"
          onKeyDown={handleSidebarKeyDown}
          style={{
          borderRight: '1px solid #1f1f2a',
          display: 'flex',
          flexDirection: 'column',
          background: 'rgba(15, 15, 22, 0.7)',
          }}
        >
          <div style={{ padding: '0.75rem', borderBottom: '1px solid #1f1f2a' }}>
            <div style={{ display: 'flex', gap: '0.4rem', alignItems: 'stretch' }}>
            <button ref={newChatButtonRef} type="button" className="chat-new-button" onClick={newChat} style={{
              flex: 1,
              minWidth: 0,
              padding: '0.65rem 0.8rem',
              background: 'transparent',
              border: '1px solid #2a2a3a',
              borderRadius: 8,
              color: '#cdd6e4',
              cursor: 'pointer',
              fontSize: '0.88rem',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              fontFamily: 'inherit',
            }}>
              <span style={{ fontWeight: 500 }}>+ New chat</span>
              <span style={{ color: '#7a7a8c', fontSize: '0.75rem' }}>⌘N</span>
            </button>
            {compactViewport && (
              <button
                type="button"
                className="chat-icon-button"
                aria-label="Close conversations"
                onClick={closeCompactSidebar}
                style={{
                  flex: '0 0 44px', background: 'transparent', border: '1px solid #2a2a3a',
                  borderRadius: 8, color: '#cdd6e4', cursor: 'pointer', fontSize: '1.1rem', fontFamily: 'inherit',
                }}
              >
                ×
              </button>
            )}
            </div>
            <div style={{ display: 'flex', gap: '0.25rem', marginTop: '0.5rem' }}>
              {(['active', 'archived', 'trash'] as ConversationScope[]).map((candidate) => (
                <button
                  key={candidate}
                  type="button"
                  className="chat-scope-button"
                  aria-pressed={scope === candidate}
                  onClick={() => setScope(candidate)}
                  style={{
                  flex: 1, padding: '0.3rem 0.2rem', borderRadius: 5,
                  border: scope === candidate ? '1px solid #8b7cff' : '1px solid #2a2a3a',
                  background: scope === candidate ? 'rgba(139,124,255,.16)' : 'transparent',
                  color: scope === candidate ? '#dcd7ff' : '#7a7a8c',
                  cursor: 'pointer', fontSize: '0.68rem', fontFamily: 'inherit',
                  }}
                >{candidate}</button>
              ))}
            </div>
          </div>
          <div style={{ flex: 1, overflowY: 'auto', padding: '0.4rem' }}>
            {convs.length === 0 && (
              <div style={{ padding: '0.6rem 0.7rem', color: '#7a7a8c', fontSize: '0.8rem' }}>
                no conversations yet
              </div>
            )}
            {convs.map((c) => (
              <div key={c.id} className="chat-conversation-row">
                <button
                  type="button"
                  className="chat-conversation-select"
                  aria-current={c.id === currentConv ? 'true' : undefined}
                  onClick={() => void loadConv(c.id)}
                  style={{
                    flex: 1,
                    minWidth: 0,
                    padding: '0.55rem 0.75rem',
                    background: c.id === currentConv ? 'rgba(192, 132, 252, 0.18)' : 'transparent',
                    border: c.id === currentConv ? '1px solid rgba(192, 132, 252, 0.35)' : '1px solid transparent',
                    borderRadius: 6,
                    color: c.id === currentConv ? '#e6e6f0' : '#cdd6e4',
                    textAlign: 'left',
                    cursor: 'pointer',
                    fontSize: '0.85rem',
                    overflow: 'hidden',
                    fontFamily: 'inherit',
                  }}
                >
                  <span style={{ display: 'block', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {c.title || `Conversation #${c.id}`}
                  </span>
                  <span style={{ display: 'block', fontSize: '0.7rem', color: MUTED_TEXT, marginTop: 2 }}>
                    {new Date(c.last_active_iso).toLocaleString()}
                  </span>
                </button>
              </div>
            ))}
          </div>
        </aside>
      )}

      {/* MAIN */}
      <div className="chat-main" style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0 }}>
        {/* HEADER */}
        <header className="chat-header" style={{
          borderBottom: '1px solid #1f1f2a',
          display: 'flex',
          alignItems: 'center',
          fontSize: '0.85rem',
          color: '#9aa0a6',
        }}>
          <button
            ref={sidebarToggleRef}
            type="button"
            className="chat-icon-button"
            onClick={() => setSidebarOpen((open) => !open)}
            aria-label={sidebarOpen ? 'Close conversations' : 'Open conversations'}
            aria-expanded={sidebarOpen}
            aria-controls="apocrypha-conversations"
            style={{
            background: 'transparent',
            border: 0,
            color: '#9aa0a6',
            cursor: 'pointer',
            fontSize: '1.05rem',
            padding: '0.2rem 0.5rem',
            fontFamily: 'inherit',
            }}
            title="Toggle conversations"
          >
            ☰
          </button>
          <span className="chat-wordmark" style={{
            fontWeight: 600,
            backgroundImage: 'linear-gradient(135deg, #ffaa55, #c084fc)',
            WebkitBackgroundClip: 'text',
            WebkitTextFillColor: 'transparent',
          }}>
            Apocrypha
          </span>
          <ApocryphaAvatar className="chat-header-avatar" state={streaming ? 'thinking' : error ? 'degraded' : 'ready'} size={40} detail="compact" />
          <span style={{ flex: 1 }} />
          {/* /apocrypha renders without SiteShell, so this is the only route back to the site. */}
          <nav className="chat-site-nav" aria-label="Site">
            <Link href="/" className="chat-site-link">Home</Link>
            <Link href="/account" className="chat-site-link">Account</Link>
          </nav>
          <button
            type="button"
            className="chat-settings-button"
            onClick={() => setSettingsOpen((open) => !open)}
            aria-expanded={settingsOpen}
            aria-controls="apocrypha-settings"
            style={{
              background: 'transparent', border: '1px solid #2a2a3a',
              borderRadius: 6, color: '#9aa0a6', cursor: 'pointer',
              padding: '0.25rem 0.5rem', fontFamily: 'inherit', fontSize: '0.75rem',
            }}
          >
            settings
          </button>
          <span className="chat-conversation-id" style={{ color: '#7a7a8c', fontSize: '0.75rem' }}>
            {currentConv ? `conv #${currentConv}` : 'new conversation'}
          </span>
        </header>

        {settingsOpen && (
          <section
            id="apocrypha-settings"
            aria-label="Chat settings"
            style={{
              padding: '0.65rem 1rem', borderBottom: '1px solid #1f1f2a',
              background: 'rgba(20, 20, 30, 0.9)', color: '#cdd6e4',
              fontSize: '0.8rem',
            }}
          >
            <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', cursor: 'pointer' }}>
              <input type="checkbox" checked={showTrace} onChange={(e) => setShowTrace(e.target.checked)} />
              show tool and run trace
            </label>
            <div style={{ marginTop: '0.35rem', color: '#7a7a8c', fontSize: '0.7rem' }}>
              Presentation only; model, authority, and security policy remain server-controlled.
            </div>
          </section>
        )}

        {/* THREAD */}
        <div
          className="chat-thread-scroll"
          style={{ flex: 1, overflowY: 'auto', padding: '1.5rem 0' }}
        >
          <div style={{ maxWidth: 760, margin: '0 auto', padding: '0 1.2rem' }}>
            <div
              role="log"
              aria-label="Conversation with Apocrypha"
              aria-live="polite"
              aria-relevant="additions text"
              aria-busy={streaming}
            >
              {messages.length === 0 && !streaming && (
                <div style={{
                  color: '#7a7a8c',
                  fontSize: '1rem',
                  textAlign: 'center',
                  marginTop: '1.8rem',
                  display: 'grid',
                  justifyItems: 'center',
                }}>
                  <ApocryphaAvatar state={error ? 'degraded' : 'ready'} size={190} />
                  <div style={{
                    fontSize: '1.8rem',
                    margin: '0.35rem 0 0.6rem',
                    fontWeight: 600,
                    backgroundImage: 'linear-gradient(135deg, #ffaa55, #c084fc)',
                    WebkitBackgroundClip: 'text',
                    WebkitTextFillColor: 'transparent',
                  }}>
                    Apocrypha
                  </div>
                  <div style={{ fontSize: '0.92rem' }}>
                    A private, persistent digital entity with native state continuity and governed faculties.
                  </div>
                  <div style={{ marginTop: '0.5rem', fontSize: '0.8rem', color: MUTED_TEXT }}>
                    Speak naturally. Apocrypha will choose how deeply to think.
                  </div>
                </div>
              )}

              {messages.map((m, i) => (
                <MessageBubble key={i} msg={m} showTrace={showTrace} />
              ))}
              {streamingText && (
                <MessageBubble msg={{ role: 'apocrypha', text: streamingText, ts: new Date() }} showTrace={showTrace} />
              )}
            </div>

            {streaming && (
              <div style={{
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'flex-start',
                marginBottom: '1.5rem',
              }}>
                {streamingTools.length > 0 && (
                  <div style={{
                    display: 'flex',
                    flexWrap: 'wrap',
                    gap: '0.3rem',
                    marginBottom: '0.5rem',
                    fontSize: '0.72rem',
                    fontFamily: 'ui-monospace, SFMono-Regular, monospace',
                  }}>
                    {streamingTools.map((t, i) => (
                      <ToolChip key={i} chip={t} />
                    ))}
                  </div>
                )}
                <div role="status" aria-live="polite" aria-atomic="true" style={{
                  padding: '0.7rem 1rem',
                  borderRadius: 14,
                  background: 'rgba(192, 132, 252, 0.06)',
                  border: '1px solid rgba(192, 132, 252, 0.18)',
                  color: '#9aa0a6',
                  fontSize: '0.92rem',
                  display: 'flex',
                  alignItems: 'center',
                  gap: '0.4rem',
                }}>
                  <PulsingDot />
                  <span>{streamingPhase}</span>
                  {activeJob && (
                    <button type="button" onClick={() => void cancelActiveJob()} style={{
                      marginLeft: '0.5rem', border: '1px solid #4a4058', borderRadius: 999,
                      padding: '0.3rem 0.55rem', color: '#c5bfd0', background: 'transparent', cursor: 'pointer',
                    }}>
                      Cancel
                    </button>
                  )}
                </div>
              </div>
            )}

            {error && (
              <div role="alert" style={{
                marginBottom: '1.5rem',
                padding: '0.65rem 0.9rem',
                background: 'rgba(255, 136, 136, 0.08)',
                border: '1px solid rgba(255, 136, 136, 0.3)',
                borderRadius: 8,
                color: '#ff8888',
                fontSize: '0.88rem',
              }}>
                Apocrypha paused: {error}
              </div>
            )}

            <div ref={messagesEndRef} aria-hidden="true" />
          </div>
        </div>

        {/* COMPOSER */}
        <div className="chat-composer" style={{ borderTop: '1px solid #1f1f2a' }}>
          <div style={{ maxWidth: 760, margin: '0 auto' }}>
            <div style={{
              display: 'flex',
              gap: '0.5rem',
              alignItems: 'flex-end',
              padding: '0.5rem',
              background: 'rgba(20, 20, 30, 0.7)',
              border: '1px solid #2a2a3a',
              borderRadius: 16,
            }}>
              <textarea
                ref={textareaRef}
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={handleKey}
                aria-label="Message Apocrypha"
                aria-describedby="apocrypha-composer-help"
                placeholder="Message Apocrypha…"
                rows={1}
                style={{
                  flex: 1,
                  background: 'transparent',
                  color: '#e6e6f0',
                  border: 0,
                  resize: 'none',
                  padding: '0.55rem 0.7rem',
                  fontSize: '0.95rem',
                  fontFamily: 'inherit',
                  minHeight: 36,
                  maxHeight: 200,
                  lineHeight: 1.45,
                }}
              />
              <button
                onClick={() => void handleSend()}
                disabled={streaming || !draft.trim()}
                aria-label="Send message"
                style={{
                  padding: '0.55rem 0.9rem',
                  background: draft.trim() && !streaming
                    ? 'linear-gradient(135deg, #ffaa55 0%, #c084fc 100%)'
                    : 'rgba(40, 40, 60, 0.5)',
                  color: draft.trim() && !streaming ? '#0a0a10' : '#5a5a6a',
                  border: 0,
                  borderRadius: 12,
                  cursor: draft.trim() && !streaming ? 'pointer' : 'not-allowed',
                  fontWeight: 700,
                  fontSize: '1rem',
                  fontFamily: 'inherit',
                  alignSelf: 'flex-end',
                  minWidth: 44,
                }}>
                {streaming ? '⋯' : '↑'}
              </button>
            </div>
            <div id="apocrypha-composer-help" style={{
              marginTop: '0.4rem',
              fontSize: '0.7rem',
              color: MUTED_TEXT,
              textAlign: 'center',
            }}>
              Enter to send · Shift+Enter for newline · instruments remain governed by Apocrypha
            </div>
          </div>
        </div>
      </div>
      <style jsx>{`
        .chat-shell {
          position: relative;
          min-width: 0;
          min-height: 0;
          overflow: hidden;
        }
        .chat-sidebar {
          position: relative;
          z-index: 2;
          width: 280px;
          min-width: 280px;
          min-height: 0;
        }
        .chat-sidebar-backdrop { display: none; }
        .chat-main,
        .chat-thread-scroll { min-width: 0; min-height: 0; }
        .chat-header { gap: .6rem; padding: .6rem 1rem; }
        .chat-composer {
          flex: 0 0 auto;
          padding: .9rem 1rem 1.4rem;
        }
        .chat-new-button,
        .chat-icon-button,
        .chat-settings-button,
        .chat-conversation-select { min-height: 44px; }
        .chat-icon-button { min-width: 44px; }
        .chat-conversation-row {
          display: flex;
          align-items: stretch;
          gap: 2px;
          width: 100%;
          margin-bottom: 2px;
        }
        .chat-conversation-action {
          flex: 0 0 44px;
          width: 44px;
          padding: 0;
          border: 1px solid transparent;
          border-radius: 6px;
          color: #a9a9bc;
          background: transparent;
          cursor: pointer;
          font-family: inherit;
          font-size: 1.1rem;
          font-weight: 700;
          line-height: 1;
        }
        .chat-conversation-action:hover { background: rgba(192, 132, 252, .1); }
        .chat-site-nav { display: flex; gap: .25rem; align-items: center; }
        .chat-site-nav :global(.chat-site-link) {
          display: inline-flex;
          align-items: center;
          min-height: 44px;
          padding: 0 .55rem;
          border-radius: 6px;
          color: #9aa0a6;
          font-size: .78rem;
          text-decoration: none;
        }
        .chat-site-nav :global(.chat-site-link:hover) { color: #e6e6f0; background: rgba(192, 132, 252, .1); }
        .chat-shell button:focus-visible,
        .chat-shell textarea:focus-visible,
        .chat-shell input:focus-visible {
          outline: 2px solid #c9b8ff;
          outline-offset: 2px;
        }
        @media (max-width: 767px) {
          .chat-sidebar-backdrop {
            display: block;
            position: fixed;
            inset: 0;
            z-index: 29;
            width: 100%;
            height: 100%;
            padding: 0;
            border: 0;
            background: rgba(0, 0, 8, .64);
            cursor: default;
          }
          .chat-sidebar {
            position: fixed;
            inset: 0 auto 0 0;
            z-index: 30;
            width: min(86vw, 280px);
            min-width: 0;
            max-width: calc(100vw - 44px);
            box-shadow: 18px 0 48px rgba(0, 0, 0, .52);
          }
          .chat-header { gap: .35rem; padding: .45rem .5rem; }
          .chat-conversation-id { display: none; }
          .chat-site-nav :global(.chat-site-link) { padding: 0 .4rem; }
          .chat-composer {
            padding:
              .65rem
              max(.6rem, env(safe-area-inset-right))
              calc(.65rem + env(safe-area-inset-bottom))
              max(.6rem, env(safe-area-inset-left));
          }
        }
        @media (max-width: 419px) {
          .chat-site-nav :global(.chat-site-link[href="/account"]) { display: none; }
        }
        @media (max-width: 359px) {
          .chat-wordmark { display: none; }
        }
      `}</style>
    </div>
  );
}

// ─── presentation sub-components ──────────────────────────────────

function MessageBubble({ msg, showTrace }: { msg: ChatMessage; showTrace: boolean }) {
  const isUser = msg.role === 'user';
  return (
    <div style={{
      marginBottom: '1.5rem',
      display: 'flex',
      flexDirection: 'column',
      alignItems: isUser ? 'flex-end' : 'flex-start',
    }}>
      <div style={{
        maxWidth: '85%',
        padding: '0.75rem 1.05rem',
        borderRadius: 16,
        background: isUser
          ? 'rgba(124, 211, 252, 0.13)'
          : 'rgba(192, 132, 252, 0.06)',
        border: isUser
          ? '1px solid rgba(124, 211, 252, 0.22)'
          : '1px solid rgba(192, 132, 252, 0.16)',
        fontSize: '0.96rem',
        lineHeight: 1.6,
        whiteSpace: 'pre-wrap',
        wordBreak: 'break-word',
        color: '#e6e6f0',
      }}>
        {msg.text}
        {showTrace && msg.toolCalls && msg.toolCalls.length > 0 && (
          <div style={{
            marginTop: '0.7rem',
            paddingTop: '0.6rem',
            borderTop: '1px solid rgba(255, 255, 255, 0.06)',
            display: 'flex',
            flexWrap: 'wrap',
            gap: '0.3rem',
          }}>
            {msg.toolCalls.map((tc, j) => (
              <ToolChip key={j} chip={tc} />
            ))}
          </div>
        )}
      </div>
      {!isUser && showTrace && (msg.halt || msg.elapsed_s != null || msg.cost_usd != null) && (
        <div style={{
          fontSize: '0.68rem',
          color: MUTED_TEXT,
          marginTop: '0.3rem',
          marginLeft: '0.3rem',
          fontFamily: 'ui-monospace, SFMono-Regular, monospace',
        }}>
          {msg.halt && <span>halt={msg.halt}</span>}
          {msg.elapsed_s != null && <span> · {msg.elapsed_s.toFixed(2)}s</span>}
          {msg.cost_usd != null && <span> · ${msg.cost_usd.toFixed(4)}</span>}
        </div>
      )}
    </div>
  );
}

function ToolChip({ chip }: { chip: ToolCallChip }) {
  return (
    <span style={{
      padding: '0.18rem 0.5rem',
      borderRadius: 4,
      background: chip.ok ? 'rgba(127, 209, 127, 0.13)' : 'rgba(255, 136, 136, 0.13)',
      color: chip.ok ? '#9ddb9d' : '#ff8888',
      border: `1px solid ${chip.ok ? 'rgba(127, 209, 127, 0.22)' : 'rgba(255, 136, 136, 0.22)'}`,
      fontSize: '0.72rem',
      fontFamily: 'ui-monospace, SFMono-Regular, monospace',
      whiteSpace: 'nowrap',
    }}>
      {chip.ok ? '✓' : '✗'} {chip.name}
      {chip.elapsed_ms != null && ` · ${chip.elapsed_ms}ms`}
    </span>
  );
}

function PulsingDot() {
  return (
    <>
      <span className="apocrypha-thinking-dot" style={{
        display: 'inline-block',
        width: 8,
        height: 8,
        borderRadius: '50%',
        background: '#c084fc',
        animation: 'apocrypha-pulse 1.4s ease-in-out infinite',
      }} />
      <style>{`
        @keyframes apocrypha-pulse {
          0%, 100% { opacity: 0.3; transform: scale(0.9); }
          50% { opacity: 1; transform: scale(1.1); }
        }
        @media (prefers-reduced-motion: reduce) {
          .apocrypha-thinking-dot { animation: none !important; }
        }
      `}</style>
    </>
  );
}
