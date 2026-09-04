import Link from 'next/link';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type {
  BrainMemory,
  BrainMessage,
  BrainRuntimeStatus,
  BrainSnapshot,
} from '@/lib/brain/contracts';
import {
  openMiniBrain,
  probeMiniBrainCortex,
  warmMiniBrainOfflineShell,
  type MiniBrainCortexProbe,
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

type BrainView = 'graph' | 'timeline' | 'tunnel';
type ServerAccess = 'owner' | 'forbidden' | 'unavailable';

interface RuntimeSessionSummary {
  readonly session_id: string;
  readonly title: string;
  readonly updated_at: string;
  readonly message_count: number;
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
const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

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
      || !UUID_V4.test(row.session_id)
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
  input: {
    readonly operation: 'pull' | 'append';
    readonly sessionId: string;
    readonly requestId: string;
    readonly baseCursor: string | null;
    readonly text?: string;
  },
): Promise<MiniBrainSyncResponse> {
  const request = await vault.signedRequest({
    operation: input.operation,
    sessionId: input.sessionId,
    requestId: input.requestId,
    baseCursor: input.baseCursor,
    payload: input.operation === 'append' ? { text: input.text ?? '' } : null,
  });
  return jsonRequest<MiniBrainSyncResponse>('/api/brain/mobile/sync', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request),
  });
}

function miniConversation(state: MiniBrainState | null): readonly MiniBrainMessage[] {
  return state?.sessions.find(session => session.session_id === state.current_session_id)?.messages ?? [];
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
              <strong>No promoted public package is attached.</strong>
              <p>{manifest.build.missing.length} release gate{manifest.build.missing.length === 1 ? '' : 's'} remain. Candidate work is visible here without being mislabeled as released.</p>
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
  const { access, refresh } = useSiteSession();
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
  const [miniState, setMiniState] = useState<MiniBrainState | null>(null);
  const [miniStatus, setMiniStatus] = useState<'initializing' | 'ready' | 'unbound' | 'unavailable'>('initializing');
  const [cortex, setCortex] = useState<MiniBrainCortexProbe | null>(null);
  const [online, setOnline] = useState(true);
  const [installPrompt, setInstallPrompt] = useState<InstallPromptEvent | null>(null);
  const [installed, setInstalled] = useState(false);
  const [offlineShellReady, setOfflineShellReady] = useState(false);
  const [syncConflict, setSyncConflict] = useState(false);
  const [draft, setDraft] = useState('');
  const [sending, setSending] = useState(false);
  const vaultRef = useRef<MiniBrainVault | null>(null);
  const stateRef = useRef<MiniBrainState | null>(null);

  const commitState = useCallback((state: MiniBrainState): MiniBrainState => {
    stateRef.current = state;
    setMiniState(state);
    setMiniStatus('ready');
    return state;
  }, []);

  const pullSession = useCallback(async (
    vault: MiniBrainVault,
    state: MiniBrainState,
    sessionId: string,
  ): Promise<MiniBrainState> => {
    const response = await syncMiniBrain(vault, {
      operation: 'pull',
      sessionId,
      requestId: randomSessionId(),
      baseCursor: miniCursor(state, sessionId),
    });
    const selected = state.current_session_id === sessionId
      ? state
      : await vault.save({ ...state, current_session_id: sessionId });
    try { sessionStorage.setItem(SESSION_STORAGE_KEY, sessionId); } catch { /* private mode can deny storage */ }
    return vault.applySync(selected, response);
  }, []);

  const flushQueue = useCallback(async (
    vault: MiniBrainVault,
    initial: MiniBrainState,
    rebase = false,
  ): Promise<MiniBrainState> => {
    let state = initial;
    setSyncConflict(false);
    for (const turn of [...state.queue]) {
      try {
        const response = await syncMiniBrain(vault, {
          operation: 'append',
          sessionId: turn.session_id,
          requestId: turn.request_id,
          baseCursor: rebase ? miniCursor(state, turn.session_id) : turn.base_cursor,
          text: turn.text,
        });
        state = await vault.applySync(state, response);
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
    setSyncNotice(state.queue.length === 0 ? 'Device queue and desktop worldline are current.' : `${state.queue.length} encrypted turn${state.queue.length === 1 ? '' : 's'} remain queued.`);
    return state;
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
      setCortex(probeMiniBrainCortex());
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
        setSyncNotice(state ? 'Offline · encrypted recent history and deterministic local reflection remain available.' : 'Offline · this browser has not completed owner/device binding yet.');
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
          const listing = await jsonRequest<{ sessions?: unknown }>('/api/brain/runtime/sessions');
          const remoteSessions = sessionSummaries(listing);
          setSessions(remoteSessions);
          let saved = '';
          try { saved = sessionStorage.getItem(SESSION_STORAGE_KEY) ?? ''; } catch { saved = ''; }
          const chosen = remoteSessions.find(item => item.session_id === saved)?.session_id
            ?? remoteSessions.find(item => item.session_id === state?.current_session_id)?.session_id
            ?? remoteSessions[0]?.session_id
            ?? state.current_session_id;
          state = await pullSession(vault, state, chosen);
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
  }, [access, commitState, flushQueue, online, pullSession]);

  useEffect(() => {
    if (serverAccess !== 'owner' || (online && access === 'checking')) return;
    if (online && access !== 'owner') {
      setLoading(false);
      return;
    }
    void load();
  }, [access, load, online, serverAccess]);

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
    void navigator.serviceWorker.register('/brain-sw.js', { scope: '/' })
      .then(() => warmMiniBrainOfflineShell())
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
    const state = await vault.save({ ...current, current_session_id: next });
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
      const state = online && runtime?.status === 'live'
        ? await pullSession(vault, current, sessionId)
        : await vault.save({ ...current, current_session_id: sessionId });
      commitState(state);
    } catch (error) {
      setSyncNotice(error instanceof Error ? error.message : 'This worldline could not open.');
    } finally { setRuntimeLoading(false); }
  };

  const send = async (event: React.FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    const text = draft.trim();
    const vault = vaultRef.current;
    const current = stateRef.current;
    if (!text || !vault || !current || sending) return;
    setSending(true);
    setSyncNotice('');
    setDraft('');
    try {
      const queued = await vault.queueTurn(current, text);
      commitState(queued.state);
      setSyncNotice(online && runtime?.status === 'live'
        ? 'Encrypted on this device · synchronizing with desktop…'
        : 'Encrypted on this device · deterministic reflection active · desktop turn queued.');
      if (online && runtime?.status === 'live') {
        const synchronized = await flushQueue(vault, queued.state);
        commitState(synchronized);
      }
    } catch (sendError) {
      setDraft(text);
      setSyncNotice(sendError instanceof Error ? sendError.message : 'The local turn could not be encrypted.');
    } finally {
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
  const sessionId = miniState?.current_session_id ?? null;
  const isIos = typeof navigator !== 'undefined' && /iPad|iPhone|iPod/u.test(navigator.userAgent);
  const standaloneIos = typeof navigator !== 'undefined' && Boolean((navigator as Navigator & { standalone?: boolean }).standalone);
  const allowOfflineVault = !online && serverAccess === 'owner';

  if (!allowOfflineVault && (serverAccess === 'unavailable' || access === 'unavailable')) {
    return (
      <main className={styles.gate}>
        <p>PRIVATE BRAIN · DEGRADED</p><h1>Owner verification is unavailable.</h1>
        <span>No memory, source, or conversation payload was loaded.</span>
        <button type="button" onClick={() => { void refresh(); }}>Retry verification</button>
      </main>
    );
  }
  if (serverAccess === 'forbidden' || (!allowOfflineVault && access !== 'checking' && access !== 'owner')) {
    return (
      <main className={styles.gate}>
        <p>PRIVATE BRAIN · OWNER ONLY</p><h1>This boundary did not open.</h1>
        <span>The server did not authorize this identity as the owner.</span>
        <div><Link href="/account">Review account</Link><Link href="/">Return home</Link></div>
      </main>
    );
  }
  if (loading || (!allowOfflineVault && access === 'checking')) {
    return <main className={styles.gate} aria-busy="true"><p>PRIVATE BRAIN</p><h1>Verifying synapses…</h1><span role="status">No private payload renders before both owner checks complete.</span></main>;
  }

  return (
    <main className={styles.brain} data-brain-state={runtime?.status ?? 'degraded'}>
      <header className={styles.header}>
        <Link href="/" className={styles.brand} aria-label="Apocky home"><span aria-hidden="true">∞</span><strong>APOCKY</strong></Link>
        <div><p>OWNER-PRIVATE BRAIN</p><h1>Apocrypha</h1></div>
        <nav aria-label="Private Brain navigation"><a href="#brain-releases">Releases</a><Link href="/memory-tools">Memory tools</Link><Link href="/account">Account</Link></nav>
      </header>

      <section className={styles.statusStrip} aria-label="Observed connector states">
        <Connector
          label="Mini Brain"
          state={miniStatus === 'ready' ? 'live' : 'degraded'}
          detail={miniStatus === 'ready'
            ? `${miniState?.queue.length ?? 0} queued · encrypted here`
            : miniStatus === 'initializing'
              ? 'opening encrypted device vault'
              : miniStatus === 'unbound'
                ? 'encrypted vault ready · owner/device binding unavailable'
                : 'encrypted device vault unavailable'}
        />
        <Connector
          label="Mneme storage"
          state={snapshot ? 'live' : 'degraded'}
          detail={snapshot ? `${snapshot.counts.memories} records · ${snapshot.counts.source_links} source links` : 'remote memory unavailable · local digest cache only'}
        />
        <Connector
          label="Desktop Apocrypha"
          state={runtime?.status ?? 'degraded'}
          detail={runtime?.status === 'live' ? `observed HTTP ${runtime.upstream_status} · sync ready` : 'not connected · turns stay queued'}
        />
        <button type="button" onClick={() => { void load(); }}>Refresh evidence</button>
      </section>

      <section className={styles.miniBar} aria-labelledby="mini-brain-title">
        <div>
          <p>INSTALLABLE · OWNER/DEVICE BOUND</p>
          <h2 id="mini-brain-title">A useful Mini Brain when the deep node is away.</h2>
          <span>Recent conversation is AES-GCM encrypted in this browser profile. A non-exportable device key signs sync requests; desktop remains canonical.</span>
        </div>
        <div className={styles.miniActions}>
          {installPrompt && !installed ? <button type="button" onClick={() => { void install(); }}>Install Mini Brain</button> : null}
          {isIos && !standaloneIos ? <span>iPhone: Share → Add to Home Screen</span> : null}
          {installed ? <span>Installed</span> : null}
          <span>{offlineShellReady ? 'Offline shell ready' : 'Keep online once to seal the offline shell'}</span>
          <button type="button" className={styles.subtleButton} onClick={() => { void eraseOfflineCopy(); }} disabled={miniStatus !== 'ready'}>Erase offline copy</button>
        </div>
        <details>
          <summary>Local cortex capability</summary>
          <p>{cortex?.note ?? 'Checking this browser…'}</p>
          {cortex ? <code>{cortex.reason_code} · WASM {cortex.wasm ? 'yes' : 'no'} · WebGPU {cortex.webgpu ? 'yes' : 'no'}</code> : null}
        </details>
      </section>

      <ReleaseShelf />

      {memoryError ? (
        <div className={styles.error} role="status">
          <div><strong>{memoryProvisionable ? 'Mneme needs your confirmation.' : 'Mneme is degraded.'}</strong> {memoryError} The encrypted Mini Brain remains available.</div>
          {memoryProvisionable ? (
            <button type="button" onClick={() => { void provisionMneme(); }} disabled={provisioningMemory}>
              {provisioningMemory ? 'Creating private profile…' : 'Create my private memory profile'}
            </button>
          ) : null}
        </div>
      ) : null}
      {syncNotice ? <div className={styles.syncNotice} role="status">{syncNotice}</div> : null}

      <div className={styles.layout}>
        <section className={styles.conversation} aria-labelledby="brain-conversation-title">
          <header className={styles.panelHead}>
            <div><p>PERSISTENT CANVAS</p><h2 id="brain-conversation-title">Conversation</h2></div>
            <div className={styles.conversationActions}>
              {runtime?.status === 'live' && sessions.length > 0 && sessions.some(session => session.session_id === sessionId) ? (
                <label><span>Worldline</span><select value={sessionId ?? ''} onChange={(event) => { void chooseSession(event.currentTarget.value); }}>
                  {sessions.map(session => <option key={session.session_id} value={session.session_id}>{short(session.title, 42)} · {session.message_count}</option>)}
                </select></label>
              ) : null}
              <button type="button" onClick={() => { void newConversation(); }} disabled={miniStatus !== 'ready' || sending}>New</button>
            </div>
          </header>

          <div className={styles.messageLog} role="log" aria-live="polite" aria-busy={sending || runtimeLoading}>
            {runtime?.status !== 'live' && conversation.length === 0 ? (
              <div className={styles.runtimeDegraded}>
                <strong>Mini Brain is local; desktop depth is away.</strong>
                <p>You can still ask. The deterministic core recalls compact memory, states its limits, and encrypts the turn for later synchronization. It does not pretend to be the learned Apocrypha cortex.</p>
                <code>{runtime?.reason_code ?? 'BRAIN_RUNTIME_STATUS_UNAVAILABLE'}</code>
              </div>
            ) : conversation.length === 0 ? (
              <div className={styles.emptyConversation}><strong>Worldline ready.</strong><p>Ask here. The device seals the turn first; a live desktop connection then appends it to the same owner-bound history.</p></div>
            ) : conversation.map(message => (
              <article key={message.id} data-role={message.role} data-origin={message.origin}>
                <p>{message.role === 'user' ? 'You' : message.origin === 'local-reflection' ? 'Mini Brain · deterministic' : 'Apocrypha'}<time dateTime={message.recorded_at}>{formattedDate(message.recorded_at)}</time></p>
                <div>{message.content}</div>
                {message.origin !== 'desktop' ? <small>{message.origin === 'queued-mobile' ? 'encrypted queue · not yet committed on desktop' : 'local reflection · no model call'}</small> : null}
                {message.provenance_digests.length > 0 ? <small>{message.provenance_digests.length} provenance digest{message.provenance_digests.length === 1 ? '' : 's'} retained</small> : null}
              </article>
            ))}
          </div>

          {miniState && miniState.queue.length > 0 ? (
            <div className={styles.queueBar}>
              <span>{miniState.queue.length} encrypted turn{miniState.queue.length === 1 ? '' : 's'} waiting</span>
              {syncConflict
                ? <button type="button" onClick={() => { void retryConflict(); }} disabled={sending || !online || runtime?.status !== 'live'}>Retry on current history</button>
                : <button type="button" onClick={() => { void syncQueued(); }} disabled={sending || !online || runtime?.status !== 'live'}>Sync queued</button>}
            </div>
          ) : null}

          <form className={styles.composer} onSubmit={(event) => { void send(event); }}>
            <label htmlFor="brain-message">Message Apocrypha / Mini Brain</label>
            <textarea
              id="brain-message"
              value={draft}
              onChange={(event) => setDraft(event.currentTarget.value)}
              placeholder={runtime?.status === 'live' ? 'Ask, connect, compare, or continue…' : 'Ask offline; the deterministic core will reflect and queue the turn…'}
              disabled={miniStatus !== 'ready' || sending}
              maxLength={16_384}
              rows={3}
            />
            <button type="submit" disabled={miniStatus !== 'ready' || sending || !draft.trim()}>{sending ? 'Sealing…' : runtime?.status === 'live' ? 'Send + sync' : 'Reflect + queue'}</button>
            <p>Effect authority: none · training consent: off · server-derived owner partition · local cache encrypted · desktop remains authoritative</p>
          </form>
        </section>

        <section className={styles.explorer} aria-labelledby="brain-explorer-title">
          <header className={styles.panelHead}>
            <div><p>CONTEXTUAL RECALL · NO MODEL CALL</p><h2 id="brain-explorer-title">Explore the memory field</h2></div>
          </header>
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
        </section>
      </div>
    </main>
  );
}
