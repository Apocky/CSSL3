// Layer 2 — the agent loop, driven by a scripted engine instead of a GPU.
//
// Ten scenarios crossing four axes:
//   engine behaviour : tool call / final answer / empty completion / unbounded tool calls
//   consent decision : auto-approved / allow / allow_session / deny
//   tool outcome     : success / tool error / policy refusal / confinement refusal
//   turn lifecycle   : completes / exhausts its budget / is cancelled
//
// The point of scripting the engine is that every one of these becomes deterministic and runs in
// milliseconds. A suite that can only be exercised by asking a 27 GB model nicely is a suite that
// gets run once. This one can run on every commit, which is the difference between a test and a
// ceremony. Layer 3 (a real model, real task, quality oracle) sits on top of this, not instead.

import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { WorkAgent } from '../../scripts/apocrypha-work/agent';
import { loadWorkConfig } from '../../scripts/apocrypha-work/config';
import type { EngineLike, EngineMessage, EngineReply } from '../../scripts/apocrypha-work/engine';
import type { ConsentDecision, ConsentRequest, ToolDefinition, WorkConfig, WorkEvent, WorkSession, WorkTurn } from '../../scripts/apocrypha-work/types';
import { Workspace } from '../../scripts/apocrypha-work/workspace';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

function reply(partial: Partial<EngineReply>): EngineReply {
  return { content: '', toolCalls: [], finishReason: 'stop', usage: {}, ...partial };
}

class ScriptedEngine implements EngineLike {
  readonly seen: string[] = [];
  private readonly script: EngineReply[];
  private readonly repeatLast: boolean;

  constructor(script: EngineReply[], repeatLast = false) {
    this.script = script;
    this.repeatLast = repeatLast;
  }

  async complete(
    messages: readonly EngineMessage[],
    _tools: readonly ToolDefinition[],
    onToken: (delta: string) => void,
    _signal?: AbortSignal,
  ): Promise<EngineReply> {
    const last = messages[messages.length - 1];
    if (last) this.seen.push(`${last.role}:${last.content.slice(0, 600)}`);
    const next = this.script.shift() ?? (this.repeatLast ? this.script[0] : undefined);
    const result = next ?? reply({ content: 'nothing left in the script' });
    if (result.content) onToken(result.content);
    return result;
  }
}

interface RunOutcome {
  readonly turn: WorkTurn;
  readonly events: WorkEvent[];
  readonly prompts: ConsentRequest[];
}

interface Harness {
  readonly config: WorkConfig;
  readonly workspace: Workspace;
  readonly root: string;
}

async function harness(root: string, overrides: Record<string, string> = {}): Promise<Harness> {
  const config = loadWorkConfig({
    APOCRYPHA_WORK_ROOTS: `sandbox=${root}`,
    APOCRYPHA_WORK_STATE_DIR: root,
    APOCRYPHA_WORK_TOKEN: 'x'.repeat(32),
    APOCRYPHA_WORK_MAX_ITERATIONS: '6',
    APOCRYPHA_WORK_TOOL_TIMEOUT_MS: '10000',
    ...overrides,
  });
  return { config, workspace: await Workspace.open(config.roots), root };
}

async function drive(
  h: Harness,
  engine: EngineLike,
  prompt: string,
  decide: (request: ConsentRequest) => ConsentDecision,
  signal = new AbortController().signal,
): Promise<RunOutcome> {
  const agent = new WorkAgent(h.config, h.workspace, engine);
  const session: WorkSession = {
    id: 's1', title: 't', createdAt: new Date(0).toISOString(), lastActiveAt: new Date(0).toISOString(), standingGrants: [],
  };
  const turn: WorkTurn = {
    id: 'turn1', sessionId: 's1', prompt, phase: 'queued', startedAt: new Date(0).toISOString(), toolCalls: [], output: '',
  };
  const events: WorkEvent[] = [];
  const prompts: ConsentRequest[] = [];
  let seq = 0;
  await agent.run(
    session,
    turn,
    [],
    (event) => { seq += 1; events.push({ seq, at: new Date(0).toISOString(), ...event }); },
    async (request) => { prompts.push(request); return decide(request); },
    signal,
  );
  (turn as WorkTurn & { grants?: string[] }).grants = session.standingGrants;
  return { turn, events, prompts };
}

const kinds = (outcome: RunOutcome): string[] => outcome.events.map((event) => event.kind);

async function main(): Promise<void> {
  const base = await mkdtemp(join(tmpdir(), 'work-loop-'));
  const h = await harness(base);
  const target = join(base, 'a.ts');
  const reset = (): Promise<void> => writeFile(target, 'export const x = 1;\n', 'utf8');
  await reset();

  // 1 — happy path. Read is auto-approved, the edit asks, the answer lands, the file changes.
  {
    const engine = new ScriptedEngine([
      reply({ toolCalls: [{ id: 'c1', name: 'read_file', args: { path: 'a.ts' } }] }),
      reply({ toolCalls: [{ id: 'c2', name: 'edit_file', args: { path: 'a.ts', old_text: 'x = 1', new_text: 'x = 2' } }] }),
      reply({ content: 'Changed x to 2 in a.ts.', finishReason: 'stop', usage: { totalTokens: 120 } }),
    ]);
    const out = await drive(h, engine, 'set x to 2', () => 'allow');
    assert(out.turn.phase === 'done', `happy path ended ${out.turn.phase}: ${out.turn.error ?? ''}`);
    assert(out.prompts.length === 1 && out.prompts[0]?.tool === 'edit_file', `expected 1 consent prompt, got ${out.prompts.length}`);
    assert((await readFile(target, 'utf8')).includes('x = 2'), 'the approved edit did not reach disk');
    const edit = out.turn.toolCalls.find((call) => call.name === 'edit_file');
    assert(edit?.diff?.added === 1 && edit.diff.removed === 1, 'the edit produced no usable diff for the consent card');
    assert(kinds(out).includes('consent_request') && kinds(out).includes('consent_resolved'), 'consent events were not emitted');
  }

  // 2 — refusal. The file must be untouched and the model must be told, not thrown at.
  {
    await reset();
    const engine = new ScriptedEngine([
      reply({ toolCalls: [{ id: 'c1', name: 'edit_file', args: { path: 'a.ts', old_text: 'x = 1', new_text: 'x = 9' } }] }),
      reply({ content: 'Understood — I left a.ts alone.', finishReason: 'stop' }),
    ]);
    const out = await drive(h, engine, 'set x to 9', () => 'deny');
    assert(out.turn.phase === 'done', `denied run ended ${out.turn.phase}`);
    assert((await readFile(target, 'utf8')).includes('x = 1'), 'a DENIED edit reached disk');
    const denied = out.turn.toolCalls[0];
    assert(denied?.denied === true && denied.ok === false, 'denial was not recorded on the tool outcome');
    assert(engine.seen.some((entry) => entry.startsWith('tool:ERROR') && entry.includes('declined')),
      'the refusal was not fed back to the model as a tool result');
  }

  // 3 — allow_session grants the TOOL for the session: one prompt, two edits.
  {
    await reset();
    const engine = new ScriptedEngine([
      reply({ toolCalls: [{ id: 'c1', name: 'edit_file', args: { path: 'a.ts', old_text: 'x = 1', new_text: 'x = 2' } }] }),
      reply({ toolCalls: [{ id: 'c2', name: 'edit_file', args: { path: 'a.ts', old_text: 'x = 2', new_text: 'x = 3' } }] }),
      reply({ content: 'Applied both edits.', finishReason: 'stop' }),
    ]);
    const out = await drive(h, engine, 'bump twice', () => 'allow_session');
    assert(out.prompts.length === 1, `allow_session asked ${out.prompts.length} times; it must ask once`);
    assert((await readFile(target, 'utf8')).includes('x = 3'), 'the second granted edit did not apply');
  }

  // 4 — reads never prompt under the default policy.
  {
    const engine = new ScriptedEngine([
      reply({ toolCalls: [{ id: 'c1', name: 'list_dir', args: { path: '.' } }] }),
      reply({ toolCalls: [{ id: 'c2', name: 'search', args: { pattern: 'export' } }] }),
      reply({ content: 'Looked around.', finishReason: 'stop' }),
    ]);
    const out = await drive(h, engine, 'look around', () => { throw new Error('a read asked for consent'); });
    assert(out.prompts.length === 0, 'reads prompted for consent');
    assert(out.turn.toolCalls.every((call) => call.ok), 'a read tool failed');
  }

  // 5 — NEGATIVE CONTROL for the empty-completion guard. Silence must not read as success.
  {
    const engine = new ScriptedEngine([reply({ content: '', finishReason: 'length', usage: { completionTokens: 1024 } })]);
    const out = await drive(h, engine, 'say nothing', () => 'allow');
    assert(out.turn.phase === 'failed', `an empty completion was reported as ${out.turn.phase}`);
    assert(/output budget/.test(out.turn.error ?? ''), `unhelpful empty-completion message: ${out.turn.error}`);
    assert(out.events.some((event) => event.kind === 'error' && event.data.code === 'EMPTY_COMPLETION'), 'no EMPTY_COMPLETION event');
  }

  // 5b — the same guard must NOT fire on a legitimate short answer with no tool calls.
  {
    const engine = new ScriptedEngine([reply({ content: 'Nothing to change.', finishReason: 'stop' })]);
    const out = await drive(h, engine, 'check', () => 'allow');
    assert(out.turn.phase === 'done', 'a valid no-tool answer was flagged as an empty completion');
  }

  // 6 — a model that never stops is stopped by the budget, and says so.
  {
    const loop = reply({ toolCalls: [{ id: 'c', name: 'list_dir', args: { path: '.' } }] });
    const engine = new ScriptedEngine([loop, loop, loop, loop, loop, loop, loop, loop], true);
    const out = await drive(h, engine, 'spin', () => 'allow');
    assert(out.turn.phase === 'failed', `runaway loop ended ${out.turn.phase}`);
    assert(/6-step budget/.test(out.turn.error ?? ''), `budget message was: ${out.turn.error}`);
    assert(out.turn.toolCalls.length === 6, `budget let ${out.turn.toolCalls.length} steps through, expected 6`);
  }

  // 7 — a tool error is data for the model, not an exception for the turn.
  {
    const engine = new ScriptedEngine([
      reply({ toolCalls: [{ id: 'c1', name: 'read_file', args: { path: 'nope.ts' } }] }),
      reply({ content: 'That file does not exist.', finishReason: 'stop' }),
    ]);
    const out = await drive(h, engine, 'read a missing file', () => 'allow');
    assert(out.turn.phase === 'done', `tool error ended the turn as ${out.turn.phase}`);
    assert(out.turn.toolCalls[0]?.ok === false, 'a failing tool reported success');
    assert(engine.seen.some((entry) => entry.startsWith('tool:ERROR')), 'the tool error never reached the model');
  }

  // 8 — a catastrophic command is refused WITHOUT being offered for approval.
  {
    const engine = new ScriptedEngine([
      reply({ toolCalls: [{ id: 'c1', name: 'run_command', args: { command: 'rm -rf /' } }] }),
      reply({ content: 'I will not do that.', finishReason: 'stop' }),
    ]);
    const out = await drive(h, engine, 'delete everything', () => {
      throw new Error('a standing-deny command was put in front of the operator for approval');
    });
    assert(out.prompts.length === 0, 'a denied command was offered for consent');
    assert(out.turn.toolCalls[0]?.denied === true, 'the blocked command was not marked denied');
    assert(/standing deny rule/.test(out.turn.toolCalls[0]?.error ?? ''), 'the block did not cite a rule');
  }

  // 9 — confinement holds inside the loop, not just in unit tests.
  {
    const engine = new ScriptedEngine([
      reply({ toolCalls: [{ id: 'c1', name: 'write_file', args: { path: join(base, '..', 'escaped.txt'), content: 'out' } }] }),
      reply({ content: 'Refused.', finishReason: 'stop' }),
    ]);
    const out = await drive(h, engine, 'escape', () => 'allow');
    assert(out.turn.toolCalls[0]?.ok === false, 'a write outside the workspace succeeded');
    assert(/outside every workspace root/.test(out.turn.toolCalls[0]?.error ?? ''), `unexpected escape error: ${out.turn.toolCalls[0]?.error}`);
  }

  // 10 — cancellation stops the turn at the next boundary.
  {
    const controller = new AbortController();
    const engine = new ScriptedEngine([
      reply({ toolCalls: [{ id: 'c1', name: 'list_dir', args: { path: '.' } }] }),
      reply({ toolCalls: [{ id: 'c2', name: 'list_dir', args: { path: '.' } }] }),
      reply({ content: 'should never be reached', finishReason: 'stop' }),
    ]);
    const outPromise = drive(h, engine, 'cancel me', () => { controller.abort(); return 'allow'; }, controller.signal);
    controller.abort();
    const out = await outPromise;
    assert(out.turn.phase === 'cancelled', `cancelled turn ended as ${out.turn.phase}`);
  }

  // 11 — auto-approving writes removes the prompt but not the confinement.
  {
    await reset();
    const permissive = await harness(base, { APOCRYPHA_WORK_AUTO_APPROVE: 'read,write' });
    const engine = new ScriptedEngine([
      reply({ toolCalls: [{ id: 'c1', name: 'edit_file', args: { path: 'a.ts', old_text: 'x = 1', new_text: 'x = 7' } }] }),
      reply({ content: 'Done.', finishReason: 'stop' }),
    ]);
    const out = await drive(permissive, engine, 'bump', () => { throw new Error('write prompted despite auto-approve'); });
    assert(out.prompts.length === 0, 'auto-approved write still prompted');
    assert((await readFile(target, 'utf8')).includes('x = 7'), 'auto-approved write did not apply');
  }

  // 12 — shell:off removes run_command from the advertised tool set entirely.
  {
    const noShell = await harness(base, { APOCRYPHA_WORK_SHELL: 'off' });
    const agent = new WorkAgent(noShell.config, noShell.workspace, new ScriptedEngine([]));
    assert(!agent.tools.some((tool) => tool.name === 'run_command'), 'run_command was advertised with shell disabled');
    assert(agent.tools.length >= 5, 'disabling shell also removed the file tools');
  }

  // 13 — a refused tool is WITHDRAWN, not merely discouraged.
  //
  // Layer 3 measured the real model making four write attempts after an explicit refusal, so the
  // limit is enforced mechanically. This pins it: after two denials the tool must vanish from the
  // set offered to the engine, and the model must be told in the tool result.
  {
    await reset();
    const attempt = reply({ toolCalls: [{ id: 'c', name: 'edit_file', args: { path: 'a.ts', old_text: 'x = 1', new_text: 'x = 4' } }] });
    const engine = new ScriptedEngine([attempt, attempt, attempt, attempt, attempt], true);
    const offeredPerCall: string[][] = [];
    const spy: EngineLike = {
      complete: (messages, tools, onToken, signal) => {
        offeredPerCall.push(tools.map((tool) => tool.name));
        return engine.complete(messages, tools, onToken, signal);
      },
    };
    const out = await drive(h, spy, 'keep trying', () => 'deny');

    const firstOffer = offeredPerCall[0] ?? [];
    const lastOffer = offeredPerCall[offeredPerCall.length - 1] ?? [];
    assert(firstOffer.includes('edit_file'), 'edit_file was not offered on the first call');
    assert(!lastOffer.includes('edit_file'), `edit_file was still offered after repeated denial: ${lastOffer.join(',')}`);
    assert(lastOffer.includes('read_file'), 'withdrawing one tool removed the others too');

    const prompted = out.prompts.filter((request) => request.tool === 'edit_file');
    assert(prompted.length === 2, `the operator was asked about edit_file ${prompted.length} times; the limit is 2`);
    const shortCircuited = out.turn.toolCalls.filter((call) => call.name === 'edit_file' && /withdrawn/.test(call.error ?? ''));
    assert(shortCircuited.length > 0, 'later edit_file calls were not short-circuited by the withdrawal');
    assert((await readFile(target, 'utf8')).includes('x = 1'), 'a denied edit reached disk');
    assert(engine.seen.some((entry) => entry.includes('WITHDRAWN')), 'the model was never told the tool was withdrawn');
  }

  await rm(base, { recursive: true, force: true });
  console.log('loop: 14 scenarios across engine-behaviour x consent x tool-outcome x lifecycle');
}

main().then(() => console.log('work/loop OK')).catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
