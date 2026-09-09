import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { once } from 'node:events';
import { loadConfig, loadManifest, memoryManifestHash } from '../scripts/apocrypha-worker/config';
import { ControlPlaneError } from '../scripts/apocrypha-worker/control-plane';
import { AttemptJournal } from '../scripts/apocrypha-worker/journal';
import { probeMemoryAdapters, retrieveMemory } from '../scripts/apocrypha-worker/retrieval';
import { QwenClient, QwenError } from '../scripts/apocrypha-worker/qwen';
import { ApocryphaWorker } from '../scripts/apocrypha-worker/worker';
import type { ClaimedJob, WorkerConfig } from '../scripts/apocrypha-worker/types';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

async function body(request: IncomingMessage): Promise<Record<string, unknown>> {
  const chunks: Buffer[] = [];
  for await (const chunk of request) chunks.push(Buffer.from(chunk));
  return JSON.parse(Buffer.concat(chunks).toString('utf8') || '{}') as Record<string, unknown>;
}

function json(response: ServerResponse, status: number, value: unknown): void {
  response.statusCode = status;
  response.setHeader('content-type', 'application/json');
  response.end(JSON.stringify(value));
}

async function listen(handler: (request: IncomingMessage, response: ServerResponse) => void | Promise<void>): Promise<{ url: string; close: () => Promise<void> }> {
  const server = createServer((request, response) => void Promise.resolve(handler(request, response)).catch((error) => {
    json(response, 500, { code: 'MOCK_ERROR', detail: error instanceof Error ? error.message : String(error) });
  }));
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('mock server did not expose a TCP address');
  return {
    url: `http://127.0.0.1:${address.port}`,
    close: async () => {
      server.close();
      await once(server, 'close');
    },
  };
}

function config(controlPlaneUrl: string, qwenUrl: string, journalDir: string): WorkerConfig {
  const manifest = loadManifest(join(process.cwd(), 'scripts', 'apocrypha-worker', 'manifest.production.json'));
  return {
    controlPlaneUrl,
    nodeId: 'test-node',
    nodeToken: 'test-node-token-never-log',
    qwenBaseUrl: `${qwenUrl}/v1`,
    runtimeProfilePath: null,
    modelAlias: manifest.model.alias,
    profileHash: manifest.model.profileHash,
    toolRegistryVersion: manifest.tools.registryVersion,
    memoryManifestHash: memoryManifestHash(manifest),
    manifest,
    pollIntervalMs: 10,
    claimLeaseSeconds: 180,
    leaseRenewIntervalMs: 40,
    leaseExpiryGraceMs: 10,
    controlPlaneTimeoutMs: 2_000,
    chunkFlushMs: 20,
    chunkMaxChars: 64,
    qwenIdleTimeoutMs: 2_000,
    qwenMaxRuntimeMs: 10_000,
    contextWindowTokens: 4_096,
    maxOutputTokens: 512,
    journalDir,
    healthHost: '127.0.0.1',
    healthPort: 19_991,
    heartbeatIntervalMs: 1_000,
    heartbeatEnabled: false,
    memoryReadConcurrency: 1,
    memoryProbeTenantId: '11111111-1111-4111-8111-111111111111',
    memoryProbePrincipalId: '22222222-2222-4222-8222-222222222222',
    memoryProbeCapability: 'chaos_tarot_reading',
    once: true,
    probeOnly: false,
    recoverOnly: false,
  };
}

function claimedJob(workerConfig: WorkerConfig): ClaimedJob {
  return {
    jobId: '10000000-0000-4000-8000-000000000001',
    attemptId: '20000000-0000-4000-8000-000000000001',
    attemptNo: 1,
    leaseEpoch: 1,
    leaseToken: 'lease-token-secret',
    leaseExpiresAt: new Date(Date.now() + 180_000).toISOString(),
    tenantId: '30000000-0000-4000-8000-000000000001',
    ownerPrincipalId: '40000000-0000-4000-8000-000000000001',
    kind: 'chaos_oracle',
    capability: 'chaos_tarot_reading',
    request: { prompt: 'PROMPT_MARKER Interpret the Tower crossing the Star with practical specificity.', output_budget: 384 },
    modelAlias: workerConfig.modelAlias,
    profileHash: workerConfig.profileHash,
    toolRegistryVersion: workerConfig.toolRegistryVersion,
    memoryManifestHash: workerConfig.memoryManifestHash,
  };
}

async function main(): Promise<void> {
  const parsedConfig = loadConfig({
    NODE_ENV: 'test',
    APOCRYPHA_CONTROL_PLANE_URL: 'https://example.test',
    APOCRYPHA_WORKER_NODE_ID: '55555555-5555-4555-8555-555555555555',
    APOCRYPHA_WORKER_TOKEN: 'test-worker-token',
    APOCRYPHA_MEMORY_PROBE_TENANT_ID: '11111111-1111-4111-8111-111111111111',
    APOCRYPHA_MEMORY_PROBE_PRINCIPAL_ID: '22222222-2222-4222-8222-222222222222',
    APOCRYPHA_MEMORY_PROBE_CAPABILITY: 'chaos_tarot_reading',
    APOCRYPHA_MEMORY_ADDITIONAL_PROBE_SCOPES:
      '66666666-6666-4666-8666-666666666666:77777777-7777-4777-8777-777777777777:apocky_owner_chat,'
      + '88888888-8888-4888-8888-888888888888:99999999-9999-4999-8999-999999999999:apocky_member_chat',
  }, []);
  assert(parsedConfig.memoryAdditionalProbeScopes?.[0]?.capability === 'apocky_owner_chat',
    'loadConfig did not parse the additional owner readiness scope');
  assert(parsedConfig.memoryAdditionalProbeScopes?.[1]?.capability === 'apocky_member_chat',
    'loadConfig did not parse the member readiness scope');
  const journalDir = await mkdtemp(join(tmpdir(), 'apocrypha-worker-test-'));
  const qwenRequests: Array<Record<string, unknown>> = [];
  let activeMemoryRequests = 0;
  let peakMemoryRequests = 0;
  let firstMemoryCompletionAt = 0;
  let firstHeartbeatAt = 0;
  let qwenStreamFinishedAt = 0;
  let firstChunkAcknowledgedAt = 0;
  const memoryScopesSeen = new Set<string>();
  let denyOwnerProbeScope = false;
  const output = 'The Tower names the break already underway; the Star asks what remains worth carrying through it. '.repeat(5);
  const qwen = await listen(async (request, response) => {
    if (request.url === '/health') return json(response, 200, { status: 'ok' });
    if (request.url === '/v1/models') return json(response, 200, { data: [{ id: 'qwen35-35b-a3b-q4' }] });
    if (request.url === '/props') return json(response, 200, { default_generation_settings: { n_ctx: 4096 } });
    if (request.url === '/tokenize') {
      const received = await body(request);
      const tokens = Math.ceil(String(received.content ?? '').length / 3);
      return json(response, 200, { tokens: Array.from({ length: tokens }, (_item, index) => index) });
    }
    if (request.url === '/memory') {
      activeMemoryRequests += 1;
      peakMemoryRequests = Math.max(peakMemoryRequests, activeMemoryRequests);
      const received = await body(request);
      assert(received.read_only === true, 'memory request was not read-only');
      memoryScopesSeen.add(`${String(received.tenant_id)}:${String(received.principal_id)}:${String(received.capability)}`);
      assert([
        '30000000-0000-4000-8000-000000000001',
        '11111111-1111-4111-8111-111111111111',
        '33333333-3333-4333-8333-333333333333',
      ].includes(String(received.tenant_id)), 'memory request lost tenant boundary');
      if (denyOwnerProbeScope && received.tenant_id === '33333333-3333-4333-8333-333333333333') {
        activeMemoryRequests -= 1;
        return json(response, 403, { error: 'TENANT_DENIED', read_only: true });
      }
      await new Promise((resolve) => setTimeout(resolve, 80));
      activeMemoryRequests -= 1;
      if (firstMemoryCompletionAt === 0) firstMemoryCompletionAt = Date.now();
      return json(response, 200, { records: [{ id: 'tarot:tower-star', text: 'The Tower and Star pair disruption with chosen renewal.' }] });
    }
    if (request.url === '/unconfigured') {
      return json(response, 503, { error: 'ADAPTER_UNCONFIGURED', read_only: true });
    }
    if (request.url === '/slow-memory') {
      await new Promise((resolve) => setTimeout(resolve, 400));
      return json(response, 200, { records: [{ id: 'late', text: 'late record' }] });
    }
    if (request.url === '/v1/chat/completions') {
      const received = await body(request);
      qwenRequests.push(received);
      if (qwenRequests.length === 1) {
        return json(response, 400, { error: { message: 'request exceeds the available context window token limit' } });
      }
      response.statusCode = 200;
      response.setHeader('content-type', 'text/event-stream');
      for (const delta of [output.slice(0, 90), output.slice(90, 260), output.slice(260)]) {
        response.write(`data: ${JSON.stringify({ model: 'qwen35-35b-a3b-q4', choices: [{ delta: { content: delta } }] })}\n\n`);
        await new Promise((resolve) => setTimeout(resolve, 5));
      }
      response.write(`data: ${JSON.stringify({ choices: [], usage: { prompt_tokens: 100, completion_tokens: 80, total_tokens: 180 } })}\n\n`);
      response.end('data: [DONE]\n\n');
      qwenStreamFinishedAt = Date.now();
      return;
    }
    json(response, 404, { error: 'not_found' });
  });

  const chunks = new Map<number, string>();
  let completed: Record<string, unknown> | null = null;
  let claims = 0;
  const authorizations: string[] = [];
  let workerConfig: WorkerConfig;
  const control = await listen(async (request, response) => {
    authorizations.push(String(request.headers.authorization ?? ''));
    const received = await body(request);
    if (request.url === '/api/apocrypha/worker/claim') {
      claims += 1;
      await new Promise((resolve) => setTimeout(resolve, 20));
      return json(response, 200, claims === 1 ? { ok: true, data: [{
        job_id: claimedJob(workerConfig).jobId,
        attempt_id: claimedJob(workerConfig).attemptId,
        attempt_no: 1,
        lease_epoch: 1,
        lease_token: claimedJob(workerConfig).leaseToken,
        lease_expires_at: claimedJob(workerConfig).leaseExpiresAt,
        tenant_id: claimedJob(workerConfig).tenantId,
        owner_principal_id: claimedJob(workerConfig).ownerPrincipalId,
        kind: claimedJob(workerConfig).kind,
        capability: claimedJob(workerConfig).capability,
        request: claimedJob(workerConfig).request,
        model_alias: workerConfig.modelAlias,
        profile_hash: workerConfig.profileHash,
        tool_registry_version: workerConfig.toolRegistryVersion,
        memory_manifest_hash: workerConfig.memoryManifestHash,
      }] } : { ok: true, data: [] });
    }
    if (request.url === '/api/apocrypha/worker/heartbeat') {
      if (firstHeartbeatAt === 0) firstHeartbeatAt = Date.now();
      return json(response, 200, { ok: true });
    }
    assert(received.node_id === 'test-node', 'worker node identity missing');
    assert(received.lease_token === 'lease-token-secret', 'fence token missing');
    if (request.url === '/api/apocrypha/worker/lease') {
      return json(response, 200, { lease_expires_at: new Date(Date.now() + 180_000).toISOString(), cancel_requested: false });
    }
    if (request.url === '/api/apocrypha/worker/chunk') {
      const seq = Number(received.seq);
      const delta = String(received.delta);
      const existing = chunks.get(seq);
      assert(existing === undefined || existing === delta, 'idempotent chunk sequence changed content');
      chunks.set(seq, delta);
      await new Promise((resolve) => setTimeout(resolve, 120));
      if (firstChunkAcknowledgedAt === 0) firstChunkAcknowledgedAt = Date.now();
      return json(response, 200, { chunk_id: seq + 1 });
    }
    if (request.url === '/api/apocrypha/worker/complete') {
      completed = received;
      return json(response, 200, { revision: { id: 'revision-1' }, job: { status: 'succeeded' } });
    }
    if (request.url === '/api/apocrypha/worker/fail') return json(response, 200, { job: { status: 'queued' } });
    json(response, 404, { error: 'not_found' });
  });

  try {
    workerConfig = {
      ...config(control.url, qwen.url, journalDir),
      heartbeatEnabled: true,
      heartbeatIntervalMs: 60_000,
    };
    const env = {
      ...process.env,
      APOCRYPHA_MEMPALACE_READ_URL: `${qwen.url}/memory`,
      APOCRYPHA_MEMPALACE_READ_TOKEN: 'memory-read-token',
      APOCRYPHA_BRAINMONSOON_READ_URL: `${qwen.url}/unconfigured`,
      APOCRYPHA_BRAINMONSOON_READ_TOKEN: 'memory-read-token',
      APOCRYPHA_ANAMNESIS_READ_URL: `${qwen.url}/slow-memory`,
      APOCRYPHA_ANAMNESIS_READ_TOKEN: 'memory-read-token',
      APOCRYPHA_ANAMNESIS_READ_TIMEOUT_MS: '250',
    };
    const worker = new ApocryphaWorker(workerConfig, { env });
    await worker.run();
    worker.stop('test complete');

    assert(worker.runtime.completedJobs === 1, 'worker did not complete claimed job');
    assert(firstHeartbeatAt > 0 && firstHeartbeatAt < firstMemoryCompletionAt,
      'adapter probe blocked publication of the worker heartbeat');
    assert(completed !== null, 'completion was not delivered');
    const completion = completed as Record<string, unknown>;
    assert(completion.content === output, 'completion content differs from streamed Qwen output');
    assert(completion.revision_role === 'primary', 'Qwen completion was not committed as primary');
    assert([...chunks.values()].join('') === output, 'buffered chunks do not reconstruct the final output');
    assert([...chunks.values()].every((value) => value.length <= 64), 'chunk exceeded configured buffer size');
    assert(qwenStreamFinishedAt > 0 && firstChunkAcknowledgedAt > qwenStreamFinishedAt,
      'slow control-plane chunk delivery blocked Qwen generation');
    assert(authorizations.every((value) => value === 'Bearer test-node-token-never-log'), 'worker bearer authentication missing');
    assert(qwenRequests.length === 2, 'context rejection did not cause exactly one bounded retry');
    const qwenRequest = qwenRequests[1] as Record<string, unknown>;
    assert(qwenRequest.model === 'qwen35-35b-a3b-q4', 'worker did not use accepted Qwen alias');
    assert(qwenRequests.every((request) => request.max_tokens === 384), 'worker ignored the claimed output_budget');
    assert((qwenRequest.chat_template_kwargs as Record<string, unknown>).enable_thinking === false, 'ordinary reading left model thinking enabled');
    const messages = qwenRequest.messages as Array<{ role: string; content: string }>;
    assert(messages[0]?.content.includes('tarot:tower-star'), 'admitted memory provenance was not supplied to Qwen');
    assert(messages.some((message) => message.content.includes('PROMPT_MARKER')), 'overflow retry lost the user prompt');
    const firstBytes = Buffer.byteLength(JSON.stringify(qwenRequests[0]?.messages), 'utf8');
    const retryBytes = Buffer.byteLength(JSON.stringify(qwenRequests[1]?.messages), 'utf8');
    assert(retryBytes < firstBytes, 'context retry did not use a smaller deterministic prompt');
    assert(worker.runtime.adapterStates.brainmonsoon === 'unconfigured', 'gateway unconfigured state was flattened to a generic error');
    assert(worker.runtime.adapterStates.anamnesis === 'timeout', 'per-adapter timeout override was not enforced');
    assert(worker.runtime.adapterProbeAt === null, 'partial adapter configuration minted fresh operational evidence');
    assert(await worker.journal.pendingCount() === 0, 'journal remained after terminal server acknowledgement');

    const probeEnv: NodeJS.ProcessEnv = { ...env };
    for (const adapter of workerConfig.manifest.memory.adapters) probeEnv[adapter.urlEnv] = `${qwen.url}/memory`;
    const dualScopeConfig: WorkerConfig = {
      ...workerConfig,
      memoryAdditionalProbeScopes: [{
        tenantId: '33333333-3333-4333-8333-333333333333',
        principalId: '44444444-4444-4444-8444-444444444444',
        capability: 'apocky_owner_chat',
      }],
    };
    const successfulChaosRead = await retrieveMemory(dualScopeConfig, claimedJob(workerConfig), probeEnv);
    assert(successfulChaosRead.results.every((result) => result.state === 'ok') && successfulChaosRead.probedAt === null,
      'successful single-scope Chaos job minted global adapter freshness');
    const operationalProbe = await probeMemoryAdapters(dualScopeConfig, probeEnv);
    assert(operationalProbe?.probedAt !== null, 'six real adapter reads did not mint probe freshness');
    assert(operationalProbe?.results.length === 6 && operationalProbe.results.every((result) => result.state === 'ok'),
      'periodic adapter probe did not report all six runtime states');
    assert(peakMemoryRequests === 1, 'heartbeat probe overlapped job retrieval or bounded adapter reads');
    assert(memoryScopesSeen.has('11111111-1111-4111-8111-111111111111:22222222-2222-4222-8222-222222222222:chaos_tarot_reading'),
      'primary Chaos readiness scope was not probed');
    assert(memoryScopesSeen.has('33333333-3333-4333-8333-333333333333:44444444-4444-4444-8444-444444444444:apocky_owner_chat'),
      'additional Apocky owner readiness scope was not probed');
    denyOwnerProbeScope = true;
    const deniedOwnerProbe = await probeMemoryAdapters(dualScopeConfig, probeEnv);
    assert(deniedOwnerProbe?.probedAt === null && deniedOwnerProbe?.results.every((result) => result.state === 'denied'),
      'denied Apocky owner scope left the memory rail falsely healthy');
    denyOwnerProbeScope = false;
    const failedProbeEnv = { ...probeEnv, APOCRYPHA_GRAPHIFY_READ_URL: `${qwen.url}/unconfigured` };
    const failedProbe = await probeMemoryAdapters(workerConfig, failedProbeEnv);
    assert(failedProbe?.probedAt === null, 'failed adapter result minted fresh operational evidence');
  } finally {
    await Promise.all([control.close(), qwen.close()]);
    await rm(journalDir, { recursive: true, force: true });
  }

  const deliveryJoinDir = await mkdtemp(join(tmpdir(), 'apocrypha-worker-delivery-join-test-'));
  try {
    const deliveryJoinConfig = {
      ...config('http://127.0.0.1:1', 'http://127.0.0.1:2', deliveryJoinDir),
      leaseRenewIntervalMs: 60_000,
    };
    const journal = new AttemptJournal(
      deliveryJoinDir,
      deliveryJoinConfig.nodeToken,
      deliveryJoinConfig.nodeId,
    );
    await journal.initialize();
    let releaseAppend!: () => void;
    const appendGate = new Promise<void>((resolve) => { releaseAppend = resolve; });
    let appendStarted = false;
    let failureDelivered = false;
    let qwenPassedDelta = false;
    const controlPlane = {
      appendChunk: async () => {
        appendStarted = true;
        await appendGate;
        return {};
      },
      fail: async () => {
        failureDelivered = true;
        return {};
      },
      renew: async () => ({
        leaseExpiresAt: new Date(Date.now() + 180_000).toISOString(),
        cancelRequested: false,
      }),
    };
    const qwenClient = {
      probe: async () => ({ healthy: true, model: deliveryJoinConfig.modelAlias, detail: 'ok' }),
      generate: async (
        _messages: unknown,
        _generation: unknown,
        onDelta: (delta: string) => Promise<void>,
      ) => {
        await onDelta('x'.repeat(deliveryJoinConfig.chunkMaxChars));
        qwenPassedDelta = true;
        throw new QwenError('forced generation failure', 'QWEN_HTTP_500', true);
      },
    };
    const worker = new ApocryphaWorker(deliveryJoinConfig, {
      controlPlane: controlPlane as never,
      qwen: qwenClient as never,
      journal,
      env: { NODE_ENV: 'test' },
    });
    let processSettled = false;
    const processPromise = (worker as unknown as { processClaim: (claim: ClaimedJob) => Promise<void> })
      .processClaim(claimedJob(deliveryJoinConfig))
      .finally(() => { processSettled = true; });
    for (let attempt = 0; attempt < 50 && !appendStarted; attempt += 1) {
      await new Promise((resolve) => setTimeout(resolve, 2));
    }
    assert(appendStarted, 'in-flight chunk delivery was not reached');
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert(qwenPassedDelta, 'slow chunk delivery blocked Qwen after its delta callback');
    assert(!processSettled, 'worker crossed the terminal boundary before joining chunk delivery');
    assert(!failureDelivered, 'worker delivered failure before the in-flight chunk settled');
    assert(await journal.pendingCount() === 1, 'in-flight chunk was not durably journaled');
    releaseAppend();
    await processPromise;
    assert(failureDelivered, 'worker did not deliver failure after joining chunk delivery');
    assert(await journal.pendingCount() === 0, 'late chunk acknowledgement resurrected the terminal journal');
  } finally {
    await rm(deliveryJoinDir, { recursive: true, force: true });
  }

  const deliveryFenceDir = await mkdtemp(join(tmpdir(), 'apocrypha-worker-delivery-fence-test-'));
  try {
    const deliveryFenceConfig = {
      ...config('http://127.0.0.1:1', 'http://127.0.0.1:2', deliveryFenceDir),
      leaseRenewIntervalMs: 60_000,
    };
    const journal = new AttemptJournal(deliveryFenceDir, deliveryFenceConfig.nodeToken, deliveryFenceConfig.nodeId);
    await journal.initialize();
    let secondDeltaAccepted = false;
    let failureDelivered = false;
    const fenceControlPlane = {
      appendChunk: async () => {
        throw new ControlPlaneError('stale delivery fence', {
          status: 409,
          code: 'STALE_FENCE',
          fenceLost: true,
        });
      },
      fail: async () => {
        failureDelivered = true;
        return {};
      },
      renew: async () => ({
        leaseExpiresAt: new Date(Date.now() + 180_000).toISOString(),
        cancelRequested: false,
      }),
    };
    const fenceAwareQwen = {
      probe: async () => ({ healthy: true, model: deliveryFenceConfig.modelAlias, detail: 'ok' }),
      generate: async (
        _messages: unknown,
        _generation: unknown,
        onDelta: (delta: string) => Promise<void>,
        signal: AbortSignal,
      ) => {
        await onDelta('x'.repeat(deliveryFenceConfig.chunkMaxChars));
        for (let attempt = 0; attempt < 50 && !signal.aborted; attempt += 1) {
          await new Promise((resolve) => setTimeout(resolve, 2));
        }
        assert(signal.aborted, 'definitive stale fence did not abort Qwen');
        await onDelta('must not be accepted');
        secondDeltaAccepted = true;
        throw new Error('unreachable');
      },
    };
    const worker = new ApocryphaWorker(deliveryFenceConfig, {
      controlPlane: fenceControlPlane as never,
      qwen: fenceAwareQwen as never,
      journal,
      env: { NODE_ENV: 'test' },
    });
    await (worker as unknown as { processClaim: (claim: ClaimedJob) => Promise<void> })
      .processClaim(claimedJob(deliveryFenceConfig));
    assert(!secondDeltaAccepted, 'Qwen accepted output after definitive fence loss');
    assert(!failureDelivered, 'stale-fenced attempt was sent through ordinary failure');
    assert(await journal.pendingCount() === 0, 'stale-fenced attempt remained in active recovery journal');
  } finally {
    await rm(deliveryFenceDir, { recursive: true, force: true });
  }

  const completionRecoveryDir = await mkdtemp(join(tmpdir(), 'apocrypha-worker-completion-recovery-test-'));
  try {
    const recoveryConfig = {
      ...config('http://127.0.0.1:1', 'http://127.0.0.1:2', completionRecoveryDir),
      leaseRenewIntervalMs: 60_000,
    };
    const journal = new AttemptJournal(completionRecoveryDir, recoveryConfig.nodeToken, recoveryConfig.nodeId);
    await journal.initialize();
    let completionJournaled = false;
    const setCompletion = journal.setCompletion.bind(journal);
    journal.setCompletion = async (state, payload) => {
      await setCompletion(state, payload);
      completionJournaled = true;
    };
    const recoveryOutput = 'A completed Oracle answer must survive a slow or unavailable delivery boundary. '.repeat(3);
    let releaseFailedAppend!: () => void;
    const failedAppendGate = new Promise<void>((resolve) => { releaseFailedAppend = resolve; });
    let appendStarted = false;
    let originalSettled = false;
    let unexpectedFailure = false;
    const firstControlPlane = {
      appendChunk: async () => {
        appendStarted = true;
        await failedAppendGate;
        throw new Error('forced append outage');
      },
      fail: async () => {
        unexpectedFailure = true;
        return {};
      },
      renew: async () => ({
        leaseExpiresAt: new Date(Date.now() + 180_000).toISOString(),
        cancelRequested: false,
      }),
    };
    const completingQwen = {
      probe: async () => ({ healthy: true, model: recoveryConfig.modelAlias, detail: 'ok' }),
      generate: async (
        _messages: unknown,
        _generation: unknown,
        onDelta: (delta: string) => Promise<void>,
      ) => {
        await onDelta(recoveryOutput);
        return {
          content: recoveryOutput,
          usage: { promptTokens: 10, completionTokens: 20, totalTokens: 30 },
          model: recoveryConfig.modelAlias,
          firstTokenMs: 1,
          durationMs: 2,
        };
      },
    };
    const firstWorker = new ApocryphaWorker(recoveryConfig, {
      controlPlane: firstControlPlane as never,
      qwen: completingQwen as never,
      journal,
      env: { NODE_ENV: 'test' },
    });
    const firstProcess = (firstWorker as unknown as { processClaim: (claim: ClaimedJob) => Promise<void> })
      .processClaim(claimedJob(recoveryConfig))
      .finally(() => { originalSettled = true; });
    for (let attempt = 0; attempt < 500 && !(appendStarted && completionJournaled); attempt += 1) {
      await new Promise((resolve) => setTimeout(resolve, 2));
    }
    const savedBeforeDelivery = (await journal.list())[0]?.state;
    assert(appendStarted, 'completion recovery never reached the blocked append');
    assert(savedBeforeDelivery?.terminal?.kind === 'complete', 'completed Qwen answer was not journaled before delivery');
    assert(savedBeforeDelivery.terminal.payload.content === recoveryOutput, 'journaled completion changed Qwen output');
    assert(savedBeforeDelivery.pendingChunks.length > 0, 'completion recovery fixture has no pending chunks');
    assert(!originalSettled, 'worker completed while its delivery boundary was still blocked');
    releaseFailedAppend();
    await firstProcess;
    assert(!unexpectedFailure, 'delivery outage replaced a completed answer with failure');
    assert(await journal.pendingCount() === 1, 'delivery outage discarded the recoverable completed answer');

    const replayedChunks: Array<{ seq: number; delta: string }> = [];
    let replayedCompletionContent: unknown = null;
    let recoveryRenewals = 0;
    let latestRecoveryLeaseExpiry = 0;
    const recoveryControlPlane = {
      renew: async () => {
        recoveryRenewals += 1;
        latestRecoveryLeaseExpiry = Date.now() + 300;
        return {
          leaseExpiresAt: new Date(latestRecoveryLeaseExpiry).toISOString(),
          cancelRequested: false,
        };
      },
      appendChunk: async (_fence: unknown, chunk: { seq: number; delta: string }) => {
        await new Promise((resolve) => setTimeout(resolve, 120));
        replayedChunks.push({ seq: chunk.seq, delta: chunk.delta });
        return {};
      },
      complete: async (_fence: unknown, completion: Record<string, unknown>) => {
        assert(Date.now() < latestRecoveryLeaseExpiry, 'recovery committed after its latest lease expired');
        replayedCompletionContent = completion.content;
        return {};
      },
      fail: async () => { throw new Error('recovery must not fail a completed answer'); },
    };
    const recoveryWorker = new ApocryphaWorker({ ...recoveryConfig, leaseRenewIntervalMs: 50 }, {
      controlPlane: recoveryControlPlane as never,
      qwen: completingQwen as never,
      journal,
      env: { NODE_ENV: 'test' },
    });
    await recoveryWorker.recoverPendingAttempts();
    assert(replayedChunks.map((chunk) => chunk.seq).join(',') === '0,1,2,3', 'recovery did not replay chunks in exact order');
    assert(replayedChunks.map((chunk) => chunk.delta).join('') === recoveryOutput, 'recovery changed the completed output bytes');
    assert(replayedCompletionContent === recoveryOutput, 'recovery did not commit the exact saved completion');
    assert(recoveryRenewals > 1, 'long recovery replay did not renew its lease');
    assert(await journal.pendingCount() === 0, 'recovered completion journal remained after acknowledgement');

    const ambiguousState = await journal.create(claimedJob(recoveryConfig));
    await journal.setCompletion(ambiguousState, {
      content: 'already committed remotely',
      revisionRole: 'primary',
    });
    let ambiguousReplayCount = 0;
    const ambiguousWorker = new ApocryphaWorker(recoveryConfig, {
      controlPlane: {
        renew: async () => { throw new Error('terminal attempt cannot renew'); },
        complete: async () => {
          ambiguousReplayCount += 1;
          return {};
        },
      } as never,
      qwen: completingQwen as never,
      journal,
      env: { NODE_ENV: 'test' },
    });
    await ambiguousWorker.recoverPendingAttempts();
    assert(ambiguousReplayCount === 1, 'ambiguous terminal acknowledgement was not replayed exactly once');
    assert(await journal.pendingCount() === 0, 'acknowledged ambiguous terminal journal was not removed');

    const pendingState = await journal.create(claimedJob(recoveryConfig));
    await journal.addPendingChunk(pendingState, { seq: 0, chunkKind: 'token', delta: 'must remain pending' });
    await journal.setCompletion(pendingState, { content: 'must remain recoverable', revisionRole: 'primary' });
    let unsafeTerminalReplay = false;
    const pendingRenewalWorker = new ApocryphaWorker(recoveryConfig, {
      controlPlane: {
        renew: async () => { throw new Error('transient renewal outage'); },
        complete: async () => {
          unsafeTerminalReplay = true;
          return {};
        },
      } as never,
      qwen: completingQwen as never,
      journal,
      env: { NODE_ENV: 'test' },
    });
    await pendingRenewalWorker.recoverPendingAttempts();
    assert(!unsafeTerminalReplay, 'terminal replay bypassed undelivered pending chunks');
    assert(await journal.pendingCount() === 1, 'renewal outage discarded pending completed output');
  } finally {
    await rm(completionRecoveryDir, { recursive: true, force: true });
  }

  const encryptedDir = await mkdtemp(join(tmpdir(), 'apocrypha-journal-test-'));
  try {
    const workerConfig2 = config('http://127.0.0.1:1', 'http://127.0.0.1:2', encryptedDir);
    const journal = new AttemptJournal(encryptedDir, workerConfig2.nodeToken, workerConfig2.nodeId);
    const state = await journal.create(claimedJob(workerConfig2));
    await journal.addPendingChunk(state, { seq: 0, chunkKind: 'token', delta: 'private-output-fragment' });
    const name = (await readdir(encryptedDir)).find((item) => item.endsWith('.journal'));
    assert(name, 'encrypted journal file was not created');
    const raw = await readFile(join(encryptedDir, name), 'utf8');
    assert(!raw.includes('private-output-fragment'), 'journal exposed output plaintext');
    assert(!raw.includes('lease-token-secret'), 'journal exposed lease token plaintext');
  } finally {
    await rm(encryptedDir, { recursive: true, force: true });
  }

  const boundedConfig = config('http://127.0.0.1:1', 'http://127.0.0.1:2', tmpdir());
  boundedConfig.maxOutputTokens = 64;
  const oversizedDelta = 'x'.repeat(4_097);
  const oversizedStream = `data: ${JSON.stringify({ choices: [{ delta: { content: oversizedDelta } }] })}\n\ndata: [DONE]\n\n`;
  const boundedQwen = new QwenClient(boundedConfig, (async () => new Response(oversizedStream, {
    status: 200,
    headers: { 'content-type': 'text/event-stream' },
  })) as typeof fetch);
  let outputLimitCode = '';
  try {
    await boundedQwen.generate([], { maxTokens: 64 }, () => undefined);
  } catch (error) {
    outputLimitCode = error instanceof QwenError ? error.code : String(error);
  }
  assert(outputLimitCode === 'QWEN_OUTPUT_LIMIT_EXCEEDED', 'malformed Qwen stream bypassed the bounded output limit');

  const unterminatedTransport = 'data: ' + 'x'.repeat(65_537);
  const rawBoundedQwen = new QwenClient(boundedConfig, (async () => new Response(unterminatedTransport, {
    status: 200,
    headers: { 'content-type': 'text/event-stream' },
  })) as typeof fetch);
  let transportLimitCode = '';
  try {
    await rawBoundedQwen.generate([], { maxTokens: 64 }, () => undefined);
  } catch (error) {
    transportLimitCode = error instanceof QwenError ? error.code : String(error);
  }
  assert(transportLimitCode === 'QWEN_TRANSPORT_LIMIT_EXCEEDED', 'unterminated Qwen stream bypassed the raw transport limit');

  console.log('apocrypha-worker.test : OK · serialized memory, bounded Qwen retry/output, durable ordered recovery, encrypted journal');
}

void main();
