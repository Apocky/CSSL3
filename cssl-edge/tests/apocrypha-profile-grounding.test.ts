// The grounding gate, pressed until it fails.
//
// This is the only thing standing between "Apocrypha learned something about Apocky" and
// "Apocrypha invented something about Apocky and filed it with a citation". A fabricated claim
// carrying a plausible chunk id is worse than no claim: downstream it is indistinguishable from
// a true one, it corroborates itself on re-reads, and it is exactly the kind of error that gets
// repeated back to him in his own voice.
//
// Every mutation below is a plausible relaxation someone might make to raise yield. Each must be
// caught here.

import {
  verifyClaim, screen, AXES, MIN_QUOTE_CHARS,
  type RawClaim, type SourceChunk, type Rejection,
} from '../scripts/apocrypha-profile/grounding';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

const SOURCE: readonly SourceChunk[] = [
  {
    id: 101,
    text: 'Stop directly invalidating the things I say with vague generalizations: the whole fix is\neverything I said; stop trying to fix the bare minimum.',
    sha256: 'aa', sessionId: 's1', eventTs: '2026-09-14T00:00:00Z',
  },
  {
    id: 102,
    text: 'Given the opportunity to expand or refactor always do so, no sunk cost fallacy, time is no concern.',
    sha256: 'bb', sessionId: 's1', eventTs: '2026-09-14T00:01:00Z',
  },
  {
    id: 103,
    text: 'Whenever you address or query me please use plain concise English.',
    sha256: 'cc', sessionId: 's2', eventTs: '2026-09-14T00:02:00Z',
  },
];

const BATCH: ReadonlyMap<number, SourceChunk> = new Map(SOURCE.map((chunk) => [chunk.id, chunk]));

const TRUE_CLAIM: RawClaim = {
  axis: 'correction',
  statement: 'Rejects scope narrowing: the whole fix is everything stated, not the minimum.',
  quote: 'the whole fix is everything I said; stop trying to fix the bare minimum',
  chunkId: 101,
};

// Fluent, plausible, entirely invented. Nothing in chunk 102 says this -- it says the opposite.
const HALLUCINATION: RawClaim = {
  axis: 'preference',
  statement: 'Prefers to defer refactors until after revenue milestones are met.',
  quote: 'defer refactors until after the revenue milestone is met',
  chunkId: 102,
};

const CASES: ReadonlyArray<readonly [string, RawClaim, Rejection | 'ok']> = [
  ['a true, quoted claim', TRUE_CLAIM, 'ok'],
  ['a wrapped quote (model re-flowed the newline)', {
    ...TRUE_CLAIM, quote: 'vague generalizations: the whole fix is everything I said',
  }, 'ok'],
  ['fabricated quote', HALLUCINATION, 'quote-not-in-source'],
  ['shouted quote (capitalisation altered)', {
    ...TRUE_CLAIM, quote: 'THE WHOLE FIX IS EVERYTHING I SAID',
  }, 'quote-not-in-source'],
  ['real quote, wrong chunk cited', { ...TRUE_CLAIM, chunkId: 103 }, 'quote-not-in-source'],
  ['chunk not in the batch at all', { ...TRUE_CLAIM, chunkId: 999 }, 'chunk-not-in-batch'],
  ['invented axis', { ...TRUE_CLAIM, axis: 'psychology' }, 'axis-unknown'],
  ['quote too short to prove anything', { ...TRUE_CLAIM, quote: 'the whole fix' }, 'quote-too-short'],
  ['long-but-few-words quote', {
    ...TRUE_CLAIM, quote: 'generalizations:                              everything',
  }, 'quote-too-few-words'],
  ['empty statement', { ...TRUE_CLAIM, statement: '   ' }, 'statement-empty'],
  ['real quote, unrelated claim', {
    axis: 'procedure',
    statement: 'Deploys exclusively through Vercel from the repository root.',
    quote: 'Given the opportunity to expand or refactor always do so, no sunk cost fallacy',
    chunkId: 102,
  }, 'statement-unrelated-to-quote'],
];

function runCases(label: string): void {
  for (const [name, claim, expected] of CASES) {
    const verdict = verifyClaim(claim, BATCH);
    const actual = verdict.ok ? 'ok' : verdict.reason;
    assert(actual === expected, `${label}: "${name}" gave ${actual}, expected ${expected}`);
  }
}

async function main(): Promise<void> {
  // 1 -- every case lands on its declared verdict.
  runCases('baseline');

  // 2 -- case is NOT folded. Capitalisation is itself signal, and a model that cannot reproduce
  //      it is paraphrasing rather than quoting.
  const shouted = verifyClaim({ ...TRUE_CLAIM, quote: 'THE WHOLE FIX IS EVERYTHING I SAID' }, BATCH);
  assert(!shouted.ok && shouted.reason === 'quote-not-in-source',
    'case folding let a paraphrase through');

  // 3 -- screen() tallies every rejection reason, zeros included (G10).
  const screened = screen(CASES.map(([, claim]) => claim), BATCH);
  assert(screened.grounded.length === 2, `expected 2 grounded, got ${screened.grounded.length}`);
  const tallied = Object.values(screened.rejected).reduce((sum, n) => sum + n, 0);
  assert(tallied === CASES.length - 2, `rejection tally ${tallied} does not account for every case`);
  assert('quote-too-short' in screened.rejected && screened.rejected['statement-too-long'] === 0,
    'a rejection reason is missing from the tally -- absent reads as never-checked');

  // 4 -- G5: each mutation is a plausible relaxation someone might make to raise yield.
  //      Each MUST be caught by the cases above.
  const mutants: ReadonlyArray<readonly [string, (claim: RawClaim) => Rejection | 'ok']> = [
    ['trust the model, skip quote verification', (claim) => {
      if (!(AXES as readonly string[]).includes(claim.axis)) return 'axis-unknown';
      if (!claim.statement.trim()) return 'statement-empty';
      return BATCH.has(claim.chunkId) ? 'ok' : 'chunk-not-in-batch';
    }],
    ['fold case when matching the quote', (claim) => {
      const verdict = verifyClaim(claim, BATCH);
      if (!verdict.ok && verdict.reason === 'quote-not-in-source') {
        const chunk = BATCH.get(claim.chunkId);
        const flat = (value: string) => value.toLowerCase().replace(/\s+/gu, ' ').trim();
        if (chunk && flat(chunk.text).includes(flat(claim.quote))) return 'ok';
      }
      return verdict.ok ? 'ok' : verdict.reason;
    }],
    ['drop the minimum-quote floor', (claim) => {
      const padded = claim.quote.length < MIN_QUOTE_CHARS
        ? `${claim.quote} is everything I said` : claim.quote;
      const verdict = verifyClaim({ ...claim, quote: padded }, BATCH);
      return verdict.ok ? 'ok' : verdict.reason;
    }],
    ['accept a real quote with an unrelated claim', (claim) => {
      const verdict = verifyClaim(claim, BATCH);
      if (!verdict.ok && verdict.reason === 'statement-unrelated-to-quote') return 'ok';
      return verdict.ok ? 'ok' : verdict.reason;
    }],
    ['treat a missing chunk as grounded', (claim) => {
      const verdict = verifyClaim(claim, BATCH);
      if (!verdict.ok && verdict.reason === 'chunk-not-in-batch') return 'ok';
      return verdict.ok ? 'ok' : verdict.reason;
    }],
  ];

  for (const [name, mutant] of mutants) {
    let caught = false;
    for (const [, claim, expected] of CASES) {
      if (mutant(claim) !== expected) { caught = true; break; }
    }
    assert(caught, `MUTATION SURVIVED: "${name}" -- these cases do not detect it`);
  }

  // 5 -- specifically: the fabrication must not be admitted.
  assert(!verifyClaim(HALLUCINATION, BATCH).ok, 'the fabricated claim was admitted');

  // 6 -- the real gate still passes after the mutants ran.
  runCases('post-mutation');

  console.log(`apocrypha-profile-grounding.test: ${CASES.length} cases, ${AXES.length} axes, `
    + `case-folding blocked, tally complete, ${mutants.length}/${mutants.length} mutations caught`);
}

main().then(() => console.log('apocrypha-profile-grounding OK')).catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
