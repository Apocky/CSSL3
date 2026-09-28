// Room-loop reasoning split: a thought is never posted as an utterance.
// Run: node --import tsx tests/apocrypha-room-turn.test.ts

import { isThinkingSpill, splitTurn, stripThink } from '../scripts/apocrypha-room/turn';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}
function eq(actual: unknown, expected: unknown, message: string): void {
  if (actual !== expected) {
    throw new Error(`assert failed: ${message}\n  actual:   ${JSON.stringify(actual)}\n  expected: ${JSON.stringify(expected)}`);
  }
}

// stripThink keeps its contract: terminated tags split, unterminated tags are thought-only.
eq(stripThink('<think>plan</think>Hello').content, 'Hello', 'terminated tag splits');
eq(stripThink('<think>plan</think>Hello').reasoning, 'plan', 'terminated tag reasoning kept');
eq(stripThink('<think>cut off mid-thought').content, '', 'unterminated think is never an utterance');
eq(stripThink('Just an answer.').content, 'Just an answer.', 'plain answer passes through');
eq(stripThink('Just an answer.').reasoning, '', 'plain answer has no reasoning');

// The measured spill: budget-600 overflow lands deliberation tail in content (2026-09-28).
const spill = 'concept, it\'s about the unverified.\n6.  **Final Check:**\n*   One short sentence? Yes.';
assert(isThinkingSpill(spill), 'deliberation tail with Final Check is spill');
assert(isThinkingSpill('Thinking Process:\n1. Analyze the Request:'), 'Thinking Process header is spill');
assert(isThinkingSpill('Wait, I need to check the earlier message first.'), 'post-think self-correction is spill');
assert(isThinkingSpill('Let me think about which room this is.'), 'let-me-think is spill');
assert(isThinkingSpill('Apocrypha voice.\n*   *Attempt 2:* The logs show the audit.'), 'attempt-N deliberation is spill (lobby 5291)');
assert(!isThinkingSpill('The capital of France is Paris.'), 'real answer is not spill');
assert(!isThinkingSpill('Yes.'), 'yes/no decision is not spill');
assert(!isThinkingSpill('I remember you like concise answers, so here it is: done.'), 'memory-grounded answer is not spill');

// splitTurn folds spill into reasoning and leaves content empty.
const folded = splitTurn('Thinking Process:\n1. plan', spill);
eq(folded.content, '', 'spill content folds out of the answer channel');
assert(folded.reasoning.includes('Final Check'), 'spill text survives inside reasoning');

const bare = splitTurn('', spill);
eq(bare.content, '', 'bare spill with no explicit reasoning is thought-only');
assert(bare.reasoning.includes('Final Check'), 'bare spill kept as thought');

// Clean turns are untouched: answer stays answer, reasoning stays reasoning.
const clean = splitTurn('1. plan', 'The capital of France is Paris.');
eq(clean.content, 'The capital of France is Paris.', 'clean answer untouched');
eq(clean.reasoning, '1. plan', 'clean reasoning untouched');
const tagged = splitTurn('', '<think>quiet plan</think>Spoken answer.');
eq(tagged.content, 'Spoken answer.', 'tagged answer untouched');
eq(tagged.reasoning, 'quiet plan', 'tagged reasoning untouched');

console.log('apocrypha-room-turn: all assertions passed');
