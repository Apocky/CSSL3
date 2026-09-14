// Layer 0 — the diff the operator approves a write against.
//
// This one matters more than it looks. The consent card shows a diff, and the operator's decision
// is made on that diff. A diff that understates a change is a consent bug wearing a formatting
// bug's clothes. So the counts are checked against an algebraic invariant that holds for ANY
// correct LCS alignment, then fuzzed over several hundred random pairs rather than a few examples.

import { unifiedDiff } from '../../scripts/apocrypha-work/diff';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

// Deterministic PRNG: a fuzz case that cannot be reproduced is a bug report nobody can act on.
function rng(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state ^= state << 13; state >>>= 0;
    state ^= state >> 17;
    state ^= state << 5; state >>>= 0;
    return state / 0x1_0000_0000;
  };
}

function lines(random: () => number, count: number, alphabet: number): string[] {
  return Array.from({ length: count }, () => `line-${Math.floor(random() * alphabet)}`);
}

async function main(): Promise<void> {
  // Identity produces nothing at all — not an empty hunk, not a header.
  const same = unifiedDiff('a\nb\nc', 'a\nb\nc', 'f.ts');
  assert(same.patch === '' && same.added === 0 && same.removed === 0, 'identity produced a diff');

  const created = unifiedDiff('', 'x\ny', 'new.ts');
  assert(created.added === 2 && created.removed === 0, `creation counted +${created.added} -${created.removed}`);

  const deleted = unifiedDiff('x\ny', '', 'gone.ts');
  assert(deleted.removed === 2 && deleted.added === 0, `deletion counted +${deleted.added} -${deleted.removed}`);

  const edited = unifiedDiff('a\nb\nc', 'a\nB\nc', 'f.ts');
  assert(edited.added === 1 && edited.removed === 1, `one-line edit counted +${edited.added} -${edited.removed}`);
  assert(edited.patch.includes('-b') && edited.patch.includes('+B'), 'edit patch lost its changed lines');
  assert(edited.patch.includes('--- a/f.ts') && edited.patch.includes('+++ b/f.ts'), 'patch lost its header');

  // Fuzz. For any correct alignment: added − removed === after.length − before.length, and every
  // emitted +/− line must genuinely come from the side it claims.
  const random = rng(0xC0FFEE);
  let cases = 0;
  for (let i = 0; i < 400; i += 1) {
    const beforeLines = lines(random, Math.floor(random() * 40), 12);
    const afterLines = lines(random, Math.floor(random() * 40), 12);
    const before = beforeLines.join('\n');
    const after = afterLines.join('\n');
    const result = unifiedDiff(before, after, 'fuzz.ts');
    if (before === after) { assert(result.patch === '', 'identical fuzz pair produced a patch'); continue; }

    const expectedDelta = (after === '' ? 0 : afterLines.length) - (before === '' ? 0 : beforeLines.length);
    assert(
      result.added - result.removed === expectedDelta,
      `case ${i}: +${result.added} −${result.removed} implies ${result.added - result.removed}, expected ${expectedDelta}`,
    );

    const beforeSet = new Set(beforeLines);
    const afterSet = new Set(afterLines);
    for (const line of result.patch.split('\n')) {
      if (line.startsWith('+++') || line.startsWith('---') || line.startsWith('@@')) continue;
      if (line.startsWith('+')) assert(afterSet.has(line.slice(1)), `case ${i}: patch added a line absent from "after": ${line}`);
      if (line.startsWith('-')) assert(beforeSet.has(line.slice(1)), `case ${i}: patch removed a line absent from "before": ${line}`);
    }
    cases += 1;
  }
  assert(cases > 300, `fuzz coverage collapsed to ${cases} cases`);

  // Past the bound the diff must say so rather than quietly computing a 200k-line table.
  const huge = Array.from({ length: 3_000 }, (_, i) => `l${i}`).join('\n');
  const hugeChanged = Array.from({ length: 3_000 }, (_, i) => `L${i}`).join('\n');
  const bounded = unifiedDiff(huge, hugeChanged, 'big.ts');
  assert(bounded.patch.includes('whole-file replacement'), 'oversized diff did not fall back');
  assert(bounded.added === 3_000 && bounded.removed === 3_000, 'oversized diff reported wrong totals');

  // A file just under the bound must still produce a real diff, or the fallback is swallowing
  // ordinary source files.
  const nearly = Array.from({ length: 2_400 }, (_, i) => `l${i}`).join('\n');
  const nearlyChanged = nearly.replace('l1200', 'CHANGED');
  const real = unifiedDiff(nearly, nearlyChanged, 'near.ts');
  assert(!real.patch.includes('whole-file replacement'), 'a 2400-line file fell back unnecessarily');
  assert(real.added === 1 && real.removed === 1, `near-bound diff counted +${real.added} −${real.removed}`);

  console.log(`diff: ${cases} fuzz cases, bound at 2500 lines verified from both sides`);
}

main().then(() => console.log('work/diff OK')).catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
