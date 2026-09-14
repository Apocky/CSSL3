export type ConsentDecision = 'allow' | 'allow_session' | 'deny';

export type RiskTier = 'read' | 'write' | 'execute';

export interface ToolDefinition {
  readonly name: string;
  readonly risk: RiskTier;
  readonly description: string;
  readonly parameters: Record<string, unknown>;
}

export interface ToolCallRequest {
  readonly id: string;
  readonly name: string;
  readonly args: Record<string, unknown>;
}

export interface ToolCallOutcome {
  readonly id: string;
  readonly name: string;
  readonly ok: boolean;
  readonly elapsedMs: number;
  readonly summary: string;
  readonly content: string;
  readonly error?: string;
  readonly denied?: boolean;
  /** Set when the tool changed a file, so the UI can show a diff without re-reading. */
  readonly diff?: { path: string; added: number; removed: number; patch: string };
}

export type TurnPhase =
  | 'queued'
  | 'thinking'
  | 'awaiting_consent'
  | 'tool'
  | 'writing'
  | 'done'
  | 'failed'
  | 'cancelled';

export interface WorkEvent {
  readonly seq: number;
  readonly at: string;
  readonly kind:
    | 'phase'
    | 'token'
    | 'tool_request'
    | 'tool_result'
    | 'consent_request'
    | 'consent_resolved'
    | 'usage'
    | 'error'
    | 'session';
  readonly data: Record<string, unknown>;
}

export interface ConsentRequest {
  readonly id: string;
  readonly turnId: string;
  readonly tool: string;
  readonly risk: RiskTier;
  readonly summary: string;
  readonly detail: string;
  readonly createdAt: string;
}

export interface WorkTurn {
  readonly id: string;
  readonly sessionId: string;
  readonly prompt: string;
  phase: TurnPhase;
  startedAt: string;
  endedAt?: string;
  error?: string;
  toolCalls: ToolCallOutcome[];
  output: string;
  usage?: { promptTokens?: number; completionTokens?: number; totalTokens?: number; elapsedS?: number };
}

export interface WorkSession {
  readonly id: string;
  title: string;
  createdAt: string;
  lastActiveAt: string;
  /** Tool names the operator has blanket-approved for the life of this session. */
  standingGrants: string[];
}

export interface EngineProfile {
  readonly alias: string;
  readonly baseUrl: string;
  readonly contextWindow: number;
  readonly maxOutputTokens: number;
  readonly temperature: number;
  readonly topP: number;
  readonly topK: number;
}

export interface ArbiterSettings {
  readonly mode: 'off' | 'manual' | 'auto';
  readonly enginePort: number;
  readonly chatModelPath: string;
  readonly workModelPath: string;
  readonly chatLauncher: string;
  readonly workLauncher: string;
  readonly workProfile: string;
  readonly chatWorkerHealthUrl: string;
  readonly drainTimeoutMs: number;
  readonly startTimeoutMs: number;
  readonly idleYieldMs: number;
  readonly launcherLogDir: string;
}

export interface WorkConfig {
  readonly host: string;
  readonly port: number;
  readonly token: string;
  readonly stateDir: string;
  readonly roots: readonly { label: string; path: string; writable: boolean }[];
  readonly engine: EngineProfile;
  readonly maxToolIterations: number;
  readonly toolTimeoutMs: number;
  readonly turnTimeoutMs: number;
  readonly shellAllowed: boolean;
  readonly shellDenyPatterns: readonly RegExp[];
  readonly autoApprove: readonly RiskTier[];
  readonly arbiter: ArbiterSettings;
}
