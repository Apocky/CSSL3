import type { LazarusRun, LazarusTask } from '@/lib/lazarus/types';
import {
  buildDryRunTesseraResult,
  buildTesseraGoalEnvelope,
  isTesseraBridgeEnabled,
  parseBooleanFlag,
  validateTesseraGoalEnvelope,
} from '@/lib/tessera/bridge';

function assert(condition: unknown, message: string): void {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function assertThrows(action: () => unknown, message: string): void {
  try {
    action();
  } catch {
    return;
  }
  throw new Error(`assert failed: ${message}`);
}

function makeTask(overrides: Partial<LazarusTask> = {}): LazarusTask {
  return {
    id: 'task_bridge_001',
    title: 'Implement Tessera bridge dry-run',
    prompt: 'Implement a dry-run bridge between Lazarus and Tessera without model calls.',
    repo_path: 'C:\\Users\\Apocky\\source\\repos\\CSSLv3',
    model_mode: 'stub-safe',
    cost_ceiling_usd: 0,
    sensorium_enabled: false,
    playtest_enabled: false,
    status: 'leased',
    created_at: '2026-05-12T20:00:00.000Z',
    updated_at: '2026-05-12T20:01:00.000Z',
    leased_by: 'runner_bridge',
    metadata: { lane: 'bridge-contract' },
    ...overrides,
  };
}

function makeRun(overrides: Partial<LazarusRun> = {}): LazarusRun {
  return {
    id: 'run_bridge_001',
    task_id: 'task_bridge_001',
    runner_id: 'runner_bridge',
    status: 'running',
    model_mode: 'stub-safe',
    started_at: '2026-05-12T20:01:00.000Z',
    finished_at: null,
    summary: null,
    cost_usd_estimate: 0,
    metadata: { trace: 'bridge' },
    ...overrides,
  };
}

export function testBridgeFlagParsing(): void {
  assert(parseBooleanFlag('1') === true, '1 enables flag');
  assert(parseBooleanFlag('TRUE') === true, 'TRUE enables flag');
  assert(parseBooleanFlag('off') === false, 'off disables flag');
  assert(isTesseraBridgeEnabled({ LAZARUS_TESSERA_BRIDGE: 'yes' }) === true, 'env yes enables bridge');
  assert(isTesseraBridgeEnabled({}) === false, 'missing env disables bridge');
}

export function testDryRunEnvelopeByDefault(): void {
  const envelope = buildTesseraGoalEnvelope(makeTask(), makeRun(), {
    bridge_enabled: false,
    model_calls_enabled: false,
  });

  assert(envelope.schema_version === 'tessera.goal.v1', 'schema version set');
  assert(envelope.lazarus_task_id === 'task_bridge_001', 'task id preserved');
  assert(envelope.lazarus_run_id === 'run_bridge_001', 'run id preserved');
  assert(envelope.trace_id === 'tessera_task_bridge_001_run_bridge_001', 'trace id deterministic');
  assert(envelope.dry_run === true, 'bridge defaults to dry-run');
  assert(envelope.role === 'coder', 'role inferred from prompt');
  assert(envelope.tier_policy.allowed_tiers.length === 1, 'only one tier in stub mode');
  assert(envelope.tier_policy.allowed_tiers[0] === 'T1', 'stub mode uses T1 only');
  assert(envelope.budget.max_cost_usd === 0, 'zero cost preserved');
  assert(envelope.approval_policy.required_gates.includes('git.push'), 'git push approval gate present');
  assert(envelope.artifact_policy.allowed_kinds.includes('trace'), 'trace artifact allowed');
}

export function testLiveCloudRequiresExplicitPrivacy(): void {
  const cloudTask = makeTask({ model_mode: 'deepseek-v4-pro', cost_ceiling_usd: 1 });
  const cloudRun = makeRun({ model_mode: 'deepseek-v4-pro' });

  assertThrows(() => buildTesseraGoalEnvelope(cloudTask, cloudRun, {
    bridge_enabled: true,
    model_calls_enabled: true,
  }), 'live cloud bridge requires external-ok privacy');

  const envelope = buildTesseraGoalEnvelope(cloudTask, cloudRun, {
    bridge_enabled: true,
    model_calls_enabled: true,
    fleet: {
      privacy_class: 'external-ok',
      max_cost_usd_per_run: 0.25,
      review_required: true,
    },
  });

  assert(envelope.dry_run === false, 'explicit live bridge can leave dry-run');
  assert(envelope.tier_policy.preferred_tier === 'T3', 'DeepSeek Pro maps to T3');
  assert(envelope.tier_policy.allow_cloud === true, 'cloud tier allowed');
  assert(envelope.budget.max_cost_usd === 0.25, 'fleet cost cap clamps task ceiling');
}

export function testEnvelopeValidationGuards(): void {
  assertThrows(() => buildTesseraGoalEnvelope(makeTask(), makeRun({ task_id: 'other_task' })), 'task/run mismatch rejected');
  assertThrows(() => buildTesseraGoalEnvelope(makeTask(), makeRun(), { max_depth: 4 }), 'depth > 3 rejected');

  const envelope = buildTesseraGoalEnvelope(makeTask(), makeRun());
  assert(validateTesseraGoalEnvelope(envelope) === envelope, 'valid envelope returns itself');
  assertThrows(() => validateTesseraGoalEnvelope({ ...envelope, lazarus_task_id: '' }), 'empty task id rejected');
}

export function testDryRunResultHasNoSideEffects(): void {
  const envelope = buildTesseraGoalEnvelope(makeTask(), makeRun());
  const result = buildDryRunTesseraResult(envelope);

  assert(result.schema_version === 'tessera.result.v1', 'result schema set');
  assert(result.status === 'succeeded', 'dry-run succeeds');
  assert(result.confidence === 1, 'dry-run confidence exact');
  assert(result.cost.estimated_usd === 0, 'dry-run has zero cost');
  assert(result.artifacts.length === 0, 'dry-run writes no artifacts');
  assert(result.approvals_requested.length === 0, 'dry-run requests no approvals');
  assert(result.events.some((event) => event.kind === 'goal.accepted'), 'goal accepted event present');
  assert(result.events.some((event) => event.kind === 'goal.completed'), 'goal completed event present');
}

async function runAll(): Promise<void> {
  testBridgeFlagParsing();
  testDryRunEnvelopeByDefault();
  testLiveCloudRequiresExplicitPrivacy();
  testEnvelopeValidationGuards();
  testDryRunResultHasNoSideEffects();
  // eslint-disable-next-line no-console
  console.log('tessera-bridge.test : OK · 5 tests passed');
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