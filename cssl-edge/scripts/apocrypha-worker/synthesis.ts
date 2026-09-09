// Retrieval synthesis: deterministic relevance ranking, cross-faculty
// de-duplication, and fair interleaving of the six admitted memory faculties.
//
// Before synthesis the bundle was `results.flatMap(r => r.records)` — adapter
// manifest order was priority order, so the first faculty consumed the whole
// prompt budget and a decisive record from a later faculty was never rendered.
// Nothing here calls a model or reaches the network: the same inputs always
// produce the same ranking, so a turn stays replayable and auditable.
import type { RetrievalAdapterResult, RetrievalRecord } from './types';

/** One admitted record with its synthesis verdict attached. */
export interface SynthesizedRecord extends RetrievalRecord {
  /** Relevance in [0, 1]; 0 means "no lexical support from the query". */
  readonly score: number;
  /** 1-based position in the synthesized order. */
  readonly rank: number;
  /** Provenance ids of near-duplicate records folded into this one. */
  readonly mergedFrom?: readonly string[];
}

const STOPWORDS = new Set([
  'a', 'about', 'all', 'also', 'am', 'an', 'and', 'any', 'are', 'as', 'at', 'be', 'been', 'but', 'by',
  'can', 'did', 'do', 'does', 'for', 'from', 'get', 'had', 'has', 'have', 'how', 'i', 'if', 'in', 'is',
  'it', 'its', 'just', 'me', 'my', 'no', 'not', 'of', 'on', 'or', 'our', 'out', 'so', 'some', 'that',
  'the', 'their', 'them', 'then', 'there', 'these', 'they', 'this', 'to', 'up', 'use', 'was', 'we',
  'were', 'what', 'when', 'which', 'who', 'why', 'will', 'with', 'you', 'your',
]);

const MAX_QUERY_TERMS = 48;
const DUPLICATE_JACCARD = 0.82;
const SHINGLE_SIZE = 4;

function terms(value: string): string[] {
  return value
    .toLowerCase()
    .split(/[^\p{L}\p{N}_]+/u)
    .filter((term) => term.length > 1 && !STOPWORDS.has(term));
}

/** Query terms and adjacent-pair phrases, order-preserving and de-duplicated. */
export function queryTerms(query: string): { unigrams: string[]; bigrams: string[] } {
  const all = terms(query);
  const unigrams = [...new Set(all)].slice(0, MAX_QUERY_TERMS);
  const bigrams = [...new Set(all.slice(0, -1).map((term, index) => `${term} ${all[index + 1]}`))].slice(0, MAX_QUERY_TERMS);
  return { unigrams, bigrams };
}

function shingles(value: string): Set<string> {
  const words = terms(value);
  if (words.length <= SHINGLE_SIZE) return new Set(words.length ? [words.join(' ')] : []);
  const result = new Set<string>();
  for (let index = 0; index + SHINGLE_SIZE <= words.length; index += 1) {
    result.add(words.slice(index, index + SHINGLE_SIZE).join(' '));
  }
  return result;
}

function jaccard(left: Set<string>, right: Set<string>): number {
  if (left.size === 0 || right.size === 0) return 0;
  let shared = 0;
  for (const item of left) if (right.has(item)) shared += 1;
  return shared / (left.size + right.size - shared);
}

/**
 * Relevance of one record to the query, in [0, 1].
 *
 * Coverage (how much of the question the record speaks to) dominates raw term
 * frequency, so a short exact answer outranks a long document that merely
 * repeats one word; phrase hits and provenance-id hits break ties.
 */
export function scoreRecord(record: RetrievalRecord, query: { unigrams: string[]; bigrams: string[] }): number {
  if (query.unigrams.length === 0) return 0;
  const haystack = `${record.text}\n${record.provenanceId}`.toLowerCase();
  const bodyTerms = terms(record.text);
  const counts = new Map<string, number>();
  for (const term of bodyTerms) counts.set(term, (counts.get(term) ?? 0) + 1);

  let covered = 0;
  let saturated = 0;
  for (const term of query.unigrams) {
    const hits = counts.get(term) ?? 0;
    if (hits > 0) {
      covered += 1;
      // Saturating frequency: the 1st hit is worth much more than the 10th.
      saturated += hits / (hits + 1.5);
    }
  }
  const coverage = covered / query.unigrams.length;
  const frequency = query.unigrams.length ? saturated / query.unigrams.length : 0;
  const phrases = query.bigrams.length
    ? query.bigrams.filter((phrase) => haystack.includes(phrase)).length / query.bigrams.length
    : 0;
  const idHits = query.unigrams.filter((term) => record.provenanceId.toLowerCase().includes(term)).length;
  const identifier = query.unigrams.length ? Math.min(1, idHits / query.unigrams.length) : 0;
  // A very long record dilutes its own signal; a very short one is rarely a
  // complete answer. Both effects are mild and never zero the score.
  const length = bodyTerms.length;
  const brevity = length === 0 ? 0 : Math.min(1, Math.max(0.55, 220 / Math.max(220, length)));

  const raw = 0.50 * coverage + 0.18 * frequency + 0.22 * phrases + 0.10 * identifier;
  return Math.min(1, raw * brevity);
}

function fold(records: readonly RetrievalRecord[]): Array<RetrievalRecord & { mergedFrom?: string[] }> {
  const kept: Array<{ record: RetrievalRecord & { mergedFrom?: string[] }; shingles: Set<string> }> = [];
  for (const record of records) {
    const fingerprint = shingles(record.text);
    const duplicate = kept.find((item) => jaccard(item.shingles, fingerprint) >= DUPLICATE_JACCARD);
    if (duplicate) {
      // Two faculties holding the same fact is corroboration, not two facts.
      // Keep the longer rendering and record where the echo came from.
      const merged = [...(duplicate.record.mergedFrom ?? []), `${record.source}:${record.provenanceId}`];
      if (record.text.length > duplicate.record.text.length) {
        duplicate.record = { ...record, mergedFrom: merged };
        duplicate.shingles = fingerprint;
      } else {
        duplicate.record = { ...duplicate.record, mergedFrom: merged };
      }
      continue;
    }
    kept.push({ record, shingles: fingerprint });
  }
  return kept.map((item) => item.record);
}

/**
 * Merge every faculty's records into one ranked, de-duplicated reading order.
 *
 * Selection is round-robin across faculties by descending score: every faculty
 * that returned anything is represented before any faculty gets a second slot,
 * then remaining slots go to the best records outright. A single faculty can
 * still dominate on merit, but it can no longer do so merely by being first in
 * the manifest.
 */
export function synthesizeRecords(
  results: readonly RetrievalAdapterResult[],
  query: string,
  maxRecords = 40,
): SynthesizedRecord[] {
  const parsed = queryTerms(query);
  const byAdapter = results
    .filter((result) => result.records.length > 0)
    .map((result) => ({
      name: result.name,
      queue: fold(result.records)
        .map((record) => ({ record, score: scoreRecord(record, parsed) }))
        .sort((left, right) => right.score - left.score),
    }))
    .filter((adapter) => adapter.queue.length > 0);

  const selected: Array<{ record: RetrievalRecord & { mergedFrom?: string[] }; score: number }> = [];
  const seen = new Set<string>();
  const take = (entry: { record: RetrievalRecord & { mergedFrom?: string[] }; score: number }): boolean => {
    const key = `${entry.record.source}:${entry.record.provenanceId}`;
    if (seen.has(key)) return false;
    seen.add(key);
    selected.push(entry);
    return true;
  };

  // Pass 1: fair round-robin so no faculty is silenced by manifest order.
  let progressed = true;
  while (progressed && selected.length < maxRecords) {
    progressed = false;
    for (const adapter of byAdapter) {
      if (selected.length >= maxRecords) break;
      const next = adapter.queue.shift();
      if (!next) continue;
      progressed = true;
      take(next);
    }
  }
  // Pass 2: anything left goes strictly on merit.
  const remainder = byAdapter
    .flatMap((adapter) => adapter.queue)
    .sort((left, right) => right.score - left.score);
  for (const entry of remainder) {
    if (selected.length >= maxRecords) break;
    take(entry);
  }

  // Cross-faculty de-duplication after selection, then final ranked order.
  const deduped = fold(selected.map((entry) => entry.record));
  const scores = new Map(selected.map((entry) => [`${entry.record.source}:${entry.record.provenanceId}`, entry.score]));
  return deduped
    .map((record) => ({ record, score: scores.get(`${record.source}:${record.provenanceId}`) ?? scoreRecord(record, parsed) }))
    .sort((left, right) => right.score - left.score
      || left.record.source.localeCompare(right.record.source)
      || left.record.provenanceId.localeCompare(right.record.provenanceId))
    .map((entry, index): SynthesizedRecord => ({
      ...entry.record,
      score: Number(entry.score.toFixed(4)),
      rank: index + 1,
    }));
}

/** One line naming which faculties actually contributed, for the prompt header. */
export function synthesisSummary(records: readonly SynthesizedRecord[]): string {
  if (records.length === 0) return 'no records';
  const counts = new Map<string, number>();
  for (const record of records) counts.set(record.source, (counts.get(record.source) ?? 0) + 1);
  const contributions = [...counts.entries()]
    .sort((left, right) => right[1] - left[1] || left[0].localeCompare(right[0]))
    .map(([name, count]) => `${name}x${count}`)
    .join(', ');
  const best = records[0]?.score ?? 0;
  return `${records.length} records from ${counts.size} faculties (${contributions}); top relevance ${best.toFixed(2)}`;
}
