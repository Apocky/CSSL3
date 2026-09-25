import { ControlPlaneClient, ControlPlaneError, fenceFromClaim } from './control-plane';
import { AttemptJournal } from './journal';
import { log } from './log';
import { composeQwenRequest } from './prompt';
import { QwenClient, QwenError, hostedEngineConfig, type QwenMessage } from './qwen';
import { MEMORY_TOOLS, TOOL_GUIDANCE, runTool } from './tools';

const MAX_TOOL_ROUNDS = 3;

/** Memory tools reach the owner's private memory: only the owner's own turns may use them. */
function toolsAllowed(job: ClaimedJob): boolean {
  return job.capability === 'apocky_owner_chat' && process.env.APOCRYPHA_MEMORY_TOOLS !== 'off';
}
import { presentable } from '../../lib/apocrypha/deliberation';
import { probeMemoryAdapters, retrieveMemory } from './retrieval';
import { WorkingMemory } from './working-memory';

// Appended only after a blank answer. It names the failure rather than restating
// the question, so the retry cannot drift into answering something else.
const EMPTY_COMPLETION_NUDGE = 'Your previous attempt produced no answer at all.'
  + ' Reply now with the answer itself, directly and in full, and nothing else.';
import type {
  AttemptJournalState,
  ClaimedJob,
  FailurePayload,
  OutputChunk,
  QwenResult,
  EngineLane,
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
  /** Hosted flagship lane client; when absent, built from config.hosted or left off. */
  hosted?: QwenClient | null;
  journal?: AttemptJournal;
  env?: NodeJS.ProcessEnv;
  fetchImpl?: typeof fetch;
}

/**
 * Working-set key for a job.
 *
 * Always prefixed with tenant and owner: a conversation id arrives inside a request payload, so
 * it is caller-supplied, and a caller-supplied key must never be able to address another
 * principal's memory. The prefix makes that structural rather than a matter of validation.
 */
/** This turn's question, normalised: a different question needs a read of its own. */
function turnQuestion(job: ClaimedJob): string {
  const request = (job.request ?? {}) as Record<string, unknown>;
  const raw = [request.retrieval_query, request.question, request.prompt].find((v) => typeof v === 'string' && v.trim() !== '');
  return typeof raw === 'string' ? raw.trim().toLowerCase().replace(/\s+/gu, ' ').slice(0, 400) : '';
}

function conversationKey(job: ClaimedJob): string {
  const request = job.request ?? {};
  const supplied = ['conversation_id', 'conversationId', 'thread_id', 'session_id']
    .map((field) => (request as Record<string, unknown>)[field])
    .find((value) => typeof value === 'string' && value.trim() !== '');
  const scope = typeof supplied === 'string' ? supplied.trim().slice(0, 128) : 'default';
  return `${job.tenantId}\u0000${job.ownerPrincipalId}\u0000${scope}`;
}

export class ApocryphaWorker {
  readonly runtime: WorkerRuntimeState;
  readonly journal: AttemptJournal;
  readonly qwen: QwenClient;
  readonly hosted: QwenClient | null;
  private readonly config: WorkerConfig;
  private readonly controlPlane: ControlPlaneClient;
  private readonly env: NodeJS.ProcessEnv;
  private readonly fetchImpl: typeof fetch;
  private readonly stopController = new AbortController();
  private readonly workingMemory = new WorkingMemory();
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
    const hostedConfig = hostedEngineConfig(config);
    this.hosted = dependencies.hosted !== undefined
      ? dependencies.hosted
      : hostedConfig ? new QwenClient(hostedConfig, this.fetchImpl, { hosted: true }) : null;
    this.journal = dependencies.journal ?? new AttemptJournal(config.journalDir, config.nodeToken, config.nodeId);
    this.runtime = {
      phase: 'starting',
      recentErrors: [],
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
      adapterProbeRanAt: null,
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
      // ON THE TRANSITION ONLY, both of them. The log already did this; the error
      // buffer did not, and it polls about every 1.5 s. One ordinary engine
      // restart therefore wrote 20 identical QWEN_NOT_READY rows in 30 seconds
      // and evicted the entire buffer -- observed 2026-09-17, where the only
      // errors a 15-minute window could show were twenty copies of "the engine is
      // still loading", and anything that had actually gone wrong was gone.
      // An engine that is loading is a WAIT, not twenty faults.
      if (this.runtime.lastError?.code !== 'QWEN_NOT_READY') {
        log('warn', 'worker.claim.deferred', { code: 'QWEN_NOT_READY', detail: probe.detail });
        this.recordError('QWEN_NOT_READY', probe.detail);
      }
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
      // Lane: the job asks for 'flagship' only when the control plane admitted it (premium
      // entitlement, enforced in SQL). Without a hosted lane on this node the turn still answers,
      // on the local lane, and the receipt says so instead of failing silently.
      const requestedLane: EngineLane = claim.request.engine_lane === 'flagship' ? 'flagship' : 'local';
      const engine = requestedLane === 'flagship' && this.hosted ? this.hosted : this.qwen;
      const lane: EngineLane = engine === this.hosted && this.hosted ? 'flagship' : 'local';
      const laneFallback = requestedLane === 'flagship' && lane === 'local' ? 'hosted lane is not enabled on this worker' : null;
      // Engine-facing limits come from config, not the client: test doubles carry no config.
      const engineConfig = (lane === 'flagship' ? hostedEngineConfig(this.config) : null) ?? this.config;
      if (laneFallback) log('warn', 'worker.lane.fallback', { job_id: claim.jobId, requested: requestedLane, detail: laneFallback });
      const probe = await engine.probe(abortController.signal);
      if (!probe.healthy) throw new QwenError(`${lane} engine is not ready: ${probe.detail}`, 'QWEN_NOT_READY', true);
      this.runtime.phase = 'retrieving';
      // Memory is loaded on every turn now, so it must not cost a federated probe on every turn.
      // The working set is served warm and revalidated behind the turn; only a cold or expired
      // conversation actually waits. See working-memory.ts.
      const memoryKey = conversationKey(claim);
      const held = await this.workingMemory.ensure(
        memoryKey,
        () => this.serializeMemoryOperation(
          () => retrieveMemory(this.config, claim, this.env, this.fetchImpl),
        ),
        turnQuestion(claim),
      );
      memory = held.bundle;
      log('info', 'worker.memory.working_set', {
        origin: held.origin,
        ageMs: held.ageMs,
        turnRecords: held.episodicRecords,
        ...this.workingMemory.stats(),
      });
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
      // Floor of 16 rather than 64: at the coder's measured ~95 char/s, 64 characters is another
      // two-thirds of a second of blank screen on top of prefill, for no benefit.
      const timedFlushChars = Math.min(this.config.chunkMaxChars, Math.max(16, Math.ceil(this.config.chunkMaxChars / 8)));
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
            metadata: { model_alias: engineConfig.modelAlias, engine_lane: lane },
          };
          this.runtime.phase = 'delivering';
          await enqueueChunk(chunk);
          seq += 1;
          lastFlushAt = Date.now();
          this.runtime.phase = 'generating';
        }
      };
      let firstFragmentSent = false;
      const onDelta = async (delta: string): Promise<void> => {
        if (abortController.signal.aborted) throw abortController.signal.reason;
        streamed = true;
        buffer += delta;
        // The FIRST fragment leaves the instant it exists, whatever its length.
        //
        // Prefill emits nothing at all, and at an 11k-token prompt that silence was measured at
        // 36 s. Making the reader then wait for a 64-character buffer on top of it is the
        // difference between "slow" and "frozen" -- and a reader who believes it is frozen gives
        // up or reloads, which is what the timeout reports look like. Later fragments still batch.
        if (!firstFragmentSent) {
          firstFragmentSent = true;
          await flush(true);
          return;
        }
        const timedFlushReady = buffer.length >= timedFlushChars && Date.now() - lastFlushAt >= this.config.chunkFlushMs;
        if (buffer.length >= this.config.chunkMaxChars || timedFlushReady) {
          await flush(false);
          if (timedFlushReady && buffer.length > 0) await flush(true);
        }
      };
      // The server's real window is authoritative; a larger configured window
      // would only produce overflow rejections.
      const serverContext = probe.contextTokens;
      const laneConfig: WorkerConfig = lane === 'flagship'
        ? { ...this.config, contextWindowTokens: engineConfig.contextWindowTokens, maxOutputTokens: engineConfig.maxOutputTokens }
        : this.config;
      const effectiveConfig = serverContext && serverContext < laneConfig.contextWindowTokens
        ? { ...laneConfig, contextWindowTokens: serverContext }
        : laneConfig;
      if (effectiveConfig !== this.config && !this.contextClampLogged) {
        this.contextClampLogged = true;
        log('warn', 'worker.qwen.context_clamped', {
          configured: this.config.contextWindowTokens,
          server_n_ctx: serverContext,
        });
      }
      const generate = async (overflowRetry = false, nudge?: string): Promise<QwenResult> => {
        let request = composeQwenRequest(effectiveConfig, claim, memory as RetrievalBundle, { overflowRetry });
        if (!overflowRetry) {
          // Exact count from the server; fall back to the byte estimate when unavailable.
          const counter = (engine as { tokenCount?: QwenClient['tokenCount'] }).tokenCount;
          const count = counter ? await counter.call(engine, request.messages, abortController.signal) : null;
          const ceiling = effectiveConfig.contextWindowTokens - (request.generation.maxTokens ?? effectiveConfig.maxOutputTokens) - 64;
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
        let messages: QwenMessage[] = nudge ? [...request.messages, { role: 'system' as const, content: nudge }] : request.messages;
        // The owner's turns get Apocrypha's own memory tools (tools.ts): the model may search
        // UniRecall or any single region itself before answering. Rounds that offer tools are held
        // back (a reply that turns into tool calls must not reach the reader half-said); the last
        // round offers none and streams, so what is stored is exactly what was streamed.
        if (toolsAllowed(claim) && !overflowRetry) {
          messages = [{ role: 'system', content: TOOL_GUIDANCE }, ...messages];
          for (let round = 0; round < MAX_TOOL_ROUNDS; round += 1) {
            const held: string[] = [];
            const step = await engine.generate(messages, { ...request.generation, tools: MEMORY_TOOLS as unknown as ReadonlyArray<Record<string, unknown>> }, (delta) => { held.push(delta); }, abortController.signal);
            if (!step.toolCalls?.length) {
              for (const delta of held) await onDelta(delta);
              return step;
            }
            const results = await Promise.all(step.toolCalls.slice(0, 6).map(async (call) => {
              const output = await runTool(call);
              log('info', 'worker.tool.ran', { job_id: claim.jobId, tool: call.name, chars: output.length, round });
              return { call, output };
            }));
            messages = [
              ...messages,
              { role: 'assistant', content: step.content, tool_calls: results.map(({ call }) => ({ id: call.id, type: 'function' as const, function: { name: call.name, arguments: call.arguments } })) },
              ...results.map(({ call, output }) => ({ role: 'tool' as const, content: output, tool_call_id: call.id })),
            ];
          }
        }
        return engine.generate(messages, request.generation, onDelta, abortController.signal);
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
      // An answer can come back empty: the model can spend its whole output budget
      // before it says anything, or stop at a boundary having said nothing. Delivered
      // as-is that reaches the reader as a blank turn with nothing to explain it --
      // the same silent-absence failure the expectation ledger exists to catch. Nothing
      // has been streamed in that case, so one retry is safe; a second blank fails the
      // job loudly rather than publishing silence.
      if (!result.content.trim() && !streamed) {
        log('warn', 'worker.qwen.empty_completion', {
          job_id: claim.jobId,
          attempt_id: claim.attemptId,
          prompt_tokens: result.usage.promptTokens,
          completion_tokens: result.usage.completionTokens,
          phase: 'first_attempt',
        });
        result = await generate(false, EMPTY_COMPLETION_NUDGE);
        if (!result.content.trim()) {
          log('error', 'worker.qwen.empty_completion', {
            job_id: claim.jobId,
            attempt_id: claim.attemptId,
            prompt_tokens: result.usage.promptTokens,
            completion_tokens: result.usage.completionTokens,
            phase: 'after_retry',
          });
          throw new QwenError('Qwen produced no answer twice', 'QWEN_EMPTY_COMPLETION', true);
        }
      }
      await flush(true);
      // A leaked thought is never the answer: the stored revision is what a reader may see.
      const shown = presentable(result.content);
      if (shown.withheld) log('warn', 'worker.answer.withheld', { job_id: claim.jobId, lane, why: shown.withheld });
      const hostedRate = lane === 'flagship' ? this.config.hosted : null;
      const totalCostUsd = hostedRate
        ? ((result.usage.promptTokens ?? 0) * hostedRate.promptUsdPerMillion
          + (result.usage.completionTokens ?? 0) * hostedRate.completionUsdPerMillion) / 1_000_000
        : 0;
      const completion = {
        content: shown.text,
        revisionRole: 'primary' as const,
        provenance: {
          model_alias: result.model,
          engine_lane: lane,
          lane_requested: requestedLane,
          ...(laneFallback ? { lane_fallback: laneFallback } : {}),
          withheld: shown.withheld,
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
          elapsed_s: Math.round(result.durationMs / 100) / 10,
          engine_lane: lane,
          model: result.model,
          total_cost_usd: Math.round(totalCostUsd * 1_000_000) / 1_000_000,
          withheld: shown.withheld !== null,
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
    const at = new Date().toISOString();
    this.runtime.lastError = { code, detail, at };
    this.runtime.recentErrors.push({
      code,
      detail: detail.slice(0, 300),
      at,
      phase: this.runtime.phase,
      jobId: this.runtime.currentJobId,
      attemptId: this.runtime.currentAttemptId,
    });
    if (this.runtime.recentErrors.length > 20) this.runtime.recentErrors.splice(0, this.runtime.recentErrors.length - 20);
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
      // Unconditional: the probe came back, which is the fact this records.
      this.runtime.adapterProbeRanAt = new Date().toISOString();
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
        // The control plane collapses most failures to a generic 503; its `code`
        // (and whether it was a network fault at all) is the only signal we get.
        log('warn', 'worker.heartbeat.failed', {
          detail: boundedError(error),
          code: error instanceof ControlPlaneError ? error.code : null,
          status: error instanceof ControlPlaneError ? error.status : null,
        });
      } finally {
        this.heartbeatInFlight = false;
      }
    };
    void send();
    this.heartbeatTimer = setInterval(() => void send(), this.config.heartbeatIntervalMs);
  }
}
