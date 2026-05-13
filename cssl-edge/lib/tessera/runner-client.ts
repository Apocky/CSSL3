import type { LazarusRun, LazarusTask } from '@/lib/lazarus/types';
import {
  buildDryRunTesseraResult,
  buildTesseraGoalEnvelope,
} from '@/lib/tessera/bridge';
import type {
  CreateTesseraEnvelopeOptions,
  TesseraGoalEnvelope,
  TesseraResult,
} from '@/lib/tessera/types';

export interface TesseraRunnerEvent {
  kind: string;
  message: string;
  level: 'info' | 'warn' | 'error';
}

export interface TesseraRunnerSubmission {
  ok: boolean;
  envelope: TesseraGoalEnvelope | null;
  result: TesseraResult;
  runner_events: TesseraRunnerEvent[];
}

export interface SubmitTesseraGoalFromRunnerOptions extends CreateTesseraEnvelopeOptions {
  force_dry_run?: boolean;
}

export function submitTesseraGoalFromRunner(
  task: LazarusTask,
  run: LazarusRun,
  options: SubmitTesseraGoalFromRunnerOptions = {},
): TesseraRunnerSubmission {
  try {
    const forceDryRun = options.force_dry_run ?? true;
    const envelope = buildTesseraGoalEnvelope(task, run, {
      ...options,
      model_calls_enabled: forceDryRun ? false : options.model_calls_enabled,
    });
    const result = buildDryRunTesseraResult(envelope);
    const runnerEvents = buildSuccessRunnerEvents(envelope, result);
    return { ok: true, envelope, result, runner_events: runnerEvents };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      ok: false,
      envelope: null,
      result: buildFailedRunnerResult(task, run, message),
      runner_events: [
        {
          kind: 'tessera.bridge.failed',
          message,
          level: 'error',
        },
      ],
    };
  }
}

function buildSuccessRunnerEvents(envelope: TesseraGoalEnvelope, result: TesseraResult): TesseraRunnerEvent[] {
  const events: TesseraRunnerEvent[] = [
    {
      kind: 'tessera.envelope.built',
      message: `role=${envelope.role}; tier=${envelope.tier_policy.preferred_tier}; max_cost_usd=${envelope.budget.max_cost_usd}`,
      level: 'info',
    },
    {
      kind: 'tessera.submission.started',
      message: `trace_id=${envelope.trace_id}; dry_run=${envelope.dry_run}`,
      level: 'info',
    },
  ];

  if (envelope.dry_run) {
    events.push({
      kind: 'tessera.dry_run.mode',
      message: `model_calls_enabled=${envelope.tier_policy.model_calls_enabled}; cost_ceiling_usd=${envelope.budget.max_cost_usd}`,
      level: 'info',
    });
  }

  events.push(
    {
      kind: 'tessera.result.received',
      message: `status=${result.status}; cost_usd=${result.cost.estimated_usd}; events=${result.events.length}`,
      level: result.status === 'succeeded' ? 'info' : 'warn',
    },
    {
      kind: 'tessera.cost.accounted',
      message: `estimated_usd=${result.cost.estimated_usd}; tokens_in=${result.cost.tokens_in}; tokens_out=${result.cost.tokens_out}`,
      level: 'info',
    },
  );

  return events;
}

function buildFailedRunnerResult(task: LazarusTask, run: LazarusRun, message: string): TesseraResult {
  const traceId = `tessera_${task.id}_${run.id}`.replace(/[^a-zA-Z0-9_-]/g, '_');
  return {
    schema_version: 'tessera.result.v1',
    status: 'failed',
    summary: `Tessera bridge failed before execution: ${message}`,
    confidence: 0,
    cost: {
      estimated_usd: 0,
      tokens_in: 0,
      tokens_out: 0,
    },
    events: [
      {
        kind: 'goal.failed',
        message,
        payload: { trace_id: traceId },
      },
    ],
    artifacts: [],
    approvals_requested: [],
    provenance: [traceId],
    next_goals: [],
    metadata: {
      lazarus_task_id: task.id,
      lazarus_run_id: run.id,
      bridge: 'failed',
    },
  };
}