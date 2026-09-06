import Link from 'next/link';
import ConversationMessageContent from '@/components/apocrypha/ConversationMessageContent';
import ChatTools from '@/components/apocrypha/ChatTools';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type {
  BrainMemory,
  BrainMessage,
  BrainRuntimeStatus,
  BrainSnapshot,
} from '@/lib/brain/contracts';
import {
  openMiniBrain,
  registerMiniBrainOfflineShell,
  type MiniBrainDeviceRegistration,
  type MiniBrainMessage,
  type MiniBrainState,
  type MiniBrainVault,
} from '@/lib/brain/mini-brain';
import type { MiniBrainSyncResponse } from '@/lib/brain/mobile-contracts';
import {
  apocryphaRelease,
  publicReleaseDownload,
  type ReleaseDocumentBinding,
  type ReleaseDocumentLink,
} from '@/lib/brain/release-manifest';
import { authFetch } from '@/lib/browser-auth';
import { useSiteSession } from '@/components/hub/SiteSession';
import styles from './BrainExperience.module.css';
import BrainDiagnostics from './BrainDiagnostics';

type BrainView = 'graph' | 'timeline' | 'tunnel';
type ServerAccess = 'owner' | 'forbidden' | 'unavailable';

interface RuntimeSessionSummary {
  readonly session_id: string;
  readonly title: string;
  readonly updated_at: string;
  readonly message_count: number;
}

interface RuntimeSessionPage {
  readonly sessions: RuntimeSessionSummary[];
  readonly next_cursor: string | null;
  readonly has_more: boolean;
}

interface ApiError {
  readonly error?: string;
  readonly code?: string;
}

interface InstallPromptEvent extends Event {
  readonly platforms?: readonly string[];
  prompt(): Promise<void>;
  readonly userChoice: Promise<{ outcome: 'accepted' | 'dismissed'; platform: string }>;
}

class BrainApiError extends Error {
  constructor(
    message: string,
    readonly code: string,
    readonly status: number,
    readonly payload: Record<string, unknown>,
  ) {
    super(message);
    this.name = 'BrainApiError';
  }
}

const SESSION_STORAGE_KEY = 'apocky.owner-brain.session.v1';
const OWNER_CONVERSATION_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[45][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

function randomSessionId(): string {
  return crypto.randomUUID();
}

function formattedDate(value: string): string {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  return new Intl.DateTimeFormat('en-US', { dateStyle: 'medium', timeStyle: 'short' }).format(parsed);
}

function short(value: string, maximum = 42): string {
  const normalized = value.replace(/\s+/g, ' ').trim();
  return normalized.length <= maximum ? normalized : `${normalized.slice(0, maximum - 1)}…`;
}

async function jsonRequest<T>(url: string, init: RequestInit = {}): Promise<T> {
  const response = await authFetch(url, { cache: 'no-store', ...init });
  const payload = await response.json() as T & ApiError;
  if (!response.ok) {
    const code = payload.code ?? `HTTP_${response.status}`;
    throw new BrainApiError(
      `${payload.error ?? 'The private Brain could not answer.'} (${code})`,
      code,
      response.status,
      payload as Record<string, unknown>,
    );
  }
  return payload;
}

function memorySearchScore(memory: BrainMemory, terms: readonly string[]): number {
  if (terms.length === 0) return 1;
  const topic = memory.topic_key?.toLowerCase() ?? '';
  const paraphrase = memory.paraphrase.toLowerCase();
  const csl = memory.csl.toLowerCase();
  const queries = memory.search_queries.join(' ').toLowerCase();
  return terms.reduce((score, term) => (
    score
      + (topic.includes(term) ? 8 : 0)
      + (paraphrase.includes(term) ? 5 : 0)
      + (queries.includes(term) ? 3 : 0)
      + (csl.includes(term) ? 2 : 0)
  ), 0);
}

function filterMemories(memories: readonly BrainMemory[], query: string): BrainMemory[] {
  const terms = [...new Set(query.toLowerCase().split(/[^a-z0-9]+/).filter(term => term.length > 1))];
  return memories
    .map(memory => ({ memory, score: memorySearchScore(memory, terms) }))
    .filter(item => terms.length === 0 || item.score > 0)
    .sort((left, right) => right.score - left.score || right.memory.created_at.localeCompare(left.memory.created_at))
    .map(item => item.memory);
}

function sourceMessages(memory: BrainMemory, messages: readonly BrainMessage[]): BrainMessage[] {
  const ids = new Set(memory.source_msg_ids);
  return messages.filter(message => ids.has(message.id));
}

function sessionSummaries(value: unknown): RuntimeSessionSummary[] {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return [];
  const raw = (value as Record<string, unknown>).sessions;
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((entry) => {
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) return [];
    const row = entry as Record<string, unknown>;
    if (
      typeof row.session_id !== 'string'
      || !OWNER_CONVERSATION_UUID.test(row.session_id)
      || typeof row.title !== 'string'
      || typeof row.updated_at !== 'string'
      || typeof row.message_count !== 'number'
    ) return [];
    return [{
      session_id: row.session_id,
      title: row.title,
      updated_at: row.updated_at,
      message_count: row.message_count,
    }];
  });
}

function sessionListing(value: unknown): RuntimeSessionPage {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Conversation history response is invalid.');
  const row = value as Record<string, unknown>;
  const sessions = sessionSummaries(value);
  const cursor = row.next_cursor;
  const cursorParts = typeof cursor === 'string' ? cursor.split(':') : [];
  const cursorId = /^[0-9a-f]{8}-[0-9a-f]{4}-[45][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
  if (row.schema_version !== 'apocky.owner-brain.sessions.v1' || row.status !== 'live'
    || row.discovery_scope !== 'owner_conversations_page' || row.history_surface !== 'g12_chat_history'
    || !Array.isArray(row.sessions) || sessions.length !== row.sessions.length || sessions.length > 32
    || row.count !== sessions.length || new Set(sessions.map(session => session.session_id)).size !== sessions.length
    || typeof row.has_more !== 'boolean'
    || (row.has_more ? sessions.length === 0 || typeof cursor !== 'string' || cursor.length !== 77
      || cursorParts.length !== 3 || cursorParts[0] !== 'cs1' || !cursorParts.slice(1).every(id => cursorId.test(id))
      : cursor !== null)) throw new Error('Conversation history response is invalid.');
  return { sessions, next_cursor: cursor as string | null, has_more: row.has_more };
}

async function bindMiniBrainDevice(vault: MiniBrainVault): Promise<MiniBrainState> {
  const registration = await jsonRequest<MiniBrainDeviceRegistration>('/api/brain/mobile/device', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ device_id: vault.deviceId, public_key_jwk: vault.publicKeyJwk }),
  });
  return vault.bind(registration);
}

async function syncMiniBrain(
  vault: MiniBrainVault,
  state: MiniBrainState,
  input: {
    readonly operation: 'pull' | 'append';
    readonly sessionId: string;
    readonly requestId: string;
    readonly baseCursor: string | null;
    readonly text?: string;
    readonly signal?: AbortSignal;
  },
): Promise<MiniBrainState> {
  return vault.deliverSync(state, {
    operation: input.operation,
    sessionId: input.sessionId,
    requestId: input.requestId,
    baseCursor: input.baseCursor,
    payload: input.operation === 'append' ? { text: input.text ?? '' } : null,
  }, (request, signal) => jsonRequest<MiniBrainSyncResponse>('/api/brain/mobile/sync', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request),
    signal,
  }), { signal: input.signal });
}

function miniConversation(state: MiniBrainState | null): readonly MiniBrainMessage[] {
  return state?.sessions.find(session => session.session_id === state.current_session_id)?.messages.filter(message => message.origin !== 'local-reflection') ?? [];
}

function miniCursor(state: MiniBrainState, sessionId: string): string | null {
  return state.sessions.find(session => session.session_id === sessionId)?.cursor ?? null;
}

function Connector({ label, state, detail }: { label: string; state: string; detail: string }): JSX.Element {
  return (
    <div className={styles.connector} data-state={state}>
      <span aria-hidden="true" />
      <div><strong>{label}</strong><small>{detail}</small></div>
    </div>
  );
}

function ReleaseShelf(): JSX.Element {
  if (apocryphaRelease.status === 'degraded') {
    return (
      <section id="brain-releases" className={styles.releaseShelf} aria-labelledby="brain-release-title">
        <div className={styles.releaseUnavailable}>
          <div><p>VERSIONED RELEASE SHELF</p><h2 id="brain-release-title">Release evidence unavailable</h2></div>
          <code>{apocryphaRelease.code}</code>
        </div>
      </section>
    );
  }
  const manifest = apocryphaRelease.manifest;
  const downloadable = publicReleaseDownload(manifest);
  const documents: readonly (ReleaseDocumentBinding | ReleaseDocumentLink)[] = [
    manifest.documents.plan,
    manifest.documents.changelog,
    manifest.documents.manifest,
  ];
  return (
    <section id="brain-releases" className={styles.releaseShelf} aria-labelledby="brain-release-title">
      <details>
        <summary>
          <span><b id="brain-release-title">Release shelf</b><small>{manifest.version} · integrity-linked evidence</small></span>
          <em data-release-state={manifest.release_state}>{manifest.release_label}</em>
        </summary>
        <div className={styles.releaseBody}>
          <p className={styles.releaseBoundary}>{manifest.claim_boundary}</p>
          <dl className={styles.releaseMeta}>
            <div><dt>Version</dt><dd>{manifest.version}</dd></div>
            <div><dt>Build</dt><dd>{manifest.build.state}</dd></div>
            <div><dt>Gate</dt><dd>{manifest.build.release_gate}</dd></div>
            <div><dt>Manifest integrity</dt><dd title={manifest.content_digest}>{manifest.content_digest.slice(0, 14)}…</dd></div>
          </dl>
          <nav className={styles.releaseLinks} aria-label="Apocrypha release evidence">
            {documents.map(document => (
              <a key={document.href} href={document.href}>
                <strong>{document.label}</strong>
                <small>{'sha256' in document ? `SHA-256 ${document.sha256.slice(0, 10)}… · ${document.bytes.toLocaleString()} bytes` : 'Canonical public-safe JSON'}</small>
              </a>
            ))}
          </nav>
          {downloadable ? (
            <a className={styles.releaseDownload} href={downloadable.href} download>
              <strong>Download {downloadable.filename}</strong>
              <small>{downloadable.platform} · {downloadable.bytes.toLocaleString()} bytes · SHA-256 {downloadable.sha256.slice(0, 12)}… · signature verified</small>
            </a>
          ) : (
            <div className={styles.releaseHold}>
              <strong>Get Apocrypha for your phone</strong>
              <p><Link href="/download/apocrypha">Android downloads and iPhone availability</Link></p>
              <p>This shelf describes the browser release. The download page lists each native app and its current release status.</p>
            </div>
          )}
        </div>
      </details>
    </section>
  );
}

function BrainGraph({
  memories,
  selectedId,
  onSelect,
}: {
  memories: readonly BrainMemory[];
  selectedId: string | null;
  onSelect: (memory: BrainMemory) => void;
}): JSX.Element {
  const nodes = memories.slice(0, 18).map((memory, index, all) => {
    const angle = (-Math.PI / 2) + (index / Math.max(all.length, 1)) * Math.PI * 2;
    const radius = index % 3 === 0 ? 29 : index % 2 === 0 ? 39 : 47;
    return {
      memory,
      x: 50 + Math.cos(angle) * radius,
      y: 50 + Math.sin(angle) * radius,
    };
  });
  const edges = nodes.flatMap((node, index) => nodes.slice(index + 1).flatMap(other => (
    node.memory.topic_key && node.memory.topic_key === other.memory.topic_key ? [{ node, other }] : []
  ))).slice(0, 30);

  if (nodes.length === 0) {
    return <div className={styles.emptyPanel}>No memories match this local recall filter.</div>;
  }

  return (
    <div>
      <div className={styles.graphPlane} aria-label="Memory graph">
        <svg viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
          {edges.map(({ node, other }) => (
            <line
              key={`${node.memory.id}-${other.memory.id}`}
              x1={node.x}
              y1={node.y}
              x2={other.x}
              y2={other.y}
            />
          ))}
        </svg>
        {nodes.map(({ memory, x, y }) => (
          <button
            type="button"
            key={memory.id}
            className={styles.graphNode}
            data-selected={memory.id === selectedId ? 'true' : undefined}
            data-type={memory.type}
            style={{ left: `${x}%`, top: `${y}%` }}
            onClick={() => onSelect(memory)}
            title={memory.paraphrase}
          >
            <span aria-hidden="true" />
            <small>{short(memory.topic_key ?? memory.type, 19)}</small>
          </button>
        ))}
      </div>
      <div className={styles.legend}>
        <span><i className={styles.legendLine} /> same exact topic</span>
        <span><i className={styles.legendNode} /> memory record</span>
        <span>Showing {nodes.length}/{memories.length} filtered records</span>
      </div>
      <ol className={styles.accessibleGraph} aria-label="Memory graph as a readable list">
        {nodes.map(({ memory }) => (
          <li key={memory.id}>
            <button type="button" onClick={() => onSelect(memory)}>
              <strong>{memory.topic_key ?? memory.type}</strong><span>{short(memory.paraphrase, 74)}</span>
            </button>
          </li>
        ))}
      </ol>
    </div>
  );
}

function Timeline({ memories, messages, onSelect }: {
  memories: readonly BrainMemory[];
  messages: readonly BrainMessage[];
  onSelect: (memory: BrainMemory) => void;
}): JSX.Element {
  const events = [
    ...memories.map(memory => ({ kind: 'memory' as const, at: memory.created_at, memory })),
    ...messages.filter(message => !message.source_only).map(message => ({ kind: 'message' as const, at: message.ts, message })),
  ].sort((left, right) => right.at.localeCompare(left.at)).slice(0, 80);
  if (events.length === 0) return <div className={styles.emptyPanel}>No timeline records are available in this bounded projection.</div>;
  return (
    <ol className={styles.timeline}>
      {events.map((event, index) => (
        <li key={event.kind === 'memory' ? event.memory.id : `${event.message.id}-${index}`}>
          <time dateTime={event.at}>{formattedDate(event.at)}</time>
          {event.kind === 'memory' ? (
            <button type="button" onClick={() => onSelect(event.memory)}>
              <span>{event.memory.type} · {event.memory.topic_key ?? 'unclassified'}</span>
              <strong>{event.memory.paraphrase}</strong>
            </button>
          ) : (
            <div>
              <span>source message · {event.message.role}</span>
              <strong>{short(event.message.content, 150)}</strong>
            </div>
          )}
        </li>
      ))}
    </ol>
  );
}

function Tunnel({ memory, memories, messages, onSelect }: {
  memory: BrainMemory | null;
  memories: readonly BrainMemory[];
  messages: readonly BrainMessage[];
  onSelect: (memory: BrainMemory) => void;
}): JSX.Element {
  if (!memory) return <div className={styles.emptyPanel}>Choose a graph node or recall result to open its provenance tunnel.</div>;
  const sources = sourceMessages(memory, messages);
  const neighbors = memory.topic_key
    ? memories.filter(candidate => candidate.id !== memory.id && candidate.topic_key === memory.topic_key).slice(0, 8)
    : [];
  return (
    <article className={styles.tunnel}>
      <header><span>{memory.type}</span><time dateTime={memory.created_at}>{formattedDate(memory.created_at)}</time></header>
      <h3>{memory.topic_key ?? 'Unclassified memory'}</h3>
      <p>{memory.paraphrase}</p>
      <details>
        <summary>Canonical CSL record</summary>
        <code>{memory.csl}</code>
      </details>
      <section aria-labelledby="brain-source-title">
        <h4 id="brain-source-title">Source-linked messages</h4>
        {sources.length > 0 ? sources.map(source => (
          <blockquote key={source.id} id={`brain-source-${source.id}`}>
            <p>{source.content}</p>
            <footer>{source.role} · {formattedDate(source.ts)} · <code>{source.id}</code></footer>
          </blockquote>
        )) : (
          <p className={styles.muted}>This record carries {memory.source_msg_ids.length} source reference{memory.source_msg_ids.length === 1 ? '' : 's'}, but none are inside the bounded source projection.</p>
        )}
      </section>
      <section aria-labelledby="brain-neighbor-title">
        <h4 id="brain-neighbor-title">Same-topic neighbors</h4>
        {neighbors.length > 0 ? (
          <ul className={styles.neighbors}>{neighbors.map(neighbor => (
            <li key={neighbor.id}><button type="button" onClick={() => onSelect(neighbor)}>{short(neighbor.paraphrase, 96)}</button></li>
          ))}</ul>
        ) : <p className={styles.muted}>No other loaded record has this exact topic key.</p>}
      </section>
      <p className={styles.projectionNote}>This tunnel preserves exact topic, time, CSL, and source-message links. It omits records outside the bounded private snapshot.</p>
    </article>
  );
}

export default function BrainExperience({ serverAccess }: { serverAccess: ServerAccess }): JSX.Element {
  const { access, refresh, subjectKey } = useSiteSession();
  const [snapshot, setSnapshot] = useState<BrainSnapshot | null>(null);
  const [runtime, setRuntime] = useState<BrainRuntimeStatus | null>(null);
  const [view, setView] = useState<BrainView>('graph');
  const [query, setQuery] = useState('');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [memoryError, setMemoryError] = useState('');
  const [memoryProvisionable, setMemoryProvisionable] = useState(false);
  const [provisioningMemory, setProvisioningMemory] = useState(false);
  const [syncNotice, setSyncNotice] = useState('');
  const [loading, setLoading] = useState(serverAccess === 'owner');
  const [runtimeLoading, setRuntimeLoading] = useState(false);
  const [sessions, setSessions] = useState<readonly RuntimeSessionSummary[]>([]);
  const [historyCursor, setHistoryCursor] = useState<string | null>(null);
  const [historyLoading, setHistoryLoading] = useState(false);
  const historyReadRef = useRef<Promise<RuntimeSessionSummary[]> | null>(null);
  const historyAbortRef = useRef<AbortController | null>(null);
  const [miniState, setMiniState] = useState<MiniBrainState | null>(null);
  const [miniStatus, setMiniStatus] = useState<'initializing' | 'ready' | 'unbound' | 'unavailable'>('initializing');
  const [online, setOnline] = useState(true);
  const [installPrompt, setInstallPrompt] = useState<InstallPromptEvent | null>(null);
  const [installed, setInstalled] = useState(false);
  const [activePanel, setActivePanel] = useState<'history' | 'memory' | 'settings' | 'tools' | null>(null);
  const auxiliaryRef = useRef<HTMLDivElement | null>(null);
  const panelTriggerRef = useRef<HTMLButtonElement | null>(null);
  const composerRef = useRef<HTMLTextAreaElement | null>(null);
  const messageLogRef = useRef<HTMLDivElement | null>(null);
  const followedLogRef = useRef<HTMLDivElement | null>(null);
  const followedSessionRef = useRef<string | null | undefined>(undefined);
  const nearLatestRef = useRef(true);
  const [showLatest, setShowLatest] = useState(false);
  const scrollToLatest = useCallback(() => {
    const log = messageLogRef.current;
    if (!log) return;
    log.scrollTop = log.scrollHeight;
    nearLatestRef.current = true;
    setShowLatest(false);
  }, []);
  const closePanel = useCallback(() => {
    setActivePanel(null);
    requestAnimationFrame(() => {
      const trigger = panelTriggerRef.current;
      if (trigger?.isConnected && !trigger.closest('[hidden]')) trigger.focus();
      else composerRef.current?.focus();
    });
  }, []);
  const togglePanel = (panel: 'history' | 'memory' | 'settings' | 'tools', trigger: HTMLButtonElement): void => {
    if (activePanel === panel) { closePanel(); return; }
    panelTriggerRef.current = trigger;
    setActivePanel(panel);
  };
  useEffect(() => {
    if (!activePanel) return;
    const frame = requestAnimationFrame(() => auxiliaryRef.current?.querySelector<HTMLButtonElement>('[data-panel-active="true"] [data-panel-close]')?.focus());
    const escape = (event: KeyboardEvent): void => { if (event.key === 'Escape') { event.preventDefault(); closePanel(); } };
    document.addEventListener('keydown', escape);
    return () => { cancelAnimationFrame(frame); document.removeEventListener('keydown', escape); };
  }, [activePanel, closePanel]);
  const [offlineShellReady, setOfflineShellReady] = useState(false);
  const [syncConflict, setSyncConflict] = useState(false);
  const [draft, setDraft] = useState('');
  const [sending, setSending] = useState(false);
  const vaultRef = useRef<MiniBrainVault | null>(null);
  const [observedVault, setObservedVault] = useState<MiniBrainVault | null>(null);
  const stateRef = useRef<MiniBrainState | null>(null);
  const sendFlightRef = useRef(false);
  const queueFlightRef = useRef<{ vault: MiniBrainVault; promise: Promise<MiniBrainState> } | null>(null);

  const commitState = useCallback((state: MiniBrainState): MiniBrainState => {
    setMiniStatus('ready');
    const current = stateRef.current;
    if (current && current.owner_ref === state.owner_ref && current.device_id === state.device_id
      && (state.revision ?? 0) <= (current.revision ?? 0)) return current;
    stateRef.current = state;
    setMiniState(state);
    return state;
  }, []);

  const loadSessionPage = useCallback(async (cursor: string | null = null, signal?: AbortSignal): Promise<RuntimeSessionSummary[]> => {
    if (historyReadRef.current) return historyReadRef.current;
    const controller = new AbortController();
    historyAbortRef.current = controller;
    const cancel = (): void => controller.abort();
    if (signal?.aborted) controller.abort();
    signal?.addEventListener('abort', cancel, { once: true });
    const timer = setTimeout(cancel, 45_000);
    setHistoryLoading(true);
    const read = (async (): Promise<RuntimeSessionSummary[]> => {
      const query = new URLSearchParams();
      if (cursor !== null) query.set('cursor', cursor);
      query.set('limit', '24');
      const page = sessionListing(await jsonRequest<unknown>('/api/brain/runtime/sessions?' + query.toString(), { signal: controller.signal }));
      if (controller.signal.aborted) throw new Error('Conversation history refresh was cancelled.');
      setSessions(current => {
        const ordered = cursor === null ? [...page.sessions, ...current] : [...current, ...page.sessions];
        const seen = new Set<string>();
        return ordered.filter(session => !seen.has(session.session_id) && Boolean(seen.add(session.session_id)));
      });
      setHistoryCursor(page.next_cursor);
      return page.sessions;
    })();
    historyReadRef.current = read;
    try { return await read; }
    finally {
      clearTimeout(timer);
      signal?.removeEventListener('abort', cancel);
      historyReadRef.current = null;
      historyAbortRef.current = null;
      setHistoryLoading(false);
    }
  }, []);

  useEffect(() => () => historyAbortRef.current?.abort(), []);

  const pullSession = useCallback(async (
    vault: MiniBrainVault,
    state: MiniBrainState,
    sessionId: string,
    signal?: AbortSignal,
  ): Promise<MiniBrainState> => {
    return syncMiniBrain(vault, state, {
      operation: 'pull',
      sessionId,
      signal,
      requestId: randomSessionId(),
      baseCursor: miniCursor(state, sessionId),
    });
  }, []);

  const flushQueue = useCallback((
    vault: MiniBrainVault,
    initial: MiniBrainState,
    rebase = false,
    signal?: AbortSignal,
  ): Promise<MiniBrainState> => {
    const active = queueFlightRef.current;
    if (active?.vault === vault) return active.promise;
    const run = (async () => {
    let state = await vault.load() ?? initial;
    setSyncConflict(false);
    for (const turn of [...state.queue]) {
      try {
        state = await syncMiniBrain(vault, state, {
          operation: 'append',
          sessionId: turn.session_id,
          requestId: turn.request_id,
          baseCursor: rebase ? miniCursor(state, turn.session_id) : turn.base_cursor,
          text: turn.text,
          signal,
        });
        commitState(state);
      } catch (error) {
        if (error instanceof BrainApiError && (error.code === 'BRAIN_SYNC_CONFLICT' || error.code === 'BRAIN_SYNC_REMOTE_ABSENT')) {
          try {
            state = await pullSession(vault, state, turn.session_id);
            commitState(state);
          } catch { /* preserve the encrypted queue and original conflict */ }
          setSyncConflict(true);
          setSyncNotice('Desktop history changed while this device was away. The latest history is loaded; review it, then choose “Retry on current history.”');
          return state;
        }
        setSyncNotice(error instanceof Error ? error.message : 'Encrypted turns remain queued for the next connection.');
        return state;
      }
    }
    const failed = state.sessions.some(session => session.messages.some(message => message.terminal_failure));
    setSyncNotice(state.queue.length === 0
      ? failed ? 'History is synchronized. Messages without replies are marked in their conversations.'
        : 'Device queue and desktop worldline are current.'
      : `${state.queue.length} encrypted turn${state.queue.length === 1 ? '' : 's'} remain queued.`);
    return state;
    })();
    const promise = run.finally(() => {
      if (queueFlightRef.current?.promise === promise) queueFlightRef.current = null;
    });
    queueFlightRef.current = { vault, promise };
    return promise;
  }, [commitState, pullSession]);

  const load = useCallback(async (): Promise<void> => {
    setLoading(true);
    setMemoryError('');
    setMemoryProvisionable(false);
    setSyncNotice('');
    try {
      const opened = await openMiniBrain();
      if (!opened.vault) {
        setMiniStatus('unavailable');
        setSyncNotice(`${opened.reason_code ?? 'MINI_BRAIN_OPEN_FAILED'} · encrypted local continuity is unavailable in this browser.`);
        return;
      }
      const vault = opened.vault;
      vaultRef.current = vault;
      setObservedVault(vault);
      let state = await vault.load();
      if (online && access === 'owner' && (!vault.isBound || vault.tokenExpired)) {
        try {
          state = await bindMiniBrainDevice(vault);
        } catch (error) {
          setMiniStatus(vault.isBound ? 'ready' : 'unbound');
          setSyncNotice(error instanceof Error ? error.message : 'This browser could not renew its owner/device binding.');
        }
      }
      if (!state && vault.isBound) state = await vault.freshState();
      if (state) commitState(state);
      else setMiniStatus('unbound');

      if (!online) {
        setRuntime({
          schema_version: 'apocky.owner-brain.runtime-status.v1',
          status: 'degraded', reason_code: 'BRAIN_OFFLINE', observed_at: new Date().toISOString(),
          latency_ms: null, upstream_status: null, served_by: 'device', ts: new Date().toISOString(),
        });
        setSyncNotice(state ? 'Desktop connection unavailable. Your message will stay encrypted here until it can be delivered.' : 'Offline · this browser has not completed owner/device binding yet.');
        return;
      }

      const [memoryResult, runtimeResult] = await Promise.allSettled([
        jsonRequest<BrainSnapshot>('/api/brain/snapshot'),
        jsonRequest<BrainRuntimeStatus>('/api/brain/runtime/status'),
      ]);
      if (memoryResult.status === 'fulfilled') {
        setSnapshot(memoryResult.value);
        setSelectedId(current => current ?? memoryResult.value.memories[0]?.id ?? null);
        if (state && vault.isBound) {
          state = await vault.cacheSnapshot(state, memoryResult.value);
          commitState(state);
        }
      } else {
        setSnapshot(null);
        setMemoryError(memoryResult.reason instanceof Error ? memoryResult.reason.message : 'Private Mneme storage is unavailable.');
        setMemoryProvisionable(memoryResult.reason instanceof BrainApiError && memoryResult.reason.code === 'MNEME_PROFILE_NOT_PROVISIONED');
      }
      if (runtimeResult.status === 'rejected') {
        setRuntime({
          schema_version: 'apocky.owner-brain.runtime-status.v1',
          status: 'degraded', reason_code: 'BRAIN_RUNTIME_STATUS_UNAVAILABLE', observed_at: new Date().toISOString(),
          latency_ms: null, upstream_status: null, served_by: 'edge', ts: new Date().toISOString(),
        });
        setSyncNotice(runtimeResult.reason instanceof Error ? runtimeResult.reason.message : 'Desktop status is unavailable.');
        return;
      }
      const runtimePayload = runtimeResult.value;
      setRuntime(runtimePayload);
      if (runtimePayload.status === 'live' && state && vault.isBound) {
        setRuntimeLoading(true);
        try {
          const remoteSessions = await loadSessionPage().catch(error => {
            setSyncNotice(error instanceof Error ? error.message : 'Conversation history is unavailable.');
            return [];
          });
          let saved = '';
          try { saved = sessionStorage.getItem(SESSION_STORAGE_KEY) ?? ''; } catch { saved = ''; }
          const chosen = remoteSessions.find(item => item.session_id === saved)?.session_id
            ?? remoteSessions.find(item => item.session_id === state?.current_session_id)?.session_id
            ?? remoteSessions[0]?.session_id
            ?? state.current_session_id;
          state = await vault.adoptDiscoveredSession(state, chosen);
          commitState(state);
          state = await pullSession(vault, state, state.current_session_id);
          commitState(state);
          state = await flushQueue(vault, state);
          commitState(state);
        } catch (runtimeError) {
          setSyncNotice(runtimeError instanceof Error ? runtimeError.message : 'Desktop history could not synchronize.');
        } finally { setRuntimeLoading(false); }
      }
    } catch (loadError) {
      setMiniStatus('unavailable');
      setSyncNotice(loadError instanceof Error ? loadError.message : 'The encrypted Mini Brain could not open.');
    } finally {
      setLoading(false);
    }
  }, [access, commitState, flushQueue, loadSessionPage, online, pullSession]);

  useEffect(() => {
    if (serverAccess !== 'owner' || (online && access === 'checking')) return;
    if (online && access !== 'owner') {
      setLoading(false);
      return;
    }
    void load();
  }, [access, load, online, serverAccess]);

  useEffect(() => {
    if (!observedVault || serverAccess !== 'owner' || (online && access !== 'owner')) return;
    let disposed = false;
    let localRead: Promise<MiniBrainState | null> | null = null;
    let readAgain = false;
    let remoteRead = false;
    let activeAbort: AbortController | null = null;
    const readLocal = (): Promise<MiniBrainState | null> => {
      if (localRead) { readAgain = true; return localRead; }
      localRead = observedVault.load().then(state => {
        if (!disposed && state) commitState(state);
        return state;
      }).catch(error => {
        if (!disposed) setSyncNotice(error instanceof Error ? error.message : 'Saved messages could not refresh.');
        return null;
      }).finally(() => {
        localRead = null;
        if (readAgain && !disposed) { readAgain = false; void readLocal(); }
      });
      return localRead;
    };
    const refreshVisible = async (): Promise<void> => {
      if (disposed || document.visibilityState === 'hidden' || remoteRead) return;
      remoteRead = true;
      const abort = new AbortController();
      activeAbort = abort;
      const timer = setTimeout(() => abort.abort(new Error('Connection refresh timed out.')), 120_000);
      try {
        const state = await readLocal();
        if (!state || !online || access !== 'owner' || disposed) return;
        const status = await jsonRequest<BrainRuntimeStatus>('/api/brain/runtime/status', { signal: abort.signal });
        if (!disposed) setRuntime(status);
        if (status.status === 'live' && !disposed) {
          const [listing, conversation] = await Promise.allSettled([
            loadSessionPage(null, abort.signal),
            pullSession(observedVault, state, state.current_session_id, abort.signal),
          ]);
          if (conversation.status === 'rejected') throw conversation.reason;
          if (!disposed) {
            commitState(conversation.value);
            if (!syncConflict && conversation.value.queue.length > 0) {
              const synchronized = await flushQueue(observedVault, conversation.value, false, abort.signal);
              if (!disposed && !abort.signal.aborted) commitState(synchronized);
            }
            if (listing.status === 'rejected' && !abort.signal.aborted) {
              setSyncNotice(listing.reason instanceof Error ? listing.reason.message : 'Conversation history is unavailable.');
            }
          }
        }
      } catch (error) {
        if (!disposed && !abort.signal.aborted) setSyncNotice(error instanceof Error ? error.message : 'The conversation could not refresh.');
      } finally { clearTimeout(timer); activeAbort = null; remoteRead = false; }
    };
    // Broadcasts only reread the committed vault; they never pull, write or rebroadcast.
    const unsubscribe = observedVault.subscribe(() => { void readLocal(); });
    let pollTimer: ReturnType<typeof setInterval> | null = null;
    const stopPolling = (): void => {
      if (pollTimer !== null) clearInterval(pollTimer);
      pollTimer = null;
    };
    const startPolling = (): void => {
      stopPolling();
      if (!online || access !== 'owner' || document.visibilityState === 'hidden') return;
      pollTimer = setInterval(() => {
        const state = stateRef.current;
        const session = state?.sessions.find(item => item.session_id === state.current_session_id);
        if (state?.queue.length || (session && (session.cursor || session.messages.length > 0))) void refreshVisible();
      }, 30_000);
    };
    const onVisible = (): void => {
      if (document.visibilityState === 'hidden') {
        stopPolling();
        activeAbort?.abort();
        return;
      }
      startPolling();
      void refreshVisible();
    };
    window.addEventListener('focus', onVisible);
    document.addEventListener('visibilitychange', onVisible);
    startPolling();
    void readLocal();
    return () => {
      disposed = true;
      stopPolling();
      activeAbort?.abort();
      unsubscribe();
      window.removeEventListener('focus', onVisible);
      document.removeEventListener('visibilitychange', onVisible);
    };
  }, [access, commitState, flushQueue, loadSessionPage, observedVault, online, pullSession, serverAccess, syncConflict]);

  useEffect(() => {
    setOnline(navigator.onLine);
    const onOnline = (): void => setOnline(true);
    const onOffline = (): void => setOnline(false);
    window.addEventListener('online', onOnline);
    window.addEventListener('offline', onOffline);
    return () => {
      window.removeEventListener('online', onOnline);
      window.removeEventListener('offline', onOffline);
    };
  }, []);

  useEffect(() => {
    if (serverAccess !== 'owner' || !('serviceWorker' in navigator)) return undefined;
    void registerMiniBrainOfflineShell()
      .then(setOfflineShellReady)
      .catch(() => {
        setOfflineShellReady(false);
        setSyncNotice('Install shell unavailable; the encrypted browser vault still works while this page stays open.');
      });
    const media = window.matchMedia('(display-mode: standalone)');
    const updateInstalled = (): void => setInstalled(media.matches || Boolean((navigator as Navigator & { standalone?: boolean }).standalone));
    updateInstalled();
    media.addEventListener?.('change', updateInstalled);
    const beforeInstall = (event: Event): void => {
      event.preventDefault();
      setInstallPrompt(event as InstallPromptEvent);
    };
    window.addEventListener('beforeinstallprompt', beforeInstall);
    return () => {
      media.removeEventListener?.('change', updateInstalled);
      window.removeEventListener('beforeinstallprompt', beforeInstall);
    };
  }, [serverAccess]);

  const filteredMemories = useMemo(() => filterMemories(snapshot?.memories ?? [], query), [query, snapshot]);
  const selected = snapshot?.memories.find(memory => memory.id === selectedId) ?? filteredMemories[0] ?? null;

  const selectMemory = useCallback((memory: BrainMemory) => {
    setSelectedId(memory.id);
    setView('tunnel');
  }, []);

  const newConversation = async (): Promise<void> => {
    const vault = vaultRef.current;
    const current = stateRef.current;
    if (!vault || !current) return;
    const next = randomSessionId();
    const state = await vault.selectSession(current, next);
    commitState(state);
    setDraft('');
    try { sessionStorage.setItem(SESSION_STORAGE_KEY, next); } catch { /* private mode can deny storage */ }
  };

  const chooseSession = async (sessionId: string): Promise<void> => {
    const vault = vaultRef.current;
    const current = stateRef.current;
    if (!vault || !current) return;
    setRuntimeLoading(true);
    try {
      const selected = await vault.selectSession(current, sessionId);
      commitState(selected);
      try { sessionStorage.setItem(SESSION_STORAGE_KEY, sessionId); } catch { /* private mode can deny storage */ }
      if (online && runtime?.status === 'live') {
        commitState(await pullSession(vault, selected, sessionId));
      }
    } catch (error) {
      setSyncNotice(error instanceof Error ? error.message : 'This worldline could not open.');
    } finally { setRuntimeLoading(false); }
  };

  const send = async (event: React.FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    const text = draft.trim();
    const vault = vaultRef.current;
    const current = stateRef.current;
    if (!text || !vault || !current || sending || sendFlightRef.current) return;
    sendFlightRef.current = true;
    setSending(true);
    setSyncNotice('');
    setDraft('');
    try {
      const queued = await vault.queueTurn(current, text);
      commitState(queued.state);
      setSyncNotice(online && runtime?.status === 'live'
        ? 'Encrypted on this device · synchronizing with desktop…'
        : 'Desktop connection unavailable. Your message will stay encrypted here until it can be delivered.');
      if (online && runtime?.status === 'live') {
        const synchronized = await flushQueue(vault, queued.state);
        commitState(synchronized);
      }
    } catch (sendError) {
      setDraft(text);
      setSyncNotice(sendError instanceof Error ? sendError.message : 'The local turn could not be encrypted.');
    } finally {
      sendFlightRef.current = false;
      setSending(false);
    }
  };

  const retryConflict = async (): Promise<void> => {
    const vault = vaultRef.current;
    const current = stateRef.current;
    if (!vault || !current || !online || runtime?.status !== 'live') return;
    setSending(true);
    try {
      const synchronized = await flushQueue(vault, current, true);
      commitState(synchronized);
    } finally { setSending(false); }
  };

  const install = async (): Promise<void> => {
    if (!installPrompt) return;
    await installPrompt.prompt();
    const choice = await installPrompt.userChoice;
    if (choice.outcome === 'accepted') setInstalled(true);
    setInstallPrompt(null);
  };

  const syncQueued = async (): Promise<void> => {
    const vault = vaultRef.current;
    const current = stateRef.current;
    if (!vault || !current || !online || runtime?.status !== 'live') return;
    setSending(true);
    try {
      const synchronized = await flushQueue(vault, current);
      commitState(synchronized);
    } finally { setSending(false); }
  };

  const provisionMneme = async (): Promise<void> => {
    if (!memoryProvisionable || provisioningMemory) return;
    setProvisioningMemory(true);
    setSyncNotice('Creating the owner-bound private Mneme profile…');
    try {
      await jsonRequest('/api/brain/mneme/bootstrap', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ confirmation: 'CREATE_OWNER_PRIVATE_MNEME_PROFILE' }),
      });
      await load();
      setSyncNotice('Private Mneme profile created for this verified owner session. It is empty until you choose what to remember.');
    } catch (error) {
      setSyncNotice(error instanceof Error ? error.message : 'The private Mneme profile was not created.');
    } finally {
      setProvisioningMemory(false);
    }
  };

  const eraseOfflineCopy = async (): Promise<void> => {
    const vault = vaultRef.current;
    if (!vault || !window.confirm('Erase this browser’s encrypted Mini Brain, device binding, and offline shell? Desktop history is not deleted.')) return;
    await vault.erase();
    window.location.assign('/');
  };

  const conversation = miniConversation(miniState);
  const desktopConnected = online && runtime?.status === 'live';
  const sessionId = miniState?.current_session_id ?? null;
  const latestMessageId = conversation[conversation.length - 1]?.id;
  useEffect(() => {
    const log = messageLogRef.current;
    if (!log) return;
    const reset = followedLogRef.current !== log || followedSessionRef.current !== sessionId;
    followedLogRef.current = log;
    followedSessionRef.current = sessionId;
    const frame = requestAnimationFrame(() => {
      if (reset || nearLatestRef.current) scrollToLatest();
      else setShowLatest(true);
    });
    return () => cancelAnimationFrame(frame);
  }, [sessionId, latestMessageId, conversation.length, loading, sending, scrollToLatest]);
  useEffect(() => {
    const log = messageLogRef.current;
    if (!log || typeof ResizeObserver === 'undefined') return;
    let frame: number | undefined;
    const observer = new ResizeObserver(() => {
      if (!nearLatestRef.current) return;
      if (frame !== undefined) cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => { if (nearLatestRef.current) scrollToLatest(); });
    });
    observer.observe(log);
    return () => { observer.disconnect(); if (frame !== undefined) cancelAnimationFrame(frame); };
  }, [loading, sessionId, scrollToLatest]);
  const isIos = typeof navigator !== 'undefined' && /iPad|iPhone|iPod/u.test(navigator.userAgent);
  const standaloneIos = typeof navigator !== 'undefined' && Boolean((navigator as Navigator & { standalone?: boolean }).standalone);
  const allowOfflineVault = !online && serverAccess === 'owner';

  if (!allowOfflineVault && (serverAccess === 'unavailable' || access === 'unavailable')) {
    return (
      <main className={styles.gate}>
        <p>APOCRYPHA</p><h1>Account verification is unavailable.</h1>
        <span>No memory, source, or conversation payload was loaded.</span>
        <button type="button" onClick={() => { void refresh(); }}>Retry verification</button>
      </main>
    );
  }
  if (serverAccess === 'forbidden' || (!allowOfflineVault && access !== 'checking' && access !== 'owner')) {
    return (
      <main className={styles.gate}>
        <p>APOCRYPHA</p><h1>This conversation belongs to another account.</h1>
        <span>The server did not authorize this identity as the owner.</span>
        <div><Link href="/account">Review account</Link><Link href="/">Return home</Link></div>
      </main>
    );
  }
  if (loading || (!allowOfflineVault && access === 'checking')) {
    return <main className={styles.gate} aria-busy="true"><p>APOCRYPHA</p><h1>Opening your conversation…</h1><span role="status">Verifying your account before loading messages.</span></main>;
  }

  const insertToolText = (text: string): boolean => {
    if (miniStatus !== 'ready' || sending || !text.trim()) return false;
    const next = draft ? draft + '\n\n' + text : text;
    if (next.length > 16_384 || new TextEncoder().encode(next).length > 16_384) {
      return false;
    }
    setDraft(next);
    setActivePanel(null);
    requestAnimationFrame(() => composerRef.current?.focus());
    return true;
  };
  const visibleNotice = syncNotice && !syncNotice.startsWith('Device queue and desktop worldline are current.')
    && !syncNotice.startsWith('History is synchronized.') ? short(syncNotice, 180) : '';

  return (
    <main className={styles.brain} data-brain-state={runtime?.status ?? 'degraded'}>
      <header className={styles.roomHeader}>
        <Link href="/" className={styles.roomHome} aria-label="Apocky home"><span className="apx-brand-mark" aria-hidden="true" /></Link>
        <div className={styles.roomTitle}>
          <h1>Apocrypha</h1>
          <button type="button" className={styles.connectionState} data-connected={desktopConnected}
            aria-expanded={activePanel === 'settings'} aria-controls="brain-settings-panel"
            onClick={event => togglePanel('settings', event.currentTarget)}>
            <span aria-hidden="true" />{desktopConnected ? 'Desktop connected' : 'Waiting for desktop'}
          </button>
        </div>
        <nav className={styles.roomActions} aria-label="Conversation controls">
          <button type="button" aria-label="Conversation history" title="Conversation history" aria-expanded={activePanel === 'history'} aria-controls="brain-history-panel" onClick={event => togglePanel('history', event.currentTarget)}>
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 10a9 9 0 1 1 2 8M3 4v6h6M12 7v5l3 2" /></svg>
          </button>
          <button type="button" aria-label="Conversation settings" title="Conversation settings" aria-expanded={activePanel === 'settings'} aria-controls="brain-settings-panel" onClick={event => togglePanel('settings', event.currentTarget)}>
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 6h16M4 12h16M4 18h16M8 3v6M16 9v6M10 15v6" /></svg>
          </button>
        </nav>
      </header>

      <div className={styles.roomLayout}>
        <section className={styles.roomConversation} aria-label="Conversation with Apocrypha">
          <div ref={messageLogRef} tabIndex={0} className={styles.messageLog} role="log" aria-label="Messages" aria-live="polite" aria-busy={sending || runtimeLoading}
            onScroll={event => {
              const log = event.currentTarget;
              const near = log.scrollHeight - log.clientHeight - log.scrollTop <= 80;
              nearLatestRef.current = near;
              setShowLatest(!near);
            }}>
            {!desktopConnected && conversation.length === 0 ? (
              <div className={styles.runtimeDegraded}><strong>Start wherever you are.</strong><p>Your desktop is not connected yet. You can write now; your message will be saved on this device until it can be delivered.</p></div>
            ) : conversation.length === 0 ? (
              <div className={styles.emptyConversation}><strong>What’s on your mind?</strong><p>Ask a question, follow a thought, or make something together.</p></div>
            ) : conversation.map(message => (
              <article key={message.id} data-role={message.role} data-origin={message.origin}>
                <p><span className={styles.messageAuthor}><span className={styles.messageAvatar} aria-hidden="true">{message.role === 'user' ? 'Y' : '∞'}</span><strong>{message.role === 'user' ? 'You' : 'Apocrypha'}</strong></span><time dateTime={message.recorded_at}>{formattedDate(message.recorded_at)}</time></p>
                <ConversationMessageContent content={message.content} assistant={message.role === 'assistant'} />
                {message.origin === 'queued-mobile' ? <small>Saved on this device · waiting to send</small> : null}
                {message.terminal_failure ? <div className={styles.messageFailure}>
                  <span>{message.terminal_failure.code === 'chat_prompt_capacity_exceeded' ? 'This message exceeded the conversation capacity. Its text is preserved.' : 'Apocrypha could not reply to this message. Its text is preserved.'}</span>
                  <button type="button" disabled={sending || Boolean(draft.trim())} onClick={() => { setDraft(message.content); setSyncNotice('Review and send to retry. The earlier failed message stays in history.'); composerRef.current?.focus(); }}>Retry message</button>
                </div> : null}
              </article>
            ))}
            {sending ? <p className={styles.responding} role="status">{desktopConnected ? 'Apocrypha is responding…' : 'Saving your message…'}</p> : null}
          </div>

          {showLatest ? <button type="button" className={styles.latestMessages} onClick={() => { scrollToLatest(); messageLogRef.current?.focus(); }}>Latest messages <span aria-hidden="true">↓</span></button> : null}

          <div className={styles.composerDock}>
            <div className={styles.composerFeedback}>
            {visibleNotice ? <div className={styles.roomNotice} role="status"><span>{visibleNotice}</span><button type="button" onClick={event => togglePanel('settings', event.currentTarget)}>Details</button></div> : null}
            {miniState && miniState.queue.length > 0 ? <div className={styles.roomQueue}>
              <span>{miniState.queue.length} message{miniState.queue.length === 1 ? '' : 's'} waiting</span>
              {syncConflict ? <button type="button" onClick={() => { void retryConflict(); }} disabled={sending || !online || runtime?.status !== 'live'}>Retry on current history</button>
                : <button type="button" onClick={() => { void syncQueued(); }} disabled={sending || !online || runtime?.status !== 'live'}>Try delivery</button>}
            </div> : null}
            </div>
            <form className={styles.roomComposer} onSubmit={event => { void send(event); }}>
              <label className={styles.roomSrOnly} htmlFor="brain-message">Message Apocrypha</label>
              <textarea ref={composerRef} id="brain-message" value={draft} onChange={event => setDraft(event.currentTarget.value)}
                onKeyDown={event => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); if (draft.trim() && miniStatus === 'ready' && !sending) event.currentTarget.form?.requestSubmit(); } }}
                placeholder="Message Apocrypha…" disabled={miniStatus !== 'ready' || sending} maxLength={16_384} rows={2} />
              <div className={styles.composerControls}>
                <div>
                  <button type="button" aria-expanded={activePanel === 'tools'} aria-controls="brain-tools-panel" onClick={event => togglePanel('tools', event.currentTarget)}>+ Create</button>
                  <button type="button" aria-expanded={activePanel === 'memory'} aria-controls="brain-memory-panel" onClick={event => togglePanel('memory', event.currentTarget)}>Memory</button>
                  <button type="button" disabled={miniStatus !== 'ready' || sending} onClick={() => { void newConversation(); }}>New</button>
                </div>
                <button className={styles.roomSend} type="submit" disabled={miniStatus !== 'ready' || sending || !draft.trim()}>{sending ? 'Saving…' : desktopConnected ? 'Send' : 'Queue message'}</button>
              </div>
            </form>
            <p className={styles.composerHint}>Enter to send · Shift + Enter for a new line</p>
          </div>
        </section>

        <div ref={auxiliaryRef} className={styles.auxiliary} hidden={activePanel === null}>
          <section id="brain-history-panel" className={styles.auxPanel} role="region" aria-labelledby="brain-history-title" hidden={activePanel !== 'history'} data-panel-active={activePanel === 'history'}>
            <header className={styles.auxHeader}><h2 id="brain-history-title">Your conversations</h2><button type="button" data-panel-close aria-label="Close conversation history" onClick={closePanel}>×</button></header>
            <div className={styles.auxBody}>
              {runtime?.status === 'live' ? <>
                <label className={styles.historyPicker}><span>Open a conversation</span><select value={sessionId ?? ''} disabled={runtimeLoading || sending}
                  onFocus={() => { void loadSessionPage().catch(error => setSyncNotice(error instanceof Error ? error.message : 'Conversation history is unavailable.')); }}
                  onChange={event => { void chooseSession(event.currentTarget.value); closePanel(); }}>
                  {!sessions.some(session => session.session_id === sessionId) ? <option value={sessionId ?? ''}>{miniState && miniCursor(miniState, sessionId ?? '') ? 'Current conversation' : 'Current local draft'}</option> : null}
                  {sessions.map(session => <option key={session.session_id} value={session.session_id}>{short(session.title, 60)} · {session.message_count} messages</option>)}
                </select></label>
                {historyCursor !== null ? <button type="button" className={styles.panelButton} disabled={historyLoading} onClick={() => { void loadSessionPage(historyCursor).catch(error => setSyncNotice(error instanceof Error ? error.message : 'Older conversations could not load.')); }}>{historyLoading ? 'Loading conversations…' : 'Load older conversations'}</button> : null}
              </> : <p>Your saved conversation stays on this device. Connect your desktop to browse its history.</p>}
            </div>
          </section>

          <section id="brain-memory-panel" className={styles.auxPanel} role="region" aria-labelledby="brain-explorer-title" hidden={activePanel !== 'memory'} data-panel-active={activePanel === 'memory'}>
            <header className={styles.auxHeader}><h2 id="brain-explorer-title">Memory</h2><button type="button" data-panel-close aria-label="Close memory" onClick={closePanel}>×</button></header>
            <div className={styles.memoryBody}><p className={styles.memoryIntro}>Explore your saved records and their source links.</p>
          {snapshot ? (
            <>
              <label className={styles.search}>
                <span>Find a memory, topic, or phrase</span>
                <input type="search" value={query} onChange={(event) => setQuery(event.currentTarget.value)} placeholder="Search the private snapshot…" />
                <small>{filteredMemories.length} of {snapshot.memories.length} loaded records match</small>
              </label>
              <div className={styles.views} role="group" aria-label="Memory projection">
                {(['graph', 'timeline', 'tunnel'] as const).map(candidate => (
                  <button type="button" key={candidate} aria-pressed={view === candidate} onClick={() => setView(candidate)}>{candidate}</button>
                ))}
              </div>
              <div className={styles.projection} data-view={view}>
                {view === 'graph' ? <BrainGraph memories={filteredMemories} selectedId={selected?.id ?? null} onSelect={selectMemory} /> : null}
                {view === 'timeline' ? <Timeline memories={filteredMemories} messages={snapshot.messages} onSelect={selectMemory} /> : null}
                {view === 'tunnel' ? <Tunnel memory={selected} memories={snapshot.memories} messages={snapshot.messages} onSelect={selectMemory} /> : null}
              </div>
            </>
          ) : (
            <div className={styles.localMemory}>
              <strong>Remote Mneme projection is unavailable.</strong>
              <p>The Mini Brain retained only compact paraphrases and one-way source/record digests—not raw source messages.</p>
              {miniState?.memories.slice(0, 8).map(memory => (
                <article key={memory.record_digest}><span>{memory.topic}</span><p>{memory.paraphrase}</p><code>{memory.record_digest.slice(0, 14)}…</code></article>
              ))}
              {(miniState?.memories.length ?? 0) === 0 ? <code>MNEME_STORAGE_UNAVAILABLE · NO_LOCAL_DIGEST_CACHE</code> : null}
            </div>
          )}

            </div>
          </section>

          <section id="brain-settings-panel" className={styles.auxPanel} role="region" aria-labelledby="brain-settings-title" hidden={activePanel !== 'settings'} data-panel-active={activePanel === 'settings'}>
            <header className={styles.auxHeader}><h2 id="brain-settings-title">Conversation settings</h2><button type="button" data-panel-close aria-label="Close conversation settings" onClick={closePanel}>×</button></header>
            <div className={styles.auxBody}>
              <nav className={styles.settingsLinks} aria-label="Your Apocrypha"><Link href="/account">Your account</Link><Link href="/download/apocrypha">Get the app</Link></nav>
              <h3>Connection & device details</h3>
              <p>{desktopConnected ? 'Your desktop is connected.' : 'Your desktop is not connected yet.'} This device keeps an encrypted copy; replies come from your desktop.</p>
              <div className={styles.connectionFacts}>
                <Connector label="Mini Brain" state={miniStatus === 'ready' ? 'live' : 'degraded'} detail={miniStatus === 'ready' ? String(miniState?.queue.length ?? 0) + ' queued · encrypted here' : miniStatus === 'initializing' ? 'opening encrypted device vault' : miniStatus === 'unbound' ? 'encrypted vault ready · owner/device binding unavailable' : 'encrypted device vault unavailable'} />
                <Connector label="Mneme storage" state={snapshot ? 'live' : 'degraded'} detail={snapshot ? String(snapshot.counts.memories) + ' records · ' + String(snapshot.counts.source_links) + ' source links' : 'remote memory unavailable · local digest cache only'} />
              </div>
              {runtime?.reason_code ? <code className={styles.settingsCode}>{runtime.reason_code}</code> : null}
              {syncNotice ? <p className={styles.settingsNotice}>{syncNotice}</p> : null}
              {memoryError ? <div className={styles.settingsNotice}><strong>{memoryProvisionable ? 'Memory needs your confirmation.' : 'Memory is unavailable.'}</strong><p>{memoryError}</p>{memoryProvisionable ? <button type="button" className={styles.panelButton} onClick={() => { void provisionMneme(); }} disabled={provisioningMemory}>{provisioningMemory ? 'Creating private profile…' : 'Create my private memory profile'}</button> : null}</div> : null}
              <button type="button" className={styles.panelButton} onClick={() => { void load(); }}>Refresh connection</button>
              <h3>On this device</h3>
              <p>{offlineShellReady ? 'The offline shell is ready.' : 'Keep this page online once to prepare the offline shell.'}</p>
              {installPrompt && !installed ? <button type="button" className={styles.panelButton} onClick={() => { void install(); }}>Install Mini Brain</button> : null}
              {isIos && !standaloneIos ? <p>iPhone: Share → Add to Home Screen</p> : null}
              {installed ? <p>Installed on this device.</p> : null}
              <button type="button" className={styles.panelButton} onClick={() => { void eraseOfflineCopy(); }} disabled={miniStatus !== 'ready'}>Erase offline copy</button>
              <BrainDiagnostics />
              <ReleaseShelf />
            </div>
          </section>

          <section id="brain-tools-panel" className={styles.auxPanel} role="region" aria-labelledby="brain-tools-title" hidden={activePanel !== 'tools'} data-panel-active={activePanel === 'tools'}>
            <header className={styles.auxHeader}><h2 id="brain-tools-title">Create in chat</h2><button type="button" data-panel-close aria-label="Close Create in chat" onClick={closePanel}>×</button></header>
            <div className={styles.auxBody}><ChatTools key={subjectKey} onInsert={insertToolText} disabled={miniStatus !== 'ready' || sending} /></div>
          </section>
        </div>
      </div>
    </main>
  );
}
