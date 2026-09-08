export type JobKind = 'apocky_chat' | 'chaos_oracle' | 'tool_workflow' | string;

export type CapabilityProfile = 'apocky_owner_chat' | 'chaos_tarot_reading' | string;

export interface ClaimedJob {
  jobId: string;
  attemptId: string;
  attemptNo: number;
  leaseEpoch: number;
  leaseToken: string;
  leaseExpiresAt: string;
  tenantId: string;
  ownerPrincipalId: string;
  kind: JobKind;
  capability: CapabilityProfile;
  request: Record<string, unknown>;
  modelAlias: string;
  profileHash: string;
  toolRegistryVersion: string;
  memoryManifestHash: string;
}

export interface Fence {
  jobId: string;
  attemptId: string;
  leaseEpoch: number;
  leaseToken: string;
}

export interface OutputChunk {
  seq: number;
  chunkKind: string;
  delta: string;
  metadata?: Record<string, unknown>;
  snapshotNo?: number;
  snapshotBody?: string;
  snapshotState?: Record<string, unknown>;
}

export interface CompletionPayload {
  content: string;
  revisionRole: 'primary' | 'corroboration' | 'synthesis';
  provenance?: Record<string, unknown>;
  usage?: Record<string, unknown>;
}

export interface FailurePayload {
  errorCode: string;
  errorDetail: string;
  retryable: boolean;
  metrics?: Record<string, unknown>;
}

export interface LeaseResult {
  leaseExpiresAt: string;
  cancelRequested: boolean;
}

export interface WorkerManifest {
  schema: 'apocrypha.worker-manifest.v1';
  model: {
    alias: string;
    profileHash: string;
    endpointEnv: string;
  };
  tools: {
    registryVersion: string;
    mode: 'read-only';
  };
  memory: {
    manifestVersion: string;
    tenantScoped: true;
    adapters: MemoryAdapterManifest[];
  };
  capabilities: string[];
}

export interface MemoryAdapterManifest {
  name: string;
  urlEnv: string;
  tokenEnv?: string;
  timeoutMs?: number;
  maxChars?: number;
  requiredCapabilities?: string[];
}

export interface MemoryProbeScope {
  tenantId: string;
  principalId: string;
  capability: string;
}

export interface WorkerConfig {
  controlPlaneUrl: string;
  nodeId: string;
  nodeToken: string;
  qwenBaseUrl: string;
  runtimeProfilePath: string | null;
  modelAlias: string;
  profileHash: string;
  toolRegistryVersion: string;
  memoryManifestHash: string;
  manifest: WorkerManifest;
  pollIntervalMs: number;
  claimLeaseSeconds: number;
  leaseRenewIntervalMs: number;
  leaseExpiryGraceMs: number;
  controlPlaneTimeoutMs: number;
  chunkFlushMs: number;
  chunkMaxChars: number;
  qwenIdleTimeoutMs: number;
  qwenMaxRuntimeMs: number;
  contextWindowTokens: number;
  maxOutputTokens: number;
  journalDir: string;
  healthHost: string;
  healthPort: number;
  heartbeatIntervalMs: number;
  heartbeatEnabled: boolean;
  memoryReadConcurrency: number;
  memoryProbeTenantId: string | null;
  memoryProbePrincipalId: string;
  memoryProbeCapability: string;
  memoryAdditionalProbeScopes?: ReadonlyArray<MemoryProbeScope>;
  once: boolean;
  probeOnly: boolean;
  recoverOnly: boolean;
}

export interface RetrievalRecord {
  source: string;
  provenanceId: string;
  text: string;
  metadata?: Record<string, unknown>;
}

export type AdapterState = 'ok' | 'unconfigured' | 'timeout' | 'error' | 'denied';

export interface RetrievalAdapterResult {
  name: string;
  state: AdapterState;
  durationMs: number;
  records: RetrievalRecord[];
  detail?: string;
}

export interface RetrievalBundle {
  query: string;
  results: RetrievalAdapterResult[];
  records: RetrievalRecord[];
  digest: string;
  probedAt: string | null;
}

export interface QwenUsage {
  promptTokens?: number;
  completionTokens?: number;
  totalTokens?: number;
}

export interface QwenResult {
  content: string;
  usage: QwenUsage;
  model: string;
  firstTokenMs?: number;
  durationMs: number;
}

export type JournalTerminal = {
  kind: 'complete';
  payload: CompletionPayload;
} | {
  kind: 'fail';
  payload: FailurePayload;
};

export interface AttemptJournalState {
  version: 1;
  claim: ClaimedJob;
  pendingChunks: OutputChunk[];
  lastAcknowledgedSeq: number;
  terminal: JournalTerminal | null;
  createdAt: string;
  updatedAt: string;
}

export interface WorkerRuntimeState {
  phase: 'starting' | 'idle' | 'recovering' | 'retrieving' | 'generating' | 'delivering' | 'stopping' | 'stopped';
  currentJobId: string | null;
  currentAttemptId: string | null;
  startedAt: string;
  lastClaimAt: string | null;
  lastCompletionAt: string | null;
  lastError: { code: string; detail: string; at: string } | null;
  completedJobs: number;
  failedJobs: number;
  recoveredAttempts: number;
  adapterStates: Record<string, AdapterState>;
  adapterProbeAt: string | null;
}
