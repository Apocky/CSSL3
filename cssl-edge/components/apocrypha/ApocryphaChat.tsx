// The Apocrypha room. One of them.
//
// There used to be three: GuestChat for strangers, AccountChat for members, ChatThread for the
// owner. They were written at different times and drifted, so signing in visibly DOWNGRADED the
// product — the open room had been redesigned and the one an owner actually uses had not. Three
// codebases meant every fix had to be made three times, and in practice never was.
//
// This is the whole interface for all three. What differs between them is transport and
// capability, and both of those arrive as a `ChatLane` (see lib/apocrypha/chat-lanes.ts). Nothing
// below branches on WHO is looking; it branches on what the lane can do. A guest has no
// conversation list because their lane says `conversations: false`, not because this file knows
// what a guest is.

import Link from 'next/link';
import { useCallback, useEffect, useRef, useState } from 'react';

import { inApocryphaApp } from '@/lib/app-shell';
import type { ChatLane, ChatToolCall, ConversationSummary, LaneMessage } from '@/lib/apocrypha/chat-lanes';
import styles from '@/styles/ApocryphaChat.module.css';

// Polling cadence. None of the three job paths has a streaming transport, so the reader sees the
// answer arrive only as fast as this loop asks.
//
// A flat interval meant text landed in 1.5s jumps and every transition (queued -> leased -> first
// token -> done) cost up to a full interval of nothing. So the cadence follows the work: fast while
// the answer is actually growing, backing off when nothing is changing. Strictly fewer requests
// than a flat interval when idle, and far more responsive when it matters.
const JOB_POLL_FAST_MS = 250;
const JOB_POLL_SLOW_MS = 1_500;
// How long to keep following one answer. A local GPU working through a real question is slow, and
// giving up early throws away an answer that was on its way.
const JOB_FOLLOW_MS = 20 * 60_000;

const COMPACT_CHAT_QUERY = '(max-width: 767px)';
const MAX_TEXT = 8_000;
const MAX_LOCAL_TURNS = 40;

const OPENERS = [
  'What are you, and what are you for?',
  'Help me think through a decision I keep avoiding.',
  'Explain something you find genuinely difficult.',
];

// A turn in flight. `id` is null between writing this record and the server accepting the job —
// the window in which a crash or a closed tab would otherwise lose the message with no way to tell
// whether it was sent. Written BEFORE the request, so that window is recoverable rather than
// invisible.
interface ActiveJob {
  readonly id: string | null;
  readonly prompt: string;
  readonly submittedAt: string;
  readonly conversationId: string | null;
}

export interface ApocryphaChatProps {
  readonly lane: ChatLane;
  readonly signedIn: boolean;
  /** Shown when a member lane reports the account cannot be used for chat. */
  readonly laneNotice?: string | null;
  /** Override the room's own full-viewport height when it is embedded in a shell that has its own. */
  readonly height?: string;
  readonly onPendingChange?: (pending: boolean) => void;
}

function newId(): string {
  return typeof crypto !== 'undefined' && 'randomUUID' in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.floor(Math.random() * 1e9)}`;
}

function activeJobKey(lane: string): string { return `apx.chat.active-job.${lane}.v1`; }
function localThreadKey(lane: string): string { return `apx.chat.thread.${lane}.v1`; }
// Set when the reader presses New chat, cleared the moment they actually send something. It exists
// because "reopen the most recent conversation" and "I asked for a blank one" are both correct and
// only the reader knows which applies — so the choice has to outlive the page, not just the React
// tree. Without it, New chat cannot survive a reload and every chat is the same chat.
function blankChatKey(lane: string): string { return `apx.chat.blank.${lane}.v1`; }

function readStored<T>(key: string): T | null {
  try {
    const raw = window.localStorage.getItem(key);
    return raw ? JSON.parse(raw) as T : null;
  } catch {
    return null; // Private windows and blocked storage are ordinary, not errors.
  }
}

function writeStored(key: string, value: unknown): void {
  try { window.localStorage.setItem(key, JSON.stringify(value)); } catch { /* storage unavailable */ }
}

function dropStored(key: string): void {
  try { window.localStorage.removeItem(key); } catch { /* storage unavailable */ }
}

function waitFor(signal: AbortSignal, ms: number): Promise<void> {
  return new Promise((resolve) => {
    if (signal.aborted) { resolve(); return; }
    const timer = window.setTimeout(resolve, ms);
    signal.addEventListener('abort', () => { window.clearTimeout(timer); resolve(); }, { once: true });
  });
}

// A refusal the server made BEFORE taking responsibility for the message: bad input, denied
// origin, rejected size. Nothing is in flight, so the message can be handed straight back. Anything
// else — a dropped connection, a 5xx, a timeout — leaves the outcome genuinely unknown, and
// pretending otherwise is how a sent message silently disappears.
function isDefiniteRefusal(error: unknown): boolean {
  const status = (error as { publicStatus?: number; status?: number } | null)?.publicStatus
    ?? (error as { status?: number } | null)?.status;
  return typeof status === 'number' && status >= 400 && status < 500 && status !== 408 && status !== 429;
}

function phaseFor(status: string): string {
  if (status === 'queued') return 'Accepted. Waiting for the Apocrypha node…';
  if (status === 'leased') return 'The Apocrypha node has claimed this thought…';
  if (status === 'cancel_requested') return 'Stopping after the current safe boundary…';
  return 'Apocrypha is composing the answer…';
}

export function ApocryphaChat({ lane, signedIn, laneNotice, height, onPendingChange }: ApocryphaChatProps): JSX.Element {
  const [messages, setMessages] = useState<LaneMessage[]>([]);
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [currentConv, setCurrentConv] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const [activeJob, setActiveJob] = useState<ActiveJob | null>(null);
  const [streaming, setStreaming] = useState(false);
  const [streamingText, setStreamingText] = useState('');
  const [streamingPhase, setStreamingPhase] = useState('');
  const [streamingTools, setStreamingTools] = useState<ChatToolCall[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [compactViewport, setCompactViewport] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [showTrace, setShowTrace] = useState(false);
  const [hydrated, setHydrated] = useState(false);
  const [unresolved, setUnresolved] = useState<ActiveJob | null>(null);
  // Resolved after mount: navigator does not exist during server rendering, and a wrong guess would
  // flash an install prompt at someone already inside the app.
  const [inApp, setInApp] = useState(false);

  const logRef = useRef<HTMLDivElement>(null);
  const endRef = useRef<HTMLDivElement>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const sidebarToggleRef = useRef<HTMLButtonElement>(null);
  const firstSidebarControlRef = useRef<HTMLButtonElement>(null);
  const following = useRef(true);
  const openedInitialRef = useRef(false);

  const can = lane.capabilities;
  const laneId = lane.id;

  // ── mount: local thread, recoverable job, viewport ───────────────────────────────────────────

  useEffect(() => {
    setInApp(inApocryphaApp());
    if (readStored<unknown>(blankChatKey(laneId))) openedInitialRef.current = true;

    if (!can.durableHistory) {
      const stored = readStored<LaneMessage[]>(localThreadKey(laneId));
      if (Array.isArray(stored)) {
        setMessages(stored.flatMap((turn) => (
          typeof turn?.text === 'string' && (turn.role === 'user' || turn.role === 'apocrypha')
            ? [{ ...turn, at: new Date(turn.at) }]
            : []
        )).slice(-MAX_LOCAL_TURNS));
      }
    }

    const recovered = readStored<ActiveJob>(activeJobKey(laneId));
    if (recovered?.prompt) {
      const key = recovered.id ? `${recovered.id}:user` : `pending:${recovered.submittedAt}`;
      if (recovered.conversationId) setCurrentConv(recovered.conversationId);
      setMessages((prior) => (
        prior.some((message) => message.id === key)
          ? prior
          : [...prior, { id: key, role: 'user' as const, text: recovered.prompt, at: new Date(recovered.submittedAt) }]
      ));
      if (recovered.id) {
        setActiveJob(recovered);
        setStreaming(true);
        setStreamingPhase('Reconnected. Apocrypha is continuing this answer…');
      } else {
        // The record exists but never got a job id, so whether the server took it is genuinely
        // unknown. Saying "sent" would be a lie and saying "failed" might be too — so it says what
        // is true and offers the one safe action. Re-sending is safe: the lanes are idempotent on
        // their request key.
        setUnresolved(recovered);
        setError('That message was interrupted before Apocrypha confirmed it. Nothing was lost — you can send it again.');
      }
    }
    setHydrated(true);
  }, [can.durableHistory, laneId]);

  // A thread that lives only in this browser has to be written back, or closing the tab loses it.
  useEffect(() => {
    if (!hydrated || can.durableHistory) return;
    writeStored(localThreadKey(laneId), messages.slice(-MAX_LOCAL_TURNS));
  }, [can.durableHistory, hydrated, laneId, messages]);

  useEffect(() => {
    const media = window.matchMedia(COMPACT_CHAT_QUERY);
    const sync = (compact: boolean) => {
      setCompactViewport(compact);
      setSidebarOpen(!compact && can.conversations);
    };
    sync(media.matches);
    const onChange = (event: MediaQueryListEvent) => sync(event.matches);
    media.addEventListener('change', onChange);
    return () => media.removeEventListener('change', onChange);
  }, [can.conversations]);

  useEffect(() => { onPendingChange?.(activeJob !== null); }, [activeJob, onPendingChange]);

  // ── conversation list and history ────────────────────────────────────────────────────────────

  const refreshConversations = useCallback(async () => {
    if (!lane.listConversations) return [] as ConversationSummary[];
    try {
      const listed = await lane.listConversations();
      setConversations(listed);
      return listed;
    } catch {
      // A failed listing must not take the conversation down with it: the reader can still read and
      // send in the one they have open.
      return [] as ConversationSummary[];
    }
  }, [lane]);

  // `protect` names messages belonging to a turn that is still in flight. Server history is the
  // authority for everything settled, but it must never overwrite the live turn: a job that is
  // mid-answer has a bounded prefix stored, and letting that land on top of a full terminal
  // revision truncates an answer the reader already has.
  const loadConv = useCallback(async (id: string, protect: readonly string[] = []) => {
    if (!lane.loadConversation) return false;
    try {
      const loaded = await lane.loadConversation(id);
      const guarded = new Set(protect);
      setMessages((prior) => {
        const held = new Map(prior.flatMap((message) => (
          message.id && guarded.has(message.id) ? [[message.id, message] as const] : []
        )));
        const merged = loaded.map((message) => (
          message.id && held.has(message.id) ? held.get(message.id)! : message
        ));
        const known = new Set(merged.flatMap((message) => (message.id ? [message.id] : [])));
        // A live turn the server has not recorded yet still belongs on screen.
        const missing = [...held.values()].filter((message) => !known.has(message.id!));
        return [...merged, ...missing];
      });
      setCurrentConv(id);
      setStreamingTools([]);
      setError(null);
      if (compactViewport) {
        setSidebarOpen(false);
        requestAnimationFrame(() => composerRef.current?.focus());
      }
      return true;
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : 'That conversation could not be opened.');
      return false;
    }
  }, [compactViewport, lane]);

  useEffect(() => { void refreshConversations(); }, [refreshConversations]);

  // Reloading mid-answer used to show the recovered prompt alone, with the conversation it belongs
  // to missing until the job finished. The turns before it are durable and already on the server,
  // so they are fetched back — with the live turn protected from being overwritten by the bounded
  // prefix the server has stored for it so far.
  const hydratedJobRef = useRef<string | null>(null);
  useEffect(() => {
    // `currentConv` is the fallback for a journal written before conversation ids were recorded
    // with the job: the poll resolves the id from the job itself, and the history behind it is
    // then just as recoverable as any other.
    const conversationId = activeJob?.conversationId ?? currentConv;
    if (!hydrated || !can.durableHistory || !activeJob || !conversationId) return;
    const key = activeJob.id ?? activeJob.submittedAt;
    if (hydratedJobRef.current === key) return;
    hydratedJobRef.current = key;
    const live = activeJob.id
      ? [`${activeJob.id}:user`, `${activeJob.id}:apocrypha`]
      : [`pending:${activeJob.submittedAt}`];
    void loadConv(conversationId, live);
  }, [activeJob, can.durableHistory, currentConv, hydrated, loadConv]);

  useEffect(() => {
    if (!hydrated || openedInitialRef.current || activeJob || currentConv || conversations.length === 0) return;
    openedInitialRef.current = true;
    void loadConv(conversations[0]!.id);
  }, [activeJob, conversations, currentConv, hydrated, loadConv]);

  const newChat = useCallback(() => {
    setMessages([]);
    setCurrentConv(null);
    setStreamingTools([]);
    setStreamingText('');
    setStreamingPhase('');
    setStreaming(false);
    setActiveJob(null);
    setError(null);
    openedInitialRef.current = true;
    hydratedJobRef.current = null;
    // The persisted record has to go too. Clearing only React state left the job in storage, so the
    // next load read it back and restored the previous conversation: New chat appeared to work, and
    // then reopening the app put you straight back where you were.
    dropStored(activeJobKey(laneId));
    if (!can.durableHistory) dropStored(localThreadKey(laneId));
    writeStored(blankChatKey(laneId), 1);
    if (compactViewport) setSidebarOpen(false);
    setTimeout(() => composerRef.current?.focus(), 0);
  }, [can.durableHistory, compactViewport, laneId]);

  // ── follow the active job ────────────────────────────────────────────────────────────────────

  useEffect(() => {
    const jobId = activeJob?.id;
    if (!activeJob || !jobId) return undefined;
    const controller = new AbortController();
    let disposed = false;
    // Seeded fast: the first tick after submitting is the one most worth spending a request on.
    let delay = JOB_POLL_FAST_MS;
    let lastSeen = '';
    const deadline = Date.now() + JOB_FOLLOW_MS;

    void (async () => {
      while (!controller.signal.aborted) {
        let progressed = false;
        try {
          const snapshot = await lane.poll(jobId, controller.signal);
          if (disposed) return;
          setError(null);
          if (snapshot.conversationId) setCurrentConv(snapshot.conversationId);
          setStreamingText(snapshot.text);
          if (snapshot.tools) setStreamingTools(snapshot.tools);
          // "Did anything move" is the whole input to the cadence below. Status counts as movement
          // as well as text: queued -> leased is a transition the reader is waiting on even though
          // no answer has appeared yet.
          const mark = `${snapshot.status}:${snapshot.text.length}`;
          progressed = mark !== lastSeen;
          lastSeen = mark;
          setStreamingPhase(phaseFor(snapshot.status));

          if (snapshot.done) {
            const failed = snapshot.status !== 'succeeded' && snapshot.status !== 'completed';
            if (failed) {
              setError(snapshot.status === 'cancelled'
                ? 'This answer was stopped. You can send another message.'
                : 'Apocrypha could not finish that reply. Your message is saved — sending it again is safe.');
              setStreamingText(snapshot.text);
            } else {
              setMessages((prior) => {
                const answer: LaneMessage = {
                  id: `${jobId}:apocrypha`,
                  role: 'apocrypha',
                  text: snapshot.text || 'Apocrypha completed the thought without words.',
                  at: new Date(),
                  tools: snapshot.tools,
                };
                const at = prior.findIndex((message) => message.id === answer.id);
                return at < 0 ? [...prior, answer] : prior.map((m, i) => (i === at ? answer : m));
              });
              setStreamingText('');
              setStreamingTools([]);
              void refreshConversations();
              // A job can reach a terminal state on the very first poll — recovered from a reload,
              // or simply fast — before the hydration effect below ever had a conversation id to
              // work with. Without this the answer lands alone and the turns before it stay
              // missing. The live ids are protected, so the bounded prefix the server has stored
              // for this turn cannot overwrite the full revision just composed.
              const settled = snapshot.conversationId ?? activeJob.conversationId;
              if (can.durableHistory && settled && hydratedJobRef.current !== jobId) {
                hydratedJobRef.current = jobId;
                void loadConv(settled, [`${jobId}:user`, `${jobId}:apocrypha`]);
              }
            }
            setStreaming(false);
            setActiveJob(null);
            dropStored(activeJobKey(laneId));
            return;
          }
        } catch (pollError) {
          if (controller.signal.aborted || disposed) return;
          setStreamingPhase('Connection interrupted. The job is safe; reconnecting…');
          setError(pollError instanceof Error ? pollError.message : 'Connection interrupted.');
        }
        if (Date.now() > deadline) {
          if (!disposed) {
            setStreaming(false);
            setError('Apocrypha is still working. Your message is saved — reopen this conversation later for the reply.');
          }
          return;
        }
        // Something moved this tick, so the next answer is probably close: ask again soon. Nothing
        // moved, so widen towards the slow cadence rather than hammering a job still in a queue.
        delay = progressed ? JOB_POLL_FAST_MS : Math.min(Math.round(delay * 1.5), JOB_POLL_SLOW_MS);
        await waitFor(controller.signal, delay);
      }
    })();

    return () => { disposed = true; controller.abort(); };
  }, [activeJob, can.durableHistory, lane, laneId, loadConv, refreshConversations]);

  // ── sending ──────────────────────────────────────────────────────────────────────────────────

  const send = useCallback(async (raw: string) => {
    const text = raw.trim().slice(0, MAX_TEXT);
    if (!text || streaming) return;
    setDraft('');
    setError(null);
    setUnresolved(null);
    setStreamingTools([]);
    setStreamingText('');
    following.current = true;
    const submittedAt = new Date();
    // Written BEFORE the request, not after. Between "the browser sent this" and "the server said
    // yes" there is a window in which the tab can close, and a record written only on success loses
    // the message there with no way to tell whether it was received. This record makes that window
    // recoverable: on the next load it is either upgraded with a job id, or offered back.
    const pending: ActiveJob = {
      id: null,
      prompt: text,
      submittedAt: submittedAt.toISOString(),
      conversationId: currentConv,
    };
    writeStored(activeJobKey(laneId), pending);
    const localId = `pending:${pending.submittedAt}`;
    setMessages((prior) => [...prior, { id: localId, role: 'user', text, at: submittedAt }]);
    setStreaming(true);
    setStreamingPhase('Saving your message…');
    try {
      const receipt = await lane.send({ text, conversationId: currentConv, history: messages });
      const record: ActiveJob = { ...pending, id: receipt.jobId, conversationId: receipt.conversationId };
      writeStored(activeJobKey(laneId), record);
      // The blank chat has been used, so the preference is spent. From here a reload should reopen
      // THIS conversation, which is what the default already does.
      dropStored(blankChatKey(laneId));
      if (receipt.conversationId) setCurrentConv(receipt.conversationId);
      // Re-key the local echo to the job so the completed turn replaces it instead of doubling it.
      setMessages((prior) => prior.map((message) => (
        message.id === localId ? { ...message, id: `${receipt.jobId}:user` } : message
      )));
      setActiveJob(record);
      setStreamingPhase('Accepted. Waiting for the Apocrypha node…');
    } catch (sendError) {
      setStreaming(false);
      setStreamingPhase('');
      const refused = isDefiniteRefusal(sendError);
      if (refused) {
        // The server refused it outright, so nothing is in flight: drop the record, take the
        // message back out of the thread, and hand the words back to be edited.
        dropStored(activeJobKey(laneId));
        setMessages((prior) => prior.filter((message) => message.id !== localId));
        setDraft(text);
      } else {
        setUnresolved(pending);
      }
      setError(sendError instanceof Error ? sendError.message : 'That message could not be sent.');
    }
  }, [currentConv, lane, laneId, messages, streaming]);

  // Re-send a turn whose fate was never resolved. Safe because every lane keys submission by a
  // fresh request id and the queues are idempotent on it.
  const retryUnresolved = useCallback(() => {
    if (!unresolved) return;
    const text = unresolved.prompt;
    dropStored(activeJobKey(laneId));
    setMessages((prior) => prior.filter((message) => message.id !== `pending:${unresolved.submittedAt}`));
    setUnresolved(null);
    setError(null);
    void send(text);
  }, [laneId, send, unresolved]);

  const cancelActive = useCallback(async () => {
    if (!activeJob?.id || !lane.cancel) return;
    try {
      await lane.cancel(activeJob.id);
      setStreamingPhase('Cancellation requested…');
    } catch {
      setError('Cancellation could not be delivered. The job remains recoverable.');
    }
  }, [activeJob, lane]);

  // ── scrolling, sizing, sidebar focus ─────────────────────────────────────────────────────────

  useEffect(() => {
    if (following.current) endRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' });
  }, [messages, streamingText, streamingTools]);

  useEffect(() => {
    const box = composerRef.current;
    if (!box) return;
    box.style.height = 'auto';
    box.style.height = `${Math.min(box.scrollHeight, 200)}px`;
  }, [draft]);

  const closeCompactSidebar = useCallback(() => {
    setSidebarOpen(false);
    requestAnimationFrame(() => sidebarToggleRef.current?.focus());
  }, []);

  useEffect(() => {
    if (!compactViewport || !sidebarOpen) return undefined;
    const focusFirst = requestAnimationFrame(() => firstSidebarControlRef.current?.focus());
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      closeCompactSidebar();
    };
    document.addEventListener('keydown', onKeyDown);
    return () => {
      cancelAnimationFrame(focusFirst);
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

  // ── render ───────────────────────────────────────────────────────────────────────────────────

  const empty = messages.length === 0 && !streaming;
  const notice = laneNotice ?? error;

  return <main id="main-content" className={styles.page} style={height ? { height } : undefined}>
    <header className={styles.header}>
      {can.conversations ? <button
        ref={sidebarToggleRef}
        type="button"
        className={styles.iconButton}
        onClick={() => setSidebarOpen((open) => !open)}
        aria-expanded={sidebarOpen}
        aria-controls="apocrypha-conversations"
      >
        <span aria-hidden="true">☰</span>
        <span className={styles.srOnly}>Conversations</span>
      </button> : <Link href="/" className={styles.brand} aria-label="Apocky home">
        <span className="apx-brand-mark" aria-hidden="true" />
      </Link>}

      <div className={styles.title}>
        <h1>Apocrypha</h1>
        <p>{streaming ? 'Thinking…' : error ? 'Reconnecting' : 'Room to think'}</p>
      </div>

      <nav aria-label="Apocrypha navigation" className={styles.nav}>
        {signedIn ? <button
          type="button"
          className={styles.iconButton}
          onClick={() => setSettingsOpen((open) => !open)}
          aria-expanded={settingsOpen}
          aria-controls="apocrypha-settings"
        >Settings</button> : <>
          {inApp ? null : <Link href="/download/apocrypha">Get the app</Link>}
          <Link href="/login?next=%2Fapocrypha" className={styles.signIn}>Sign in</Link>
        </>}
      </nav>
    </header>

    <div className={styles.body}>
      {can.conversations && compactViewport && sidebarOpen ? <button
        type="button"
        className={styles.sidebarBackdrop}
        aria-label="Close conversations"
        onClick={closeCompactSidebar}
      /> : null}

      {can.conversations && sidebarOpen ? <aside
        id="apocrypha-conversations"
        className={styles.sidebar}
        aria-label="Conversations"
        aria-modal={compactViewport || undefined}
        role={compactViewport ? 'dialog' : undefined}
        onKeyDown={handleSidebarKeyDown}
      >
        {can.newConversation ? <button
          ref={firstSidebarControlRef}
          type="button"
          className={styles.newChat}
          onClick={newChat}
        >New chat</button> : null}
        <ul className={styles.convList}>
          {conversations.map((c) => <li key={c.id}>
            <button
              type="button"
              className={c.id === currentConv ? styles.convCurrent : styles.conv}
              aria-current={c.id === currentConv || undefined}
              onClick={() => void loadConv(c.id)}
            >{c.title ?? 'New conversation'}</button>
          </li>)}
        </ul>
        {conversations.length === 0
          ? <p className={styles.convEmpty}>Conversations you start will be listed here.</p>
          : null}
      </aside> : null}

      <section className={styles.conversation} aria-label="Apocrypha conversation">
        {settingsOpen ? <div id="apocrypha-settings" className={styles.settings}>
          <h2>Settings</h2>
          {can.trace ? <label className={styles.settingRow}>
            <input type="checkbox" checked={showTrace} onChange={(e) => setShowTrace(e.target.checked)} />
            Show tool and run trace
          </label> : null}
          {currentConv ? <p className={styles.settingMeta}>
            This conversation<br /><code>{currentConv}</code>
          </p> : null}
          <p className={styles.settingNote}>
            These are display choices only. The model, authority, and security policy remain server-controlled.
          </p>
        </div> : null}

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
            <p>{signedIn ? 'Pick up where you left off, or start something new.' : 'Ask anything. No account needed.'}</p>
            <ul className={styles.openers}>
              {OPENERS.map((opener) => <li key={opener}>
                <button type="button" onClick={() => void send(opener)} disabled={streaming}>{opener}</button>
              </li>)}
            </ul>
          </div> : null}

          {messages.map((message, index) => <article
            key={message.id ?? `${index}-${message.at.getTime()}`}
            className={message.role === 'user' ? styles.you : styles.apocrypha}
          >
            <span className={styles.who}>{message.role === 'user' ? 'You' : 'Apocrypha'}</span>
            <div className={styles.text}>{message.text}</div>
            {can.trace && showTrace && message.tools?.length
              ? <ToolTrace tools={message.tools} />
              : null}
          </article>)}

          {streaming ? <article className={styles.apocrypha}>
            <span className={styles.who}>Apocrypha</span>
            {streamingText
              ? <div className={styles.text}>{streamingText}<span className={styles.caret} aria-hidden="true" /></div>
              : <p className={styles.phase} role="status">{streamingPhase || 'Working…'}</p>}
            {can.trace && showTrace && streamingTools.length ? <ToolTrace tools={streamingTools} /> : null}
          </article> : null}

          <div ref={endRef} />
        </div>

        {notice ? <div className={styles.notice} role="alert">
          <p>{notice}</p>
          <div className={styles.noticeActions}>
            {unresolved ? <button type="button" onClick={retryUnresolved}>Send the same message again</button> : null}
            {/* An expired session cannot be recovered from inside the room, so the notice carries
                the only thing that resolves it rather than leaving the reader to find it. */}
            {notice.startsWith('Your sign-in') ? <Link href="/login?next=%2Fapocrypha">Sign in again</Link> : null}
            {streaming && !can.cancel ? <button type="button" onClick={() => {
              // Stops the browser waiting. It does NOT cancel the job — saying so would be a lie
              // on a lane that has no cancel, and the answer is still being written.
              setStreaming(false);
              setStreamingPhase('');
              setActiveJob(null);
              setError('Stopped waiting. Your message is saved and Apocrypha is still working — reopen this conversation for the reply.');
            }}>Stop waiting</button> : null}
          </div>
        </div> : null}

        <form className={styles.composer} onSubmit={(event) => { event.preventDefault(); void send(draft); }}>
          <label htmlFor="apocrypha-composer" className={styles.srOnly}>Message Apocrypha</label>
          <textarea
            ref={composerRef}
            id="apocrypha-composer"
            aria-label="Message Apocrypha"
            aria-describedby="apocrypha-composer-help"
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
            disabled={streaming}
          />
          {streaming && can.cancel ? <button
            type="button"
            className={styles.stop}
            onClick={() => void cancelActive()}
          >Stop</button> : null}
          {/* A stable accessible name: the visible label changes to "Sending…" mid-flight, and a
              control that renames itself under a screen reader is hard to follow. */}
          <button type="submit" aria-label="Send message" className={styles.send} disabled={streaming || !draft.trim()}>
            {streaming ? 'Sending…' : 'Send'}
          </button>
        </form>

        <p id="apocrypha-composer-help" className={styles.footnote}>
          {can.durableHistory
            ? 'Enter sends, Shift+Enter starts a new line. This conversation is saved to your account.'
            : <>Enter sends, Shift+Enter starts a new line. This conversation stays
                {inApp ? ' on this device' : ' in this browser'} —{' '}
                <Link href="/login?next=%2Fapocrypha">sign in</Link> or{' '}
                <Link href="/register?next=%2Fapocrypha">create an account</Link> to keep it across your devices
                {inApp ? '.' : <>, or <Link href="/download/apocrypha">get the app</Link>.</>}</>}
        </p>
      </section>
    </div>
  </main>;
}

function ToolTrace({ tools }: { tools: readonly ChatToolCall[] }): JSX.Element {
  return <ul className={styles.trace} aria-label="Tool and run trace">
    {tools.map((tool, index) => <li key={`${tool.name}-${index}`} className={tool.ok ? styles.traceOk : styles.traceBad}>
      <span>{tool.name}</span>
      {typeof tool.elapsed_ms === 'number' ? <span>{Math.round(tool.elapsed_ms)}ms</span> : null}
      {tool.error ? <span>{tool.error}</span> : null}
    </li>)}
  </ul>;
}

export default ApocryphaChat;
