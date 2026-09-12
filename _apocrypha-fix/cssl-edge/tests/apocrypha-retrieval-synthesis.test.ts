import assert from 'node:assert/strict';

import { renderMemoryContext } from '../scripts/apocrypha-worker/retrieval';
import { queryTerms, scoreRecord, synthesizeRecords, synthesisSummary } from '../scripts/apocrypha-worker/synthesis';
import type { RetrievalAdapterResult, RetrievalBundle } from '../scripts/apocrypha-worker/types';

const query = 'name the three languages of the Trinity language stack';

const record = (source: string, provenanceId: string, text: string) => ({ source, provenanceId, text, metadata: {} });
const ok = (name: string, records: ReturnType<typeof record>[]): RetrievalAdapterResult =>
  ({ name, state: 'ok', durationMs: 5, records });

// mempalace is first in the manifest and returns bulk with no answer;
// graphify is last and holds the decisive record.
const noise = (label: string, index: number) =>
  record('mempalace', `drawer_${label}_${index}`, `${'unrelated drawer content about deployment scheduling and invoices. '.repeat(40)}`);

const results: RetrievalAdapterResult[] = [
  ok('mempalace', [noise('problems', 1), noise('problems', 2), noise('planning', 3)]),
  ok('brainmonsoon', [record('brainmonsoon', 'claim-1', 'projection lens chronicle over unrelated corpus. '.repeat(30))]),
  ok('anamnesis', [record('anamnesis', 'ckpt:2026-09-07', 'decision: the Trinity language stack is NIL, CSL and CSSL.')]),
  ok('graphify', [record('graphify', 'node:Trinity', 'Trinity languages: NIL (notation owner), CSL (notation), CSSL/Sigil (compiler).')]),
  { name: 'mneme', state: 'ok', durationMs: 3, records: [] },
  { name: 'metaharness', state: 'error', durationMs: 12, records: [], detail: 'NATIVE_OBSERVER_UNAVAILABLE' },
];

const synthesized = synthesizeRecords(results, query, 40);

assert(synthesized.length > 0, 'synthesis produced no records');
assert(['graphify', 'anamnesis'].includes(synthesized[0]!.source),
  `manifest order still decided the top record (got ${synthesized[0]!.source})`);
const targetedOnly = synthesized.filter((item) => item.lane === 'targeted');
assert(targetedOnly[0]!.score >= (targetedOnly.at(-1)?.score ?? 1),
  'targeted records were not ordered by relevance');
// brainmonsoon ignores the cue (its adapter sends a fixed lineage hash), so it
// must not hold a targeted slot on fairness alone.
assert(synthesized.filter((item) => item.source === 'brainmonsoon')
  .every((item) => item.lane === 'ambient'), 'a query-independent faculty took a targeted slot');
assert(new Set(synthesized.map((item) => item.source)).size >= 3,
  'round-robin did not represent multiple faculties');
assert(targetedOnly.every((item, index) => item.rank === index + 1), 'targeted ranks are not contiguous');

// A record with no lexical support must not outrank one that answers the query.
const answer = record('graphify', 'node:Trinity', 'Trinity languages: NIL, CSL and CSSL.');
const irrelevant = record('mempalace', 'drawer_x', 'invoice reconciliation for the March billing cycle.');
const parsed = queryTerms(query);
assert(scoreRecord(answer, parsed) > scoreRecord(irrelevant, parsed), 'scoring is not relevance-ordered');
assert(scoreRecord(irrelevant, parsed) >= 0 && scoreRecord(answer, parsed) <= 1, 'score left the [0,1] range');

// Cross-faculty duplicates fold into one corroborated record.
const duplicated = synthesizeRecords([
  ok('anamnesis', [record('anamnesis', 'a1', 'The Trinity language stack is NIL, CSL and CSSL, with Sigil as the compiler surface.')]),
  ok('graphify', [record('graphify', 'g1', 'The Trinity language stack is NIL, CSL and CSSL, with Sigil as the compiler surface.')]),
], query, 40);
assert(duplicated.length === 1, `cross-faculty duplicate was not folded (${duplicated.length} records)`);
assert((duplicated[0]!.mergedFrom ?? []).length === 1, 'corroboration provenance was lost');
// XL-10: anamnesis and graphify are different lineage families, so two of them
// are close to two witnesses...
assert((duplicated[0]!.independentWitnesses ?? 0) > 1.5,
  'cross-family corroboration was discounted as if it were one trace');
// ...while mempalace and anamnesis both carry the same trace, so they are not.
const sameFamily = synthesizeRecords([
  ok('mempalace', [record('mempalace', 'm1', 'The Trinity language stack is NIL, CSL and CSSL, with Sigil as the compiler surface.')]),
  ok('anamnesis', [record('anamnesis', 'a1', 'The Trinity language stack is NIL, CSL and CSSL, with Sigil as the compiler surface.')]),
], query, 40);
assert(sameFamily.length === 1, 'same-family duplicate was not folded');
assert((sameFamily[0]!.independentWitnesses ?? 9) < (duplicated[0]!.independentWitnesses ?? 0),
  'genealogical agreement was counted as independent corroboration (XL-10)');

// Determinism: same input, same order.
assert.deepEqual(
  synthesizeRecords(results, query, 40).map((item) => `${item.source}:${item.provenanceId}:${item.score}`),
  synthesized.map((item) => `${item.source}:${item.provenanceId}:${item.score}`),
  'synthesis is not deterministic',
);

// Rendering: a tight budget must not let record #1 consume everything.
const bundle = { query, results, records: synthesized, digest: 'd'.repeat(64), probedAt: null } as RetrievalBundle;
const rendered = renderMemoryContext(bundle, 2_400, results);
assert(rendered.length <= 2_400, `render exceeded its budget (${rendered.length})`);
assert(rendered.startsWith('Synthesis: '), 'render lost the synthesis header');
assert(rendered.includes('NIL') && rendered.includes('CSSL'), 'render dropped the decisive record');
assert((rendered.match(/\n\[/gu) ?? []).length >= 1 && rendered.split('\n\n').length >= 3,
  'render collapsed to a single record');
assert(rendered.includes('relevance '), 'render lost per-record relevance');
// 54_FIDELITY_QUERY_ENGINE: "failed dataset named; N! silent partial answer".
const summary = synthesisSummary(synthesized, results);
assert(summary.includes('faculties'), 'summary lost its faculty roll-call');
assert(summary.includes('metaharness:error'), 'summary hid a degraded faculty');
assert(summary.includes('ambient'), 'summary lost the lane split');

// An empty bundle stays explicit rather than silently blank.
assert(renderMemoryContext({ ...bundle, records: [] }, 2_400).startsWith('No admitted memory records'),
  'empty bundle lost its explicit statement');

console.log('apocrypha-retrieval-synthesis.test: OK · relevance ranking, fair interleave, duplicate folding, budgeted render');
