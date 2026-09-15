// One mind. Every surface that speaks as Apocrypha assembles its prompt here.
//
// The live room talked straight to the engine: raw history, no persona, no memory, no profile. So
// apocky.com and the room were two different minds wearing one name, which is the thing the owner
// said must not happen ("I want what is hosted on apocky.com and in the apps to be all the same
// mind/person/brain responding/generating", 2026-09-14).
//
// The persona is IMPORTED from the worker rather than restated, so the two cannot drift: if the
// worker's voice changes, this changes with it. A copied constant would pass every test on the day
// it was written and quietly diverge forever after.

import { DatabaseSync } from 'node:sqlite';
import { baseSystem } from '../apocrypha-worker/prompt';
import type { ClaimedJob } from '../apocrypha-worker/types';
import { AXES } from '../apocrypha-profile/grounding';

export interface MindMessage {
  readonly role: 'system' | 'user' | 'assistant';
  readonly content: string;
}

export interface AssembleOptions {
  readonly messages: readonly MindMessage[];
  readonly profileDb?: string | null;
  readonly anamnesisDb?: string | null;
  readonly profileChars?: number;
  readonly memoryChars?: number;
  readonly memoryRecords?: number;
}

export interface Assembled {
  readonly messages: MindMessage[];
  readonly profileClaims: number;
  readonly memoryRecords: number;
  readonly personaBytes: number;
}

// A synthetic owner-chat job: the capability and kind that select the owner persona in the worker.
// Nothing here reaches the control plane; it exists only to address baseSystem().
const OWNER_JOB = {
  jobId: 'mind', attemptId: 'mind', attemptNo: 1, leaseEpoch: 1, leaseToken: '',
  leaseExpiresAt: '', tenantId: 'local', ownerPrincipalId: 'owner',
  kind: 'apocky_chat', capability: 'apocky_owner_chat',
  request: {}, modelAlias: '', profileHash: '', toolRegistryVersion: '', memoryManifestHash: '',
} as unknown as ClaimedJob;

export function personaFor(): string {
  return baseSystem(OWNER_JOB);
}

/**
 * Render what Apocrypha has learned about the person it is speaking to.
 *
 * Ordered by corroboration, because a thing said fifty times outranks a thing said once, and
 * budgeted hard: a profile that evicts the conversation has made Apocrypha worse, not better.
 */
export function profileForPrompt(databasePath: string, budgetChars = 2_400): string {
  let database: DatabaseSync;
  try {
    database = new DatabaseSync(databasePath, { readOnly: true });
  } catch {
    return ''; // No profile yet is a legitimate state, not an error.
  }
  try {
    const rows = database.prepare(
      "SELECT axis, statement, corroborations FROM claims WHERE status = 'live' "
      + 'ORDER BY corroborations DESC, last_seen DESC LIMIT 400',
    ).all() as unknown as Array<{ axis: string; statement: string; corroborations: number }>;
    if (rows.length === 0) return '';

    // Round-robin across axes so a single prolific axis cannot crowd the rest out. Corrections
    // outnumber everything else roughly four to one in the corpus.
    const byAxis = new Map<string, typeof rows>();
    for (const row of rows) {
      const bucket = byAxis.get(row.axis) ?? [];
      bucket.push(row);
      byAxis.set(row.axis, bucket);
    }
    const ordered: typeof rows = [];
    for (let depth = 0; ordered.length < rows.length; depth += 1) {
      let advanced = false;
      for (const axis of AXES) {
        const bucket = byAxis.get(axis);
        const row = bucket?.[depth];
        if (row) { ordered.push(row); advanced = true; }
      }
      if (!advanced) break;
    }

    const header = 'What you have learned about this person, from what he has actually written:';
    const lines: string[] = [];
    let used = header.length;
    for (const row of ordered) {
      const line = `- [${row.axis} x${row.corroborations}] ${row.statement}`;
      if (used + line.length + 1 > budgetChars) break;
      lines.push(line);
      used += line.length + 1;
    }
    if (lines.length === 0) return '';
    return [header, ...lines].join('\n');
  } catch {
    return '';
  } finally {
    database.close();
  }
}

function terms(query: string): string[] {
  return [...query.toLowerCase().matchAll(/[a-z0-9][a-z0-9_.-]*/gu)]
    .map((match) => match[0])
    .filter((token) => token.length >= 3)
    .slice(0, 24);
}

/**
 * Admitted memory for this turn, read straight from the ledger.
 *
 * Read-only, and never returns redacted rows. This is narrower than the worker's six-adapter
 * federation -- it is the ledger, not mempalace/3MNEME/brainmonsoon -- and the caller is told the
 * record count so an empty read is visible rather than assumed.
 */
export function memoryForPrompt(
  databasePath: string,
  query: string,
  options: { readonly maxRecords?: number; readonly budgetChars?: number } = {},
): { text: string; records: number } {
  const maxRecords = options.maxRecords ?? 12;
  const budget = options.budgetChars ?? 6_000;
  const tokens = terms(query);
  if (tokens.length === 0) return { text: '', records: 0 };

  let database: DatabaseSync;
  try {
    database = new DatabaseSync(databasePath, { readOnly: true });
  } catch {
    return { text: '', records: 0 };
  }
  try {
    const match = tokens.map((token) => `"${token}"`).join(' OR ');
    const rows = database.prepare(
      'SELECT r.id, r.ts, r.kind, r.ref, r.payload, r.provenance, bm25(records_fts) AS rank '
      + 'FROM records_fts JOIN records r ON r.id = records_fts.rowid '
      + 'WHERE records_fts MATCH ? AND r.redacted = 0 ORDER BY rank, r.id DESC LIMIT ?',
    ).all(match, maxRecords) as unknown as Array<{
      id: number; ts: string; kind: string; ref: string | null; payload: string | null; provenance: string | null;
    }>;
    if (rows.length === 0) return { text: '', records: 0 };

    const header = 'Admitted memory records. Cite a record by its bracketed source and id when you rely on it:';
    const lines: string[] = [];
    let used = header.length;
    for (const row of rows) {
      const body = (row.payload ?? '').replace(/\s+/gu, ' ').trim();
      const line = `[anamnesis:${row.id} ${row.kind}${row.provenance ? ` prov=${row.provenance}` : ''}] ${body}`;
      const clipped = line.length > 900 ? `${line.slice(0, 900)}...` : line;
      if (used + clipped.length + 1 > budget) break;
      lines.push(clipped);
      used += clipped.length + 1;
    }
    return { text: lines.length ? [header, ...lines].join('\n') : '', records: lines.length };
  } catch {
    return { text: '', records: 0 };
  } finally {
    database.close();
  }
}

/**
 * Build the message list the engine actually receives.
 *
 * Order is deliberate and prefix-cache friendly: the persona is byte-stable across every turn, so
 * it stays first and the engine can reuse its KV. Profile changes only when a distillation pass
 * lands, so it comes next. Memory and the conversation change every turn, so they come last --
 * putting either of them earlier would invalidate the cached prefix on every single message.
 */
export function assemble(options: AssembleOptions): Assembled {
  const persona = personaFor();
  const history = options.messages.filter((message) => message.role !== 'system');
  const lastUser = [...history].reverse().find((message) => message.role === 'user');
  const query = lastUser?.content ?? '';

  const profile = options.profileDb
    ? profileForPrompt(options.profileDb, options.profileChars ?? 2_400)
    : '';
  const memory = options.anamnesisDb
    ? memoryForPrompt(options.anamnesisDb, query, {
      maxRecords: options.memoryRecords ?? 12,
      budgetChars: options.memoryChars ?? 6_000,
    })
    : { text: '', records: 0 };

  const messages: MindMessage[] = [{ role: 'system', content: persona }];
  if (profile) messages.push({ role: 'system', content: profile });
  // Evidence is its own message, never folded into the system text: the caller's last message must
  // remain the last message, byte for byte.
  if (memory.text) messages.push({ role: 'system', content: memory.text });
  messages.push(...history);

  return {
    messages,
    profileClaims: profile ? profile.split('\n').length - 1 : 0,
    memoryRecords: memory.records,
    personaBytes: persona.length,
  };
}
