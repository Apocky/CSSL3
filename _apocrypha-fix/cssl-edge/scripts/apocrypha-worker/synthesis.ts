// Retrieval synthesis: deterministic relevance ranking, genealogy-aware
// corroboration, novelty-weighted selection, and lane separation.
//
// Written against Apocky's canon, which constrains this file more than any
// generic RAG intuition would:
//
//   54_FIDELITY_QUERY_ENGINE  "fusion := deterministic lexical overlap +
//     content/excerpt dedup + stable ordering", "contradiction groups remain
//     plural; N! majority vote into truth", "failed dataset named; N! silent
//     partial answer", "read-only query confers no consent/effect authority".
//   44_ONE_BRAIN_COGNITIVE_RUNTIME  "fusion may rank ; N! erase
//     source/authority/evidence differences", "source-family dedup prevents
//     repetition multiplying evidence", "derived evidence grade <= minimum(parent
//     grades) ; authority N! increases through derivation", and the workspace
//     priority ladder ("priority = lexicographic law, not occult scalar").
//   57_RESIDENT_MEMORY  turn sequence: "targeted recall over resident indexes ->
//     ambient continuity bundle -> ... -> cognition". Targeted and ambient are
//     different lanes, not one ranked pile.
//   CRYSTALLIZATIONS XL-10  merged confidence scales by eff-N = N/(1+(N-1)rho),
//     not N. Faculties that carry the same underlying trace are not independent
//     witnesses, so a raw corroboration count overstates confidence.
//   MEMORY_LOBE_FALSIFIER  under a budget, surprise-driven selection beats naive
//     selection at every budget (+0.27..+0.40), and `naive_recency` was the worst
//     policy measured. Hence the novelty term below, and no recency term at all.
//   KENYON_SPARSE_VERDICT  REFUTED for this memory layer - no sparse top-k
//     expansion here; the codes are already near-orthogonal.
//
// Nothing here calls a model or touches the network: identical inputs always
// produce an identical ranking, so a turn stays replayable and auditable.
import type { RetrievalAdapterResult, RetrievalRecord } from './types';

/** One admitted record with its synthesis verdict attached. */
export interface SynthesizedRecord extends RetrievalRecord {
  /** Relevance to the cue in [0, 1]; 0 means no lexical support from the query. */
  readonly score: number;
  /** 1-based position within its lane. */
  readonly rank: number;
  /** Targeted recall answers the cue; ambient is continuity the cue did not select. */
  readonly lane: 'targeted' | 'ambient';
  /** provenance keys of near-identical records folded into this one. */
  readonly mergedFrom?: readonly string[];
  /**
   * Independent witness count after the XL-10 genealogy discount. Faculties in
   * one lineage family (a trace held in mempalace, re-recorded in anamnesis and
   * shipped through mneme) count once, not three times.
   */
  readonly independentWitnesses?: number;
}

/**
 * Lineage families, per PLAN_OF_RECORD T6-RESOLVED: "MemPalace = L1 verbatim +
 * semantic; anamnesis = L2 records + provenance; 3MNEME = transport tier;
 * graphify = code-structure memory", joined by provenance URIs. Agreement inside
 * a family is one trace seen from three angles; agreement across families is
 * closer to genuine corroboration.
 */
const LINEAGE: Readonly<Record<string, string>> = {
  mempalace: 'trace', anamnesis: 'trace', mneme: 'trace',
  graphify: 'topology', brainmonsoon: 'analysis', metaharness: 'observer',
};

/**
 * Faculties whose adapter ignores the cue and returns a fixed projection.
 * brainmonsoon's gateway adapter passes `brainmonsoonLineageSha256` where the
 * query belongs, so it answers the same thing every turn. That is continuity,
 * not recall, and it belongs in the ambient lane rather than competing for a
 * fair share of targeted slots it can never earn on relevance.
 */
const QUERY_INDEPENDENT = new Set(['brainmonsoon']);

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
/** Correlation assumed between two faculties of one lineage family (XL-10 rho). */
const WITHIN_FAMILY_RHO = 0.85;
/** Weight of marginal novelty against raw relevance when selecting under budget. */
const NOVELTY_WEIGHT = 0.25;

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
 * Relevance of one record to the cue, in [0, 1].
 *
 * Coverage (how much of the question the record speaks to) dominates raw term
 * frequency, so a short exact answer outranks a long document that merely
 * repeats one word; phrase hits and provenance-id hits break ties. Deterministic
 * lexical fusion is what 54_FIDELITY_QUERY_ENGINE specifies for this slice -
 * semantic and graph channels are explicitly gated behind a frozen retrieval
 * evaluation that has not been run.
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

interface Candidate {
  record: RetrievalRecord;
  score: number;
  shingles: Set<string>;
  mergedFrom: string[];
  families: Set<string>;
}

function candidate(record: RetrievalRecord, score: number): Candidate {
  return {
    record, score,
    shingles: shingles(record.text),
    mergedFrom: [],
    families: new Set([LINEAGE[record.source] ?? record.source]),
  };
}

/**
 * Fold near-identical records together, keeping the fuller rendering and both
 * provenances. 44_ONE_BRAIN: "source-family dedup prevents repetition
 * multiplying evidence" - the fold exists so that one trace echoed by three
 * faculties does not read to the model as three findings.
 */
function fold(candidates: Candidate[]): Candidate[] {
  const kept: Candidate[] = [];
  for (const item of candidates) {
    const duplicate = kept.find((existing) => jaccard(existing.shingles, item.shingles) >= DUPLICATE_JACCARD);
    if (!duplicate) { kept.push(item); continue; }
    const echo = `${item.record.source}:${item.record.provenanceId}`;
    const merged = [...duplicate.mergedFrom, echo];
    const families = new Set([...duplicate.families, ...item.families]);
    if (item.record.text.length > duplicate.record.text.length) {
      // The fuller text wins, but the surviving record can never outrank the
      // best of its parents (derived grade <= min(parent grades)).
      duplicate.record = item.record;
      duplicate.shingles = item.shingles;
    }
    duplicate.score = Math.min(duplicate.score, item.score);
    duplicate.mergedFrom = merged;
    duplicate.families = families;
  }
  return kept;
}

/**
 * eff-N per CRYSTALLIZATIONS XL-10: N/(1+(N-1)rho). Witnesses inside one lineage
 * family are strongly correlated, so three views of one trace count as roughly
 * one independent witness rather than three.
 */
export function independentWitnesses(families: ReadonlySet<string>, total: number): number {
  if (total <= 1) return total;
  const across = families.size;
  const within = total / Math.max(1, across);
  const effWithin = within / (1 + (within - 1) * WITHIN_FAMILY_RHO);
  return Math.round(across * effWithin * 10) / 10;
}

/**
 * Marginal value of a record given what is already selected: relevance
 * discounted by redundancy. MEMORY_LOBE_FALSIFIER measured surprise-driven
 * selection beating naive selection at every budget, and measured plain recency
 * as the worst policy of all - so novelty is weighted here and recency is absent.
 */
function marginalValue(item: Candidate, selected: readonly Candidate[]): number {
  if (selected.length === 0) return item.score;
  let redundancy = 0;
  for (const chosen of selected) redundancy = Math.max(redundancy, jaccard(chosen.shingles, item.shingles));
  return item.score * (1 - NOVELTY_WEIGHT * redundancy);
}

/**
 * Merge every faculty's records into ranked lanes.
 *
 * Targeted lane: faculties that answer the cue, selected round-robin by
 * descending marginal value so no faculty is silenced by its position in the
 * manifest, then filled on merit.
 * Ambient lane: faculties that return a fixed projection regardless of the cue.
 * 57_RESIDENT_MEMORY orders a turn as targeted recall, then ambient continuity;
 * mixing them lets a constant blob hold a slot that a real answer needed.
 */
export function synthesizeRecords(
  results: readonly RetrievalAdapterResult[],
  query: string,
  maxRecords = 40,
  ambientBudget = 3,
): SynthesizedRecord[] {
  const parsed = queryTerms(query);
  const laneOf = (name: string): 'targeted' | 'ambient' => (QUERY_INDEPENDENT.has(name) ? 'ambient' : 'targeted');

  const queues = results
    .filter((result) => result.records.length > 0)
    .map((result) => ({
      name: result.name,
      lane: laneOf(result.name),
      queue: fold(result.records.map((record) => candidate(record, scoreRecord(record, parsed))))
        .sort((left, right) => right.score - left.score
          || left.record.provenanceId.localeCompare(right.record.provenanceId)),
    }))
    .filter((adapter) => adapter.queue.length > 0);

  const pick = (lane: 'targeted' | 'ambient', budget: number): Candidate[] => {
    const adapters = queues.filter((adapter) => adapter.lane === lane).map((adapter) => ({ ...adapter, queue: [...adapter.queue] }));
    const selected: Candidate[] = [];
    // Pass 1: fair round-robin, best marginal value first within each faculty.
    let progressed = true;
    while (progressed && selected.length < budget) {
      progressed = false;
      for (const adapter of adapters) {
        if (selected.length >= budget) break;
        if (adapter.queue.length === 0) continue;
        adapter.queue.sort((left, right) => marginalValue(right, selected) - marginalValue(left, selected)
          || left.record.provenanceId.localeCompare(right.record.provenanceId));
        selected.push(adapter.queue.shift() as Candidate);
        progressed = true;
      }
    }
    // Pass 2: remaining slots strictly on marginal value.
    const rest = adapters.flatMap((adapter) => adapter.queue);
    while (selected.length < budget && rest.length > 0) {
      rest.sort((left, right) => marginalValue(right, selected) - marginalValue(left, selected)
        || left.record.provenanceId.localeCompare(right.record.provenanceId));
      selected.push(rest.shift() as Candidate);
    }
    return selected;
  };

  const targeted = fold(pick('targeted', Math.max(0, maxRecords - ambientBudget)));
  const ambient = fold(pick('ambient', ambientBudget));

  const emit = (items: Candidate[], lane: 'targeted' | 'ambient'): SynthesizedRecord[] => items
    .sort((left, right) => right.score - left.score
      || left.record.source.localeCompare(right.record.source)
      || left.record.provenanceId.localeCompare(right.record.provenanceId))
    .map((item, index): SynthesizedRecord => ({
      ...item.record,
      score: Number(item.score.toFixed(4)),
      rank: index + 1,
      lane,
      ...(item.mergedFrom.length ? {
        mergedFrom: item.mergedFrom,
        independentWitnesses: independentWitnesses(item.families, item.mergedFrom.length + 1),
      } : {}),
    }));

  return [...emit(targeted, 'targeted'), ...emit(ambient, 'ambient')];
}

/**
 * One line naming which faculties contributed and how far the corroboration
 * actually goes. 54_FIDELITY_QUERY_ENGINE requires a failed dataset to be named
 * rather than silently folded into a partial answer, so degraded faculties are
 * reported here alongside the ones that answered.
 */
export function synthesisSummary(
  records: readonly SynthesizedRecord[],
  results: readonly RetrievalAdapterResult[] = [],
): string {
  const degraded = results
    .filter((result) => result.state !== 'ok')
    .map((result) => `${result.name}:${result.state}`)
    .join(', ');
  const suffix = degraded ? ` degraded: ${degraded}` : '';
  if (records.length === 0) return `no records.${suffix}`;
  const counts = new Map<string, number>();
  for (const record of records) counts.set(record.source, (counts.get(record.source) ?? 0) + 1);
  const contributions = [...counts.entries()]
    .sort((left, right) => right[1] - left[1] || left[0].localeCompare(right[0]))
    .map(([name, count]) => `${name}x${count}`)
    .join(', ');
  const ambient = records.filter((record) => record.lane === 'ambient').length;
  const best = records[0]?.score ?? 0;
  return `${records.length - ambient} targeted + ${ambient} ambient from ${counts.size} faculties `
    + `(${contributions}); top relevance ${best.toFixed(2)}.${suffix}`;
}
