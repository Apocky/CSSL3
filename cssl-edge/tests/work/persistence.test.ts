import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { EventEmitter } from 'node:events';
import { existsSync } from 'node:fs';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import type { ServerResponse } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { test, type TestContext } from 'node:test';
import { WorkAgent } from '../../scripts/apocrypha-work/agent';
import { loadWorkConfig } from '../../scripts/apocrypha-work/config';
import type { EngineLike, EngineReply } from '../../scripts/apocrypha-work/engine';
import { TurnRunner } from '../../scripts/apocrypha-work/runner';
import { SessionStore } from '../../scripts/apocrypha-work/sessions';
import type { WorkEvent, WorkSession, WorkTurn } from '../../scripts/apocrypha-work/types';
import { Workspace } from '../../scripts/apocrypha-work/workspace';

function deferred<Value>() {
  let resolve!: (value: Value) => void;
  const promise = new Promise<Value>((accept) => { resolve = accept; });
  return { promise, resolve };
}

function reply(partial: Partial<EngineReply> = {}): EngineReply {
  return { content: 'Scripted answer.', toolCalls: [], finishReason: 'stop', usage: {}, ...partial };
}

function durableTurns(root: string, sessionId: string): WorkTurn[] {
  const database = new DatabaseSync(join(root, 'sessions', 'tasks.sqlite'), { readOnly: true });
  try {
    const rows = database.prepare('SELECT payload FROM turns WHERE session_id = ? ORDER BY position').all(sessionId) as { payload: string }[];
    return rows.map((row) => JSON.parse(row.payload) as WorkTurn);
  } finally { database.close(); }
}

function turnFor(session: WorkSession, id: string, phase: WorkTurn['phase'] = 'queued'): WorkTurn {
  return { id, sessionId: session.id, prompt: `Prompt ${id}`, phase, startedAt: new Date(0).toISOString(), toolCalls: [], output: '' };
}

function executeFixtureSQL(root: string, sql: string): void {
  const database = new DatabaseSync(join(root, 'sessions', 'tasks.sqlite'));
  try { database.exec(sql); }
  finally { database.close(); }
}

class Stream extends EventEmitter {
  readonly frames: string[] = [];

  writeHead(): this { return this; }
  write(frame: string): boolean {
    this.frames.push(frame);
    const data = frame.split('\n').find((line) => line.startsWith('data: '));
    if (data) {
      const event = JSON.parse(data.slice(6)) as WorkEvent;
      if (typeof event.seq === 'number') this.emit('event', event);
    }
    return true;
  }
  end(): void { this.emit('close'); }

  events(): WorkEvent[] {
    return this.frames.flatMap((frame) => {
      const data = frame.split('\n').find((line) => line.startsWith('data: '));
      const event = data ? JSON.parse(data.slice(6)) as WorkEvent : undefined;
      return event && typeof event.seq === 'number' ? [event] : [];
    });
  }

  waitFor(predicate: (event: WorkEvent) => boolean): Promise<WorkEvent> {
    const existing = this.events().find(predicate);
    if (existing) return Promise.resolve(existing);
    return new Promise((resolve) => {
      const listener = (event: WorkEvent) => {
        if (!predicate(event)) return;
        this.off('event', listener);
        resolve(event);
      };
      this.on('event', listener);
    });
  }

  response(): ServerResponse { return this as unknown as ServerResponse; }
}

async function fixture(context: TestContext) {
  const root = await mkdtemp(join(tmpdir(), 'work-persistence-'));
  const stores: SessionStore[] = [];
  const runners: TurnRunner[] = [];
  const config = loadWorkConfig({
    APOCRYPHA_WORK_ROOTS: `sandbox=${root}`,
    APOCRYPHA_WORK_STATE_DIR: root,
    APOCRYPHA_WORK_TOKEN: 'x'.repeat(32),
  });
  context.after(async () => {
    for (const runner of runners) await runner.stopAll();
    for (const store of stores) (store as SessionStore & { close?: () => void }).close?.();
    await rm(root, { recursive: true, force: true });
  });
  return {
    root,
    async open() {
      const store = await SessionStore.open(root);
      stores.push(store);
      return store;
    },
    async runner(store: SessionStore, engine?: EngineLike) {
      const scripted: EngineLike = engine ?? {
        async complete(_messages, _tools, onToken) {
          onToken('Scripted answer.');
          return { content: 'Scripted answer.', toolCalls: [], finishReason: 'stop', usage: {} };
        },
      };
      const agent = new WorkAgent(config, await Workspace.open(config.roots), scripted);
      const runner = new TurnRunner(config, agent, store);
      runners.push(runner);
      return runner;
    },
  };
}

test('P1 integration: reopen replays all 405 committed events, not an empty RAM buffer', async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const session = await store.create('Replay');
  for (let index = 1; index <= 405; index += 1) {
    await store.appendEvent(session.id, {
      seq: index, at: new Date(0).toISOString(), kind: 'token', data: { text: `token-${index}` },
    });
  }
  (store as SessionStore & { close?: () => void }).close?.();
  const reopened = await harness.open();
  assert.equal((await reopened.list()).length, 1);
  const runner = await harness.runner(reopened);
  const stream = new Stream();
  await runner.attachStream(session.id, stream.response());
  const events = stream.events();
  assert.equal(events.length, 405, 'one missing committed event must fail this exact replay oracle');
  assert.deepEqual(events.map((event) => event.seq), Array.from({ length: 405 }, (_, index) => index + 1));
  assert.equal(events[404]?.data.text, 'token-405');
});

test('P1 behavioral: malformed legacy JSON is refused, never treated as an empty session', async (context) => {
  const harness = await fixture(context);
  await mkdir(join(harness.root, 'sessions'), { recursive: true });
  await writeFile(join(harness.root, 'sessions', 'broken.json'), '{"session":', 'utf8');
  await assert.rejects(harness.open(), /corrupt|invalid|malformed|JSON/i);
});

test('P1 behavioral: traversal session IDs are refused before filesystem access', async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  await assert.rejects(store.load('../outside'), /invalid.*id/i);
});

test('P1 integration: start acknowledges only a durable in-flight intent', { timeout: 5_000 }, async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const session = await store.create('Intent');
  const gate = deferred<EngineReply>();
  const runner = await harness.runner(store, { complete: async () => gate.promise });
  const stream = new Stream();
  await runner.attachStream(session.id, stream.response());
  let recorded: WorkTurn | undefined;
  let initial: WorkEvent | undefined;
  let turn: WorkTurn;
  try {
    turn = await runner.start(session, 'Do not lose the acknowledged prompt');
    recorded = durableTurns(harness.root, session.id).find((entry) => entry.id === turn.id);
    initial = (await store.eventsAfter(session.id))[0];
  } finally {
    gate.resolve(reply());
    await stream.waitFor((event) => event.data.terminal === true);
  }
  assert.equal(recorded?.prompt, 'Do not lose the acknowledged prompt');
  assert.ok(recorded && ['queued', 'thinking'].includes(recorded.phase));
  assert.equal(initial?.kind, 'session');
  assert.equal(initial?.data.phase, 'queued');
  assert.equal(initial?.data.turn_id, recorded?.id);
  assert.equal(stream.events()[0]?.kind, 'session');
});

test('P1 integration: every terminal phase notification follows the committed final turn', { timeout: 5_000 }, async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const session = await store.create('Terminal');
  const runner = await harness.runner(store);
  const stream = new Stream();
  const observed: (WorkTurn | undefined)[] = [];
  stream.on('event', (event: WorkEvent) => {
    if (event.kind === 'phase' && ['done', 'failed', 'cancelled'].includes(String(event.data.phase))) {
      observed.push(durableTurns(harness.root, session.id)[0]);
    }
  });
  await runner.attachStream(session.id, stream.response());
  await runner.start(session, 'Finish durably');
  await stream.waitFor((event) => event.data.terminal === true);
  assert.ok(observed.length > 0, 'the oracle must inspect a real terminal notification');
  assert.ok(observed.every((turn) => turn?.phase === 'done' && turn.output === 'Scripted answer.' && !!turn.endedAt),
    'a terminal notification preceded its durable final turn');
});

test('P1 behavioral: failed history loading releases the session reservation', async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const session = await store.create('History failure');
  const runner = await harness.runner(store);
  const original = store.history.bind(store);
  store.history = async () => { throw new Error('injected history failure'); };
  await assert.rejects(runner.start(session, 'Fail before execution'), /injected history failure/);
  store.history = original;
  assert.equal(runner.activeCount(), 0, 'a failed history read wedged the session');
});

test('P1 integration: abort at approval delivery denies the wait and refuses stale consent', { timeout: 5_000 }, async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const session = await store.create('Approval cancellation');
  const runner = await harness.runner(store, {
    async complete() {
      return reply({ content: '', toolCalls: [{ id: 'write-once', name: 'write_file', args: { path: 'denied.txt', content: 'must not be written' } }] });
    },
  });
  const stream = new Stream();
  stream.on('event', (event: WorkEvent) => {
    if (event.kind === 'consent_request') runner.cancel(session.id);
  });
  await runner.attachStream(session.id, stream.response());
  await runner.start(session, 'Ask before writing');
  const request = await stream.waitFor((event) => event.kind === 'consent_request');
  const accepted = runner.resolveConsent(String(request.data.id), 'allow');
  const terminal = await stream.waitFor((event) => event.data.terminal === true);
  assert.equal(accepted, false, 'a resolution issued after cancellation was accepted');
  assert.equal(runner.resolveConsent(String(request.data.id), 'deny'), false);
  assert.equal(terminal.data.phase, 'cancelled');
  assert.equal(existsSync(join(harness.root, 'denied.txt')), false);
  assert.equal(runner.activeCount(), 0);
});

test('P1 integration: event persistence failure aborts visibly without streaming uncommitted tokens', { timeout: 5_000 }, async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const session = await store.create('Persistence failure');
  const append = store.appendEvent.bind(store);
  store.appendEvent = async (sessionId, event) => {
    if (event.kind === 'token') throw new Error('injected event persistence failure');
    return append(sessionId, event);
  };
  const runner = await harness.runner(store);
  const stream = new Stream();
  await runner.attachStream(session.id, stream.response());
  await runner.start(session, 'Do not stream an uncommitted token');
  const terminal = await stream.waitFor((event) => event.data.terminal === true);
  assert.equal(terminal.data.phase, 'failed');
  assert.equal(stream.events().filter((event) => event.kind === 'token').length, 0);
  assert.match(durableTurns(harness.root, session.id)[0]?.error ?? '', /persistence failure/);
  assert.ok(stream.frames.some((frame) => frame.includes('PERSISTENCE_FAILED')));
});

test('P1 integration: legacy import preserves exact source bytes, counts, hashes and sequence reset evidence', async (context) => {
  const harness = await fixture(context);
  const directory = join(harness.root, 'sessions');
  await mkdir(directory, { recursive: true });
  const first: WorkSession = { id: 'legacy-first', title: 'First', createdAt: new Date(0).toISOString(), lastActiveAt: new Date(0).toISOString(), standingGrants: ['read_file'] };
  const second: WorkSession = { ...first, id: 'legacy-second', title: 'Second', standingGrants: [] };
  const completed = { ...turnFor(first, 'old-turn', 'done'), output: 'A preserved answer.', endedAt: new Date(1).toISOString() };
  const snapshot = Buffer.from(JSON.stringify({ session: first, turns: [completed] }, null, 2) + '\r\n');
  const events: WorkEvent[] = [
    { seq: 1, at: first.createdAt, kind: 'token', data: { delta: 'first' } },
    { seq: 2, at: first.createdAt, kind: 'phase', data: { phase: 'done' } },
    { seq: 1, at: first.createdAt, kind: 'token', data: { delta: 'after old process restart' } },
  ];
  const jsonl = Buffer.from(events.map((event) => JSON.stringify(event)).join('\r\n') + '\r\n');
  await writeFile(join(directory, `${first.id}.json`), snapshot);
  await writeFile(join(directory, `${first.id}.events.jsonl`), jsonl);
  await writeFile(join(directory, `${second.id}.json`), JSON.stringify({ session: second, turns: [] }));
  const imported = await harness.open();
  assert.equal((await imported.list()).length, 2);
  assert.deepEqual((await imported.load(first.id))?.turns, [completed]);
  assert.deepEqual((await imported.eventsAfter(first.id)).map((event) => [event.seq, event.data]), events.map((event, index) => [index + 1, event.data]));
  const database = new DatabaseSync(join(directory, 'tasks.sqlite'), { readOnly: true });
  try {
    const record = database.prepare('SELECT * FROM legacy_imports WHERE session_id = ?').get(first.id) as Record<string, unknown> | undefined;
    assert.equal(record?.snapshot_hash, createHash('sha256').update(snapshot).digest('hex'));
    assert.equal(record?.events_hash, createHash('sha256').update(jsonl).digest('hex'));
    assert.equal(record?.snapshot_bytes, snapshot.length);
    assert.equal(record?.events_bytes, jsonl.length);
    assert.equal(record?.turn_count, 1);
    assert.equal(record?.event_count, 3);
    const importedEvents = database.prepare('SELECT legacy_seq FROM events WHERE session_id = ? ORDER BY seq').all(first.id) as { legacy_seq: number }[];
    assert.deepEqual(importedEvents.map((row) => row.legacy_seq), [1, 2, 1]);
  } finally { database.close(); }
  await imported.addTurn(first.id, { ...turnFor(first, 'new-turn', 'done'), output: 'New durable answer.' });
  await imported.rename(first.id, 'Renamed after import');
  imported.close();
  const reopened = await harness.open();
  reopened.close();
  const reopenedAgain = await harness.open();
  assert.equal((await reopenedAgain.list()).length, 2);
  assert.equal((await reopenedAgain.load(first.id))?.turns.length, 2);
  assert.equal((await reopenedAgain.load(first.id))?.session.title, 'Renamed after import');
  assert.equal((await reopenedAgain.eventsAfter(first.id)).length, 3);
  assert.deepEqual(await readFile(join(directory, `${first.id}.json`)), snapshot);
  assert.deepEqual(await readFile(join(directory, `${first.id}.events.jsonl`)), jsonl);
});

test('P1 behavioral: a changed migration source is refused instead of overwriting durable work', async (context) => {
  const harness = await fixture(context);
  const directory = join(harness.root, 'sessions');
  await mkdir(directory, { recursive: true });
  const session: WorkSession = { id: 'legacy', title: 'Original', createdAt: new Date(0).toISOString(), lastActiveAt: new Date(0).toISOString(), standingGrants: [] };
  const path = join(directory, 'legacy.json');
  await writeFile(path, JSON.stringify({ session, turns: [] }));
  const store = await harness.open();
  await store.rename(session.id, 'Durable title');
  store.close();
  await writeFile(path, JSON.stringify({ session: { ...session, title: 'Foreign edit' }, turns: [] }));
  await assert.rejects(harness.open(), /changed after import|refusing overwrite/i);
  const database = new DatabaseSync(join(directory, 'tasks.sqlite'), { readOnly: true });
  try {
    const row = database.prepare('SELECT payload FROM sessions WHERE id = ?').get(session.id) as { payload: string } | undefined;
    assert.equal((JSON.parse(String(row?.payload)) as WorkSession).title, 'Durable title');
  } finally { database.close(); }
});

test('P1 behavioral: corrupt JSONL rolls back the entire legacy batch and preserves its sources', async (context) => {
  const harness = await fixture(context);
  const directory = join(harness.root, 'sessions');
  await mkdir(directory, { recursive: true });
  const session: WorkSession = { id: 'legacy', title: 'Original', createdAt: new Date(0).toISOString(), lastActiveAt: new Date(0).toISOString(), standingGrants: [] };
  const snapshot = JSON.stringify({ session, turns: [] });
  const invalid = JSON.stringify({ seq: 1, at: session.createdAt, kind: 'token', data: { delta: 'preserved' } }) + '\n{';
  await writeFile(join(directory, 'legacy.json'), snapshot);
  await writeFile(join(directory, 'legacy.events.jsonl'), invalid);
  await assert.rejects(harness.open(), /Corrupt JSON/);
  const database = new DatabaseSync(join(directory, 'tasks.sqlite'), { readOnly: true });
  try { assert.equal((database.prepare('SELECT COUNT(*) AS count FROM sessions').get() as { count: number }).count, 0); }
  finally { database.close(); }
  assert.equal(await readFile(join(directory, 'legacy.json'), 'utf8'), snapshot);
  assert.equal(await readFile(join(directory, 'legacy.events.jsonl'), 'utf8'), invalid);
});

test('P1 behavioral: malformed legacy shapes and mismatched IDs are refused', async (context) => {
  const harness = await fixture(context);
  const directory = join(harness.root, 'sessions');
  await mkdir(directory, { recursive: true });
  const session: WorkSession = { id: 'legacy', title: 'Original', createdAt: new Date(0).toISOString(), lastActiveAt: new Date(0).toISOString(), standingGrants: [] };
  const values = [
    {},
    { session: { ...session, id: 'another-id' }, turns: [] },
    { session, turns: 'not an array' },
    { session, turns: [{ ...turnFor(session, 'bad-turn'), toolCalls: [{}] }] },
    { session, turns: [turnFor(session, 'duplicate'), turnFor(session, 'duplicate')] },
  ];
  for (const value of values) {
    await writeFile(join(directory, 'legacy.json'), JSON.stringify(value));
    await assert.rejects(harness.open(), /invalid|duplicate/i);
  }
});

test('P1 behavioral: invalid IDs, unknown sessions and invalid cursors cannot mutate storage', async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const session = await store.create('Validation');
  for (const id of ['../outside', '..\\outside', 'a/b', 'a\\b', 'C:bad', '', '.', '..', '%2e%2e', 'CON', 'nul', 'has.dot']) {
    await assert.rejects(store.load(id), /invalid.*id/i);
    await assert.rejects(store.persist(id), /invalid.*id/i);
    await assert.rejects(store.rename(id, 'No'), /invalid.*id/i);
    await assert.rejects(store.appendEvent(id, { at: new Date(0).toISOString(), kind: 'token', data: {} }), /invalid.*id/i);
  }
  await assert.rejects(store.appendEvent('unknown-session', { at: new Date(0).toISOString(), kind: 'token', data: {} }), /unknown session/);
  await assert.rejects(store.addTurn(session.id, { ...turnFor(session, 'invalid-turn'), sessionId: 'another-session' }), /invalid turn/i);
  for (const cursor of [-1, 0.5, NaN, Infinity]) await assert.rejects(store.eventsAfter(session.id, cursor), /invalid event cursor/i);
  assert.equal((await store.list()).length, 1);
  assert.deepEqual((await store.load(session.id))?.turns, []);
  assert.deepEqual(await store.eventsAfter(session.id), []);
});

test('P1 integration: restart continues ordered cursors and replays strictly after the supplied cursor', { timeout: 5_000 }, async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const session = await store.create('Restart cursors');
  const runner = await harness.runner(store);
  const firstStream = new Stream();
  await runner.attachStream(session.id, firstStream.response());
  await runner.start(session, 'First turn');
  const firstTerminal = await firstStream.waitFor((event) => event.data.terminal === true);
  await runner.stopAll();
  store.close();
  const reopened = await harness.open();
  const next = await harness.runner(reopened);
  const stream = new Stream();
  await next.attachStream(session.id, stream.response(), firstTerminal.seq);
  assert.deepEqual(stream.events(), []);
  await next.start((await reopened.load(session.id))!.session, 'Second turn');
  await stream.waitFor((event) => event.data.terminal === true);
  const events = [...firstStream.events(), ...stream.events()];
  assert.deepEqual(events.map((event) => event.seq), Array.from({ length: events.length }, (_, index) => index + 1));
  assert.equal(stream.events()[0]?.seq, firstTerminal.seq + 1);
  assert.equal((await reopened.load(session.id))?.turns.length, 2);
  assert.deepEqual(await reopened.history(session.id), [
    { role: 'user', content: 'First turn' }, { role: 'assistant', content: 'Scripted answer.' },
    { role: 'user', content: 'Second turn' }, { role: 'assistant', content: 'Scripted answer.' },
  ]);
});

test('P1 integration: separate sessions run concurrently without interleaving their state', { timeout: 5_000 }, async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const first = await store.create('First');
  const second = await store.create('Second');
  const gates = [deferred<EngineReply>(), deferred<EngineReply>()];
  const runner = await harness.runner(store, {
    async complete(messages, _tools, onToken) {
      const prompt = messages[messages.length - 1]?.content;
      const result = await gates[prompt === 'First' ? 0 : 1]!.promise;
      onToken(result.content);
      return result;
    },
  });
  const streams = [new Stream(), new Stream()];
  await Promise.all([runner.attachStream(first.id, streams[0]!.response()), runner.attachStream(second.id, streams[1]!.response())]);
  let active = 0;
  try {
    await Promise.all([runner.start(first, 'First'), runner.start(second, 'Second')]);
    active = runner.activeCount();
    await assert.rejects(runner.start(first, 'Duplicate'), /already running/);
    assert.equal(durableTurns(harness.root, first.id).length, 1);
    assert.equal(durableTurns(harness.root, second.id).length, 1);
  } finally {
    gates[0]!.resolve(reply({ content: 'First answer' }));
    gates[1]!.resolve(reply({ content: 'Second answer' }));
    await Promise.all(streams.map((stream) => stream.waitFor((event) => event.data.terminal === true)));
  }
  assert.equal(active, 2);
  assert.equal(runner.activeCount(), 0);
  assert.equal(durableTurns(harness.root, first.id)[0]?.output, 'First answer');
  assert.equal(durableTurns(harness.root, second.id)[0]?.output, 'Second answer');
  for (const stream of streams) assert.deepEqual(stream.events().map((event) => event.seq), Array.from({ length: stream.events().length }, (_, index) => index + 1));
});

test('P1 integration: interrupted turns recover once with partial output and unknown effects, never automatic execution', async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const session = await store.create('Interrupted');
  const turn = turnFor(session, 'interrupted-turn');
  await store.beginTurn(session.id, turn);
  await store.appendEvent(session.id, { at: turn.startedAt, kind: 'token', data: { turn_id: turn.id, delta: 'Partial output' } });
  await store.appendEvent(session.id, { at: turn.startedAt, kind: 'tool_request', data: { turn_id: turn.id, id: 'uncertain-call', name: 'run_command', args: { command: 'unknown-effect' } } });
  store.close();
  const reopened = await harness.open();
  const recovered = (await reopened.load(session.id))!.turns[0]!;
  assert.equal(recovered.phase, 'failed');
  assert.match(recovered.error ?? '', /unknown.*reconciliation|reconciliation.*unknown/i);
  assert.equal(recovered.output, 'Partial output');
  const events = await reopened.eventsAfter(session.id);
  assert.equal(events.length, 5);
  assert.equal(events[3]?.data.code, 'TURN_INTERRUPTED_RECONCILIATION_REQUIRED');
  assert.equal(events[4]?.data.needs_reconciliation, true);
  let modelCalls = 0;
  const runner = await harness.runner(reopened, { async complete() { modelCalls += 1; return reply(); } });
  const stream = new Stream();
  await runner.attachStream(session.id, stream.response());
  assert.equal(modelCalls, 0);
  assert.equal(runner.activeCount(), 0);
  reopened.close();
  const reopenedAgain = await harness.open();
  assert.deepEqual(await reopenedAgain.eventsAfter(session.id), events);
  assert.deepEqual((await reopenedAgain.load(session.id))?.turns, [recovered]);
});

test('P1 integration: a legacy event-only intent is recovered rather than discarded', async (context) => {
  const harness = await fixture(context);
  const directory = join(harness.root, 'sessions');
  await mkdir(directory, { recursive: true });
  const session: WorkSession = { id: 'legacy', title: 'Interrupted old runner', createdAt: new Date(0).toISOString(), lastActiveAt: new Date(0).toISOString(), standingGrants: [] };
  await writeFile(join(directory, 'legacy.json'), JSON.stringify({ session, turns: [] }));
  await writeFile(join(directory, 'legacy.events.jsonl'), JSON.stringify({ seq: 1, at: session.createdAt, kind: 'session', data: { turn_id: 'lost-turn', phase: 'queued', prompt: 'Acknowledged before the old runner crashed' } }) + '\n');
  const store = await harness.open();
  const recovered = (await store.load(session.id))!.turns[0]!;
  assert.equal(recovered.prompt, 'Acknowledged before the old runner crashed');
  assert.equal(recovered.phase, 'failed');
  assert.match(recovered.error ?? '', /reconciliation required/);
  assert.equal((await store.eventsAfter(session.id)).length, 3);
});

test('P1 integration: failed first-event commit rolls back the intent, cursor and session metadata', async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const session = await store.create('Untitled task');
  const original = structuredClone(session);
  const turn = turnFor(session, 'rolled-back');
  executeFixtureSQL(harness.root, "CREATE TRIGGER reject_events BEFORE INSERT ON events BEGIN SELECT RAISE(ABORT, 'injected event write fault'); END;");
  await assert.rejects(store.beginTurn(session.id, turn), /injected event write fault/);
  assert.deepEqual(durableTurns(harness.root, session.id), []);
  assert.deepEqual(await store.eventsAfter(session.id), []);
  assert.deepEqual((await store.load(session.id))?.session, original);
  executeFixtureSQL(harness.root, 'DROP TRIGGER reject_events;');
  assert.equal((await store.beginTurn(session.id, turn)).seq, 1);
});

test('P1 integration: a failed terminal transaction emits no terminal success and leaves a recoverable intent', { timeout: 5_000 }, async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const session = await store.create('Failed terminal write');
  executeFixtureSQL(harness.root, "CREATE TRIGGER reject_terminal BEFORE INSERT ON events WHEN json_extract(NEW.payload, '$.data.terminal') = 1 BEGIN SELECT RAISE(ABORT, 'injected terminal write fault'); END;");
  const runner = await harness.runner(store);
  const stream = new Stream();
  await runner.attachStream(session.id, stream.response());
  const turn = await runner.start(session, 'Finish only after committing');
  await runner.stopAll();
  assert.equal(stream.events().filter((event) => event.data.terminal === true).length, 0);
  assert.equal(turn.phase, 'failed', 'a failed terminal transaction left the in-memory turn looking successful');
  assert.equal(durableTurns(harness.root, session.id)[0]?.phase === 'done', false);
  assert.equal((await store.eventsAfter(session.id)).filter((event) => event.data.terminal === true).length, 0);
  executeFixtureSQL(harness.root, 'DROP TRIGGER reject_terminal;');
  store.close();
  const reopened = await harness.open();
  assert.equal((await reopened.load(session.id))?.turns[0]?.phase, 'failed');
  assert.equal((await reopened.eventsAfter(session.id)).at(-1)?.data.needs_reconciliation, true);
});

test('P1 integration: replay respects writable backpressure without truncating durable history', async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const session = await store.create('Slow replay');
  for (let index = 0; index < 1_005; index += 1) await store.appendEvent(session.id, { at: session.createdAt, kind: 'token', data: { delta: String(index) } });
  class SlowStream extends Stream {
    private writes = 0;
    override write(frame: string): boolean {
      super.write(frame);
      this.writes += 1;
      if (this.writes % 17 !== 0) return true;
      queueMicrotask(() => this.emit('drain'));
      return false;
    }
  }
  const runner = await harness.runner(store);
  const stream = new SlowStream();
  await runner.attachStream(session.id, stream.response());
  assert.equal(stream.events().length, 1_005);
  assert.equal(stream.events().at(-1)?.data.delta, '1004');
  assert.ok(stream.frames.includes(': attached\n\n'));
});

test('P1 integration: shutdown waits for a starting turn reservation to settle', { timeout: 5_000 }, async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const session = await store.create('Shutdown during history');
  const gate = deferred<Awaited<ReturnType<SessionStore['history']>>>();
  store.history = async () => gate.promise;
  let modelCalls = 0;
  const runner = await harness.runner(store, { async complete() { modelCalls += 1; return reply(); } });
  const starting = runner.start(session, 'Pending startup');
  const refused = assert.rejects(starting, /stopped/);
  let stopped = false;
  const stopping = runner.stopAll().then(() => { stopped = true; });
  await Promise.resolve();
  await Promise.resolve();
  const premature = stopped;
  gate.resolve([]);
  await Promise.all([stopping, refused]);
  assert.equal(premature, false, 'shutdown returned while a reserved start could still touch storage');
  assert.equal(modelCalls, 0);
  assert.equal(runner.activeCount(), 0);
  assert.deepEqual(durableTurns(harness.root, session.id), []);
});

test('P1 integration: rejected session metadata writes cannot leak through the read cache', async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const session = await store.create('Committed title');
  executeFixtureSQL(harness.root, "CREATE TRIGGER reject_metadata BEFORE UPDATE OF payload ON sessions BEGIN SELECT RAISE(ABORT, 'injected metadata fault'); END;");
  await assert.rejects(store.rename(session.id, 'Uncommitted title'), /injected metadata fault/);
  assert.equal((await store.load(session.id))?.session.title, 'Committed title');
  executeFixtureSQL(harness.root, 'DROP TRIGGER reject_metadata;');
  await store.rename(session.id, 'New committed title');
  assert.equal(session.title, 'New committed title');
});

test('P1 integration: malformed turn events roll back the event, projection and cursor together', async (context) => {
  const harness = await fixture(context);
  const store = await harness.open();
  const session = await store.create('Event rollback');
  const turn = turnFor(session, 'validated-turn');
  await store.beginTurn(session.id, turn);
  await assert.rejects(store.appendEvent(session.id, { at: turn.startedAt, kind: 'token', data: { turn_id: turn.id, delta: 1 } }), /invalid token event/i);
  assert.equal((await store.eventsAfter(session.id)).length, 1);
  assert.equal(durableTurns(harness.root, session.id)[0]?.output, '');
  const committed = await store.appendEvent(session.id, { seq: 999, at: turn.startedAt, kind: 'token', data: { turn_id: turn.id, delta: 'committed text' } });
  assert.equal(committed.seq, 2);
  assert.equal(durableTurns(harness.root, session.id)[0]?.output, 'committed text');
});