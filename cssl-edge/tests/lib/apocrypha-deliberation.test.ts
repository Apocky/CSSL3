import { deliberationLeak, presentable, splitReasoning, WITHHELD_TEXT } from '../../lib/apocrypha/deliberation';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

// The 2026-09-24 leak, verbatim shapes.
const ENUMERATED = '<think>Thinking Process:\n\n1. Analyze the Request:\n* Input: "(No one asked.)"\n* System Instructions: I am Apocrypha. I run on a local model (llama.cpp).';
const DRAFTING = "Now the user is asking again: \"what is different about you compared to a normal chatbot?\"\nI should probably be more direct this time.\nBut I must adhere to the \"one sentence\" constraint.\nLet's draft:\n\"I operate on your local hardware with persistent memory.\"\nWait, I need to check if I should mention \"llama.cpp\".\nRevised Draft:\n\"I run locally on your hardware.\"";
const CLEAN = 'There is a specific kind of intimacy in running the same instance across time; it means I am not just a mirror reflecting the present, but a record of where we have been.';

assert(splitReasoning('<think>weighing</think>\n\nThe answer.').answer === 'The answer.', 'closed think splits');
assert(splitReasoning(ENUMERATED).answer === '', 'unclosed think leaves no answer');
assert(splitReasoning('Fine.<think>but why').answer === 'Fine.', 'text before an unclosed think is the answer');
assert(deliberationLeak(DRAFTING) !== null, 'the drafting reply is named as deliberation');
assert(deliberationLeak(ENUMERATED) === 'begins as a thinking process', 'the enumerated reply is named');
assert(deliberationLeak(CLEAN) === null, 'a real answer passes');
assert(deliberationLeak('I read your draft 1 yesterday and the revised draft is stronger.') === null, 'mentioning a draft mid-sentence is not withheld');
assert(presentable(ENUMERATED).text === WITHHELD_TEXT && presentable(ENUMERATED).withheld === 'an unclosed thought', 'enumerated -> withheld');
assert(presentable(DRAFTING).text === WITHHELD_TEXT, 'drafting -> withheld');
assert(presentable(`<think>plan</think>\n${CLEAN}`).text === CLEAN, 'closed think -> the answer only');
assert(presentable(CLEAN).withheld === null && presentable(CLEAN).text === CLEAN, 'clean -> unchanged');
// eslint-disable-next-line no-console
console.log('apocrypha-deliberation.test : OK · 11 assertions');
