import assert from 'node:assert/strict';
import test from 'node:test';
import { EMPTY_WORK, reduceWork, type LocalSession, type WorkEnvelope } from './local-work.ts';
import { attachTask, createLocalIpc } from './local-ipc.ts';

const sessionId = '46ed0eab-9678-4a4a-bfc9-120db71d0ed8';
const turnId = 'e6bcce20-972b-40d5-86f3-a1b59157ee47';
const snapshot: LocalSession = {
  session: { id: sessionId, title: 'Fix the parser', createdAt: '2026-09-16', lastActiveAt: '2026-09-16', standingGrants: [] },
  turns: [{ id: turnId, sessionId, prompt: 'Fix the parser', phase: 'writing', startedAt: '2026-09-16', output: '', toolCalls: [] }],
  epoch: 7,
};

function envelope(seq: number, delta: string, epoch = 7): WorkEnvelope {
  return { session_id: sessionId, epoch, event: { seq, at: '2026-09-16', kind: 'token', data: { delta } } };
}

test('behavioral/unlocking: reconnect replay cannot append a duplicate or older token (floor: one token)', () => {
  const opened = reduceWork(EMPTY_WORK, { type: 'open', snapshot });
  const first = reduceWork(opened, { type: 'event', envelope: envelope(1, 'Fixed') });
  const next = reduceWork(first, { type: 'event', envelope: envelope(2, ' the parser.') });
  const replay = reduceWork(next, { type: 'event', envelope: envelope(1, 'WRONG') });
  assert.equal(replay.turns[0]!.output, 'Fixed the parser.');
  assert.equal(replay.lastSeq, 2);
  assert.equal(replay, next);
  assert.throws(() => assert.equal('Fixed the parser.WRONG', replay.turns[0]!.output));
});

test('behavioral/unlocking: a prior stream epoch cannot change a reopened task (floor: one epoch)', () => {
  const opened = reduceWork(EMPTY_WORK, { type: 'open', snapshot });
  const stale = reduceWork(opened, { type: 'event', envelope: envelope(500, 'WRONG', 6) });
  assert.equal(stale, opened);
  const current = reduceWork(opened, { type: 'event', envelope: envelope(1, 'Current') });
  assert.equal(current.turns[0]!.output, 'Current');
  assert.throws(() => assert.equal(stale.turns[0]!.output, current.turns[0]!.output));
});

test('behavioral/unlocking: restored output and exact failed tool diff survive stream attach', () => {
  const restored: LocalSession = { ...snapshot, turns: [{ ...snapshot.turns[0]!, phase: 'failed', output: 'The test failed.' }] };
  const opened = reduceWork(EMPTY_WORK, { type: 'open', snapshot: restored });
  const event: WorkEnvelope = {
    session_id: sessionId, epoch: 7,
    event: { seq: 1, at: '2026-09-16', kind: 'tool_result', data: {
      id: 'tool-1', name: 'file_edit', ok: false, elapsedMs: 10, summary: 'Changed parser', content: 'exit code 1',
      diff: { path: 'src/parser.ts', added: 1, removed: 1, patch: '-broken\n+fixed' },
    } },
  };
  const result = reduceWork(opened, { type: 'event', envelope: event });
  assert.equal(result.turns[0]!.output, 'The test failed.');
  assert.equal(result.turns[0]!.toolCalls[0]!.ok, false);
  assert.equal(result.turns[0]!.toolCalls[0]!.diff?.patch, '-broken\n+fixed');
  assert.throws(() => assert.equal(result.turns[0]!.toolCalls[0]!.ok, true));
});

test('behavioral/unlocking: replay of two completed turns neither rewrites history nor revives consent', () => {
  const restored: LocalSession = { ...snapshot, turns: [
    { ...snapshot.turns[0]!, phase: 'done', output: 'Saved first answer.' },
    { ...snapshot.turns[0]!, id: 'other-turn', phase: 'done', output: 'Saved second answer.' },
  ] };
  let state = reduceWork(EMPTY_WORK, { type: 'open', snapshot: restored });
  for (const [index, [kind, data]] of [
    ['session', { turn_id: turnId, prompt: 'Fix the parser' }],
    ['token', { delta: 'Replay must not be appended' }],
    ['consent_request', { id: 'stale-approval', detail: 'old command' }],
    ['session', { turn_id: 'new-turn', prompt: 'Run the tests' }],
    ['token', { delta: 'Live answer' }],
  ].entries()) {
    state = reduceWork(state, { type: 'event', envelope: {
      session_id: sessionId, epoch: 7,
      event: { seq: index + 1, at: 'now', kind, data } as WorkEnvelope['event'],
    } });
  }
  assert.deepEqual(state.turns.map((turn) => turn.output), ['Saved first answer.', 'Saved second answer.', 'Live answer']);
  assert.equal(state.consent, null);
  assert.throws(() => assert.equal(state.turns[0]!.output, 'Replay must not be appended'));
});

test('behavioral/unlocking: a one-event gap stops projection without consuming the missing sequence', () => {
  const opened = reduceWork(EMPTY_WORK, { type: 'open', snapshot });
  const gap = reduceWork(opened, { type: 'event', envelope: envelope(2, 'out of order') });
  assert.equal(gap.connection, 'gap');
  assert.equal(gap.lastSeq, 0);
  assert.equal(gap.turns[0]!.output, '');
  const ordered = reduceWork(opened, { type: 'event', envelope: envelope(1, 'in order') });
  assert.equal(ordered.turns[0]!.output, 'in order');
});

test('integration/unlocking: snapshot is accepted before subscribe can emit the first event', async () => {
  let state = EMPTY_WORK;
  const calls: unknown[] = [];
  const api = createLocalIpc(async <Result>(command: string, args?: Record<string, unknown>) => {
    calls.push([command, args]);
    assert.equal(state.epoch, 7);
    state = reduceWork(state, { type: 'event', envelope: envelope(1, 'first frame') });
    return { subscribed: true } as Result;
  });
  await attachTask(api, snapshot, (next) => { state = reduceWork(state, { type: 'open', snapshot: next }); });
  assert.equal(state.turns[0]!.output, 'first frame');
  assert.deepEqual(calls, [['local_request', { request: { operation: 'subscribe', session_id: sessionId, epoch: 7 } }]]);
  const wrongOrder = reduceWork(EMPTY_WORK, { type: 'event', envelope: envelope(1, 'lost') });
  assert.equal(wrongOrder.turns.length, 0);
});

test('integration/unlocking: cancel and exact approval IPC do not wait for an unresolved send', async () => {
  const calls: unknown[] = [];
  let acknowledge: ((value: { turn_id: string }) => void) | undefined;
  const pending = new Promise<{ turn_id: string }>((resolve) => { acknowledge = resolve; });
  const api = createLocalIpc(async <Result>(command: string, args?: Record<string, unknown>) => {
    calls.push([command, args]);
    if (command === 'local_send') return await pending as Result;
    return { cancelled: true, resolved: true } as Result;
  });
  const sent = api.send(sessionId, 'Run the real tests', 'precise', { temperature: 0.3 });
  assert.equal((await api.cancel(sessionId)).cancelled, true);
  await api.consent(sessionId, 7, turnId, 'deny');
  assert.deepEqual(calls, [
    ['local_send', { sessionId, options: { prompt: 'Run the real tests', preset: 'precise', sampling: { temperature: 0.3 } } }],
    ['local_cancel', { sessionId }],
    ['local_consent', { sessionId, epoch: 7, requestId: turnId, decision: 'deny' }],
  ]);
  acknowledge!({ turn_id: turnId });
  assert.deepEqual(await sent, { turn_id: turnId });
});