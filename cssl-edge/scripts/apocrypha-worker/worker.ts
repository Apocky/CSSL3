import { ControlPlaneClient, ControlPlaneError, fenceFromClaim } from './control-plane';
import { AttemptJournal } from './journal';
import { log } from './log';
import { composeQwenRequest } from './prompt';
import { QwenClient, QwenError } from './qwen';
import { probeMemoryAdapters, retrieveMemory } from './retrieval';
import type {
  AttemptJournalState,
  ClaimedJob,
  FailurePayload,
  OutputChunk,
  QwenResult,
  RetrievalBundle,
  WorkerConfig,
  WorkerRuntimeState,
} from './types';

function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve) => {
    if (signal?.aborted) return resolve();
    const timer = setTimeout(resolve, ms);
    signal?.addEventListener('abort', () => {
      clearTimeout(timer);
      resolve();
    }, { once: true });
  });
}

function boundedError(error: unknown): string {
  return (error instanceof Error ? error.message : String(error)).replace(/[\r\n]+/g, ' ').slice(0, 2_000);
}

class LeaseLostError extends Error {
  readonly code: string;
  readonly cancelled: boolean;

  constructor(message: string, code = 'LEASE_LOST', cancelled = false) {
    super(message);
    this.name = 'LeaseLostError';
    this.code = code;
    this.cancelled = cancelled;
  }
}

interface Dependencies {
  controlPlane?: ControlPlaneClient;
  qwen?: QwenClient;
  journal?: AttemptJournal;
  env?: NodeJS.ProcessEnv;
  fetchImpl?: typeof fetch;
}

export class ApocryphaWorker {
  readonly runtime: WorkerRuntimeState;
  readonly journal: AttemptJournal;
  readonly qwen: QwenClient;
  private readonly config: WorkerConfig;
  private readonly controlPlane: ControlPlaneClient;
  private readonly env: NodeJS.ProcessEnv;
  private readonly fetchImpl: typeof fetch;
  private readonly stopController = new AbortController();
  private heartbeatTimer: NodeJS.Timeout | null = null;
  private contextClampLogged = false;
  private heartbeatRetryAt = 0;
  private heartbeatInFlight = false;
  private memoryProbeInFlight = false;
  private memoryOperationTail: Promise<void> = Promise.resolve();

  constructor(config: WorkerConfig, dependencies: Dependencies = {}) {
    this.config = config;
    this.env = dependencies.env ?? process.env;
    this.fetchImpl = dependencies.fetchImpl ?? fetch;
    this.controlPlane = dependencies.controlPlane ?? new ControlPlaneClient(config, this.fetchImpl);
    this.qwen = dependencies.qwen ?? new QwenClient(config, this.fetchImpl);
    this.journal = dependencies.journal ?? new AttemptJournal(config.journalDir, config.nodeToken, config.nodeId);
    this.runtime = {
      phase: 'starting',
      currentJobId: null,
      currentAttemptId: null,
      startedAt: new Date().toISOString(),
      lastClaimAt: null,
      lastCompletionAt: null,
      lastError: null,
      completedJobs: 0,
      failedJobs: 0,
      recoveredAttempts: 0,
      adapterStates: Object.fromEntries(config.manifest.memory.adapters.map((adapter) => [adapter.name, 'unconfigured'])),
      adapterProbeAt: null,
      capabilityAdapterStates: Object.fromEntries(config.manifest.capabilities.map((capability) => [
        capability,
        Object.fromEntries(config.manifest.memory.adapters.map((adapter) => [adapter.name, 'unconfigured'])),
      ])),
      capabilityAdapterProbeAt: Object.fromEntries(config.manifest.capabilities.map((capability) => [capability, null])),
    };
  }

  stop(reason = 'worker stop requested'): void {
    if (this.stopController.signal.aborted) return;
    this.runtime.phase = 'stopping';
    this.stopController.abort(new Error(reason));
    if (this.heartbeatTimer) clearInterval(this.heartbeatTimer);
  }

  async initialize(): Promise<void> {
    await this.journal.initialize();
    await this.journal.pruneArchives();
    await this.recoverPendingAttempts();
    this.runtime.phase = 'idle';
    if (this.config.heartbeatEnabled) this.startHeartbeat();
  }

  async run(): Promise<void> {
    await this.initialize();
    if (this.config.recoverOnly) {
      this.runtime.phase = 'stopped';
      return;
    }
    if (this.config.probeOnly) {
      const probe = await this.qwen.probe(this.stopController.signal);
      if (!probe.healthy) throw new QwenError(`Qwen probe failed: ${probe.detail}`, 'QWEN_NOT_READY', true);
      log('info', 'worker.probe.ok', probe);
      this.runtime.phase = 'stopped';
      return;
    }
    do {
      // `runOnce` already reports whether it did work, and the loop used to
      // throw that away and sleep regardless. The cost was paid by every queued
      // turn: a worker that had just finished one job waited a full poll
      // interval before looking for the next, and a turn submitted a moment
      // after a poll waited most of an interval before being seen at all. With
      // the default 1500ms that is up to 1.5s of latency per turn, on a path
      // where the client is also polling at 1500ms.
      let didWork = false;
      try {
        didWork = await this.runOnce();
      } catch (error) {
        // Deliberately NOT treated as work. Looping straight back into a claim
        // that just threw would spin against a failing control plane as fast as
        // the network allows, which turns one outage into a self-inflicted
        // flood. An error rests for the full interval.
        didWork = false;
        this.recordError(error instanceof ControlPlaneError ? error.code : 'WORKER_LOOP_ERROR', boundedError(error));
        log('error', 'worker.loop.error', { code: this.runtime.lastError?.code, detail: this.runtime.lastError?.detail });
      }
      if (this.config.once || this.stopController.signal.aborted) break;
      // Rest only when idle. `runOnce` returns false for "nothing to claim" AND
      // for "the journal still has pending work", and both of those must back
      // off - the second especially, since looping on it would be a hot spin
      // against local state that only time resolves.
      if (!didWork) await sleep(this.config.pollIntervalMs, this.stopController.signal);
    } while (!this.stopController.signal.aborted);
    this.runtime.phase = 'stopped';
  }

  async runOnce(): Promise<boolean> {
    if (this.stopController.signal.aborted) return false;
    if (await this.journal.pendingCount() > 0) {
      await this.recoverPendingAttempts();
      if (await this.journal.pendingCount() > 0) return false;
    }
    this.runtime.phase = 'idle';
    // Claiming while Qwen is still loading burns every retry within seconds
    // (control-plane backoff is 2^n s), so hold the claim until the probe passes.
    const probe = await this.qwen.probe(this.stopController.signal);
    if (!probe.healthy) {
      if (this.runtime.lastError?.code !== 'QWEN_NOT_READY') {
        log('warn', 'worker.claim.deferred', { code: 'QWEN_NOT_READY', detail: probe.detail });
      }
      this.recordError('QWEN_NOT_READY', probe.detail);
      return false;
    }
    if (this.runtime.lastError?.code === 'QWEN_NOT_READY') {
      log('info', 'worker.claim.resumed', { detail: probe.detail });
      this.runtime.lastError = null;
    }
    const claim = await this.controlPlane.claim();
    this.runtime.lastClaimAt = new Date().toISOString();
    if (!claim) return false;
    await this.processClaim(claim);
    return true;
  }

  async recoverPendingAttempts(): Promise<void> {
    const pending = await this.journal.list();
    if (pending.length === 0) return;
    this.runtime.phase = 'recovering';
    for (const item of pending) {
      const { state } = item;
      const fence = fenceFromClaim(state.claim);
      this.runtime.currentJobId = state.claim.jobId;
      this.runtime.currentAttemptId = state.claim.attemptId;
      try {
        let lease: { leaseExpiresAt: string; cancelRequested: boolean };
        try {
          lease = await this.controlPlane.renew(fence);
        } catch (renewError) {
          // A terminal RPC may have committed remotely just before the worker
          // crashed, leaving only the local acknowledgement cleanup undone.
          // Terminal RPCs are idempotent; replay the exact saved action before
          // treating a rejected renewal as a lost attempt.
          if (!state.terminal || state.pendingChunks.length > 0) throw renewError;
          if (state.terminal.kind === 'complete') {
            await this.controlPlane.complete(fence, state.terminal.payload);
          } else {
            await this.controlPlane.fail(fence, state.terminal.payload);
          }
          await this.journal.remove(state);
          this.runtime.recoveredAttempts += 1;
          log('info', 'worker.recovery.terminal_replayed', {
            job_id: state.claim.jobId,
            attempt_id: state.claim.attemptId,
          });
          continue;
        }
        if (lease.cancelRequested) throw new LeaseLostError('job was cancelled while worker was offline', 'CANCELLED', true);
        state.claim.leaseExpiresAt = lease.leaseExpiresAt;
        await this.journal.save(state);
        const recoveryAbortController = new AbortController();
        const maintainedLease = this.maintainLease(state, recoveryAbortController);
        try {
          await this.replayPendingChunks(state, recoveryAbortController.signal);
          if (!state.terminal) {
            const failure: FailurePayload = {
              errorCode: 'WORKER_RESTART_INTERRUPTED_ATTEMPT',
              errorDetail: 'The worker restarted during generation. The isolated attempt will be retried without appending abandoned output.',
              retryable: true,
            };
            // Persist the exact terminal action while background renewal is
            // still active; the final renew below then fences that durable
            // action immediately before its remote commit.
            await this.journal.setFailure(state, failure);
          }
          await maintainedLease.stop();
          const recoveryLeaseError = maintainedLease.error();
          if (recoveryLeaseError) throw recoveryLeaseError;
          if (recoveryAbortController.signal.aborted) throw recoveryAbortController.signal.reason;
          const terminalLease = await this.controlPlane.renew(fence);
          if (terminalLease.cancelRequested) {
            throw new LeaseLostError('job was cancelled before recovery completion', 'CANCELLED', true);
          }
          if (state.terminal?.kind === 'complete') {
            await this.controlPlane.complete(fence, state.terminal.payload);
          } else if (state.terminal?.kind === 'fail') {
            await this.controlPlane.fail(fence, state.terminal.payload);
          } else {
            throw new Error('recovery terminal journal was not persisted');
          }
        } finally {
          await maintainedLease.stop();
        }
        await this.journal.remove(state);
        this.runtime.recoveredAttempts += 1;
        log('info', 'worker.recovery.acknowledged', { job_id: state.claim.jobId, attempt_id: state.claim.attemptId });
      } catch (error) {
        if ((error instanceof ControlPlaneError && error.fenceLost) || error instanceof LeaseLostError) {
          await this.journal.orphan(state, error instanceof LeaseLostError ? error.code : error.code);
          log('warn', 'worker.recovery.fenced', { job_id: state.claim.jobId, attempt_id: state.claim.attemptId, detail: boundedError(error) });
          continue;
        }
        this.recordError('RECOVERY_DEFERRED', boundedError(error));
        log('warn', 'worker.recovery.deferred', { job_id: state.claim.jobId, attempt_id: state.claim.attemptId, detail: boundedError(error) });
      }
    }
    this.runtime.currentJobId = null;
    this.runtime.currentAttemptId = null;
  }

  private async processClaim(claim: ClaimedJob): Promise<void> {
    this.runtime.currentJobId = claim.jobId;
    this.runtime.currentAttemptId = claim.attemptId;
    const state = await this.journal.create(claim);
    const abortController = new AbortController();
    const stopAbort = () => abortController.abort(this.stopController.signal.reason);
    this.stopController.signal.addEventListener('abort', stopAbort, { once: true });
    const lease = this.maintainLease(state, abortController);
    let memory: RetrievalBundle | null = null;
    let deliveryTail: Promise<void> = Promise.resolve();
    let deliveryError: unknown = null;
    const started = Date.now();
    try {
      this.assertCompatibleClaim(claim);
      const probe = await this.qwen.probe(abortController.signal);
      if (!probe.healthy) throw new QwenError(`Qwen is not ready: ${probe.detail}`, 'QWEN_NOT_READY', true);
      this.runtime.phase = 'retrieving';
      memory = await this.serializeMemoryOperation(
        () => retrieveMemory(this.config, claim, this.env, this.fetchImpl),
      );
      const retrievalStates = Object.fromEntries(memory.results.map((result) => [result.name, result.state]));
      if (memory.results.every((result) => result.state === 'ok')) {
        this.runtime.adapterStates = retrievalStates;
      } else {
        // A turn-scoped read can time out on a large corpus without making the
        // resident faculty unavailable. Preserve the last completed operational
        // probe for admission/readiness and surface the partial read separately.
        const failed = memory.results
          .filter((result) => result.state !== 'ok')
          .map((result) => `${result.name}:${result.state}`)
          .join(', ');
        this.recordError('MEMORY_READ_PARTIAL', failed || 'one or more memory readers returned no records');
        log('warn', 'worker.memory_read.partial', {
          job_id: claim.jobId,
          attempt_id: claim.attemptId,
          adapter_states: retrievalStates,
          // Without the per-adapter detail a degraded faculty is invisible:
          // the state alone cannot tell a misconfigured reader from a slow one.
          adapter_details: Object.fromEntries(memory.results
            .filter((result) => result.state !== 'ok' && result.detail)
            .map((result) => [result.name, String(result.detail).slice(0, 300)])),
        });
      }
      if (abortController.signal.aborted) throw abortController.signal.reason;
      this.runtime.phase = 'generating';
      let buffer = '';
      let seq = state.lastAcknowledgedSeq + 1;
      let lastFlushAt = Date.now();
      let streamed = false;
      const timedFlushChars = Math.min(this.config.chunkMaxChars, Math.max(64, Math.ceil(this.config.chunkMaxChars / 4)));
      const enqueueChunk = async (chunk: OutputChunk): Promise<void> => {
        // Persist every fragment before allowing Qwen to continue, then deliver
        // it in order without placing a remote control-plane write in Qwen's
        // token callback. A slow public endpoint can delay visibility, but it
        // cannot stall local inference or starve the worker health server.
        await this.journal.addPendingChunk(state, chunk);
        const delivery = deliveryTail.then(async () => {
          if (deliveryError) throw deliveryError;
          await this.controlPlane.appendChunk(fenceFromClaim(state.claim), chunk);
          await this.journal.acknowledgeChunk(state, chunk.seq);
        });
        deliveryTail = delivery.catch((error) => {
          deliveryError ??= error;
          if (error instanceof ControlPlaneError && error.fenceLost && !abortController.signal.aborted) {
            abortController.abort(new LeaseLostError(error.message, error.code));
          }
        });
      };
      const drainDeliveries = async (): Promise<void> => {
        await deliveryTail;
        if (deliveryError) throw deliveryError;
      };
      const flush = async (force = false): Promise<void> => {
        while (buffer.length >= this.config.chunkMaxChars || (force && buffer.length > 0)) {
          const size = force ? Math.min(buffer.length, this.config.chunkMaxChars) : this.config.chunkMaxChars;
          const delta = buffer.slice(0, size);
          buffer = buffer.slice(size);
          const chunk: OutputChunk = {
            seq,
            chunkKind: 'token',
            delta,
            metadata: { model_alias: this.config.modelAlias },
          };
          this.runtime.phase = 'delivering';
          await enqueueChunk(chunk);
          seq += 1;
          lastFlushAt = Date.now();
          this.runtime.phase = 'generating';
        }
      };
      const onDelta = async (delta: string): Promise<void> => {
        if (abortController.signal.aborted) throw abortController.signal.reason;
        streamed = true;
        buffer += delta;
        const timedFlushReady = buffer.length >= timedFlushChars && Date.now() - lastFlushAt >= this.config.chunkFlushMs;
        if (buffer.length >= this.config.chunkMaxChars || timedFlushReady) {
          await flush(false);
          if (timedFlushReady && buffer.length > 0) await flush(true);
        }
      };
      // The server's real window is authoritative; a larger configured window
      // would only produce overflow rejections.
      const serverContext = probe.contextTokens;
      const effectiveConfig = serverContext && serverContext < this.config.contextWindowTokens
        ? { ...this.config, contextWindowTokens: serverContext }
        : this.config;
      if (effectiveConfig !== this.config && !this.contextClampLogged) {
        this.contextClampLogged = true;
        log('warn', 'worker.qwen.context_clamped', {
          configured: this.config.contextWindowTokens,
          server_n_ctx: serverContext,
        });
      }
      const generate = async (overflowRetry = false): Promise<QwenResult> => {
        let request = composeQwenRequest(effectiveConfig, claim, memory as RetrievalBundle, { overflowRetry });
        if (!overflowRetry) {
          // Exact count from the server; fall back to the byte estimate when unavailable.
          const counter = (this.qwen as { tokenCount?: QwenClient['tokenCount'] }).tokenCount;
          const count = counter ? await counter.call(this.qwen, request.messages, abortController.signal) : null;
          const ceiling = effectiveConfig.contextWindowTokens - (request.generation.maxTokens ?? this.config.maxOutputTokens) - 64;
          if (count !== null && count > ceiling) {
            log('warn', 'worker.qwen.prompt_recompacted', { job_id: claim.jobId, prompt_tokens: count, ceiling });
            request = composeQwenRequest(effectiveConfig, claim, memory as RetrievalBundle, { overflowRetry: true });
          } else if (count !== null) {
            log('info', 'worker.qwen.prompt_tokens', {
              job_id: claim.jobId,
              prompt_tokens: count,
              ceiling,
              memory_records: (memory as RetrievalBundle).records.length,
              memory_sources: Object.fromEntries((memory as RetrievalBundle).results.map((result) => [result.name, result.records.length])),
            });
          }
        }
        return this.qwen.generate(request.messages, request.generation, onDelta, abortController.signal);
      };
      let result: QwenResult;
      try {
        result = await generate();
      } catch (error) {
        if (!(error instanceof QwenError) || error.code !== 'QWEN_CONTEXT_OVERFLOW' || streamed) throw error;
        log('warn', 'worker.qwen.context_retry', {
          job_id: claim.jobId,
          attempt_id: claim.attemptId,
          detail: boundedError(error),
        });
        result = await generate(true);
      }
      await flush(true);
      const completion = {
        content: result.content,
        revisionRole: 'primary' as const,
        provenance: {
          model_alias: result.model,
          model_profile_hash: this.config.profileHash,
          tool_registry_version: this.config.toolRegistryVersion,
          memory_manifest_hash: this.config.memoryManifestHash,
          memory_digest: memory.digest,
          memory_sources: memory.results.map((item) => ({ name: item.name, state: item.state, records: item.records.length, duration_ms: item.durationMs })),
          provenance_ids: memory.records.map((item) => `${item.source}:${item.provenanceId}`).slice(0, 100),
        },
        usage: {
          prompt_tokens: result.usage.promptTokens,
          completion_tokens: result.usage.completionTokens,
          total_tokens: result.usage.totalTokens,
          first_token_ms: result.firstTokenMs,
          duration_ms: result.durationMs,
        },
      };
      // Once Qwen has returned and every byte is journaled, preserve the exact
      // completion before any remote delivery wait. Recovery can then replay
      // pending chunks and commit this same answer instead of regenerating it.
      await this.journal.setCompletion(state, completion);
      this.runtime.phase = 'delivering';
      await drainDeliveries();
      await lease.stop();
      if (abortController.signal.aborted) throw abortController.signal.reason;
      const terminalLease = await this.controlPlane.renew(fenceFromClaim(claim));
      if (terminalLease.cancelRequested) {
        throw new LeaseLostError('job was cancelled before completion', 'CANCELLED_BY_USER', true);
      }
      await this.controlPlane.complete(fenceFromClaim(claim), completion);
      await this.journal.remove(state);
      this.runtime.completedJobs += 1;
      this.runtime.lastCompletionAt = new Date().toISOString();
      log('info', 'worker.job.completed', {
        job_id: claim.jobId,
        attempt_id: claim.attemptId,
        duration_ms: Date.now() - started,
        output_chars: result.content.length,
        first_token_ms: result.firstTokenMs,
      });
    } catch (error) {
      // A Qwen error, cancellation, or fence change can arrive while an
      // already-journaled chunk is still being delivered. Join that delivery
      // before writing terminal state so a late acknowledgement cannot save a
      // removed journal again or race the fail/orphan transition. The lease
      // remains active during this bounded join; control-plane requests have
      // their own timeout.
      await deliveryTail;
      let leaseError = this.asLeaseError(error, lease);
      const fenceError = deliveryError instanceof ControlPlaneError && deliveryError.fenceLost
        ? deliveryError
        : error instanceof ControlPlaneError && error.fenceLost
          ? error
          : null;
      let failure: FailurePayload | null = state.terminal?.kind === 'fail'
        ? state.terminal.payload
        : null;
      let failureJournalError: unknown = null;
      if (!leaseError && !fenceError && state.terminal?.kind !== 'complete' && !failure) {
        failure = this.failureFor(error, Date.now() - started, memory);
        try {
          // Keep the maintainer running through the durable terminal write.
          // A fresh explicit renewal below fences the saved action immediately
          // before its remote acknowledgement.
          await this.journal.setFailure(state, failure);
        } catch (journalError) {
          failureJournalError = journalError;
        }
      }
      await lease.stop();
      leaseError ??= lease.error();
      if (leaseError || fenceError) {
        const fenceCode = leaseError?.code ?? fenceError?.code ?? 'LEASE_LOST';
        await this.journal.orphan(state, fenceCode);
        log('warn', 'worker.job.fenced', { job_id: claim.jobId, attempt_id: claim.attemptId, detail: boundedError(leaseError ?? fenceError ?? error) });
      } else if (state.terminal?.kind === 'complete') {
        this.recordError('COMPLETION_DELIVERY_PENDING', boundedError(deliveryError ?? error));
        log('warn', 'worker.job.completion_delivery_pending', {
          job_id: claim.jobId,
          attempt_id: claim.attemptId,
          detail: boundedError(deliveryError ?? error),
        });
      } else if (failureJournalError) {
        this.recordError('FAILURE_JOURNAL_FAILED', boundedError(failureJournalError));
        log('error', 'worker.job.failure_journal_failed', {
          job_id: claim.jobId,
          attempt_id: claim.attemptId,
          detail: boundedError(failureJournalError),
        });
      } else {
        try {
          const terminalLease = await this.controlPlane.renew(fenceFromClaim(claim));
          if (terminalLease.cancelRequested) {
            throw new LeaseLostError('job was cancelled before failure acknowledgement', 'CANCELLED_BY_USER', true);
          }
          if (!failure) throw new Error('failure terminal journal was not persisted');
          await this.controlPlane.fail(fenceFromClaim(claim), failure);
          await this.journal.remove(state);
        } catch (failureDeliveryError) {
          if ((failureDeliveryError instanceof ControlPlaneError && failureDeliveryError.fenceLost) || failureDeliveryError instanceof LeaseLostError) {
            await this.journal.orphan(
              state,
              failureDeliveryError instanceof LeaseLostError ? failureDeliveryError.code : failureDeliveryError.code,
            );
          }
          this.recordError('FAILURE_DELIVERY_PENDING', boundedError(failureDeliveryError));
        }
        this.runtime.failedJobs += 1;
        if (failure) {
          this.recordError(failure.errorCode, failure.errorDetail);
          log('error', 'worker.job.failed', { job_id: claim.jobId, attempt_id: claim.attemptId, ...failure });
        }
      }
    } finally {
      await lease.stop();
      this.stopController.signal.removeEventListener('abort', stopAbort);
      this.runtime.currentJobId = null;
      this.runtime.currentAttemptId = null;
      if (!this.stopController.signal.aborted) this.runtime.phase = 'idle';
    }
  }

  private maintainLease(state: AttemptJournalState, abortController: AbortController): { stop: () => Promise<void>; error: () => LeaseLostError | null } {
    let currentExpiry = Date.parse(state.claim.leaseExpiresAt);
    let active = true;
    let renewal: Promise<void> | null = null;
    let leaseError: LeaseLostError | null = null;
    const renew = async (): Promise<void> => {
      if (!active || abortController.signal.aborted) return;
      try {
        const result = await this.controlPlane.renew(fenceFromClaim(state.claim));
        currentExpiry = Date.parse(result.leaseExpiresAt);
        state.claim.leaseExpiresAt = result.leaseExpiresAt;
        await this.journal.save(state);
        if (result.cancelRequested) {
          leaseError = new LeaseLostError('job cancellation requested', 'CANCELLED_BY_USER', true);
          abortController.abort(leaseError);
        }
      } catch (error) {
        if (error instanceof ControlPlaneError && error.fenceLost) {
          leaseError = new LeaseLostError(error.message, error.code);
          abortController.abort(leaseError);
        } else if (!Number.isFinite(currentExpiry) || Date.now() >= currentExpiry - this.config.leaseExpiryGraceMs) {
          leaseError = new LeaseLostError(`lease could not be renewed before expiry: ${boundedError(error)}`, 'LEASE_RENEWAL_EXPIRED');
          abortController.abort(leaseError);
        } else {
          log('warn', 'worker.lease.renew_retry', { job_id: state.claim.jobId, attempt_id: state.claim.attemptId, detail: boundedError(error) });
        }
      } finally {
        // The scheduler clears the tracked promise after this call settles.
      }
    };
    const triggerRenewal = (): void => {
      if (renewal || !active || abortController.signal.aborted) return;
      const current = renew();
      renewal = current;
      void current.finally(() => {
        if (renewal === current) renewal = null;
      });
    };
    const timer = setInterval(triggerRenewal, this.config.leaseRenewIntervalMs);
    return {
      stop: async () => {
        active = false;
        clearInterval(timer);
        await renewal?.catch(() => undefined);
      },
      error: () => leaseError,
    };
  }

  private asLeaseError(error: unknown, lease: { error: () => LeaseLostError | null }): LeaseLostError | null {
    if (error instanceof LeaseLostError) return error;
    if (lease.error()) return lease.error();
    if (error instanceof QwenError && ['QWEN_CANCELLED', 'QWEN_ABORTED'].includes(error.code) && this.stopController.signal.aborted) {
      return new LeaseLostError('worker stopped during generation', 'WORKER_STOPPED');
    }
    return null;
  }

  private async replayPendingChunks(state: AttemptJournalState, signal?: AbortSignal): Promise<void> {
    for (const chunk of [...state.pendingChunks].sort((left, right) => left.seq - right.seq)) {
      if (signal?.aborted) throw signal.reason;
      await this.controlPlane.appendChunk(fenceFromClaim(state.claim), chunk);
      await this.journal.acknowledgeChunk(state, chunk.seq);
    }
  }

  private assertCompatibleClaim(claim: ClaimedJob): void {
    const mismatches = [
      claim.modelAlias === this.config.modelAlias ? null : `model alias ${claim.modelAlias}`,
      claim.profileHash === this.config.profileHash ? null : `profile hash ${claim.profileHash}`,
      claim.toolRegistryVersion === this.config.toolRegistryVersion ? null : `tool registry ${claim.toolRegistryVersion}`,
      claim.memoryManifestHash === this.config.memoryManifestHash ? null : `memory manifest ${claim.memoryManifestHash}`,
      this.config.manifest.capabilities.includes(claim.capability) ? null : `capability ${claim.capability}`,
    ].filter(Boolean);
    if (mismatches.length) throw new QwenError(`worker manifest mismatch: ${mismatches.join(', ')}`, 'WORKER_MANIFEST_MISMATCH', true);
  }

  private failureFor(error: unknown, durationMs: number, memory: RetrievalBundle | null): FailurePayload {
    const qwen = error instanceof QwenError ? error : null;
    return {
      errorCode: qwen?.code ?? (error instanceof ControlPlaneError ? error.code : 'WORKER_ATTEMPT_ERROR'),
      errorDetail: boundedError(error),
      retryable: qwen?.retryable ?? (error instanceof ControlPlaneError ? error.retryable : true),
      metrics: {
        duration_ms: durationMs,
        memory_digest: memory?.digest,
        adapter_states: memory?.results.map((item) => ({ name: item.name, state: item.state })),
      },
    };
  }

  private recordError(code: string, detail: string): void {
    this.runtime.lastError = { code, detail, at: new Date().toISOString() };
  }

  private async serializeMemoryOperation<T>(operation: () => Promise<T>): Promise<T> {
    const previous = this.memoryOperationTail;
    let release!: () => void;
    this.memoryOperationTail = new Promise<void>((resolve) => { release = resolve; });
    await previous;
    try {
      return await operation();
    } finally {
      release();
    }
  }

  private startMemoryProbe(): void {
    if (this.memoryProbeInFlight || this.runtime.phase !== 'idle' || this.stopController.signal.aborted) return;
    this.memoryProbeInFlight = true;
    void this.serializeMemoryOperation(async () => {
      if (this.runtime.phase !== 'idle' || this.stopController.signal.aborted) return null;
      return probeMemoryAdapters(this.config, this.env, this.fetchImpl);
    }).then((memoryProbe) => {
      if (!memoryProbe) return;
      this.runtime.adapterStates = Object.fromEntries(memoryProbe.results.map((result) => [result.name, result.state]));
      this.runtime.adapterProbeAt = memoryProbe.probedAt;
      for (const capability of this.config.manifest.capabilities) {
        const capabilityProbe = memoryProbe.capabilityProbes?.[capability];
        this.runtime.capabilityAdapterStates[capability] = Object.fromEntries(
          (capabilityProbe?.results ?? this.config.manifest.memory.adapters.map((adapter) => ({
            name: adapter.name,
            state: 'unconfigured' as const,
          }))).map((result) => [result.name, result.state]),
        );
        this.runtime.capabilityAdapterProbeAt[capability] = capabilityProbe?.probedAt ?? null;
      }
    }).catch((error) => {
      this.recordError('MEMORY_PROBE_FAILED', boundedError(error));
      log('warn', 'worker.memory_probe.failed', { detail: boundedError(error) });
    }).finally(() => {
      this.memoryProbeInFlight = false;
    });
  }

  private startHeartbeat(): void {
    const send = async (): Promise<void> => {
      if (this.heartbeatInFlight || Date.now() < this.heartbeatRetryAt || this.stopController.signal.aborted) return;
      this.heartbeatInFlight = true;
      try {
        const probe = await this.qwen.probe(this.stopController.signal);
        const probeAge = this.runtime.adapterProbeAt
          ? Date.now() - Date.parse(this.runtime.adapterProbeAt)
          : Number.POSITIVE_INFINITY;
        if (this.runtime.phase === 'idle' && probeAge >= 30_000) {
          // Publish the last completed adapter evidence while the next bounded
          // probe runs. Its timestamp still expires normally if the probe stalls.
          this.startMemoryProbe();
        }
        const supported = await this.controlPlane.heartbeat(this.runtime, {
          qwenHealthy: probe.healthy,
          qwenProbeAt: new Date().toISOString(),
        });
        this.heartbeatRetryAt = supported ? 0 : Date.now() + 5 * 60_000;
      } catch (error) {
        this.recordError('HEARTBEAT_FAILED', boundedError(error));
        log('warn', 'worker.heartbeat.failed', { detail: boundedError(error) });
      } finally {
        this.heartbeatInFlight = false;
      }
    };
    void send();
    this.heartbeatTimer = setInterval(() => void send(), this.config.heartbeatIntervalMs);
  }
}
