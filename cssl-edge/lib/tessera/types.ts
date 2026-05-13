import type {
  JsonRecord,
  LazarusApprovalGate,
  LazarusArtifact,
  LazarusFleetConfig,
  LazarusModelMode,
} from '@/lib/lazarus/types';

export type TesseraSubmindRole = 'reasoner' | 'critic' | 'coder' | 'planner' | 'summarizer' | 'recaller';
export type TesseraTier = 'T1' | 'T2' | 'T3';
export type TesseraPrivacyClass = LazarusFleetConfig['privacy_class'];
export type TesseraStatus = 'succeeded' | 'waiting_approval' | 'failed' | 'blocked';

export interface TesseraTierPolicy {
  preferred_tier: TesseraTier;
  allowed_tiers: TesseraTier[];
  allow_cloud: boolean;
  source_model_mode: LazarusModelMode;
  model_calls_enabled: boolean;
}

export interface TesseraBudget {
  max_cost_usd: number;
  max_tokens: number;
  timeout_ms: number;
}

export interface TesseraApprovalPolicy {
  required_gates: LazarusApprovalGate[];
  require_human_for_external_effects: boolean;
  review_required: boolean;
}

export interface TesseraArtifactPolicy {
  allow_artifacts: boolean;
  allowed_kinds: Array<LazarusArtifact['kind']>;
  max_artifacts: number;
}

export interface TesseraGoalEnvelope {
  schema_version: 'tessera.goal.v1';
  lazarus_task_id: string;
  lazarus_run_id: string;
  trace_id: string;
  goal_text: string;
  goal_hv_hint: string | null;
  role: TesseraSubmindRole;
  tier_policy: TesseraTierPolicy;
  budget: TesseraBudget;
  privacy_class: TesseraPrivacyClass;
  approval_policy: TesseraApprovalPolicy;
  artifact_policy: TesseraArtifactPolicy;
  max_depth: number;
  deadline_ms: number;
  dry_run: boolean;
  metadata: JsonRecord;
}

export type TesseraEventKind =
  | 'goal.accepted'
  | 'submind.spawned'
  | 'submind.completed'
  | 'lr.call.started'
  | 'lr.call.completed'
  | 'memory.cue.read'
  | 'artifact.proposed'
  | 'approval.requested'
  | 'cost.debit'
  | 'confidence.updated'
  | 'prime_directive.concern'
  | 'goal.completed'
  | 'goal.failed';

export interface TesseraEvent {
  kind: TesseraEventKind;
  message: string;
  payload: JsonRecord;
}

export interface TesseraCostAccount {
  estimated_usd: number;
  tokens_in: number;
  tokens_out: number;
}

export interface TesseraArtifactRef {
  kind: LazarusArtifact['kind'];
  uri: string;
  sha256: string | null;
  metadata: JsonRecord;
}

export interface TesseraApprovalRef {
  gate: LazarusApprovalGate;
  reason: string;
  payload: JsonRecord;
}

export interface TesseraResult {
  schema_version: 'tessera.result.v1';
  status: TesseraStatus;
  summary: string;
  confidence: number;
  cost: TesseraCostAccount;
  events: TesseraEvent[];
  artifacts: TesseraArtifactRef[];
  approvals_requested: TesseraApprovalRef[];
  provenance: string[];
  next_goals: TesseraGoalEnvelope[];
  metadata: JsonRecord;
}

export interface CreateTesseraEnvelopeOptions {
  role?: TesseraSubmindRole;
  fleet?: Partial<Pick<LazarusFleetConfig, 'privacy_class' | 'max_cost_usd_per_run' | 'review_required'>>;
  bridge_enabled?: boolean;
  model_calls_enabled?: boolean;
  max_depth?: number;
  deadline_ms?: number;
  max_tokens?: number;
  trace_id?: string;
  goal_hv_hint?: string | null;
  metadata?: JsonRecord;
}