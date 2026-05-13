import type { LazarusRun, LazarusTask } from '@/lib/lazarus/types';
import { submitTesseraGoalFromRunner } from '@/lib/tessera/runner-client';

function assert(condition: unknown, message: string): void {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function makeTask(overrides: Partial<LazarusTask> = {}): LazarusTask {
  return {
    id: 'task_runner_bridge_001',
    title: 'Review Tessera runner bridge',
    prompt: 'Review the dry-run Tessera bridge event trace.',
    repo_path: 'C:\\Users\\Apocky\\source\\repos\\CSSLv3',
    model_mode: 'deepseek-v4-pro',
    cost_ceiling_usd: 1,
    sensorium_enabled: false,
    playtest_enabled: false,
    status: 'leased',
    created_at: '2026-05-12T21:00:00.000Z',
    updated_at: '2026-05-12T21:01:00.000Z',
    leased_by: 'runner_tessera',
    metadata: {},
    ...overrides,
  };
}

function makeRun(overrides: Partial<LazarusRun> = {}): LazarusRun {
  return {
    id: 'run_runner_bridge_001',
    task_id: 'task_runner_bridge_001',
    runner_id: 'runner_tessera',
    status: 'running',
    model_mode: 'deepseek-v4-pro',
    started_at: '2026-05-12T21:01:00.000Z',
    finished_at: null,
    summary: null,
    cost_usd_estimate: 0,
    metadata: {},
    ...overrides,
  };
}

export function testRunnerSubmissionForcesDryRun(): void {
  const submission = submitTesseraGoalFromRunner(makeTask(), makeRun(), {
    bridge_enabled: true,
    model_calls_enabled: true,
    force_dry_run: true,
  });

  assert(submission.ok === true, 'submission ok');
  assert(submission.envelope !== null, 'envelope present');
  assert(submission.envelope?.dry_run === true, 'runner client forces dry-run');
  assert(submission.envelope?.tier_policy.allow_cloud === false, 'dry-run blocks cloud tier');
  assert(submission.result.status === 'succeeded', 'dry-run result succeeds');
  assert(submission.result.cost.estimated_usd === 0, 'dry-run cost zero');
  assert(submission.runner_events.some((event) => event.kind === 'tessera.envelope.built'), 'envelope event emitted');
  assert(submission.runner_events.some((event) => event.kind === 'tessera.dry_run.mode'), 'dry-run event emitted');
  assert(submission.runner_events.some((event) => event.kind === 'tessera.cost.accounted'), 'cost event emitted');
}

export function testRunnerSubmissionReportsValidationFailure(): void {
  const submission = submitTesseraGoalFromRunner(makeTask(), makeRun({ task_id: 'mismatched_task' }), {
    bridge_enabled: true,
    model_calls_enabled: false,
  });

  assert(submission.ok === false, 'mismatch reports failure');
  assert(submission.envelope === null, 'failed submission has no envelope');
  assert(submission.result.status === 'failed', 'failed result status');
  assert(submission.result.cost.estimated_usd === 0, 'failed bridge has zero cost');
  assert(submission.runner_events.length === 1, 'one failure event');
  assert(submission.runner_events[0]?.kind === 'tessera.bridge.failed', 'failure event kind');
  assert(submission.runner_events[0]?.level === 'error', 'failure event level');
}

async function runAll(): Promise<void> {
  testRunnerSubmissionForcesDryRun();
  testRunnerSubmissionReportsValidationFailure();
  // eslint-disable-next-line no-console
  console.log('tessera-runner.test : OK · 2 tests passed');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  void runAll().catch((err) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}