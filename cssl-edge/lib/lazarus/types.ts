export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
export type JsonRecord = Record<string, JsonValue>;

export type LazarusTaskStatus =
  | 'queued'
  | 'leased'
  | 'running'
  | 'blocked'
  | 'completed'
  | 'failed'
  | 'cancelled';

export type LazarusRunStatus = 'leased' | 'running' | 'blocked' | 'completed' | 'failed' | 'cancelled';
export type LazarusRunnerStatus = 'online' | 'offline' | 'revoked';
export type LazarusApprovalStatus = 'pending' | 'approved' | 'denied' | 'expired';
export type LazarusEventLevel = 'info' | 'warn' | 'error' | 'debug';

export type LazarusModelMode =
  | 'deepseek-v4-pro'
  | 'deepseek-v4-flash'
  | 'reviewer'
  | 'stub-safe';

export interface LazarusTask {
  id: string;
  title: string;
  prompt: string;
  repo_path: string;
  model_mode: LazarusModelMode;
  cost_ceiling_usd: number;
  sensorium_enabled: boolean;
  playtest_enabled: boolean;
  status: LazarusTaskStatus;
  created_at: string;
  updated_at: string;
  leased_by: string | null;
  metadata: JsonRecord;
}

export interface LazarusRun {
  id: string;
  task_id: string;
  runner_id: string;
  status: LazarusRunStatus;
  model_mode: LazarusModelMode;
  started_at: string;
  finished_at: string | null;
  summary: string | null;
  cost_usd_estimate: number;
  metadata: JsonRecord;
}

export interface LazarusRunner {
  id: string;
  label: string;
  status: LazarusRunnerStatus;
  capabilities: string[];
  current_run_id: string | null;
  last_seen_at: string;
  registered_at: string;
  metadata: JsonRecord;
}

export interface LazarusEvent {
  id: number;
  run_id: string;
  ts: string;
  level: LazarusEventLevel;
  kind: string;
  message: string;
  payload: JsonRecord;
}

export interface LazarusApproval {
  id: string;
  run_id: string;
  gate: LazarusApprovalGate;
  status: LazarusApprovalStatus;
  requested_at: string;
  decided_at: string | null;
  decided_by: string | null;
  reason: string;
  payload: JsonRecord;
}

export interface LazarusArtifact {
  id: string;
  run_id: string;
  kind: 'diff' | 'log' | 'screenshot' | 'trace' | 'report';
  uri: string;
  sha256: string | null;
  created_at: string;
  metadata: JsonRecord;
}

export interface LazarusFleetConfig {
  id: string;
  privacy_class: 'local-only' | 'secret-ok' | 'external-ok';
  default_model_mode: LazarusModelMode;
  max_cost_usd_per_run: number;
  review_required: boolean;
  updated_at: string;
  metadata: JsonRecord;
}

export const LAZARUS_APPROVAL_GATES = [
  'git.push',
  'git.destructive',
  'fs.bulk_delete',
  'network.unknown_egress',
  'cost.overrun',
  'mneme.standing_write',
  'system.driver_or_setting',
  'prime.sigma.capability_sensitive',
  'hardware.mutation',
] as const;

export type LazarusApprovalGate = typeof LAZARUS_APPROVAL_GATES[number];

export interface LazarusToolSpec {
  name: string;
  group: 'build' | 'test' | 'sensorium' | 'vision' | 'input' | 'trace' | 'engine' | 'memory' | 'git';
  description: string;
  approval_gate?: LazarusApprovalGate;
}

export interface LazarusHealth {
  ok: true;
  stub: boolean;
  task_count: number;
  queued_count: number;
  active_run_count: number;
  pending_approval_count: number;
  online_runner_count: number;
  tool_count: number;
}

export interface CreateLazarusTaskInput {
  title: string;
  prompt: string;
  repo_path?: string;
  model_mode?: LazarusModelMode;
  cost_ceiling_usd?: number;
  sensorium_enabled?: boolean;
  playtest_enabled?: boolean;
  metadata?: JsonRecord;
}

export interface RegisterRunnerInput {
  runner_id?: string;
  label?: string;
  capabilities?: string[];
  metadata?: JsonRecord;
}

export interface LeaseResult {
  task: LazarusTask | null;
  run: LazarusRun | null;
  stub: boolean;
}
