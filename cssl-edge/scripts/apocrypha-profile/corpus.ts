// The authorship gate for everything Apocrypha learns about Apocky.
//
// `role='user'` in the transcript index does NOT mean "Apocky wrote this". Measured 2026-09-14 over
// anamnesis.db: 23,190 role=user chunks totalling 161.7 MB, of which 18,744 are Codex harness
// scaffolding (`<codex_internal_context>` wrappers), plus tool results and system reminders folded
// into the user turn. Apocky's own prose is ~4,420 messages / 1.4 MB -- under 1% of the bytes on
// that axis. A profile distilled from the raw set would learn to emit harness wrappers.
//
// So this module does not filter, it STRATIFIES. Every chunk lands in exactly one named stratum, a
// declared subset is admissible, and text matching no known stratum becomes `unknown`: excluded AND
// counted. G10 -- an unclassified chunk must never silently become an Apocky chunk, and a growing
// `unknown` census is the signal that a new harness shape has appeared upstream and the gate needs
// a new stratum rather than a wider net.

import { DatabaseSync } from 'node:sqlite';

export type Stratum =
  | 'apocky'
  | 'apocky-terse'
  | 'tool-result'
  | 'harness-context'
  | 'attachment-manifest'
  | 'pasted-artifact'
  | 'code-paste'
  | 'structured-payload'
  | 'unknown';

export const STRATA: readonly Stratum[] = [
  'apocky', 'apocky-terse', 'tool-result', 'harness-context',
  'attachment-manifest', 'pasted-artifact', 'code-paste', 'structured-payload', 'unknown',
];

// Only these carry Apocky's voice. Everything else is excluded from distillation.
export const ADMISSIBLE: ReadonlySet<Stratum> = new Set<Stratum>(['apocky', 'apocky-terse']);

// Machine markers match ANYWHERE in the text, not just the head: the harness appends reminders to
// the end of otherwise-real messages, and a head-only test admits those whole.
export const MACHINE_MARKERS: ReadonlyArray<readonly [Stratum, RegExp]> = [
  ['tool-result', /tool_use_id|"type"\s*:\s*"tool_result"|toolUseResult|Result of calling the \w+ tool/],
  ['harness-context', /<system-reminder>|<codex_internal_context|Treat it as the task to pursue|This session is being continued from a previous conversation|Caveat: The messages below were generated|The following is the Codex agent history/],
  ['attachment-manifest', /^#+\s*Files mentioned by the user:/m],
  ['pasted-artifact', /^PLEASE IMPLEMENT THIS PLAN|^!csl[0-9]?\b|^!legend\b/m],
];

const WORD = /[A-Za-z][A-Za-z'-]*/g;

// Share of the text sitting inside fenced code blocks. An unterminated fence runs to end-of-text,
// which is what a truncated paste looks like.
function fencedShare(text: string): number {
  if (text.length === 0) return 0;
  let inside = 0;
  for (const block of text.match(/```[\s\S]*?(?:```|$)/g) ?? []) inside += block.length;
  return inside / text.length;
}

export function classify(raw: string): Stratum {
  const text = typeof raw === 'string' ? raw : '';
  const trimmed = text.trim();
  if (trimmed === '') return 'unknown';

  for (const [stratum, marker] of MACHINE_MARKERS) if (marker.test(text)) return stratum;
  if (/^[{[]/.test(trimmed)) return 'structured-payload';
  if (fencedShare(text) > 0.6) return 'code-paste';

  if (!/\s/.test(trimmed)) {
    // One token. "yes" and "go" carry voice; a path, URL, hash or id does not.
    return /^[A-Za-z]{1,16}[.!?]*$/.test(trimmed) ? 'apocky-terse' : 'unknown';
  }

  const words = trimmed.match(WORD) ?? [];
  if (words.length === 0) return 'unknown';
  if (words.join('').length / trimmed.length < 0.35) return 'unknown'; // table, log, or data paste
  if (words.length < 3) return trimmed.length <= 24 ? 'apocky-terse' : 'unknown';
  return 'apocky';
}

export interface CorpusChunk {
  readonly id: number;
  readonly sessionId: string | null;
  readonly eventTs: string | null;
  readonly sourcePath: string | null;
  readonly sha256: string | null;
  readonly text: string;
  readonly stratum: Stratum;
}

export interface Census {
  readonly count: number;
  readonly bytes: number;
}

export interface CorpusRead {
  readonly admitted: CorpusChunk[];
  readonly census: Record<Stratum, Census>;
  readonly scanned: number;
  readonly watermark: string | null;
}

function emptyCensus(): Record<Stratum, Census> {
  // Every stratum present at zero. A stratum absent from the census is indistinguishable from one
  // that was never measured, which is the absence failure G10 exists to forbid.
  return Object.fromEntries(STRATA.map((s) => [s, { count: 0, bytes: 0 }])) as Record<Stratum, Census>;
}

export function censusOf(texts: Iterable<string>): Record<Stratum, Census> {
  const census = emptyCensus();
  for (const text of texts) {
    const stratum = classify(text);
    const prior = census[stratum];
    (census as Record<Stratum, Census>)[stratum] = { count: prior.count + 1, bytes: prior.bytes + text.length };
  }
  return census;
}

interface Row {
  id: number;
  session_id: string | null;
  event_ts: string | null;
  source_path: string | null;
  text_sha256: string | null;
  text: string | null;
}

/**
 * Read Apocky-authored chunks from the Anamnesis transcript index.
 *
 * Opened read-only. `since` takes an ISO event timestamp and reads only what arrived after it, so
 * an incremental pass does not re-scan 160 MB to find the day's new messages.
 */
export function readCorpus(
  databasePath: string,
  options: { readonly since?: string | null; readonly limit?: number } = {},
): CorpusRead {
  const database = new DatabaseSync(databasePath, { readOnly: true });
  try {
    const since = options.since ?? null;
    const statement = database.prepare(
      'SELECT id, session_id, event_ts, source_path, text_sha256, text FROM source_chunks '
      + `WHERE role = 'user' AND redacted = 0${since ? ' AND event_ts > ?' : ''} `
      + 'ORDER BY event_ts, id'
      + (options.limit ? ` LIMIT ${Number(options.limit) | 0}` : ''),
    );
    const rows = (since ? statement.all(since) : statement.all()) as unknown as Row[];

    const census = emptyCensus();
    const admitted: CorpusChunk[] = [];
    let watermark: string | null = since;

    for (const row of rows) {
      const text = row.text ?? '';
      const stratum = classify(text);
      const prior = census[stratum];
      (census as Record<Stratum, Census>)[stratum] = { count: prior.count + 1, bytes: prior.bytes + text.length };
      if (row.event_ts && (watermark === null || row.event_ts > watermark)) watermark = row.event_ts;
      if (!ADMISSIBLE.has(stratum)) continue;
      admitted.push({
        id: row.id,
        sessionId: row.session_id,
        eventTs: row.event_ts,
        sourcePath: row.source_path,
        sha256: row.text_sha256,
        text,
        stratum,
      });
    }

    return { admitted, census, scanned: rows.length, watermark };
  } finally {
    database.close();
  }
}
