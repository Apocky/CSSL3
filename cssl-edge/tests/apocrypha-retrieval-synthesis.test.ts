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
assert(synthesized[0]!.score > (synthesized.at(-1)?.score ?? 1),
  'records were not ordered by relevance');
assert(new Set(synthesized.map((item) => item.source)).size >= 3,
  'round-robin did not represent multiple faculties');
assert(synthesized.every((item, index) => item.rank === index + 1), 'ranks are not contiguous');

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

// Determinism: same input, same order.
assert.deepEqual(
  synthesizeRecords(results, query, 40).map((item) => `${item.source}:${item.provenanceId}:${item.score}`),
  synthesized.map((item) => `${item.source}:${item.provenanceId}:${item.score}`),
  'synthesis is not deterministic',
);

// Rendering: a tight budget must not let record #1 consume everything.
const bundle = { query, results, records: synthesized, digest: 'd'.repeat(64), probedAt: null } as RetrievalBundle;
const rendered = renderMemoryContext(bundle, 2_400);
assert(rendered.length <= 2_400, `render exceeded its budget (${rendered.length})`);
assert(rendered.startsWith('Synthesis: '), 'render lost the synthesis header');
assert(rendered.includes('NIL') && rendered.includes('CSSL'), 'render dropped the decisive record');
assert((rendered.match(/\n\[/gu) ?? []).length >= 1 && rendered.split('\n\n').length >= 3,
  'render collapsed to a single record');
assert(rendered.includes('relevance '), 'render lost per-record relevance');
assert(synthesisSummary(synthesized).includes('faculties'), 'summary lost its faculty roll-call');

// An empty bundle stays explicit rather than silently blank.
assert(renderMemoryContext({ ...bundle, records: [] }, 2_400).startsWith('No admitted memory records'),
  'empty bundle lost its explicit statement');

console.log('apocrypha-retrieval-synthesis.test: OK · relevance ranking, fair interleave, duplicate folding, budgeted render');
