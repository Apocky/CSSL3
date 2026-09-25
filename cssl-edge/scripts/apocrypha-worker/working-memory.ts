// Episodic working memory: memory that stays loaded between turns instead of being re-fetched,
// or skipped, on every one.
//
// Memory used to be opt-in -- isMemoryNeeded() returned false unless the message literally
// contained "recall", "remember", "memory" or "who am i". Ask Apocrypha what you are working on
// and it answered with nothing loaded. The reason it was gated is real: a federated probe across
// six adapters costs seconds, and paying that on every turn was unacceptable.
//
// Both facts are true, so the fix is not to pick one. Memory loads by default AND the cost stops
// being per-turn: a conversation keeps a warm working set, served immediately and revalidated
// behind the turn. Stale-while-revalidate, with a hard ceiling so "warm" can never mean
// "indefinitely old" -- a cache with no ceiling is how a system ends up confidently reciting
// last week's state.

import type { RetrievalBundle, RetrievalRecord } from './types';

export type MemoryOrigin =
  | 'cold'          // nothing held; the turn waited for a real read
  | 'fresh'         // served from the working set, still inside the fresh window
  | 'revalidating'  // served warm; a refresh is running behind this turn
  | 'requery'       // the question changed: this turn waited for a read OF ITS OWN QUESTION
  | 'expired'       // too old to serve; the turn waited for a real read
  | 'failed';       // refresh failed and nothing warm was holdable

export interface WorkingMemoryOptions {
  readonly freshMs?: number;
  readonly staleMs?: number;
  readonly maxAgeMs?: number;
  readonly maxConversations?: number;
  readonly maxRecords?: number;
  readonly now?: () => number;
}

export interface EnsureResult {
  readonly bundle: RetrievalBundle;
  readonly origin: MemoryOrigin;
  readonly ageMs: number;
  readonly episodicRecords: number;
}

interface Entry {
  bundle: RetrievalBundle;
  fetchedAt: number;
  touchedAt: number;
  episodic: RetrievalRecord[];
  refreshing: Promise<void> | null;
  /** The question the last read answered. A different question is a different read. */
  query: string;
}

function recordKey(record: RetrievalRecord): string {
  const provenance = typeof record.provenanceId === 'string' ? record.provenanceId.trim() : '';
  const text = typeof record.text === 'string' ? record.text : '';
  // Provenance identifies a record across reads; text is the fallback when an adapter returns
  // none, so the same fact fetched twice still collapses to one episodic entry.
  return provenance !== ''
    ? `${record.source ?? ''}:${provenance}`
    : `${record.source ?? ''}:${text.slice(0, 160)}`;
}

/**
 * Merge a newly read bundle into the conversation's episodic set.
 *
 * Newest wins on collision and sits first; older records survive underneath until the cap pushes
 * them out. This is what makes the memory *episodic* rather than a per-turn snapshot: something
 * recalled on turn 2 is still there on turn 9 without having to be recalled again.
 */
export function mergeEpisodic(
  prior: readonly RetrievalRecord[],
  incoming: readonly RetrievalRecord[],
  cap: number,
): RetrievalRecord[] {
  const merged: RetrievalRecord[] = [];
  const seen = new Set<string>();
  for (const record of [...incoming, ...prior]) {
    const key = recordKey(record);
    if (seen.has(key)) continue;
    seen.add(key);
    merged.push(record);
    if (merged.length >= cap) break;
  }
  return merged;
}

export class WorkingMemory {
  private readonly entries = new Map<string, Entry>();
  private readonly freshMs: number;
  private readonly staleMs: number;
  private readonly maxAgeMs: number;
  private readonly maxConversations: number;
  private readonly maxRecords: number;
  private readonly now: () => number;

  constructor(options: WorkingMemoryOptions = {}) {
    this.freshMs = options.freshMs ?? 60_000;
    this.staleMs = options.staleMs ?? 300_000;
    this.maxAgeMs = options.maxAgeMs ?? 900_000;
    this.maxConversations = options.maxConversations ?? 64;
    this.maxRecords = options.maxRecords ?? 60;
    this.now = options.now ?? Date.now;
  }

  /**
   * Return this conversation's memory, loading it if there is nothing warm enough to serve.
   *
   * `load` is only awaited when there is genuinely nothing holdable: a cold conversation or one
   * past the hard ceiling. Otherwise the warm set is returned straight away and any refresh runs
   * behind the turn, where it costs the user nothing.
   */
  async ensure(key: string, load: () => Promise<RetrievalBundle>, query = ''): Promise<EnsureResult> {
    const entry = this.entries.get(key);
    const at = this.now();

    // Observed 2026-09-25: a conversation that turned to Palworld kept being served the records
    // its FIRST question retrieved, for fifteen minutes, so Apocrypha "remembered nothing" about
    // the new topic. The warm set is only a stand-in for the same question; a new question reads
    // now, and what it finds is merged on top of what the conversation already holds.
    if (entry !== undefined && query !== '' && entry.query !== query) {
      try {
        const bundle = await load();
        const stored = this.store(key, bundle, at, query);
        return { bundle: this.project(stored), origin: 'requery', ageMs: 0, episodicRecords: stored.episodic.length };
      } catch {
        entry.touchedAt = at;
        return { bundle: this.project(entry), origin: 'failed', ageMs: at - entry.fetchedAt, episodicRecords: entry.episodic.length };
      }
    }

    if (entry !== undefined) {
      const age = at - entry.fetchedAt;
      entry.touchedAt = at;
      if (age <= this.freshMs) {
        return { bundle: this.project(entry), origin: 'fresh', ageMs: age, episodicRecords: entry.episodic.length };
      }
      if (age <= this.maxAgeMs) {
        // Serve now, revalidate behind the turn. A refresh already in flight is not duplicated.
        if (entry.refreshing === null && age > this.staleMs) {
          entry.refreshing = this.refresh(key, load).finally(() => {
            const current = this.entries.get(key);
            if (current) current.refreshing = null;
          });
        }
        return { bundle: this.project(entry), origin: 'revalidating', ageMs: age, episodicRecords: entry.episodic.length };
      }
    }

    // Cold, or past the ceiling: this turn pays for a real read.
    const origin: MemoryOrigin = entry === undefined ? 'cold' : 'expired';
    try {
      const bundle = await load();
      const stored = this.store(key, bundle, at, query);
      return { bundle: this.project(stored), origin, ageMs: 0, episodicRecords: stored.episodic.length };
    } catch (error) {
      // An expired entry is still better than nothing, but it must not be reported as a healthy
      // read: the caller needs to know memory is degraded, not merely old.
      if (entry !== undefined) {
        return { bundle: this.project(entry), origin: 'failed', ageMs: at - entry.fetchedAt, episodicRecords: entry.episodic.length };
      }
      throw error;
    }
  }

  private async refresh(key: string, load: () => Promise<RetrievalBundle>): Promise<void> {
    try {
      const bundle = await load();
      this.store(key, bundle, this.now(), this.entries.get(key)?.query ?? '');
    } catch {
      // Leave the warm entry in place; the next turn past maxAgeMs will block and retry properly.
    }
  }

  private store(key: string, bundle: RetrievalBundle, at: number, query: string): Entry {
    const prior = this.entries.get(key);
    const episodic = mergeEpisodic(prior?.episodic ?? [], bundle.records ?? [], this.maxRecords);
    const entry: Entry = {
      bundle, fetchedAt: at, touchedAt: at, episodic, refreshing: prior?.refreshing ?? null, query,
    };
    this.entries.set(key, entry);
    this.evict();
    return entry;
  }

  /** The bundle the caller sees carries the episodic set, not just this read's records. */
  private project(entry: Entry): RetrievalBundle {
    return { ...entry.bundle, records: entry.episodic };
  }

  private evict(): void {
    if (this.entries.size <= this.maxConversations) return;
    const ordered = [...this.entries.entries()].sort((a, b) => a[1].touchedAt - b[1].touchedAt);
    for (const [key] of ordered.slice(0, this.entries.size - this.maxConversations)) {
      this.entries.delete(key);
    }
  }

  /** Observability: a cache that cannot be inspected cannot be shown to be working (G2). */
  stats(): { conversations: number; records: number; oldestMs: number | null } {
    const at = this.now();
    let records = 0;
    let oldest: number | null = null;
    for (const entry of this.entries.values()) {
      records += entry.episodic.length;
      const age = at - entry.fetchedAt;
      if (oldest === null || age > oldest) oldest = age;
    }
    return { conversations: this.entries.size, records, oldestMs: oldest };
  }

  forget(key: string): void {
    this.entries.delete(key);
  }
}
