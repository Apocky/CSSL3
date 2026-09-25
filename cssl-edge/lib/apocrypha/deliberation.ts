// cssl-edge · lib/apocrypha/deliberation.ts
// A leaked thought is never the answer. Mirrors apocrypha-core apx-serve generate.rs
// (split_reasoning + deliberation_leak) so every surface applies the same rule.
//
// OBSERVED 2026-09-24/25 on apocky.com (screenshots): replies rendered as
//   "<think>Thinking Process: 1. Analyze the Request: * System Instructions: I am Apocrypha..."
// and, with no tag at all,
//   "Let's draft: ... Or: ... Wait, I need to check ... Revised Draft: ..."
// The model's working, hidden instructions included, on every device. This module decides
// what a reader sees: the answer after a closed <think>, nothing from an unclosed one, and
// nothing from a reply that is shaped like deliberation.

const OPEN = '<think>';
const CLOSE = '</think>';

export interface SplitReasoning {
  readonly thinking: string | null;
  readonly answer: string;
}

/** Split text into its reasoning and its answer. An unclosed <think> is thinking to the end. */
export function splitReasoning(text: string): SplitReasoning {
  const openAt = text.indexOf(OPEN);
  if (openAt < 0) return { thinking: null, answer: text.trim() };
  const start = openAt + OPEN.length;
  const closeAt = text.indexOf(CLOSE, start);
  if (closeAt < 0) {
    const thinking = text.slice(start).trim();
    return { thinking: thinking.length ? thinking : null, answer: text.slice(0, openAt).trim() };
  }
  const thinking = text.slice(start, closeAt).trim();
  return { thinking: thinking.length ? thinking : null, answer: text.slice(closeAt + CLOSE.length).trim() };
}

const LINE_MARKERS: ReadonlyArray<readonly [string, string]> = [
  ['* Draft ', 'numbered drafts'],
  ['Draft 1:', 'numbered drafts'],
  ['Revised Draft:', 'a revised draft'],
  ['*Revised Draft', 'a revised draft'],
  ['Wait, check constraints', 'checking its constraints out loud'],
  ['Wait, check memory', 'checking its memory out loud'],
  ['Wait, I need to check', 'checking itself out loud'],
  ['*Wait,', 'checking itself out loud'],
  ['1. Drafting', 'a drafting section'],
  ['1. Analyze the Request', 'an analysis of the request'],
  ['* System Instructions:', 'its system instructions restated'],
  ['* Constraint:', 'its constraints restated'],
  ['* Input: "', 'the input quoted back'],
  ["Let's go with", 'choosing between its own drafts'],
  ["Let's draft:", 'drafting out loud'],
  ['I must adhere to', 'its constraints restated'],
  ['So I just output the sentence', 'narrating its own output'],
];

/** Why an answer is deliberation that leaked, or null if it reads as an answer. */
export function deliberationLeak(answer: string): string | null {
  const head = answer.trimStart();
  if (head.startsWith('Thinking Process') || head.startsWith(OPEN)) return 'begins as a thinking process';
  for (const line of answer.split('\n')) {
    const l = line.trimStart();
    for (const [marker, why] of LINE_MARKERS) {
      if (l.startsWith(marker)) return why;
    }
  }
  return null;
}

export interface Presentable {
  readonly text: string;
  /** Null when the text is shown as-is; otherwise why the reader sees a placeholder instead. */
  readonly withheld: string | null;
}

export const WITHHELD_TEXT = 'Apocrypha’s working was withheld. Ask again.';

/** What a reader may see of an assistant reply. */
export function presentable(raw: string): Presentable {
  const { answer } = splitReasoning(raw);
  if (!answer) return { text: WITHHELD_TEXT, withheld: raw.includes(OPEN) ? 'an unclosed thought' : 'an empty reply' };
  const why = deliberationLeak(answer);
  if (why) return { text: WITHHELD_TEXT, withheld: why };
  return { text: answer, withheld: null };
}
