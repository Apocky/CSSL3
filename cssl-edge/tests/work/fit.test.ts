// The context guard. These assert the three things that make trimming safe rather than merely
// smaller: the instructions survive, the task survives, and the transcript stays well-formed.

import assert from 'node:assert/strict';
import { fitMessages, estimateTotal, type FitMessage } from '../../scripts/apocrypha-work/fit';

const CTX = 16_384;
const RESERVE = 4_096;
const opts = { contextTokens: CTX, reserveTokens: RESERVE };

const system = (n = 3_500): FitMessage => ({ role: 'system', content: 'S'.repeat(n * 3) });
const user = (text: string): FitMessage => ({ role: 'user', content: text });
function exchange(id: string, resultChars: number): FitMessage[] {
  return [
    { role: 'assistant', content: '', tool_calls: [{ id, type: 'function', function: { name: 'read_file', arguments: '{"path":"x"}' } }] },
    { role: 'tool', tool_call_id: id, content: 'R'.repeat(resultChars) },
  ];
}

// -- a short turn must be left completely alone --------------------------------------------------
// The guard is only allowed to act when it is needed; trimming a turn that fits is data loss.
const short = [system(200), user('do a small thing')];
const shortFit = fitMessages(short, opts);
assert.equal(shortFit.droppedGroups, 0, 'a turn under budget must not be trimmed');
assert.equal(shortFit.shortened, 0, 'a turn under budget must not be shortened');
assert.deepEqual(shortFit.messages, short, 'an under-budget turn must pass through untouched');

// -- the real overflow: the scenario that is reachable in ordinary use ----------------------------
// read_file allows 512 KB, tool results are capped at 24,000 chars (~7k tokens each), and the loop
// never removes anything. Two big reads plus the system-and-tools prompt already exceeds a slot.
const overflowing: FitMessage[] = [
  system(3_500),
  user('refactor the parser and run the tests'),
  ...exchange('a', 24_000),
  ...exchange('b', 24_000),
  ...exchange('c', 24_000),
];
assert.ok(estimateTotal(overflowing) > CTX, 'the fixture must actually overflow, or this proves nothing');

const fitted = fitMessages(overflowing, opts);
assert.ok(fitted.fits, `the guard must bring the turn inside the window, got ${fitted.estimatedTokensAfter}`);
assert.ok(fitted.droppedGroups > 0, 'an overflowing turn must actually drop something');

// 1. THE INSTRUCTIONS SURVIVE. Losing these is the whole failure being prevented: the model keeps
//    answering, just without knowing what it is or what it may touch.
assert.equal(fitted.messages[0]?.role, 'system', 'the system prompt must always survive');
assert.equal(fitted.messages[0]?.content, overflowing[0]?.content, 'the system prompt must survive INTACT, not shortened');

// 2. THE TASK SURVIVES. Otherwise the coder answers a question nobody asked.
const users = fitted.messages.filter((m) => m.role === 'user');
assert.equal(users.length, 1, 'the operator task must survive');
assert.equal(users[0]?.content, 'refactor the parser and run the tests');

// 3. NO ORPHANS. A tool message whose assistant tool_calls entry was dropped makes the engine
//    REJECT the request outright -- careless trimming turns a degraded turn into a failed one.
const liveCallIds = new Set(fitted.messages.flatMap((m) => (m.tool_calls ?? []).map((c) => c.id)));
for (const message of fitted.messages) {
  if (message.role === 'tool') {
    assert.ok(liveCallIds.has(message.tool_call_id ?? ''), `orphaned tool result ${message.tool_call_id}: the engine rejects this`);
  }
}

// 4. RECENCY. What it just learned matters more than its first step, so the newest exchange is the
//    one that must still be there.
const survivingIds = [...liveCallIds];
assert.ok(survivingIds.includes('c'), 'the most recent exchange must be the one kept');
assert.ok(!survivingIds.includes('a'), 'the oldest exchange must be the one dropped');

// -- one result too big to keep whole ------------------------------------------------------------
// A single 120k-character read in the last exchange cannot be dropped (it is the freshest work) and
// cannot be kept. It must be SHORTENED and labelled, because a truncated-but-marked result is
// usable while a missing instruction is not.
const huge: FitMessage[] = [system(3_500), user('read the big file'), ...exchange('z', 120_000)];
const hugeFit = fitMessages(huge, opts);
assert.ok(hugeFit.fits, 'a single oversized result must still be brought inside the window');
assert.equal(hugeFit.messages[0]?.role, 'system', 'the system prompt survives even this');
assert.ok(hugeFit.shortened > 0, 'the oversized result must be shortened rather than dropped');
const shortenedTool = hugeFit.messages.find((m) => m.role === 'tool');
assert.ok(shortenedTool?.content?.includes('did not fit the context window'),
  'a shortened result must SAY it was shortened -- a silent trim is the same disease as a silent clip');

// -- the estimate must err toward over-counting ---------------------------------------------------
// Guessing high on tokens trims slightly early, which costs nothing. Guessing low lets a prompt
// through that the engine then clips, which costs the system prompt.
const sample: FitMessage = { role: 'user', content: 'x'.repeat(3_000) };
assert.ok(estimateTotal([sample]) >= 1_000, 'the estimator must not under-count a 3,000-character message');

// -- the tool block counts toward the window -----------------------------------------------------
// REGRESSION GATE. The first version measured only `messages`, judged a turn to fit, and the engine
// rejected the real request at 17,617 tokens against a 16,384 window: the tool schemas are sent
// separately as `tools` and are thousands of tokens. A guard that ignores them does not guard.
const modest: FitMessage[] = [system(1_000), user('do a thing'), ...exchange('t', 9_000)];
const withoutTools = fitMessages(modest, opts);
assert.equal(withoutTools.droppedGroups, 0, 'this fixture must fit when nothing else is sent');
// 9,000 is chosen so the total genuinely crosses the budget; at 6,000 the fixture still fit and
// the gate failed for the wrong reason -- a test that cannot distinguish those is not worth having.
const withTools = fitMessages(modest, { ...opts, overheadTokens: 9_000 });
assert.ok(
  withTools.estimatedTokensBefore > withoutTools.estimatedTokensBefore,
  'the tool block must be counted in the before-total, or the guard cannot see it',
);
assert.ok(
  withTools.droppedGroups > 0 || withTools.shortened > 0,
  'a turn that only overflows BECAUSE of the tool schemas must still be trimmed',
);
assert.ok(withTools.fits, 'and it must end up inside the window, tools included');

console.log('work/fit OK - instructions and task survive, no orphaned tool results, oversized results labelled');
