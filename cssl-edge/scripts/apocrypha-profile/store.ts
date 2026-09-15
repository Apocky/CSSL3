// The profile store: what Apocrypha has learned about Apocky, and where each piece came from.
//
// Separate from anamnesis.db on purpose. This file is a derived, model-authored artifact about a
// person; the ledger is an append-only record of work. Keeping them apart means the profile can be
// deleted outright -- one file, one move -- without touching provenance that took months to build.
//
// Promotion is automatic (owner decision, 2026-09-14), so the safety property is not a gate before
// the write but reversibility after it: every claim AND every piece of evidence is stamped with the
// pass that produced it, and revertPass() removes that pass whole. An audit trail nobody can act on
// is decoration.

import { DatabaseSync } from 'node:sqlite';
import type { Grounded, Rejection } from './grounding';

export interface PassSummary {
  readonly id: number;
  readonly startedAt: string;
  readonly finishedAt: string | null;
  readonly chunksRead: number;
  readonly claimsOffered: number;
  readonly claimsGrounded: number;
  readonly claimsNew: number;
  readonly claimsCorroborated: number;
  readonly model: string | null;
}

const SCHEMA = `
CREATE TABLE IF NOT EXISTS passes(
  id INTEGER PRIMARY KEY,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  corpus_watermark TEXT,
  chunks_read INTEGER NOT NULL DEFAULT 0,
  claims_offered INTEGER NOT NULL DEFAULT 0,
  claims_grounded INTEGER NOT NULL DEFAULT 0,
  claims_new INTEGER NOT NULL DEFAULT 0,
  claims_corroborated INTEGER NOT NULL DEFAULT 0,
  rejected_json TEXT,
  model TEXT
);
CREATE TABLE IF NOT EXISTS claims(
  id INTEGER PRIMARY KEY,
  pass_id INTEGER NOT NULL,
  axis TEXT NOT NULL,
  statement TEXT NOT NULL,
  statement_key TEXT NOT NULL UNIQUE,
  corroborations INTEGER NOT NULL DEFAULT 1,
  first_seen TEXT,
  last_seen TEXT,
  status TEXT NOT NULL DEFAULT 'live',
  killed_by INTEGER,
  killed_reason TEXT,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS evidence(
  claim_id INTEGER NOT NULL,
  chunk_id INTEGER NOT NULL,
  pass_id INTEGER NOT NULL,
  chunk_sha TEXT,
  quote TEXT NOT NULL,
  session_id TEXT,
  event_ts TEXT,
  PRIMARY KEY(claim_id, chunk_id)
);
CREATE INDEX IF NOT EXISTS claims_axis ON claims(axis, status, corroborations DESC);
CREATE INDEX IF NOT EXISTS claims_pass ON claims(pass_id);
CREATE INDEX IF NOT EXISTS evidence_pass ON evidence(pass_id);
`;

// Dedupe key. Two passes phrasing the same observation identically are the SAME claim with two
// pieces of evidence, not two claims -- otherwise corroboration counts inflate on re-reads and a
// thing said once looks as well-attested as a thing said fifty times.
export function statementKey(axis: string, statement: string): string {
  return `${axis}::${statement.toLowerCase().replace(/[^a-z0-9]+/gu, ' ').trim()}`;
}

export type RecordOutcome = 'new' | 'corroborated' | 'duplicate';

export class ProfileStore {
  private readonly database: DatabaseSync;

  constructor(path: string, options: { readonly readOnly?: boolean } = {}) {
    this.database = new DatabaseSync(path, { readOnly: options.readOnly ?? false });
    if (!options.readOnly) {
      this.database.exec('PRAGMA journal_mode=WAL');
      this.database.exec(SCHEMA);
    }
  }

  // No watermark at begin: an unfinished pass must never look like progress. finishPass writes
  // the position actually distilled.
  beginPass(model: string, _watermark: string | null = null): number {
    const result = this.database
      .prepare('INSERT INTO passes(started_at, model) VALUES(?,?)')
      .run(new Date().toISOString(), model);
    return Number(result.lastInsertRowid);
  }

  /**
   * Record one grounded claim.
   *
   * Corroboration is defined as the number of DISTINCT source chunks supporting a statement, so
   * it is incremented only when a genuinely new piece of evidence lands. Re-running a pass over
   * the same chunks must not make a claim look better attested than it is; that would turn the
   * one number the retrieval layer ranks on into a count of how often the job ran.
   */
  record(passId: number, grounded: Grounded): RecordOutcome {
    const { claim, chunk } = grounded;
    const key = statementKey(claim.axis, claim.statement);
    const ts = chunk.eventTs ?? new Date().toISOString();

    const existing = this.database
      .prepare('SELECT id, first_seen, last_seen FROM claims WHERE statement_key = ?')
      .get(key) as { id: number; first_seen: string | null; last_seen: string | null } | undefined;

    if (existing === undefined) {
      const result = this.database.prepare(
        'INSERT INTO claims(pass_id, axis, statement, statement_key, first_seen, last_seen, created_at) '
        + 'VALUES(?,?,?,?,?,?,?)',
      ).run(passId, claim.axis, claim.statement, key, ts, ts, new Date().toISOString());
      const claimId = Number(result.lastInsertRowid);
      this.insertEvidence(claimId, passId, grounded);
      return 'new';
    }

    const inserted = this.insertEvidence(existing.id, passId, grounded);
    if (!inserted) return 'duplicate';

    const first = existing.first_seen && existing.first_seen < ts ? existing.first_seen : ts;
    const last = existing.last_seen && existing.last_seen > ts ? existing.last_seen : ts;
    this.database.prepare(
      'UPDATE claims SET corroborations = corroborations + 1, first_seen = ?, last_seen = ? WHERE id = ?',
    ).run(first, last, existing.id);
    return 'corroborated';
  }

  private insertEvidence(claimId: number, passId: number, grounded: Grounded): boolean {
    const { claim, chunk } = grounded;
    const result = this.database.prepare(
      'INSERT OR IGNORE INTO evidence(claim_id, chunk_id, pass_id, chunk_sha, quote, session_id, event_ts) '
      + 'VALUES(?,?,?,?,?,?,?)',
    ).run(claimId, claim.chunkId, passId, chunk.sha256, claim.quote, chunk.sessionId, chunk.eventTs);
    return Number(result.changes) > 0;
  }

  finishPass(
    passId: number,
    totals: {
      readonly chunksRead: number; readonly claimsOffered: number; readonly claimsGrounded: number;
      readonly claimsNew: number; readonly claimsCorroborated: number;
      readonly rejected: Record<Rejection, number>;
      readonly watermark: string | null;
    },
  ): void {
    // The watermark is written HERE, not at beginPass, and it is the position actually DISTILLED
    // rather than the position read. A pass that reads 7,512 chunks and distils 24 of them has
    // made 24 chunks of progress; recording the read position would let the next run resume past
    // everything it skipped, and those chunks would never be looked at again. Same failure family
    // as an indexer recording a file as done after extracting nothing from it.
    this.database.prepare(
      'UPDATE passes SET finished_at = ?, chunks_read = ?, claims_offered = ?, claims_grounded = ?, '
      + 'claims_new = ?, claims_corroborated = ?, rejected_json = ?, corpus_watermark = ? WHERE id = ?',
    ).run(
      new Date().toISOString(), totals.chunksRead, totals.claimsOffered, totals.claimsGrounded,
      totals.claimsNew, totals.claimsCorroborated, JSON.stringify(totals.rejected),
      totals.watermark, passId,
    );
  }

  /**
   * Undo one pass completely: the one-command revert that makes automatic promotion safe.
   *
   * Claims first seen in this pass are deleted outright with their evidence. Claims that merely
   * gained a corroboration here survive -- they were true before this pass ran -- but this pass's
   * evidence is removed and the count decremented by exactly the number of evidence rows it
   * contributed, so the invariant "corroborations == distinct evidence rows" holds after a revert
   * as well as before one.
   */
  revertPass(passId: number): { readonly deleted: number; readonly decremented: number } {
    const owned = this.database.prepare('SELECT id FROM claims WHERE pass_id = ?').all(passId) as { id: number }[];
    const ownedIds = new Set(owned.map((row) => row.id));

    const contributions = this.database.prepare(
      'SELECT claim_id AS claimId, COUNT(*) AS n FROM evidence WHERE pass_id = ? GROUP BY claim_id',
    ).all(passId) as { claimId: number; n: number }[];

    let decremented = 0;
    for (const { claimId, n } of contributions) {
      if (ownedIds.has(claimId)) continue;
      this.database.prepare(
        'UPDATE claims SET corroborations = MAX(1, corroborations - ?) WHERE id = ?',
      ).run(n, claimId);
      decremented += 1;
    }

    this.database.prepare('DELETE FROM evidence WHERE pass_id = ?').run(passId);
    for (const id of ownedIds) {
      this.database.prepare('DELETE FROM evidence WHERE claim_id = ?').run(id);
      this.database.prepare('DELETE FROM claims WHERE id = ?').run(id);
    }
    this.database.prepare('DELETE FROM passes WHERE id = ?').run(passId);
    return { deleted: ownedIds.size, decremented };
  }

  /** Top claims per axis, best-attested first. The retrieval layer reads through this. */
  top(axis: string, limit: number): Array<{ statement: string; corroborations: number; lastSeen: string | null }> {
    return this.database.prepare(
      'SELECT statement, corroborations, last_seen AS lastSeen FROM claims '
      + "WHERE axis = ? AND status = 'live' ORDER BY corroborations DESC, last_seen DESC LIMIT ?",
    ).all(axis, limit) as unknown as Array<{ statement: string; corroborations: number; lastSeen: string | null }>;
  }

  /**
   * The corpus position of the last pass that actually finished.
   *
   * Only FINISHED passes count. Resuming from an interrupted pass's watermark would skip every
   * chunk it read but had not yet distilled -- silent, permanent gaps in the profile that nothing
   * downstream could detect.
   */
  lastWatermark(): string | null {
    const row = this.database.prepare(
      'SELECT corpus_watermark FROM passes WHERE finished_at IS NOT NULL '
      + 'AND corpus_watermark IS NOT NULL ORDER BY id DESC LIMIT 1',
    ).get() as { corpus_watermark: string | null } | undefined;
    return row?.corpus_watermark ?? null;
  }

  counts(): Record<string, number> {
    const rows = this.database.prepare(
      "SELECT axis, COUNT(*) AS n FROM claims WHERE status = 'live' GROUP BY axis",
    ).all() as unknown as { axis: string; n: number }[];
    return Object.fromEntries(rows.map((row) => [row.axis, row.n]));
  }

  /** Invariant check: corroborations must equal the number of distinct evidence rows. */
  audit(): Array<{ claimId: number; corroborations: number; evidence: number }> {
    return this.database.prepare(
      'SELECT c.id AS claimId, c.corroborations, '
      + '(SELECT COUNT(*) FROM evidence e WHERE e.claim_id = c.id) AS evidence '
      + 'FROM claims c WHERE c.corroborations != '
      + '(SELECT COUNT(*) FROM evidence e WHERE e.claim_id = c.id)',
    ).all() as unknown as Array<{ claimId: number; corroborations: number; evidence: number }>;
  }

  passes(limit = 10): PassSummary[] {
    return this.database.prepare(
      'SELECT id, started_at AS startedAt, finished_at AS finishedAt, chunks_read AS chunksRead, '
      + 'claims_offered AS claimsOffered, claims_grounded AS claimsGrounded, claims_new AS claimsNew, '
      + 'claims_corroborated AS claimsCorroborated, model FROM passes ORDER BY id DESC LIMIT ?',
    ).all(limit) as unknown as PassSummary[];
  }

  close(): void {
    this.database.close();
  }
}
