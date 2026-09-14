interface DiffStat {
  readonly added: number;
  readonly removed: number;
  readonly patch: string;
}

/**
 * Line diff via a longest-common-subsequence table.
 *
 * Bounded at BOUND lines per side: the table is O(n·m) and an agent editing a generated file
 * could otherwise hand us two 200k-line inputs and stall the event loop. Past the bound we fall
 * back to a whole-file replacement summary, which is honest rather than wrong.
 */
const BOUND = 2_500;

type Tag = ' ' | '-' | '+';
interface Op { readonly tag: Tag; readonly text: string }

// Flat Int32Array rather than number[][]: one allocation instead of n row arrays. Reads go
// through `at`, whose `?? 0` doubles as the out-of-range base case of the recurrence.
interface Lcs { at: (row: number, col: number) => number; width: number }

function lcsTable(a: readonly string[], b: readonly string[]): Lcs {
  const width = b.length + 1;
  const table = new Int32Array((a.length + 1) * width);
  const at = (row: number, col: number): number => table[row * width + col] ?? 0;
  for (let i = a.length - 1; i >= 0; i -= 1) {
    for (let j = b.length - 1; j >= 0; j -= 1) {
      table[i * width + j] = a[i] === b[j] ? at(i + 1, j + 1) + 1 : Math.max(at(i + 1, j), at(i, j + 1));
    }
  }
  return { at, width };
}

function align(a: readonly string[], b: readonly string[]): Op[] {
  const { at } = lcsTable(a, b);
  const ops: Op[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    const left = a[i] ?? '';
    const right = b[j] ?? '';
    if (left === right) { ops.push({ tag: ' ', text: left }); i += 1; j += 1; }
    else if (at(i + 1, j) >= at(i, j + 1)) { ops.push({ tag: '-', text: left }); i += 1; }
    else { ops.push({ tag: '+', text: right }); j += 1; }
  }
  while (i < a.length) { ops.push({ tag: '-', text: a[i] ?? '' }); i += 1; }
  while (j < b.length) { ops.push({ tag: '+', text: b[j] ?? '' }); j += 1; }
  return ops;
}

export function unifiedDiff(before: string, after: string, label: string, context = 3): DiffStat {
  if (before === after) return { added: 0, removed: 0, patch: '' };
  const a = before.length === 0 ? [] : before.split('\n');
  const b = after.length === 0 ? [] : after.split('\n');

  if (a.length > BOUND || b.length > BOUND) {
    return {
      added: b.length,
      removed: a.length,
      patch: `--- a/${label}\n+++ b/${label}\n@@ whole-file replacement (${a.length} -> ${b.length} lines; too large to diff inline) @@\n`,
    };
  }

  const ops = align(a, b);
  const added = ops.reduce((count, op) => count + (op.tag === '+' ? 1 : 0), 0);
  const removed = ops.reduce((count, op) => count + (op.tag === '-' ? 1 : 0), 0);

  const keep = new Set<number>();
  ops.forEach((op, index) => {
    if (op.tag === ' ') return;
    for (let k = Math.max(0, index - context); k <= Math.min(ops.length - 1, index + context); k += 1) keep.add(k);
  });

  const lines: string[] = [`--- a/${label}`, `+++ b/${label}`];
  let oldLine = 1;
  let newLine = 1;
  let hunk: string[] = [];
  let hunkOldStart = 1;
  let hunkNewStart = 1;
  let hunkOldCount = 0;
  let hunkNewCount = 0;

  const flush = (): void => {
    if (hunk.length === 0) return;
    lines.push(`@@ -${hunkOldStart},${hunkOldCount} +${hunkNewStart},${hunkNewCount} @@`, ...hunk);
    hunk = [];
    hunkOldCount = 0;
    hunkNewCount = 0;
  };

  ops.forEach((op, index) => {
    if (!keep.has(index)) {
      flush();
    } else {
      if (hunk.length === 0) { hunkOldStart = oldLine; hunkNewStart = newLine; }
      hunk.push(`${op.tag}${op.text}`);
      if (op.tag !== '+') hunkOldCount += 1;
      if (op.tag !== '-') hunkNewCount += 1;
    }
    if (op.tag !== '+') oldLine += 1;
    if (op.tag !== '-') newLine += 1;
  });
  flush();

  return { added, removed, patch: lines.join('\n') };
}
