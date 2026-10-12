// One distillation pass: read what Apocky wrote, emit claims, keep only the ones that prove
// themselves against the source.
//
// Runs against the resident local engine (:19128). No cloud spend, and no second model loaded --
// the GPU holds one engine on one port serving both lanes, established 2026-09-14. This pass is
// therefore safe to schedule: it competes for slots, never for VRAM.
//
// Everything the model returns is treated as a PROPOSAL. grounding.screen() is the only thing that
// decides what is real, and it does so mechanically.
//
// The engine can be up and useless at the same time. On 2026-10-11 and 2026-10-12 it had been
// evicted from VRAM: /health said ok, every completion came back HTTP 500 "peg-native", and this
// pass sent 25 of 25 batches into it (19 min 47 s, 04:15-04:35) and then exited 0. Three
// consecutive engine failures now stop the pass, and a pass that did no work exits 1.

import { readCorpus, type CorpusChunk } from './corpus';
import { screen, AXES, type RawClaim, type SourceChunk, type Rejection } from './grounding';
import { ProfileStore } from './store';

const ENGINE = process.env.APOCRYPHA_PROFILE_ENGINE ?? 'http://127.0.0.1:19128';
const ANAMNESIS = process.env.APOCRYPHA_ANAMNESIS_DB_PATH
  ?? 'C:/Users/Apocky/source/repos/anamnesis/anamnesis.db';
const PROFILE_DB = process.env.APOCRYPHA_PROFILE_DB_PATH
  ?? 'C:/Apocrypha/profile/apocrypha-profile.db';

// Chunks per request. The engine runs 16384 ctx per slot; 8 chunks capped at 1200 chars is roughly
// 3k tokens in, leaving the output budget clear.
const BATCH = 8;
const CHUNK_CHARS = 1_200;
const MAX_OUTPUT_TOKENS = 1_400;

// Consecutive ENGINE failures (never batch rejections) that end the pass. One 5xx can be a bad
// batch; three in a row with nothing good between them is the engine, and every further request
// is five minutes of GPU spent learning nothing.
export const MAX_CONSECUTIVE_ENGINE_FAILURES = 3;

// llama.cpp b9743 reports a corrupted resident model (garbage tokens that never parse) as HTTP 500
// with this phrase in the error body. It describes the engine, never the batch.
const ENGINE_FAULT_BODY = /peg-native/iu;

const SYSTEM = [
  'You extract durable facts about one person, Apocky, from messages he wrote.',
  '',
  'You will be given numbered messages. Return ONLY a JSON array. Each element:',
  '  {"axis": <axis>, "statement": <one sentence about Apocky>, "quote": <VERBATIM span copied',
  '   from the message>, "chunk_id": <the id of the message it came from>}',
  '',
  `Valid axis values: ${AXES.join(', ')}.`,
  '  correction  - he corrected a behaviour and said how it should be done',
  '  ideal       - a value or principle he holds',
  '  notation    - how he writes: his notation, shorthand, formatting',
  '  procedure   - a concrete operating rule for how work is done',
  '  preference  - something he wants or refuses',
  '  directive   - a standing instruction',
  '  mannerism   - how he speaks: register, phrasing, tone',
  '',
  'Rules:',
  '- The quote MUST be copied character-for-character from the message, including capitalisation.',
  '  Do not paraphrase, trim, tidy or re-case it. At least 24 characters and 4 words.',
  '- The statement must be about Apocky himself and supported by the quote you attach.',
  '- Extract only durable facts. Skip anything specific to one task that will not matter later.',
  '- If a message contains nothing durable, produce nothing for it. Fewer, solid claims are better.',
  '- Output the JSON array and nothing else. No prose, no code fence.',
].join('\n');

function renderBatch(chunks: readonly CorpusChunk[]): string {
  return chunks.map((chunk) => {
    const text = chunk.text.length > CHUNK_CHARS ? `${chunk.text.slice(0, CHUNK_CHARS)}...` : chunk.text;
    return `--- message id=${chunk.id} ---\n${text}`;
  }).join('\n\n');
}

/** Pull the first JSON array out of a completion, tolerating fences and stray prose. */
export function parseClaims(raw: string): RawClaim[] {
  const text = raw.replace(/^```(?:json)?\s*/u, '').replace(/```\s*$/u, '').trim();
  const start = text.indexOf('[');
  const end = text.lastIndexOf(']');
  if (start < 0 || end <= start) return [];
  let parsed: unknown;
  try {
    parsed = JSON.parse(text.slice(start, end + 1));
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const claims: RawClaim[] = [];
  for (const item of parsed) {
    if (item === null || typeof item !== 'object') continue;
    const record = item as Record<string, unknown>;
    const chunkId = Number(record.chunk_id ?? record.chunkId);
    if (!Number.isInteger(chunkId)) continue;
    claims.push({
      axis: String(record.axis ?? ''),
      statement: String(record.statement ?? ''),
      quote: String(record.quote ?? ''),
      chunkId,
    });
  }
  return claims;
}

/**
 * The engine itself failed, as opposed to the engine rejecting one request: a 5xx, a connection
 * that never opened, a read that timed out, or a body naming the peg-native parse fault.
 *
 * A 4xx is deliberately NOT one of these. A 400 for an oversized batch is about the batch, and
 * three of them in a row must not stop a pass the engine would happily continue.
 */
export class EngineFailure extends Error {
  readonly status: number | null;

  constructor(message: string, status: number | null) {
    super(message);
    this.name = 'EngineFailure';
    this.status = status;
  }
}

export type FetchLike = (url: string, init: RequestInit) => Promise<Response>;

interface EnginePayload {
  readonly choices?: Array<{ message?: { content?: string } }>;
  readonly error?: { message?: unknown } | string;
}

function errorMessageOf(payload: EnginePayload | null): string | null {
  if (payload === null || payload.error === undefined) return null;
  const message = typeof payload.error === 'string' ? payload.error : payload.error?.message;
  return typeof message === 'string' && message ? message : '(error without message)';
}

/**
 * One completion. Throws EngineFailure when the fault is the engine's, a plain Error when it is
 * the request's; the batch loop reads that distinction and nothing else.
 */
export async function complete(prompt: string, signal: AbortSignal, fetchImpl: FetchLike = fetch): Promise<string> {
  let response: Response;
  let text: string;
  try {
    response = await fetchImpl(`${ENGINE}/v1/chat/completions`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      signal,
      body: JSON.stringify({
        model: 'resident',
        messages: [{ role: 'system', content: SYSTEM }, { role: 'user', content: prompt }],
        temperature: 0.2,
        max_tokens: MAX_OUTPUT_TOKENS,
        // Qwen templates default to thinking mode; without this the whole output budget burns on
        // reasoning that is then discarded.
        chat_template_kwargs: { enable_thinking: false },
      }),
    });
    text = await response.text();
  } catch (error) {
    // Refused, reset, or aborted by the batch timer: nothing came back, so nothing about this
    // batch can be blamed.
    const cause = error as Error;
    throw new EngineFailure(`engine fetch failed: ${cause.name}: ${cause.message}`, null);
  }

  let payload: EnginePayload | null;
  try {
    payload = JSON.parse(text) as EnginePayload;
  } catch {
    payload = null;
  }
  const errorMessage = errorMessageOf(payload);
  if (!response.ok || errorMessage !== null) {
    const detail = errorMessage ?? text.replace(/\s+/gu, ' ').trim().slice(0, 200);
    const message = `engine HTTP ${response.status}${detail ? `: ${detail}` : ''}`;
    if (response.status >= 500 || ENGINE_FAULT_BODY.test(detail)) throw new EngineFailure(message, response.status);
    throw new Error(message);
  }
  if (payload === null) throw new Error(`engine HTTP ${response.status}: body is not JSON`);
  return payload.choices?.[0]?.message?.content ?? '';
}

export interface BatchRun {
  /** Batches a request was sent for; fewer than planned when the pass stopped early. */
  readonly batchesAttempted: number;
  readonly batchesFailed: number;
  /** The line that was logged when the pass stopped early for a dead engine; null otherwise. */
  readonly aborted: string | null;
}

/**
 * Drive the batches through the engine behind a dead-engine trip wire.
 *
 * `attempt` sends one batch and returns its claims; it may throw. `accept` runs for every batch
 * that came back, with the index and the running failure count for the progress line. Only an
 * EngineFailure arms the trip wire and any batch that comes back disarms it, so a flaky engine
 * still finishes the pass while a dead one costs three requests instead of the whole window.
 */
export async function distilBatches<T>(
  slices: ReadonlyArray<readonly T[]>,
  attempt: (slice: readonly T[], signal: AbortSignal) => Promise<RawClaim[]>,
  accept: (slice: readonly T[], claims: RawClaim[], at: { readonly index: number; readonly failed: number }) => void,
  options: {
    readonly timeoutMs: number;
    readonly say: (line: string) => void;
    readonly maxConsecutiveEngineFailures?: number;
  },
): Promise<BatchRun> {
  const limit = options.maxConsecutiveEngineFailures ?? MAX_CONSECUTIVE_ENGINE_FAILURES;
  let attempted = 0;
  let failed = 0;
  let consecutive = 0;
  for (const [index, slice] of slices.entries()) {
    attempted += 1;
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), options.timeoutMs);
    let claims: RawClaim[];
    try {
      claims = await attempt(slice, controller.signal);
    } catch (error) {
      failed += 1;
      const message = (error as Error).message;
      options.say(`batch ${index + 1}: FAILED ${message}`);
      if (!(error instanceof EngineFailure)) continue;
      consecutive += 1;
      if (consecutive < limit) continue;
      const line = `engine unhealthy: stopping pass after ${limit} consecutive failures (last: ${message})`;
      options.say(line);
      return { batchesAttempted: attempted, batchesFailed: failed, aborted: line };
    } finally {
      clearTimeout(timer);
    }
    consecutive = 0;
    accept(slice, claims, { index, failed });
  }
  return { batchesAttempted: attempted, batchesFailed: failed, aborted: null };
}

export interface PassResult extends BatchRun {
  readonly passId: number;
  readonly chunksRead: number;
  readonly offered: number;
  readonly grounded: number;
  readonly fresh: number;
  readonly corroborated: number;
  readonly duplicate: number;
  readonly rejected: Record<Rejection, number>;
}

/**
 * 1 when the pass did no useful work: the trip wire stopped it, or every batch it sent failed.
 * Partial failures still exit 0 -- the pass made progress and the watermark records exactly how
 * much. An empty corpus (nothing new since the watermark) is success, not failure.
 */
export function exitCodeFor(result: BatchRun): 0 | 1 {
  if (result.aborted !== null) return 1;
  if (result.batchesAttempted > 0 && result.batchesFailed === result.batchesAttempted) return 1;
  return 0;
}

export async function runPass(options: {
  readonly since?: string | null;
  readonly resume?: boolean;
  readonly limit?: number;
  readonly batches?: number;
  readonly timeoutMs?: number;
  readonly onProgress?: (line: string) => void;
} = {}): Promise<PassResult> {
  const say = options.onProgress ?? (() => {});
  // Resume from the last FINISHED pass unless told otherwise, so a scheduled run reads only what
  // arrived since. Passing `since: null` explicitly still forces a full re-read.
  let since = options.since ?? null;
  if (since === null && options.resume !== false) {
    const previous = new ProfileStore(PROFILE_DB);
    since = previous.lastWatermark();
    previous.close();
    if (since) say(`resuming from watermark ${since}`);
  }
  const corpus = readCorpus(ANAMNESIS, { since });
  let admitted = corpus.admitted;
  if (options.limit) admitted = admitted.slice(-options.limit);
  say(`corpus: ${admitted.length} admitted chunks (scanned ${corpus.scanned})`);

  const store = new ProfileStore(PROFILE_DB);
  const passId = store.beginPass('qwen3-coder-next-80b-a3b-q2kxl', corpus.watermark);

  const rejected = Object.fromEntries(
    ['axis-unknown', 'statement-empty', 'statement-too-long', 'quote-too-short',
      'quote-too-few-words', 'chunk-not-in-batch', 'quote-not-in-source',
      'statement-unrelated-to-quote'].map((reason) => [reason, 0]),
  ) as Record<Rejection, number>;

  let offered = 0, groundedCount = 0, fresh = 0, corroborated = 0, duplicate = 0;
  // Advances only over batches that were actually distilled.
  let distilledWatermark: string | null = since;
  const planned = Math.min(options.batches ?? Infinity, Math.ceil(admitted.length / BATCH));
  const slices = Array.from({ length: planned }, (_, index) => admitted.slice(index * BATCH, (index + 1) * BATCH));

  const run = await distilBatches(
    slices,
    (slice, signal) => complete(renderBatch(slice), signal).then(parseClaims),
    (slice, claims, at) => {
      const batch: ReadonlyMap<number, SourceChunk> = new Map(slice.map((chunk) => [chunk.id, chunk]));
      offered += claims.length;
      const screened = screen(claims, batch);
      groundedCount += screened.grounded.length;
      for (const reason of Object.keys(rejected) as Rejection[]) rejected[reason] += screened.rejected[reason];
      for (const item of screened.grounded) {
        const outcome = store.record(passId, item);
        if (outcome === 'new') fresh += 1;
        else if (outcome === 'corroborated') corroborated += 1;
        else duplicate += 1;
      }
      for (const chunk of slice) {
        if (chunk.eventTs && (distilledWatermark === null || chunk.eventTs > distilledWatermark)) {
          distilledWatermark = chunk.eventTs;
        }
      }
      if ((at.index + 1) % 10 === 0 || at.index + 1 === planned) {
        say(`batch ${at.index + 1}/${planned} | offered ${offered} | grounded ${groundedCount} `
          + `| new ${fresh} corrob ${corroborated} | failed ${at.failed}`);
      }
    },
    { timeoutMs: options.timeoutMs ?? 180_000, say },
  );

  // An aborted pass is still finished: its watermark covers only what was distilled, so the next
  // run picks up exactly where the engine stopped answering.
  store.finishPass(passId, {
    chunksRead: admitted.length, claimsOffered: offered, claimsGrounded: groundedCount,
    claimsNew: fresh, claimsCorroborated: corroborated, rejected,
    watermark: distilledWatermark,
  });
  const drift = store.audit();
  if (drift.length) say(`WARNING: ${drift.length} claims violate corroborations == evidence`);
  store.close();

  return {
    passId, chunksRead: admitted.length, offered, grounded: groundedCount, fresh, corroborated, duplicate, rejected,
    batchesAttempted: run.batchesAttempted, batchesFailed: run.batchesFailed, aborted: run.aborted,
  };
}

if (process.argv[1] && import.meta.url.endsWith(process.argv[1].replace(/\\/gu, '/'))) {
  const arg = (name: string): string | undefined => {
    const hit = process.argv.find((value) => value.startsWith(`--${name}=`));
    return hit ? hit.slice(name.length + 3) : undefined;
  };
  runPass({
    limit: arg('limit') ? Number(arg('limit')) : undefined,
    batches: arg('batches') ? Number(arg('batches')) : undefined,
    since: arg('since') ?? undefined,
    resume: process.argv.includes('--full') ? false : undefined,
    onProgress: (line) => console.log(line),
  }).then((result) => {
    console.log(JSON.stringify(result, null, 2));
    // The wrapper reads this. 0 after 25 failed batches is how 2026-10-12 went unnoticed.
    process.exitCode = exitCodeFor(result);
  }).catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
