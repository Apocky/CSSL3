import { randomUUID } from 'node:crypto';
import { appendFile, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { EngineMessage } from './engine';
import type { WorkEvent, WorkSession, WorkTurn } from './types';

const HISTORY_TURNS = 8;

/**
 * Durable session store: one JSON document per session plus an append-only event log.
 *
 * The event log is never rewritten. A transcript that can be edited after the fact cannot be
 * audited, including by the person who ran it, and this lane writes to a real filesystem.
 */
export class SessionStore {
  private readonly dir: string;
  private readonly cache = new Map<string, { session: WorkSession; turns: WorkTurn[] }>();

  private constructor(dir: string) {
    this.dir = dir;
  }

  static async open(stateDir: string): Promise<SessionStore> {
    const dir = join(stateDir, 'sessions');
    await mkdir(dir, { recursive: true });
    return new SessionStore(dir);
  }

  private sessionPath(id: string): string {
    return join(this.dir, `${id}.json`);
  }

  private eventPath(id: string): string {
    return join(this.dir, `${id}.events.jsonl`);
  }

  async create(title: string): Promise<WorkSession> {
    const now = new Date().toISOString();
    const session: WorkSession = { id: randomUUID(), title: title.slice(0, 120) || 'Untitled task', createdAt: now, lastActiveAt: now, standingGrants: [] };
    this.cache.set(session.id, { session, turns: [] });
    await this.persist(session.id);
    return session;
  }

  async load(id: string): Promise<{ session: WorkSession; turns: WorkTurn[] } | null> {
    const cached = this.cache.get(id);
    if (cached) return cached;
    const raw = await readFile(this.sessionPath(id), 'utf8').catch(() => null);
    if (!raw) return null;
    try {
      const parsed = JSON.parse(raw) as { session: WorkSession; turns: WorkTurn[] };
      this.cache.set(id, parsed);
      return parsed;
    } catch {
      return null;
    }
  }

  async list(): Promise<WorkSession[]> {
    const names = await readdir(this.dir).catch(() => [] as string[]);
    const sessions: WorkSession[] = [];
    for (const name of names) {
      if (!name.endsWith('.json') || name.endsWith('.events.jsonl')) continue;
      const record = await this.load(name.slice(0, -5));
      if (record) sessions.push(record.session);
    }
    return sessions.sort((a, b) => b.lastActiveAt.localeCompare(a.lastActiveAt));
  }

  async addTurn(sessionId: string, turn: WorkTurn): Promise<void> {
    const record = await this.load(sessionId);
    if (!record) throw new Error(`unknown session ${sessionId}`);
    record.turns.push(turn);
    record.session.lastActiveAt = new Date().toISOString();
    if (record.turns.length === 1 && record.session.title === 'Untitled task') {
      record.session.title = (turn.prompt.split('\n')[0] ?? '').slice(0, 120) || 'Untitled task';
    }
    await this.persist(sessionId);
  }

  async persist(sessionId: string): Promise<void> {
    const record = this.cache.get(sessionId);
    if (!record) return;
    await writeFile(this.sessionPath(sessionId), JSON.stringify(record, null, 2), 'utf8');
  }

  async appendEvent(sessionId: string, event: WorkEvent): Promise<void> {
    await appendFile(this.eventPath(sessionId), `${JSON.stringify(event)}\n`, 'utf8');
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
    const record = await this.load(sessionId);
    if (!record) throw new Error(`unknown session ${sessionId}`);
    record.session.title = title.slice(0, 120) || record.session.title;
    await this.persist(sessionId);
  }
}
