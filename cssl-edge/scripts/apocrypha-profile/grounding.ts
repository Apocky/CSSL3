// Every claim about Apocky must prove it came from something Apocky actually wrote.
//
// A distiller is a language model reading a language model's context window. Asked to summarise
// what someone believes, it will produce fluent, plausible, well-formed claims whether or not the
// evidence supports them -- and a hallucinated claim carrying a real-looking chunk id is strictly
// worse than no claim, because it is indistinguishable from a true one downstream.
//
// So citation here is mechanical, not a matter of trust (G6: name != citation). The model must
// emit a VERBATIM span alongside each claim, and that span must be findable in the cited chunk.
// It cannot paraphrase its way past this: either the bytes are in the source or they are not.

export const AXES = [
  'correction', 'ideal', 'notation', 'procedure', 'preference', 'directive', 'mannerism',
] as const;
export type Axis = (typeof AXES)[number];

// A short quote matches everything and proves nothing. "the" is in every message.
export const MIN_QUOTE_CHARS = 24;
export const MIN_QUOTE_WORDS = 4;
export const MAX_STATEMENT_CHARS = 400;

export interface RawClaim {
  readonly axis: string;
  readonly statement: string;
  readonly quote: string;
  readonly chunkId: number;
}

export interface SourceChunk {
  readonly id: number;
  readonly text: string;
  readonly sha256: string | null;
  readonly sessionId: string | null;
  readonly eventTs: string | null;
}

export type Rejection =
  | 'axis-unknown'
  | 'statement-empty'
  | 'statement-too-long'
  | 'quote-too-short'
  | 'quote-too-few-words'
  | 'chunk-not-in-batch'
  | 'quote-not-in-source'
  | 'statement-unrelated-to-quote';

export interface Grounded {
  readonly claim: RawClaim;
  readonly chunk: SourceChunk;
}

export type Verdict =
  | { readonly ok: true; readonly grounded: Grounded }
  | { readonly ok: false; readonly reason: Rejection };

// Models re-wrap text they copy. Whitespace is normalised; case is NOT -- a model that cannot
// reproduce capitalisation is paraphrasing, and Apocky's capitalisation is itself signal.
function normalize(value: string): string {
  return value.replace(/\s+/gu, ' ').trim();
}

const STOPWORDS = new Set([
  'this', 'that', 'with', 'from', 'have', 'they', 'them', 'then', 'than', 'what', 'when',
  'your', 'yours', 'about', 'would', 'could', 'should', 'there', 'their', 'which', 'been',
  'will', 'just', 'like', 'into', 'over', 'only', 'some', 'more', 'must', 'does',
]);

function contentWords(value: string): Set<string> {
  const words = normalize(value).toLowerCase().match(/[a-z][a-z'-]{3,}/gu) ?? [];
  return new Set(words.filter((word) => !STOPWORDS.has(word)));
}

export function verifyClaim(claim: RawClaim, batch: ReadonlyMap<number, SourceChunk>): Verdict {
  if (!(AXES as readonly string[]).includes(claim.axis)) return { ok: false, reason: 'axis-unknown' };

  const statement = normalize(claim.statement ?? '');
  if (statement === '') return { ok: false, reason: 'statement-empty' };
  if (statement.length > MAX_STATEMENT_CHARS) return { ok: false, reason: 'statement-too-long' };

  const quote = normalize(claim.quote ?? '');
  if (quote.length < MIN_QUOTE_CHARS) return { ok: false, reason: 'quote-too-short' };
  if ((quote.match(/\S+/gu) ?? []).length < MIN_QUOTE_WORDS) {
    return { ok: false, reason: 'quote-too-few-words' };
  }

  const chunk = batch.get(claim.chunkId);
  if (chunk === undefined) return { ok: false, reason: 'chunk-not-in-batch' };

  // The load-bearing check. Nothing above this line can be faked by a fluent model; nothing
  // below it can be reached without the bytes being present in the source.
  if (!normalize(chunk.text).includes(quote)) return { ok: false, reason: 'quote-not-in-source' };

  // A real quote attached to an unrelated claim is still a fabrication. Require the statement to
  // share at least one content word with the span it cites.
  const quoteWords = contentWords(quote);
  const shared = [...contentWords(statement)].some((word) => quoteWords.has(word));
  if (!shared) return { ok: false, reason: 'statement-unrelated-to-quote' };

  return { ok: true, grounded: { claim: { ...claim, statement, quote }, chunk } };
}

export interface Screened {
  readonly grounded: Grounded[];
  readonly rejected: Record<Rejection, number>;
}

const REJECTIONS: readonly Rejection[] = [
  'axis-unknown', 'statement-empty', 'statement-too-long', 'quote-too-short',
  'quote-too-few-words', 'chunk-not-in-batch', 'quote-not-in-source',
  'statement-unrelated-to-quote',
];

export function screen(claims: readonly RawClaim[], batch: ReadonlyMap<number, SourceChunk>): Screened {
  // Every rejection reason present at zero: a reason missing from the tally is indistinguishable
  // from one that was never checked, and the rejection profile is how a drifting distiller is
  // noticed at all. G10.
  const rejected = Object.fromEntries(REJECTIONS.map((r) => [r, 0])) as Record<Rejection, number>;
  const grounded: Grounded[] = [];
  for (const claim of claims) {
    const verdict = verifyClaim(claim, batch);
    if (verdict.ok) grounded.push(verdict.grounded);
    else rejected[verdict.reason] += 1;
  }
  return { grounded, rejected };
}
