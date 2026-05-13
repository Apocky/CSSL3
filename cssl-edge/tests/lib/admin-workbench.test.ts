import {
  buildWorkbenchActionCards,
  commandTarget,
  estimateContextLoad,
  extractContextChips,
  isTesseraEvent,
  workbenchSummary,
  type WorkbenchSnapshot,
} from '@/lib/admin-workbench';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

function sampleSnapshot(): WorkbenchSnapshot {
  return {
    health: null,
    tasks: [{
      id: 'task-1234567890',
      title: 'Implement workbench',
      prompt: 'Build it',
      repo_path: 'C:\\repo',
      model_mode: 'deepseek-v4-pro',
      cost_ceiling_usd: 2,
      sensorium_enabled: true,
      playtest_enabled: true,
      status: 'queued',
      created_at: '2026-01-01T00:00:00Z',
      updated_at: '2026-01-01T00:00:00Z',
      leased_by: null,
      metadata: {},
    }],
    runs: [{
      id: 'run-1234567890',
      task_id: 'task-1234567890',
      runner_id: 'runner-local',
      status: 'running',
      model_mode: 'deepseek-v4-pro',
      started_at: '2026-01-01T00:00:00Z',
      finished_at: null,
      summary: null,
      cost_usd_estimate: 0.05,
      metadata: {},
    }],
    approvals: [{
      id: 'approval-1',
      run_id: 'run-1234567890',
      gate: 'git.push',
      status: 'pending',
      requested_at: '2026-01-01T00:00:00Z',
      decided_at: null,
      decided_by: null,
      reason: 'Needs deploy authority',
      payload: {},
    }],
    tools: [{
      name: 'git.push',
      group: 'git',
      description: 'Push branch to origin',
      approval_gate: 'git.push',
    }],
    events: [{
      id: 1,
      run_id: 'run-1234567890',
      ts: '2026-01-01T00:00:00Z',
      level: 'info',
      kind: 'tessera.bridge.ready',
      message: 'Tessera dry-run bridge ready',
      payload: {},
    }],
  };
}

export function testExtractContextChips(): void {
  const chips = extractContextChips('Use #task/123 #pages/admin/chat.tsx @Lazarus @lazarus @Tessera.');
  assert(chips.length === 4, `expected 4 deduped chips, got ${chips.length}`);
  assert(chips[0]?.kind === 'task', 'task token should become task chip');
  assert(chips[1]?.kind === 'file', 'path-like hash token should become file chip');
  assert(chips[2]?.label === '@Lazarus', 'resource alias should preserve Lazarus label');
  assert(chips[3]?.label === '@Tessera', 'resource alias should trim punctuation');
}

export function testCommands(): void {
  assert(commandTarget('/tools') === 'tools', '/tools should open tools');
  assert(commandTarget('/queue Build this') === 'queue-task', '/queue should queue');
  assert(commandTarget('/compact') === 'compact', '/compact should compact');
  assert(commandTarget('/new') === 'new-chat', '/new should start a new chat');
  assert(commandTarget('normal prompt') === null, 'plain prompts should not route as commands');
}

export function testActionCardsAndSummary(): void {
  const snapshot = sampleSnapshot();
  const cards = buildWorkbenchActionCards(snapshot, true);
  assert(cards.some((card) => card.id === 'queue-task' && card.mutates && card.risk === 'medium'), 'queue card should be explicit mutation');
  assert(cards.some((card) => card.target === 'approvals'), 'pending approvals should surface approval card');
  assert(cards.some((card) => card.target === 'tools'), 'tool catalog should surface tool card');
  assert(cards.some((card) => card.label.includes('Tessera')), 'Tessera events should surface trace card');
  assert(workbenchSummary(snapshot) === '1 queued · 1 active · 1 approvals · 0 runners', 'summary should derive counts from snapshot');
  assert(isTesseraEvent(snapshot.events[0]!), 'Tessera event helper should identify bridge events');
}

export function testContextLoad(): void {
  const small = estimateContextLoad([{ text: 'abc' }, { text: 'pending should not count', pending: true }], 'def', 0);
  assert(small.chars === 6, `expected 6 counted chars, got ${small.chars}`);
  assert(small.percent === 0, `expected tiny load to round to 0%, got ${small.percent}`);
  const capped = estimateContextLoad([{ text: 'abc' }], 'def', 150_000);
  assert(capped.percent === 100, `expected capped load at 100%, got ${capped.percent}`);
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  try {
    testExtractContextChips();
    testCommands();
    testActionCardsAndSummary();
    testContextLoad();
    // eslint-disable-next-line no-console
    console.log('admin-workbench.test : OK · 4 tests passed');
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  }
}