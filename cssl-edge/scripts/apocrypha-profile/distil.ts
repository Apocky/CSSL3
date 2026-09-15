// One distillation pass: read what Apocky wrote, emit claims, keep only the ones that prove
// themselves against the source.
//
// Runs against the resident local engine (:19128). No cloud spend, and no second model loaded --
// the GPU holds one engine on one port serving both lanes, established 2026-09-14. This pass is
// therefore safe to schedule: it competes for slots, never for VRAM.
//
// Everything the model returns is treated as a PROPOSAL. grounding.screen() is the only thing that
// decides what is real, and it does so mechanically.

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

async function complete(prompt: string, signal: AbortSignal): Promise<string> {
  const response = await fetch(`${ENGINE}/v1/chat/completions`, {
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
  if (!response.ok) throw new Error(`engine HTTP ${response.status}`);
  const payload = await response.json() as { choices?: Array<{ message?: { content?: string } }> };
  return payload.choices?.[0]?.message?.content ?? '';
}

export interface PassResult {
  readonly passId: number;
  readonly chunksRead: number;
  readonly offered: number;
  readonly grounded: number;
  readonly fresh: number;
  readonly corroborated: number;
  readonly duplicate: number;
  readonly rejected: Record<Rejection, number>;
  readonly batchesFailed: number;
}

export async function runPass(options: {
  readonly since?: string | null;
  readonly limit?: number;
  readonly batches?: number;
  readonly timeoutMs?: number;
  readonly onProgress?: (line: string) => void;
} = {}): Promise<PassResult> {
  const say = options.onProgress ?? (() => {});
  const corpus = readCorpus(ANAMNESIS, { since: options.since ?? null });
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

  let offered = 0, groundedCount = 0, fresh = 0, corroborated = 0, duplicate = 0, batchesFailed = 0;
  const totalBatches = Math.ceil(admitted.length / BATCH);
  const cap = options.batches ?? totalBatches;

  for (let index = 0; index < Math.min(cap, totalBatches); index += 1) {
    const slice = admitted.slice(index * BATCH, (index + 1) * BATCH);
    const batch: ReadonlyMap<number, SourceChunk> = new Map(slice.map((chunk) => [chunk.id, chunk]));
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), options.timeoutMs ?? 180_000);
    let claims: RawClaim[] = [];
    try {
      claims = parseClaims(await complete(renderBatch(slice), controller.signal));
    } catch (error) {
      batchesFailed += 1;
      say(`batch ${index + 1}: FAILED ${(error as Error).message}`);
      continue;
    } finally {
      clearTimeout(timer);
    }

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
    if ((index + 1) % 10 === 0 || index + 1 === Math.min(cap, totalBatches)) {
      say(`batch ${index + 1}/${Math.min(cap, totalBatches)} | offered ${offered} | grounded ${groundedCount} `
        + `| new ${fresh} corrob ${corroborated} | failed ${batchesFailed}`);
    }
  }

  store.finishPass(passId, {
    chunksRead: admitted.length, claimsOffered: offered, claimsGrounded: groundedCount,
    claimsNew: fresh, claimsCorroborated: corroborated, rejected,
  });
  const drift = store.audit();
  if (drift.length) say(`WARNING: ${drift.length} claims violate corroborations == evidence`);
  store.close();

  return { passId, chunksRead: admitted.length, offered, grounded: groundedCount, fresh, corroborated, duplicate, rejected, batchesFailed };
}

if (process.argv[1] && import.meta.url.endsWith(process.argv[1].replace(/\\/gu, '/'))) {
  const arg = (name: string): string | undefined => {
    const hit = process.argv.find((value) => value.startsWith(`--${name}=`));
    return hit ? hit.slice(name.length + 3) : undefined;
  };
  runPass({
    limit: arg('limit') ? Number(arg('limit')) : undefined,
    batches: arg('batches') ? Number(arg('batches')) : undefined,
    since: arg('since') ?? null,
    onProgress: (line) => console.log(line),
  }).then((result) => {
    console.log(JSON.stringify(result, null, 2));
  }).catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
}
