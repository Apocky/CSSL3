import Link from 'next/link';
import { useCallback, useEffect, useMemo, useState } from 'react';

import type {
  BrainMemory,
  BrainMessage,
  BrainRuntimeStatus,
  BrainRuntimeTurn,
  BrainSnapshot,
} from '@/lib/brain/contracts';
import { authFetch } from '@/lib/browser-auth';
import { useSiteSession } from '@/components/hub/SiteSession';
import styles from './BrainExperience.module.css';

type BrainView = 'graph' | 'timeline' | 'tunnel';
type ServerAccess = 'owner' | 'forbidden' | 'unavailable';

interface RuntimeMessage {
  readonly id: string;
  readonly role: 'user' | 'assistant';
  readonly content: string;
  readonly recordedAt: string;
  readonly memoryRefs: readonly unknown[];
}

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
    throw new Error(`${payload.error ?? 'The private Brain could not answer.'} (${payload.code ?? `HTTP_${response.status}`})`);
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

function runtimeMessages(value: unknown): RuntimeMessage[] {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return [];
  const raw = (value as Record<string, unknown>).messages;
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((entry, index) => {
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) return [];
    const row = entry as Record<string, unknown>;
    if ((row.role !== 'user' && row.role !== 'assistant') || typeof row.content !== 'string') return [];
    const receipt = row.receipt && typeof row.receipt === 'object' && !Array.isArray(row.receipt)
      ? row.receipt as Record<string, unknown>
      : null;
    const context = receipt?.context && typeof receipt.context === 'object' && !Array.isArray(receipt.context)
      ? receipt.context as Record<string, unknown>
      : null;
    const memory = context?.memory && typeof context.memory === 'object' && !Array.isArray(context.memory)
      ? context.memory as Record<string, unknown>
      : null;
    return [{
      id: typeof row.event_digest === 'string' ? row.event_digest : `restored-${index}`,
      role: row.role,
      content: row.content,
      recordedAt: typeof row.recorded_at === 'string' ? row.recorded_at : '',
      memoryRefs: Array.isArray(memory?.refs) ? memory.refs : [],
    } satisfies RuntimeMessage];
  });
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

function Connector({ label, state, detail }: { label: string; state: string; detail: string }): JSX.Element {
  return (
    <div className={styles.connector} data-state={state}>
      <span aria-hidden="true" />
      <div><strong>{label}</strong><small>{detail}</small></div>
    </div>
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
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(serverAccess === 'owner');
  const [runtimeLoading, setRuntimeLoading] = useState(false);
  const [sessions, setSessions] = useState<readonly RuntimeSessionSummary[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [conversation, setConversation] = useState<readonly RuntimeMessage[]>([]);
  const [draft, setDraft] = useState('');
  const [sending, setSending] = useState(false);

  const loadSession = useCallback(async (id: string): Promise<void> => {
    const payload = await jsonRequest<{ session?: unknown }>(`/api/brain/runtime/sessions?session_id=${encodeURIComponent(id)}`);
    setConversation(runtimeMessages(payload.session));
    setSessionId(id);
    try { sessionStorage.setItem(SESSION_STORAGE_KEY, id); } catch { /* private mode can deny storage */ }
  }, []);

  const loadSessions = useCallback(async (): Promise<void> => {
    const payload = await jsonRequest<{ sessions?: unknown }>('/api/brain/runtime/sessions');
    const next = sessionSummaries(payload);
    setSessions(next);
    let saved = '';
    try { saved = sessionStorage.getItem(SESSION_STORAGE_KEY) ?? ''; } catch { saved = ''; }
    const chosen = next.find(item => item.session_id === saved)?.session_id ?? next[0]?.session_id;
    if (chosen) await loadSession(chosen);
    else {
      const fresh = randomSessionId();
      setSessionId(fresh);
      setConversation([]);
      try { sessionStorage.setItem(SESSION_STORAGE_KEY, fresh); } catch { /* private mode can deny storage */ }
    }
  }, [loadSession]);

  const load = useCallback(async (): Promise<void> => {
    setLoading(true);
    setError('');
    try {
      const [memoryPayload, runtimePayload] = await Promise.all([
        jsonRequest<BrainSnapshot>('/api/brain/snapshot'),
        jsonRequest<BrainRuntimeStatus>('/api/brain/runtime/status'),
      ]);
      setSnapshot(memoryPayload);
      setRuntime(runtimePayload);
      setSelectedId(current => current ?? memoryPayload.memories[0]?.id ?? null);
      if (runtimePayload.status === 'live') {
        setRuntimeLoading(true);
        try { await loadSessions(); }
        catch (runtimeError) { setError(runtimeError instanceof Error ? runtimeError.message : 'Conversation history could not open.'); }
        finally { setRuntimeLoading(false); }
      }
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : 'The private Brain could not open.');
    } finally {
      setLoading(false);
    }
  }, [loadSessions]);

  useEffect(() => {
    if (serverAccess !== 'owner' || access === 'checking') return;
    if (access !== 'owner') {
      setLoading(false);
      return;
    }
    void load();
  }, [access, load, serverAccess]);

  const filteredMemories = useMemo(() => filterMemories(snapshot?.memories ?? [], query), [query, snapshot]);
  const selected = snapshot?.memories.find(memory => memory.id === selectedId) ?? filteredMemories[0] ?? null;

  const selectMemory = useCallback((memory: BrainMemory) => {
    setSelectedId(memory.id);
    setView('tunnel');
  }, []);

  const newConversation = (): void => {
    const next = randomSessionId();
    setSessionId(next);
    setConversation([]);
    setDraft('');
    try { sessionStorage.setItem(SESSION_STORAGE_KEY, next); } catch { /* private mode can deny storage */ }
  };

  const send = async (event: React.FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    const text = draft.trim();
    if (!text || !sessionId || runtime?.status !== 'live' || sending) return;
    const requestId = crypto.randomUUID();
    const optimistic: RuntimeMessage = {
      id: `local-${requestId}`,
      role: 'user',
      content: text,
      recordedAt: new Date().toISOString(),
      memoryRefs: [],
    };
    setSending(true);
    setError('');
    setDraft('');
    setConversation(current => [...current, optimistic]);
    try {
      const result = await jsonRequest<BrainRuntimeTurn>('/api/brain/runtime/turn', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ text, session_id: sessionId, request_id: requestId }),
      });
      setConversation(current => [...current, {
        id: result.response_digest || `response-${requestId}`,
        role: 'assistant',
        content: result.text,
        recordedAt: result.ts,
        memoryRefs: result.memory?.refs ?? [],
      }]);
      const history = await jsonRequest<{ sessions?: unknown }>('/api/brain/runtime/sessions');
      setSessions(sessionSummaries(history));
    } catch (sendError) {
      setConversation(current => current.filter(message => message.id !== optimistic.id));
      setDraft(text);
      setError(sendError instanceof Error ? sendError.message : 'The local provider did not answer.');
    } finally {
      setSending(false);
    }
  };

  if (serverAccess === 'unavailable' || access === 'unavailable') {
    return (
      <main className={styles.gate}>
        <p>PRIVATE BRAIN · DEGRADED</p><h1>Owner verification is unavailable.</h1>
        <span>No memory, source, or conversation payload was loaded.</span>
        <button type="button" onClick={() => { void refresh(); }}>Retry verification</button>
      </main>
    );
  }
  if (serverAccess === 'forbidden' || (access !== 'checking' && access !== 'owner')) {
    return (
      <main className={styles.gate}>
        <p>PRIVATE BRAIN · OWNER ONLY</p><h1>This boundary did not open.</h1>
        <span>The server did not authorize this identity as the owner.</span>
        <div><Link href="/account">Review account</Link><Link href="/">Return home</Link></div>
      </main>
    );
  }
  if (loading || access === 'checking') {
    return <main className={styles.gate} aria-busy="true"><p>PRIVATE BRAIN</p><h1>Verifying synapses…</h1><span role="status">No private payload renders before both owner checks complete.</span></main>;
  }
  if (!snapshot) {
    return (
      <main className={styles.gate}>
        <p>PRIVATE BRAIN · MEMORY DEGRADED</p><h1>The private projection did not open.</h1>
        <span>{error || 'No private payload was returned.'}</span>
        <button type="button" onClick={() => { void load(); }}>Retry private Brain</button>
      </main>
    );
  }

  return (
    <main className={styles.brain} data-brain-state={runtime?.status ?? 'degraded'}>
      <header className={styles.header}>
        <Link href="/" className={styles.brand} aria-label="Apocky home"><span aria-hidden="true">∞</span><strong>APOCKY</strong></Link>
        <div><p>OWNER-PRIVATE MICROSCOSM</p><h1>Brain</h1></div>
        <nav aria-label="Private Brain navigation"><Link href="/memory-tools">Memory tools</Link><Link href="/account">Account</Link></nav>
      </header>

      <section className={styles.statusStrip} aria-label="Observed connector states">
        <Connector label="Mneme storage" state="live" detail={`${snapshot.counts.memories} records loaded`} />
        <Connector label="Source projection" state="live" detail={`${snapshot.counts.source_links} explicit links`} />
        <Connector
          label="Local Apocv4"
          state={runtime?.status ?? 'degraded'}
          detail={runtime?.status === 'live' ? `observed HTTP ${runtime.upstream_status}` : 'not connected · conversation read-only'}
        />
        <button type="button" onClick={() => { void load(); }}>Refresh evidence</button>
      </section>

      {error ? <div className={styles.error} role="alert">{error}</div> : null}

      <div className={styles.layout}>
        <section className={styles.conversation} aria-labelledby="brain-conversation-title">
          <header className={styles.panelHead}>
            <div><p>PERSISTENT CANVAS</p><h2 id="brain-conversation-title">Conversation</h2></div>
            <div className={styles.conversationActions}>
              {runtime?.status === 'live' && sessions.length > 0 ? (
                <label><span>Thread</span><select value={sessionId ?? ''} onChange={(event) => { void loadSession(event.currentTarget.value); }}>
                  {sessions.map(session => <option key={session.session_id} value={session.session_id}>{short(session.title, 42)} · {session.message_count}</option>)}
                </select></label>
              ) : null}
              <button type="button" onClick={newConversation} disabled={runtime?.status !== 'live' || sending}>New</button>
            </div>
          </header>

          <div className={styles.messageLog} role="log" aria-live="polite" aria-busy={sending || runtimeLoading}>
            {runtime?.status !== 'live' ? (
              <div className={styles.runtimeDegraded}>
                <strong>Free local conversation is not connected.</strong>
                <p>The Mneme map below is live and read-only. New generated turns stay disabled until the server observes the local Apocv4 health contract.</p>
                <code>{runtime?.reason_code ?? 'BRAIN_RUNTIME_STATUS_UNAVAILABLE'}</code>
              </div>
            ) : conversation.length === 0 ? (
              <div className={styles.emptyConversation}><strong>Local channel ready.</strong><p>Ask from the composer. Completed turns persist in the owner-bound runtime thread.</p></div>
            ) : conversation.map(message => (
              <article key={message.id} data-role={message.role}>
                <p>{message.role === 'user' ? 'You' : 'Apocrypha'}<time dateTime={message.recordedAt}>{formattedDate(message.recordedAt)}</time></p>
                <div>{message.content}</div>
                {message.memoryRefs.length > 0 ? <small>{message.memoryRefs.length} memory reference{message.memoryRefs.length === 1 ? '' : 's'} in verified receipt</small> : null}
              </article>
            ))}
          </div>

          <form className={styles.composer} onSubmit={(event) => { void send(event); }}>
            <label htmlFor="brain-message">Message your local Apocrypha</label>
            <textarea
              id="brain-message"
              value={draft}
              onChange={(event) => setDraft(event.currentTarget.value)}
              placeholder={runtime?.status === 'live' ? 'Ask, connect, compare, or continue…' : 'Local provider required before a message can be sent.'}
              disabled={runtime?.status !== 'live' || sending}
              maxLength={16_384}
              rows={3}
            />
            <button type="submit" disabled={runtime?.status !== 'live' || sending || !draft.trim()}>{sending ? 'Waiting…' : 'Send'}</button>
            <p>Effect authority: none · training consent: off · owner memory partition · local provider only</p>
          </form>
        </section>

        <section className={styles.explorer} aria-labelledby="brain-explorer-title">
          <header className={styles.panelHead}>
            <div><p>CONTEXTUAL RECALL · NO MODEL CALL</p><h2 id="brain-explorer-title">Explore the memory field</h2></div>
          </header>
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
        </section>
      </div>
    </main>
  );
}
