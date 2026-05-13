import type { LazarusRun, LazarusTask } from '@/lib/lazarus/types';
import type {
  CreateTesseraEnvelopeOptions,
  TesseraArtifactPolicy,
  TesseraGoalEnvelope,
  TesseraResult,
  TesseraTierPolicy,
} from '@/lib/tessera/types';

const MAX_DEPTH = 3;
const DEFAULT_TIMEOUT_MS = 120_000;
const DEFAULT_MAX_TOKENS = 8_000;
const DEFAULT_COST_CEILING_USD = 0;

type EnvRecord = Record<string, string | undefined>;

export function isTesseraBridgeEnabled(env: EnvRecord = process.env): boolean {
  return parseBooleanFlag(env.LAZARUS_TESSERA_BRIDGE);
}

export function isTesseraModelCallsEnabled(env: EnvRecord = process.env): boolean {
  return parseBooleanFlag(env.LAZARUS_ENABLE_MODEL_CALLS);
}

export function parseBooleanFlag(value: string | undefined): boolean {
  if (!value) return false;
  return ['1', 'true', 'yes', 'on'].includes(value.trim().toLowerCase());
}

export function buildTesseraGoalEnvelope(
  task: LazarusTask,
  run: LazarusRun,
  options: CreateTesseraEnvelopeOptions = {},
): TesseraGoalEnvelope {
  if (run.task_id !== task.id) {
    throw new Error(`Tessera bridge task/run mismatch: ${task.id} !== ${run.task_id}`);
  }

  const bridgeEnabled = options.bridge_enabled ?? isTesseraBridgeEnabled();
  const modelCallsEnabled = options.model_calls_enabled ?? isTesseraModelCallsEnabled();
  const maxDepth = options.max_depth ?? MAX_DEPTH;
  const fleetMaxCost = options.fleet?.max_cost_usd_per_run ?? task.cost_ceiling_usd ?? DEFAULT_COST_CEILING_USD;
  const maxCostUsd = Math.max(0, Math.min(task.cost_ceiling_usd ?? fleetMaxCost, fleetMaxCost));

  const envelope: TesseraGoalEnvelope = {
    schema_version: 'tessera.goal.v1',
    lazarus_task_id: task.id,
    lazarus_run_id: run.id,
    trace_id: options.trace_id ?? buildTesseraTraceId(task.id, run.id),
    goal_text: task.prompt,
    goal_hv_hint: options.goal_hv_hint ?? null,
    role: options.role ?? inferTesseraRole(task),
    tier_policy: buildTierPolicy(task.model_mode, modelCallsEnabled),
    budget: {
      max_cost_usd: maxCostUsd,
      max_tokens: options.max_tokens ?? DEFAULT_MAX_TOKENS,
      timeout_ms: options.deadline_ms ?? DEFAULT_TIMEOUT_MS,
    },
    privacy_class: options.fleet?.privacy_class ?? 'local-only',
    approval_policy: {
      required_gates: [
        'git.push',
        'git.destructive',
        'fs.bulk_delete',
        'network.unknown_egress',
        'cost.overrun',
        'mneme.standing_write',
        'prime.sigma.capability_sensitive',
        'hardware.mutation',
      ],
      require_human_for_external_effects: true,
      review_required: options.fleet?.review_required ?? true,
    },
    artifact_policy: buildArtifactPolicy(),
    max_depth: maxDepth,
    deadline_ms: options.deadline_ms ?? DEFAULT_TIMEOUT_MS,
    dry_run: !bridgeEnabled || !modelCallsEnabled,
    metadata: {
      ...(options.metadata ?? {}),
      lazarus_task_title: task.title,
      lazarus_repo_path: task.repo_path,
      lazarus_runner_id: run.runner_id,
      source: 'lazarus.tessera.bridge',
    },
  };

  validateTesseraGoalEnvelope(envelope);
  return envelope;
}

export function buildDryRunTesseraResult(envelope: TesseraGoalEnvelope): TesseraResult {
  validateTesseraGoalEnvelope(envelope);

  return {
    schema_version: 'tessera.result.v1',
    status: 'succeeded',
    summary: `Tessera bridge dry-run accepted Lazarus task ${envelope.lazarus_task_id}; no model calls or external effects were executed.`,
    confidence: 1,
    cost: {
      estimated_usd: 0,
      tokens_in: 0,
      tokens_out: 0,
    },
    events: [
      {
        kind: 'goal.accepted',
        message: 'Tessera goal envelope accepted in dry-run mode.',
        payload: { trace_id: envelope.trace_id },
      },
      {
        kind: 'goal.completed',
        message: 'Dry-run completed without side effects.',
        payload: { dry_run: true },
      },
    ],
    artifacts: [],
    approvals_requested: [],
    provenance: [envelope.trace_id],
    next_goals: [],
    metadata: {
      lazarus_task_id: envelope.lazarus_task_id,
      lazarus_run_id: envelope.lazarus_run_id,
      bridge: 'dry-run',
    },
  };
}

export function validateTesseraGoalEnvelope(envelope: TesseraGoalEnvelope): TesseraGoalEnvelope {
  assertString(envelope.schema_version, 'schema_version');
  if (envelope.schema_version !== 'tessera.goal.v1') {
    throw new Error(`Unsupported Tessera goal schema: ${envelope.schema_version}`);
  }
  assertString(envelope.lazarus_task_id, 'lazarus_task_id');
  assertString(envelope.lazarus_run_id, 'lazarus_run_id');
  assertString(envelope.trace_id, 'trace_id');
  assertString(envelope.goal_text, 'goal_text');
  if (envelope.max_depth < 0 || envelope.max_depth > MAX_DEPTH) {
    throw new Error(`Tessera max_depth must be 0..${MAX_DEPTH}`);
  }
  if (envelope.deadline_ms <= 0 || envelope.budget.timeout_ms <= 0) {
    throw new Error('Tessera deadline and timeout must be positive');
  }
  if (envelope.budget.max_cost_usd < 0 || envelope.budget.max_tokens < 0) {
    throw new Error('Tessera budget cannot be negative');
  }
  if (!envelope.dry_run && !envelope.tier_policy.model_calls_enabled) {
    throw new Error('Tessera live bridge requires model_calls_enabled');
  }
  if (!envelope.dry_run && envelope.tier_policy.allow_cloud && envelope.privacy_class !== 'external-ok') {
    throw new Error('Tessera cloud tier requires external-ok privacy class');
  }
  return envelope;
}

export function buildTesseraTraceId(taskId: string, runId: string): string {
  return `tessera_${taskId}_${runId}`.replace(/[^a-zA-Z0-9_-]/g, '_');
}

function buildTierPolicy(modelMode: LazarusTask['model_mode'], modelCallsEnabled: boolean): TesseraTierPolicy {
  if (!modelCallsEnabled || modelMode === 'stub-safe' || modelMode === 'reviewer') {
    return {
      preferred_tier: 'T1',
      allowed_tiers: ['T1'],
      allow_cloud: false,
      source_model_mode: modelMode,
      model_calls_enabled: false,
    };
  }

  if (modelMode === 'deepseek-v4-pro') {
    return {
      preferred_tier: 'T3',
      allowed_tiers: ['T1', 'T3'],
      allow_cloud: true,
      source_model_mode: modelMode,
      model_calls_enabled: true,
    };
  }

  return {
    preferred_tier: 'T2',
    allowed_tiers: ['T1', 'T2'],
    allow_cloud: false,
    source_model_mode: modelMode,
    model_calls_enabled: true,
  };
}

function inferTesseraRole(task: LazarusTask): TesseraGoalEnvelope['role'] {
  const text = `${task.title}\n${task.prompt}`.toLowerCase();
  if (text.includes('review') || task.model_mode === 'reviewer') return 'critic';
  if (text.includes('code') || text.includes('implement') || text.includes('fix')) return 'coder';
  if (text.includes('plan') || text.includes('roadmap')) return 'planner';
  if (text.includes('summarize') || text.includes('summary')) return 'summarizer';
  return 'reasoner';
}

function buildArtifactPolicy(): TesseraArtifactPolicy {
  return {
    allow_artifacts: true,
    allowed_kinds: ['diff', 'log', 'trace', 'report'],
    max_artifacts: 16,
  };
}

function assertString(value: unknown, field: string): asserts value is string {
  if (typeof value !== 'string' || value.trim() === '') {
    throw new Error(`Tessera envelope requires ${field}`);
  }
}