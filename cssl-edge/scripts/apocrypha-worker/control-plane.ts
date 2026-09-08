import { randomUUID } from 'node:crypto';
import type {
  ClaimedJob,
  CompletionPayload,
  FailurePayload,
  Fence,
  LeaseResult,
  OutputChunk,
  WorkerConfig,
  WorkerRuntimeState,
} from './types';

export class ControlPlaneError extends Error {
  readonly status: number | null;
  readonly code: string;
  readonly retryable: boolean;
  readonly fenceLost: boolean;

  constructor(message: string, options: { status?: number; code?: string; retryable?: boolean; fenceLost?: boolean } = {}) {
    super(message);
    this.name = 'ControlPlaneError';
    this.status = options.status ?? null;
    this.code = options.code ?? 'CONTROL_PLANE_ERROR';
    this.retryable = options.retryable ?? false;
    this.fenceLost = options.fenceLost ?? false;
  }
}

type Fetch = typeof fetch;

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? value as Record<string, unknown> : {};
}

function jsonRecord(value: unknown): Record<string, unknown> {
  if (typeof value !== 'string') return asRecord(value);
  try {
    return asRecord(JSON.parse(value));
  } catch {
    return {};
  }
}

function text(value: unknown, name: string): string {
  if (typeof value !== 'string' || !value) throw new Error(`claim response missing ${name}`);
  return value;
}

function number(value: unknown, name: string): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) throw new Error(`claim response missing ${name}`);
  return value;
}

function unwrapApiData(payload: unknown): unknown {
  let value = payload;
  if (value && typeof value === 'object' && !Array.isArray(value) && 'data' in value) {
    value = (value as Record<string, unknown>).data;
  }
  if (Array.isArray(value)) return value.length === 0 ? null : value[0];
  return value;
}

function normalizeClaim(payload: Record<string, unknown>): ClaimedJob | null {
  const unwrapped = unwrapApiData(payload);
  if (unwrapped === null || unwrapped === undefined) return null;
  const root = asRecord(unwrapped);
  const jobValue = Object.prototype.hasOwnProperty.call(root, 'job') ? root.job : root;
  if (jobValue === null || jobValue === undefined) return null;
  const rawJob = asRecord(jobValue);
  if (Object.keys(rawJob).length === 0) return null;
  const nestedAttempt = asRecord(root.attempt);
  const nestedLease = asRecord(root.lease);
  const jobId = rawJob.job_id ?? rawJob.id;
  const attemptId = rawJob.attempt_id ?? nestedAttempt.id;
  const attemptNo = rawJob.attempt_no ?? nestedAttempt.attempt_no;
  const leaseEpoch = rawJob.lease_epoch ?? nestedLease.epoch;
  const leaseToken = rawJob.lease_token ?? nestedLease.token;
  const leaseExpiresAt = rawJob.lease_expires_at ?? nestedLease.expires_at;
  return {
    jobId: text(jobId, 'job_id'),
    attemptId: text(attemptId, 'attempt_id'),
    attemptNo: number(attemptNo, 'attempt_no'),
    leaseEpoch: number(leaseEpoch, 'lease_epoch'),
    leaseToken: text(leaseToken, 'lease_token'),
    leaseExpiresAt: text(leaseExpiresAt, 'lease_expires_at'),
    tenantId: text(rawJob.tenant_id, 'tenant_id'),
    ownerPrincipalId: text(rawJob.owner_principal_id, 'owner_principal_id'),
    kind: text(rawJob.kind, 'kind'),
    capability: text(rawJob.capability, 'capability'),
    request: jsonRecord(rawJob.request),
    modelAlias: text(rawJob.model_alias, 'model_alias'),
    profileHash: text(rawJob.profile_hash, 'profile_hash').toLowerCase(),
    toolRegistryVersion: text(rawJob.tool_registry_version, 'tool_registry_version'),
    memoryManifestHash: text(rawJob.memory_manifest_hash, 'memory_manifest_hash').toLowerCase(),
  };
}

export class ControlPlaneClient {
  private readonly config: WorkerConfig;
  private readonly fetchImpl: Fetch;
  private pendingClaimKey: string | null = null;

  constructor(config: WorkerConfig, fetchImpl: Fetch = fetch) {
    this.config = config;
    this.fetchImpl = fetchImpl;
  }

  async claim(): Promise<ClaimedJob | null> {
    const idempotencyKey = this.pendingClaimKey ?? randomUUID();
    this.pendingClaimKey = idempotencyKey;
    try {
      const response = await this.request('/api/apocrypha/worker/claim', {
        method: 'POST',
        body: { node_id: this.config.nodeId },
        idempotencyKey,
        allowNoContent: true,
        attempts: 3,
      });
      this.pendingClaimKey = null;
      return response === null ? null : normalizeClaim(response);
    } catch (error) {
      // Reuse the same key after an ambiguous network result. The control plane
      // must replay the original claim response rather than leasing a second job.
      throw error;
    }
  }

  async renew(fence: Fence): Promise<LeaseResult> {
    const response = await this.request(`/api/apocrypha/worker/lease`, {
      method: 'POST',
      body: {
        node_id: this.config.nodeId,
        job_id: fence.jobId,
        attempt_id: fence.attemptId,
        lease_epoch: fence.leaseEpoch,
        lease_token: fence.leaseToken,
        lease_seconds: this.config.claimLeaseSeconds,
      },
      idempotencyKey: `${fence.attemptId}:lease:${fence.leaseEpoch}`,
    });
    const unwrapped = unwrapApiData(response);
    if (typeof unwrapped === 'string') {
      return { leaseExpiresAt: unwrapped, cancelRequested: false };
    }
    const data = asRecord(unwrapped);
    return {
      leaseExpiresAt: text(data.lease_expires_at ?? data.lease_until, 'lease_expires_at'),
      cancelRequested: data.cancel_requested === true,
    };
  }

  async appendChunk(fence: Fence, chunk: OutputChunk): Promise<Record<string, unknown>> {
    return this.request('/api/apocrypha/worker/chunk', {
      method: 'POST',
      body: {
        node_id: this.config.nodeId,
        job_id: fence.jobId,
        attempt_id: fence.attemptId,
        lease_epoch: fence.leaseEpoch,
        lease_token: fence.leaseToken,
        seq: chunk.seq,
        chunk_kind: chunk.chunkKind,
        delta: chunk.delta,
        metadata: chunk.metadata ?? {},
        snapshot_no: chunk.snapshotNo ?? null,
        snapshot_body: chunk.snapshotBody ?? null,
        snapshot_state: chunk.snapshotState ?? null,
      },
      idempotencyKey: `${fence.attemptId}:chunk:${chunk.seq}`,
    }) as Promise<Record<string, unknown>>;
  }

  async complete(fence: Fence, payload: CompletionPayload): Promise<Record<string, unknown>> {
    return this.request('/api/apocrypha/worker/complete', {
      method: 'POST',
      body: {
        node_id: this.config.nodeId,
        job_id: fence.jobId,
        attempt_id: fence.attemptId,
        lease_epoch: fence.leaseEpoch,
        lease_token: fence.leaseToken,
        content: payload.content,
        revision_role: payload.revisionRole,
        provenance: payload.provenance ?? {},
        usage: payload.usage ?? {},
      },
      idempotencyKey: `${fence.attemptId}:complete`,
    }) as Promise<Record<string, unknown>>;
  }

  async fail(fence: Fence, payload: FailurePayload): Promise<Record<string, unknown>> {
    return this.request('/api/apocrypha/worker/fail', {
      method: 'POST',
      body: {
        node_id: this.config.nodeId,
        job_id: fence.jobId,
        attempt_id: fence.attemptId,
        lease_epoch: fence.leaseEpoch,
        lease_token: fence.leaseToken,
        error_code: payload.errorCode,
        error_detail: payload.errorDetail.slice(0, 4_000),
        retryable: payload.retryable,
        metrics: payload.metrics ?? {},
      },
      idempotencyKey: `${fence.attemptId}:fail`,
    }) as Promise<Record<string, unknown>>;
  }

  async heartbeat(
    runtime: WorkerRuntimeState,
    operational: { qwenHealthy: boolean; qwenProbeAt: string },
  ): Promise<boolean> {
    try {
      await this.request('/api/apocrypha/worker/heartbeat', {
        method: 'POST',
        body: {
          node_id: this.config.nodeId,
          status: runtime.phase,
          current_job_id: runtime.currentJobId,
          current_attempt_id: runtime.currentAttemptId,
          model_alias: this.config.modelAlias,
          profile_hash: this.config.profileHash,
          tool_registry_version: this.config.toolRegistryVersion,
          memory_manifest_hash: this.config.memoryManifestHash,
          capabilities: this.config.manifest.capabilities,
          load: { completed_jobs: runtime.completedJobs, failed_jobs: runtime.failedJobs },
          qwen_healthy: operational.qwenHealthy,
          qwen_probe_at: operational.qwenProbeAt,
          adapter_states: runtime.adapterStates,
          adapter_probe_at: runtime.adapterProbeAt,
          capability_memory: Object.fromEntries(this.config.manifest.capabilities.map((capability) => [capability, {
            adapter_states: runtime.capabilityAdapterStates[capability],
            adapter_probe_at: runtime.capabilityAdapterProbeAt[capability] ?? null,
          }])),
          generation_deadline_ms: this.config.qwenMaxRuntimeMs,
        },
        idempotencyKey: `${this.config.nodeId}:heartbeat:${Math.floor(Date.now() / this.config.heartbeatIntervalMs)}`,
        attempts: 1,
      });
      return true;
    } catch (error) {
      if (error instanceof ControlPlaneError && error.status === 404) return false;
      throw error;
    }
  }

  private async request(
    path: string,
    options: {
      method: 'POST';
      body: Record<string, unknown>;
      idempotencyKey: string;
      allowNoContent?: boolean;
      attempts?: number;
    },
  ): Promise<Record<string, unknown> | null> {
    const attempts = options.attempts ?? 3;
    let lastError: unknown;
    for (let attempt = 1; attempt <= attempts; attempt += 1) {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(new Error('control-plane request timeout')), this.config.controlPlaneTimeoutMs);
      try {
        const response = await this.fetchImpl(`${this.config.controlPlaneUrl}${path}`, {
          method: options.method,
          headers: {
            authorization: `Bearer ${this.config.nodeToken}`,
            'content-type': 'application/json',
            'idempotency-key': options.idempotencyKey,
            'user-agent': 'apocrypha-outbound-worker/1.0',
          },
          body: JSON.stringify(options.body),
          signal: controller.signal,
        });
        if (response.status === 204 && options.allowNoContent) return null;
        const responseText = await response.text();
        let payload: Record<string, unknown> = {};
        if (responseText) {
          try {
            payload = asRecord(JSON.parse(responseText));
          } catch {
            payload = { detail: responseText.slice(0, 1_000) };
          }
        }
        if (!response.ok) {
          const fenceLost = [401, 403, 409, 410, 412].includes(response.status);
          const retryable = response.status === 408 || response.status === 429 || response.status >= 500;
          throw new ControlPlaneError(
            typeof payload.detail === 'string' ? payload.detail : `control plane returned HTTP ${response.status}`,
            {
              status: response.status,
              code: typeof payload.code === 'string' ? payload.code : `HTTP_${response.status}`,
              retryable,
              fenceLost,
            },
          );
        }
        if (payload.ok === false) {
          const status = typeof payload.status === 'number' ? payload.status : 500;
          const code = typeof payload.code === 'string' ? payload.code : 'CONTROL_PLANE_REJECTED';
          const fenceLost = ['STALE_FENCE', 'LEASE_EXPIRED', 'CANCELLED', 'INVALID_WORKER_TOKEN'].includes(code);
          throw new ControlPlaneError(
            typeof payload.detail === 'string' ? payload.detail : typeof payload.error === 'string' ? payload.error : 'control plane rejected mutation',
            { status, code, retryable: status === 408 || status === 429 || status >= 500, fenceLost },
          );
        }
        return payload;
      } catch (error) {
        const normalized = error instanceof ControlPlaneError
          ? error
          : new ControlPlaneError(error instanceof Error ? error.message : 'control-plane network failure', {
            code: controller.signal.aborted ? 'CONTROL_PLANE_TIMEOUT' : 'CONTROL_PLANE_NETWORK',
            retryable: true,
          });
        lastError = normalized;
        if (!normalized.retryable || normalized.fenceLost || attempt === attempts) throw normalized;
        await new Promise((resolve) => setTimeout(resolve, Math.min(2_000, 200 * 2 ** (attempt - 1))));
      } finally {
        clearTimeout(timer);
      }
    }
    throw lastError;
  }
}

export function fenceFromClaim(claim: ClaimedJob): Fence {
  return {
    jobId: claim.jobId,
    attemptId: claim.attemptId,
    leaseEpoch: claim.leaseEpoch,
    leaseToken: claim.leaseToken,
  };
}
