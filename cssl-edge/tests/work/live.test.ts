// Layer 0 — the client-side event fold.
//
// The stream reconnects on its own and replays the channel's recent buffer each time, so the
// reducer sees the same events more than once as a matter of course. If the fold is not
// idempotent, a dropped connection silently duplicates tool steps in the transcript and doubles
// the visible answer — which looks like the agent repeating itself rather than like a bug.

import { IDLE, phaseLabel, reduceLive, type LiveState } from '../../lib/work/live';
import type { WorkEvent } from '../../lib/work/client';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

let seq = 0;
function ev(kind: WorkEvent['kind'], data: Record<string, unknown>): WorkEvent {
  seq += 1;
  return { seq, at: new Date(0).toISOString(), kind, data };
}

function fold(events: readonly WorkEvent[], from: LiveState = IDLE): LiveState {
  return events.reduce(reduceLive, from);
}

async function main(): Promise<void> {
  const script: WorkEvent[] = [
    ev('session', { turn_id: 't1', prompt: 'fix the thing', phase: 'queued' }),
    ev('phase', { phase: 'thinking', iteration: 0 }),
    ev('tool_request', { id: 'c1', name: 'read_file', risk: 'read', summary: 'repos/a.ts lines 1-40' }),
    ev('tool_result', { id: 'c1', name: 'read_file', ok: true, elapsedMs: 12, summary: 'read 40 lines', content: 'x' }),
    ev('consent_request', { id: 'r1', tool: 'edit_file', risk: 'write', summary: 'Edit repos/a.ts', detail: '- a\n+ b' }),
    ev('consent_resolved', { id: 'r1', decision: 'allow' }),
    ev('tool_result', { id: 'c2', name: 'edit_file', ok: true, elapsedMs: 8, summary: 'edited repos/a.ts', content: 'done' }),
    ev('token', { delta: 'I changed ' }),
    ev('token', { delta: 'one line.' }),
    ev('usage', { totalTokens: 900, elapsedS: 4.2 }),
    ev('phase', { phase: 'done', terminal: true, tool_calls: 2 }),
  ];

  const once = fold(script);
  assert(once.prompt === 'fix the thing', 'prompt was not captured');
  assert(once.answer === 'I changed one line.', `answer folded to "${once.answer}"`);
  assert(once.steps.length === 2, `expected 2 steps, got ${once.steps.length}`);
  assert(once.pendingStep === null, 'a pending step survived its result');
  assert(once.consent === null, 'consent card survived its resolution');
  assert(once.terminal, 'terminal phase did not mark the turn finished');
  assert(once.usage?.totalTokens === 900, 'usage was lost');

  // Replay: the same events again must change nothing at all.
  const replayed = fold(script, once);
  assert(JSON.stringify(replayed) === JSON.stringify(once), 'replaying the buffer changed the state');

  // Partial replay from the middle, the shape a reconnect actually takes.
  const reconnected = fold(script.slice(3), once);
  assert(reconnected.steps.length === 2, `reconnect duplicated steps: ${reconnected.steps.length}`);
  assert(reconnected.answer === 'I changed one line.', 'reconnect duplicated the answer');

  // Out-of-order and duplicate sequence numbers are ignored rather than applied twice.
  const stale: WorkEvent = { seq: 2, at: new Date(0).toISOString(), kind: 'token', data: { delta: 'GHOST' } };
  assert(!fold([stale], once).answer.includes('GHOST'), 'a stale event was applied');

  // A consent request must block the view until resolved, and survive intervening events.
  const waiting = fold(script.slice(0, 5));
  assert(waiting.consent?.id === 'r1', 'consent request was not surfaced');
  assert(waiting.consent?.risk === 'write', 'consent risk was lost');
  assert(phaseLabel(waiting) === 'waiting on you', `phase label read "${phaseLabel(waiting)}"`);

  // A new turn resets the view rather than appending to the last one.
  const second = fold([ev('session', { turn_id: 't2', prompt: 'next job', phase: 'queued' })], once);
  assert(second.answer === '' && second.steps.length === 0, 'a new turn inherited the previous transcript');
  assert(second.prompt === 'next job' && !second.terminal, 'a new turn did not start running');

  // An error is terminal and is not silently overwritten by a later phase line.
  const failed = fold([ev('error', { message: 'engine died', code: 'ENGINE_UNREACHABLE' })], once);
  assert(failed.error === 'engine died' && failed.terminal, 'error did not terminate the turn');
  assert(phaseLabel(failed) === 'failed', `failed turn labelled "${phaseLabel(failed)}"`);

  console.log(`live: ${script.length} events, replay + partial-replay + stale-event idempotence verified`);
}

main().then(() => console.log('work/live OK')).catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
