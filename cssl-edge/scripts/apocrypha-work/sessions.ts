import { createHash, randomUUID } from 'node:crypto';
import { mkdir, readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import type { EngineMessage } from './engine';
import type { WorkEvent, WorkSession, WorkTurn } from './types';

const HISTORY_TURNS = 8;
const TERMINAL = new Set(['done', 'failed', 'cancelled']);
const PHASES = new Set(['queued', 'thinking', 'awaiting_consent', 'tool', 'writing', ...TERMINAL]);
// Paired with the WorkEvent['kind'] union in types.ts. TypeScript cannot check a Set against a
// union, so adding a kind THERE and not HERE type-checks clean and then fails at runtime with
// 'Invalid event record; external effects require reconciliation' -- after the tool has already
// run. Observed 2026-09-17 adding 'dials'. Change both, in the same commit.
const EVENT_KINDS = new Set(['phase', 'token', 'tool_request', 'tool_result', 'consent_request', 'consent_resolved', 'usage', 'error', 'dials', 'session']);
const SCHEMA = `
CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,
  payload TEXT NOT NULL CHECK (json_valid(payload)),
  next_seq INTEGER NOT NULL DEFAULT 1 CHECK (next_seq BETWEEN 1 AND 9007199254740991)
);
CREATE TABLE IF NOT EXISTS turns (
  session_id TEXT NOT NULL REFERENCES sessions(id),
  id TEXT NOT NULL,
  position INTEGER NOT NULL,
  phase TEXT NOT NULL,
  payload TEXT NOT NULL CHECK (json_valid(payload)),
  PRIMARY KEY (session_id, id),
  UNIQUE (session_id, position)
);
CREATE TABLE IF NOT EXISTS events (
  session_id TEXT NOT NULL REFERENCES sessions(id),
  seq INTEGER NOT NULL,
  payload TEXT NOT NULL CHECK (json_valid(payload)),
  legacy_seq INTEGER,
  PRIMARY KEY (session_id, seq)
);
CREATE TABLE IF NOT EXISTS legacy_imports (
  session_id TEXT PRIMARY KEY REFERENCES sessions(id),
  snapshot_hash TEXT NOT NULL,
  events_hash TEXT,
  snapshot_bytes INTEGER NOT NULL,
  events_bytes INTEGER NOT NULL,
  turn_count INTEGER NOT NULL,
  event_count INTEGER NOT NULL
);
PRAGMA user_version = 1;
`;

function validateId(id: string): void {
  if (typeof id !== 'string' || !/^[a-z0-9][a-z0-9_-]{0,127}$/i.test(id)
    || /^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/i.test(id)) {
    throw new Error('Invalid session or turn ID');
  }
}

function object(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`Invalid ${label} record`);
  return value as Record<string, unknown>;
}

function timestamp(value: unknown): boolean {
  return typeof value === 'string' && Number.isFinite(Date.parse(value));
}

function validateSession(value: unknown, id: string): WorkSession {
  const session = object(value, 'session');
  validateId(session.id as string);
  if (session.id !== id || typeof session.title !== 'string' || !timestamp(session.createdAt)
    || !timestamp(session.lastActiveAt) || !Array.isArray(session.standingGrants)
    || !session.standingGrants.every((grant) => typeof grant === 'string')) {
    throw new Error(`Invalid session record ${id}`);
  }
  return session as unknown as WorkSession;
}

function validateTurn(value: unknown, sessionId: string): WorkTurn {
  const turn = object(value, 'turn');
  validateId(turn.id as string);
  if (turn.sessionId !== sessionId || typeof turn.prompt !== 'string' || !PHASES.has(turn.phase as string)
    || !timestamp(turn.startedAt) || (turn.endedAt !== undefined && !timestamp(turn.endedAt))
    || (turn.error !== undefined && typeof turn.error !== 'string')
    || typeof turn.output !== 'string' || !Array.isArray(turn.toolCalls)) {
    throw new Error(`Invalid turn record in session ${sessionId}`);
  }
  for (const value of turn.toolCalls) {
    const call = object(value, 'tool outcome');
    if (typeof call.id !== 'string' || typeof call.name !== 'string' || typeof call.ok !== 'boolean'
      || typeof call.summary !== 'string' || typeof call.content !== 'string'
      || typeof call.elapsedMs !== 'number' || !Number.isFinite(call.elapsedMs) || call.elapsedMs < 0
      || (call.error !== undefined && typeof call.error !== 'string')
      || (call.denied !== undefined && typeof call.denied !== 'boolean')) throw new Error('Invalid tool outcome record');
    if (call.diff !== undefined) {
      const diff = object(call.diff, 'tool diff');
      if (typeof diff.path !== 'string' || typeof diff.patch !== 'string'
        || !Number.isSafeInteger(diff.added) || Number(diff.added) < 0
        || !Number.isSafeInteger(diff.removed) || Number(diff.removed) < 0) throw new Error('Invalid tool diff record');
    }
  }
  if (turn.usage !== undefined) {
    const usage = object(turn.usage, 'usage');
    for (const key of ['promptTokens', 'completionTokens', 'totalTokens', 'elapsedS']) {
      if (usage[key] !== undefined && (typeof usage[key] !== 'number'
        || !Number.isFinite(usage[key]) || Number(usage[key]) < 0)) throw new Error('Invalid usage record');
    }
  }
  return turn as unknown as WorkTurn;
}

function validateEvent(value: unknown): WorkEvent {
  const event = object(value, 'event');
  if (!Number.isSafeInteger(event.seq) || Number(event.seq) < 1 || !timestamp(event.at)
    || !EVENT_KINDS.has(event.kind as string)) throw new Error('Invalid event record');
  object(event.data, 'event data');
  return event as unknown as WorkEvent;
}

function parse(text: string, label: string): unknown {
  try { return JSON.parse(text) as unknown; }
  catch { throw new Error(`Corrupt JSON record: ${label}`); }
}

type EventInput = Omit<WorkEvent, 'seq'> & { readonly seq?: number };
// lastSeq is what lets a reader that has ALREADY rendered the stored turns attach to the
// stream without being handed the same turn a second time. Without it the window renders the
// turn from `turns`, then the stream replays its buffer and renders it all over again.
type SessionRecord = { session: WorkSession; turns: WorkTurn[]; lastSeq: number };

/**
 * Transactional task state and ordered events. Legacy JSON/JSONL remain untouched import sources.
 */
export interface SessionSummary extends WorkSession {
  /** Turns actually stored. 0 means the session was minted by a window open and never used. */
  readonly turnCount: number;
  /** The first prompt of the session, truncated. Empty when there are no turns. */
  readonly preview: string;
}

export class SessionStore {
  private readonly database: DatabaseSync;
  private readonly cache = new Map<string, WorkSession>();
  private closed = false;

  private constructor(database: DatabaseSync) {
    this.database = database;
  }

  static async open(stateDir: string): Promise<SessionStore> {
    const dir = join(stateDir, 'sessions');
    await mkdir(dir, { recursive: true });
    const database = new DatabaseSync(join(dir, 'tasks.sqlite'));
    const store = new SessionStore(database);
    try {
      database.exec('PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;');
      const check = database.prepare('PRAGMA quick_check').all() as { quick_check: string }[];
      if (check.length !== 1 || check[0]?.quick_check !== 'ok') throw new Error('Corrupt task database');
      const version = database.prepare('PRAGMA user_version').get() as { user_version: number };
      if (version.user_version !== 0 && version.user_version !== 1) throw new Error(`Unsupported task database version ${version.user_version}`);
      store.transaction(() => database.exec(SCHEMA));
      await store.importLegacy(dir);
      store.recoverInterrupted();
      return store;
    } catch (error) {
      store.close();
      throw error;
    }
  }

  close(): void {
    if (this.closed) return;
    this.database.close();
    this.closed = true;
    this.cache.clear();
  }

  private transaction<Result>(operation: () => Result): Result {
    this.database.exec('BEGIN IMMEDIATE');
    try {
      const result = operation();
      this.database.exec('COMMIT');
      return result;
    } catch (error) {
      this.database.exec('ROLLBACK');
      throw error;
    }
  }

  async create(title: string): Promise<WorkSession> {
    const now = new Date().toISOString();
    const session: WorkSession = { id: randomUUID(), title: title.slice(0, 120) || 'Untitled task', createdAt: now, lastActiveAt: now, standingGrants: [] };
    this.database.prepare('INSERT INTO sessions(id, payload) VALUES (?, ?)').run(session.id, JSON.stringify(session));
    this.cache.set(session.id, session);
    return session;
  }

  private session(id: string): WorkSession | null {
    validateId(id);
    const row = this.database.prepare('SELECT payload FROM sessions WHERE id = ?').get(id) as { payload: string } | undefined;
    if (!row) return null;
    const stored = validateSession(parse(row.payload, id), id);
    const session = this.cache.get(id) ?? stored;
    this.cache.set(id, session);
    return session;
  }

  async load(id: string): Promise<SessionRecord | null> {
    const session = this.session(id);
    if (!session) return null;
    const rows = this.database.prepare('SELECT id, phase, payload FROM turns WHERE session_id = ? ORDER BY position').all(id) as { id: string; phase: string; payload: string }[];
    const turns = rows.map((row) => {
      const turn = validateTurn(parse(row.payload, `${id}/${row.id}`), id);
      if (turn.id !== row.id || turn.phase !== row.phase) throw new Error(`Corrupt turn projection ${row.id}`);
      return turn;
    });
    const seq = this.database.prepare('SELECT next_seq FROM sessions WHERE id = ?').get(id) as { next_seq: number } | undefined;
    return { session, turns, lastSeq: Math.max(0, Number(seq?.next_seq ?? 1) - 1) };
  }

  /**
   * The list carries the two facts that make it readable, because without them it is not.
   *
   * Every window open minted a session whether or not a word was ever typed into it, so the sidebar
   * filled with rows titled "work" holding nothing at all -- and clicking one correctly rendered
   * nothing, which is indistinguishable from a broken click. The count separates the empty ones
   * from the real ones; the preview is the first thing that was asked, so the list reads as a list
   * of questions rather than 42 copies of the same word.
   */
  async list(): Promise<SessionSummary[]> {
    const rows = this.database.prepare(`
      SELECT s.id AS id,
             (SELECT COUNT(*) FROM turns t WHERE t.session_id = s.id) AS turnCount,
             (SELECT json_extract(t.payload, '$.prompt') FROM turns t
                WHERE t.session_id = s.id ORDER BY t.position LIMIT 1) AS preview
        FROM sessions s
    `).all() as { id: string; turnCount: number; preview: string | null }[];
    const sessions = rows.map((row) => ({
      ...(this.session(row.id) as WorkSession),
      turnCount: Number(row.turnCount ?? 0),
      preview: String(row.preview ?? '').slice(0, 140),
    }));
    return sessions.sort((a, b) => b.lastActiveAt.localeCompare(a.lastActiveAt));
  }

  private writeTurn(sessionId: string, turn: WorkTurn): WorkSession {
    validateId(sessionId);
    validateTurn(turn, sessionId);
    const current = this.session(sessionId);
    if (!current) throw new Error(`unknown session ${sessionId}`);
    const existing = this.database.prepare('SELECT position, payload FROM turns WHERE session_id = ? AND id = ?').get(sessionId, turn.id) as { position: number; payload: string } | undefined;
    if (existing) {
      const previous = validateTurn(parse(existing.payload, turn.id), sessionId);
      if (previous.prompt !== turn.prompt || previous.startedAt !== turn.startedAt
        || (TERMINAL.has(previous.phase) && JSON.stringify(previous) !== JSON.stringify(turn))) {
        throw new Error(`Refusing to overwrite turn ${turn.id}`);
      }
    }
    const position = existing?.position ?? (this.database.prepare('SELECT COALESCE(MAX(position), 0) + 1 AS position FROM turns WHERE session_id = ?').get(sessionId) as { position: number }).position;
    this.database.prepare('INSERT INTO turns(session_id, id, position, phase, payload) VALUES (?, ?, ?, ?, ?) ON CONFLICT(session_id, id) DO UPDATE SET phase = excluded.phase, payload = excluded.payload')
      .run(sessionId, turn.id, position, turn.phase, JSON.stringify(turn));
    const session = structuredClone(current);
    session.lastActiveAt = new Date().toISOString();
    if (position === 1 && session.title === 'Untitled task') session.title = (turn.prompt.split('\n')[0] ?? '').slice(0, 120) || 'Untitled task';
    this.database.prepare('UPDATE sessions SET payload = ? WHERE id = ?').run(JSON.stringify(validateSession(session, sessionId)), sessionId);
    return session;
  }

  private updateCache(session: WorkSession): void {
    const current = this.cache.get(session.id);
    if (current) Object.assign(current, session);
    else this.cache.set(session.id, session);
  }

  async addTurn(sessionId: string, turn: WorkTurn): Promise<void> {
    const session = this.transaction(() => this.writeTurn(sessionId, turn));
    this.updateCache(session);
  }

  async persist(sessionId: string): Promise<void> {
    validateId(sessionId);
    const session = this.session(sessionId);
    if (!session) return;
    this.database.prepare('UPDATE sessions SET payload = ? WHERE id = ?').run(JSON.stringify(validateSession(session, sessionId)), sessionId);
  }

  private writeEvent(sessionId: string, event: EventInput, legacySeq: number | null = null): WorkEvent {
    validateId(sessionId);
    const row = this.database.prepare('SELECT next_seq FROM sessions WHERE id = ?').get(sessionId) as { next_seq: number } | undefined;
    if (!row) throw new Error(`unknown session ${sessionId}`);
    const committed = validateEvent({ ...event, seq: row.next_seq });
    this.database.prepare('INSERT INTO events(session_id, seq, payload, legacy_seq) VALUES (?, ?, ?, ?)')
      .run(sessionId, committed.seq, JSON.stringify(committed), legacySeq);
    this.database.prepare('UPDATE sessions SET next_seq = next_seq + 1 WHERE id = ?').run(sessionId);
    return committed;
  }

  async appendEvent(sessionId: string, event: EventInput): Promise<WorkEvent> {
    return this.transaction(() => {
      const committed = this.writeEvent(sessionId, event);
      this.projectEvent(sessionId, committed);
      return committed;
    });
  }

  private projectEvent(sessionId: string, event: WorkEvent): void {
    const { turn_id: turnId, ...data } = event.data;
    if (turnId === undefined) return;
    validateId(turnId as string);
    const row = this.database.prepare('SELECT payload FROM turns WHERE session_id = ? AND id = ?').get(sessionId, turnId as string) as { payload: string } | undefined;
    if (!row) throw new Error(`Unknown turn ${String(turnId)} in session ${sessionId}`);
    const turn = validateTurn(parse(row.payload, String(turnId)), sessionId);
    if (TERMINAL.has(turn.phase)) throw new Error(`Turn ${turn.id} is already terminal`);
    if (event.kind === 'token') {
      if (typeof data.delta !== 'string') throw new Error('Invalid token event');
      turn.output += data.delta;
    } else if (event.kind === 'phase') {
      if (!PHASES.has(data.phase as string) || TERMINAL.has(data.phase as string)) throw new Error('Terminal phases require a final turn transaction');
      turn.phase = data.phase as WorkTurn['phase'];
    } else if (event.kind === 'consent_request') turn.phase = 'awaiting_consent';
    else if (event.kind === 'tool_result') turn.toolCalls.push(data as unknown as WorkTurn['toolCalls'][number]);
    else if (event.kind === 'usage') turn.usage = data;
    else if (event.kind === 'error' && typeof data.message === 'string') turn.error = data.message;
    validateTurn(turn, sessionId);
    this.database.prepare('UPDATE turns SET phase = ?, payload = ? WHERE session_id = ? AND id = ?')
      .run(turn.phase, JSON.stringify(turn), sessionId, turn.id);
  }

  async eventsAfter(sessionId: string, after = 0, limit = 1_000): Promise<WorkEvent[]> {
    validateId(sessionId);
    if (!Number.isSafeInteger(after) || after < 0) throw new Error('Invalid event cursor');
    if (!Number.isSafeInteger(limit) || limit < 1 || limit > 10_000) throw new Error('Invalid event page size');
    if (!this.session(sessionId)) throw new Error(`unknown session ${sessionId}`);
    const rows = this.database.prepare('SELECT seq, payload FROM events WHERE session_id = ? AND seq > ? ORDER BY seq LIMIT ?').all(sessionId, after, limit) as { seq: number; payload: string }[];
    return rows.map((row) => {
      const event = validateEvent(parse(row.payload, `${sessionId}/event/${row.seq}`));
      if (event.seq !== row.seq) throw new Error(`Corrupt event cursor ${row.seq}`);
      return event;
    });
  }

  async beginTurn(sessionId: string, turn: WorkTurn): Promise<WorkEvent> {
    const result = this.transaction(() => {
      validateId(sessionId);
      if (turn.phase !== 'queued') throw new Error('A new turn must be queued');
      const active = this.database.prepare("SELECT id FROM turns WHERE session_id = ? AND phase NOT IN ('done', 'failed', 'cancelled')").get(sessionId);
      if (active) throw new Error('a turn is already running in this session');
      const duplicate = this.database.prepare('SELECT id FROM turns WHERE session_id = ? AND id = ?').get(sessionId, turn.id);
      if (duplicate) throw new Error(`Turn ${turn.id} already exists`);
      const session = this.writeTurn(sessionId, turn);
      const event = this.writeEvent(sessionId, { at: turn.startedAt, kind: 'session', data: { turn_id: turn.id, prompt: turn.prompt, phase: 'queued' } });
      return { session, event };
    });
    this.updateCache(result.session);
    return result.event;
  }

  async finishTurn(
    sessionId: string,
    turn: WorkTurn,
    data: Record<string, unknown> = {},
    terminalEvents: readonly Omit<WorkEvent, 'seq' | 'at'>[] = [],
  ): Promise<WorkEvent[]> {
    if (!TERMINAL.has(turn.phase)) throw new Error('A finished turn must have a terminal phase');
    const result = this.transaction(() => {
      const session = this.writeTurn(sessionId, turn);
      const events = terminalEvents.map((event) => this.writeEvent(sessionId, {
        ...event, at: turn.endedAt ?? new Date().toISOString(), data: { ...event.data, turn_id: turn.id },
      }));
      const event = this.writeEvent(sessionId, {
        at: turn.endedAt ?? new Date().toISOString(), kind: 'phase',
        data: { ...data, turn_id: turn.id, phase: turn.phase, terminal: true, tool_calls: turn.toolCalls.length, usage: turn.usage ?? null, error: turn.error ?? null },
      });
      return { session, events: [...events, event] };
    });
    this.updateCache(result.session);
    return result.events;
  }

  private async importLegacy(dir: string): Promise<void> {
    const ids = new Set<string>();
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const suffix = entry.name.endsWith('.events.jsonl') ? '.events.jsonl' : entry.name.endsWith('.json') ? '.json' : null;
      if (!suffix) continue;
      if (!entry.isFile()) throw new Error(`Invalid legacy session file ${entry.name}`);
      const id = entry.name.slice(0, -suffix.length);
      validateId(id);
      ids.add(id);
    }
    const imports: { id: string; record: SessionRecord; events: WorkEvent[]; snapshot: Buffer; eventBytes: Buffer | null }[] = [];
    for (const id of [...ids].sort()) {
      const snapshot = await readFile(join(dir, `${id}.json`));
      const eventBytes = await readFile(join(dir, `${id}.events.jsonl`)).catch((error: NodeJS.ErrnoException) => {
        if (error.code === 'ENOENT') return null;
        throw error;
      });
      const decoder = new TextDecoder('utf-8', { fatal: true });
      const raw = object(parse(decoder.decode(snapshot), `${id}.json`), 'legacy snapshot');
      const session = validateSession(raw.session, id);
      if (!Array.isArray(raw.turns)) throw new Error(`Invalid legacy turns ${id}`);
      const turns = raw.turns.map((turn) => validateTurn(turn, id));
      if (new Set(turns.map((turn) => turn.id)).size !== turns.length) throw new Error(`Duplicate legacy turn ID in ${id}`);
      const events = eventBytes ? decoder.decode(eventBytes).split(/\r?\n/).flatMap((line, index) => {
        if (!line.trim()) return [];
        return [validateEvent(parse(line, `${id}.events.jsonl:${index + 1}`))];
      }) : [];
      // The legacy file format has no event cursor; the events it carries are replayed in full.
      imports.push({ id, record: { session, turns, lastSeq: events.length }, events, snapshot, eventBytes });
    }
    this.transaction(() => {
      for (const source of imports) {
        const snapshotHash = createHash('sha256').update(source.snapshot).digest('hex');
        const eventsHash = source.eventBytes ? createHash('sha256').update(source.eventBytes).digest('hex') : null;
        const previous = this.database.prepare('SELECT snapshot_hash, events_hash FROM legacy_imports WHERE session_id = ?').get(source.id) as { snapshot_hash: string; events_hash: string | null } | undefined;
        if (previous) {
          if (previous.snapshot_hash !== snapshotHash || previous.events_hash !== eventsHash) throw new Error(`Legacy source changed after import: ${source.id}; refusing overwrite`);
          continue;
        }
        if (this.session(source.id)) throw new Error(`Legacy session ${source.id} conflicts with durable state; refusing overwrite`);
        this.database.prepare('INSERT INTO sessions(id, payload) VALUES (?, ?)').run(source.id, JSON.stringify(source.record.session));
        const turns = new Map(source.record.turns.map((turn) => [turn.id, turn]));
        for (const event of source.events) {
          if (event.kind === 'session' && event.data.phase === 'queued') {
            validateId(event.data.turn_id as string);
            if (typeof event.data.prompt !== 'string') throw new Error(`Invalid legacy turn intent ${source.id}`);
            const id = event.data.turn_id as string;
            if (!turns.has(id)) turns.set(id, { id, sessionId: source.id, prompt: event.data.prompt, phase: 'queued', startedAt: event.at, toolCalls: [], output: '' });
            else if (turns.get(id)?.prompt !== event.data.prompt) throw new Error(`Conflicting legacy turn intent ${id}`);
          }
        }
        let position = 0;
        for (const turn of turns.values()) {
          position += 1;
          this.database.prepare('INSERT INTO turns(session_id, id, position, phase, payload) VALUES (?, ?, ?, ?, ?)')
            .run(source.id, turn.id, position, turn.phase, JSON.stringify(turn));
        }
        for (const event of source.events) this.writeEvent(source.id, event, event.seq);
        this.database.prepare('INSERT INTO legacy_imports(session_id, snapshot_hash, events_hash, snapshot_bytes, events_bytes, turn_count, event_count) VALUES (?, ?, ?, ?, ?, ?, ?)')
          .run(source.id, snapshotHash, eventsHash, source.snapshot.length, source.eventBytes?.length ?? 0, source.record.turns.length, source.events.length);
      }
    });
  }

  private recoverInterrupted(): void {
    this.transaction(() => {
      const rows = this.database.prepare("SELECT session_id, payload FROM turns WHERE phase NOT IN ('done', 'failed', 'cancelled') ORDER BY session_id, position").all() as { session_id: string; payload: string }[];
      for (const row of rows) {
        const turn = validateTurn(parse(row.payload, row.session_id), row.session_id);
        turn.phase = 'failed';
        turn.endedAt = new Date().toISOString();
        turn.error = 'Turn interrupted before durable completion. External effects may have occurred; outcome unknown, reconciliation required. No actions were replayed.';
        this.writeTurn(row.session_id, turn);
        this.writeEvent(row.session_id, { at: turn.endedAt, kind: 'error', data: { turn_id: turn.id, message: turn.error, code: 'TURN_INTERRUPTED_RECONCILIATION_REQUIRED' } });
        this.writeEvent(row.session_id, { at: turn.endedAt, kind: 'phase', data: { turn_id: turn.id, phase: 'failed', terminal: true, recovered: true, needs_reconciliation: true, outcome: 'unknown' } });
      }
    });
    this.cache.clear();
  }

  /**
   * Prior turns rendered for the engine.
   *
   * Only the operator prompt and the agent's final prose are replayed. Tool traffic is recorded
   * in the event log for the operator, but replaying it would consume the whole context window
   * on file contents the agent can simply read again.
   */
  async history(sessionId: string): Promise<EngineMessage[]> {
    const record = await this.load(sessionId);
    if (!record) return [];
    const messages: EngineMessage[] = [];
    for (const turn of record.turns.slice(-HISTORY_TURNS)) {
      if (turn.phase !== 'done') continue;
      messages.push({ role: 'user', content: turn.prompt });
      const summary = turn.toolCalls.length > 0
        ? `${turn.output}\n\n[${turn.toolCalls.length} tool steps: ${turn.toolCalls.map((call) => call.summary).join('; ').slice(0, 800)}]`
        : turn.output;
      messages.push({ role: 'assistant', content: summary.slice(0, 8_000) });
    }
    return messages;
  }

  async rename(sessionId: string, title: string): Promise<void> {
    const current = this.session(sessionId);
    if (!current) throw new Error(`unknown session ${sessionId}`);
    const session = { ...current, title: title.slice(0, 120) || current.title };
    this.database.prepare('UPDATE sessions SET payload = ? WHERE id = ?').run(JSON.stringify(validateSession(session, sessionId)), sessionId);
    this.updateCache(session);
  }
}
