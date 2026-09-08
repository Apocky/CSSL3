import { MEMORY_READINESS_QUERY, probeMemoryAdapters, retrieveMemory } from '../scripts/apocrypha-worker/retrieval';
import type { ClaimedJob, WorkerConfig } from '../scripts/apocrypha-worker/types';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

const adapterUrl = 'http://127.0.0.1:19126/memory';
const config: WorkerConfig = {
  controlPlaneUrl: 'https://example.test',
  nodeId: 'test-node',
  nodeToken: 'test-token',
  qwenBaseUrl: 'http://127.0.0.1:19127/v1',
  runtimeProfilePath: null,
  modelAlias: 'qwen35-35b-a3b-q4',
  profileHash: 'profile-hash',
  toolRegistryVersion: 'apocrypha-readonly-v1',
  memoryManifestHash: 'memory-manifest-hash',
  manifest: {
    schema: 'apocrypha.worker-manifest.v1',
    model: { alias: 'qwen35-35b-a3b-q4', profileHash: 'profile-hash', endpointEnv: 'APOCRYPHA_QWEN_BASE_URL' },
    tools: { registryVersion: 'apocrypha-readonly-v1', mode: 'read-only' },
    memory: {
      manifestVersion: 'test-memory-v1',
      tenantScoped: true,
      adapters: [{
        name: 'mempalace',
        urlEnv: 'APOCRYPHA_MEMPALACE_READ_URL',
        tokenEnv: 'APOCRYPHA_MEMPALACE_READ_TOKEN',
        timeoutMs: 250,
        maxChars: 1_000,
      }],
    },
    capabilities: ['chaos_tarot_reading'],
  },
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
  journalDir: '.',
  healthHost: '127.0.0.1',
  healthPort: 19_991,
  heartbeatIntervalMs: 1_000,
  heartbeatEnabled: false,
  memoryReadConcurrency: 1,
  memoryProbeTenantId: null,
  memoryProbePrincipalId: 'test-node',
  memoryProbeCapability: 'chaos_tarot_reading',
  once: true,
  probeOnly: false,
  recoverOnly: false,
};

const job: ClaimedJob = {
  jobId: '10000000-0000-4000-8000-000000000001',
  attemptId: '20000000-0000-4000-8000-000000000001',
  attemptNo: 1,
  leaseEpoch: 1,
  leaseToken: 'lease-token',
  leaseExpiresAt: new Date(Date.now() + 180_000).toISOString(),
  tenantId: '30000000-0000-4000-8000-000000000001',
  ownerPrincipalId: '40000000-0000-4000-8000-000000000001',
  kind: 'followup',
  capability: 'chaos_tarot_reading',
  request: { retrieval_query: 'bounded retry test' },
  modelAlias: config.modelAlias,
  profileHash: config.profileHash,
  toolRegistryVersion: config.toolRegistryVersion,
  memoryManifestHash: config.memoryManifestHash,
};

const env: NodeJS.ProcessEnv = {
  NODE_ENV: 'test',
  APOCRYPHA_MEMPALACE_READ_URL: adapterUrl,
  APOCRYPHA_MEMPALACE_READ_TOKEN: 'test-read-token',
};

function response(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

async function transientServerErrorRecovers(): Promise<void> {
  let calls = 0;
  const fetchImpl = (async () => {
    calls += 1;
    return calls === 1
      ? response(502, { error: 'ADAPTER_UNAVAILABLE', cause: 'NATIVE_MEMPALACE_UNAVAILABLE' })
      : response(200, { records: [{ id: 'memory:1', text: 'admitted memory' }] });
  }) as typeof fetch;

  const bundle = await retrieveMemory(config, job, env, fetchImpl);
  assert(calls === 2, 'transient 5xx did not receive exactly one retry');
  assert(bundle.results[0]?.state === 'ok', 'recovered adapter did not preserve its final ok state');
  assert(bundle.results[0]?.records[0]?.provenanceId === 'memory:1', 'recovered records were not admitted');
}

async function timeoutRecovers(): Promise<void> {
  let calls = 0;
  const fetchImpl = (async (_input: RequestInfo | URL, init?: RequestInit) => {
    calls += 1;
    if (calls === 1) {
      return new Promise<Response>((_resolve, reject) => {
        const signal = init?.signal;
        if (!signal) return reject(new Error('missing timeout signal'));
        const abort = () => reject(signal.reason ?? new Error('aborted'));
        if (signal.aborted) return abort();
        signal.addEventListener('abort', abort, { once: true });
      });
    }
    return response(200, { records: [{ id: 'memory:2', text: 'recovered after timeout' }] });
  }) as typeof fetch;

  const bundle = await retrieveMemory(config, job, env, fetchImpl);
  assert(calls === 2, 'timeout did not receive exactly one retry');
  assert(bundle.results[0]?.state === 'ok', 'timeout recovery did not preserve the final ok state');
  assert((bundle.results[0]?.durationMs ?? 0) >= 250, 'reported duration excluded the timed-out attempt');
  assert((bundle.results[0]?.durationMs ?? 0) <= 1_250, 'timeout recovery exceeded the total adapter deadline');
}

async function finalFailureRemainsVisible(): Promise<void> {
  let calls = 0;
  const fetchImpl = (async () => {
    calls += 1;
    if (calls === 1) throw new TypeError('fetch failed');
    return response(504, { error: 'ADAPTER_TIMEOUT' });
  }) as typeof fetch;

  const bundle = await retrieveMemory(config, job, env, fetchImpl);
  assert(calls === 2, 'network error did not receive exactly one retry');
  assert(bundle.results[0]?.state === 'timeout', 'terminal retry state was flattened or hidden');
  assert(bundle.results[0]?.detail === 'ADAPTER_TIMEOUT', 'terminal retry detail was not preserved');
  assert(bundle.results[0]?.records.length === 0, 'failed retry admitted records');
}

async function permanentFailureDoesNotRetry(): Promise<void> {
  let calls = 0;
  const fetchImpl = (async () => {
    calls += 1;
    return response(503, { error: 'ADAPTER_UNCONFIGURED' });
  }) as typeof fetch;

  const bundle = await retrieveMemory(config, job, env, fetchImpl);
  assert(calls === 1, 'explicitly unconfigured adapter was retried');
  assert(bundle.results[0]?.state === 'unconfigured', 'explicit unconfigured state was not preserved');
}

async function exhaustedTimeoutStaysBounded(): Promise<void> {
  let calls = 0;
  const fetchImpl = (async (_input: RequestInfo | URL, init?: RequestInit) => {
    calls += 1;
    return new Promise<Response>((_resolve, reject) => {
      const signal = init?.signal;
      if (!signal) return reject(new Error('missing timeout signal'));
      const abort = () => reject(signal.reason ?? new Error('aborted'));
      if (signal.aborted) return abort();
      signal.addEventListener('abort', abort, { once: true });
    });
  }) as typeof fetch;

  const started = Date.now();
  const bundle = await retrieveMemory(config, job, env, fetchImpl);
  const elapsed = Date.now() - started;
  assert(calls === 2, 'exhausted timeout did not remain at two attempts');
  assert(bundle.results[0]?.state === 'timeout', 'exhausted timeout did not preserve its terminal state');
  assert(elapsed <= 1_500, 'exhausted timeout exceeded configured timeout plus bounded retry grace');
}

async function readinessUsesTaskShapedRecall(): Promise<void> {
  const observed: string[] = [];
  const probeConfig: WorkerConfig = {
    ...config,
    memoryProbeTenantId: job.tenantId,
    memoryProbePrincipalId: job.ownerPrincipalId,
  };
  const fetchImpl = (async (_input: RequestInfo | URL, init?: RequestInit) => {
    const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
    observed.push(String(body.query ?? ''));
    return response(200, { records: [] });
  }) as typeof fetch;

  const bundle = await probeMemoryAdapters(probeConfig, env, fetchImpl);
  assert(bundle !== null, 'configured readiness probe did not run');
  assert(bundle.query === MEMORY_READINESS_QUERY, 'readiness bundle lost its task-shaped query');
  assert(observed.length === 1 && observed[0] === MEMORY_READINESS_QUERY,
    'readiness sent a synthetic health lookup instead of the representative recall query');
  assert(bundle.probedAt !== null, 'successful task-shaped recall was not marked complete');
}

async function operationalProbeAvoidsSelfContention(): Promise<void> {
  const adapters = ['mempalace', 'brainmonsoon', 'anamnesis'] as const;
  const probeConfig: WorkerConfig = {
    ...config,
    memoryReadConcurrency: 3,
    memoryProbeTenantId: job.tenantId,
    memoryProbePrincipalId: job.ownerPrincipalId,
    manifest: {
      ...config.manifest,
      memory: {
        ...config.manifest.memory,
        adapters: adapters.map((name) => ({
          name,
          urlEnv: `APOCRYPHA_${name.toUpperCase()}_READ_URL`,
          timeoutMs: 250,
          maxChars: 1_000,
        })),
      },
    },
  };
  const probeEnv: NodeJS.ProcessEnv = { NODE_ENV: 'test' };
  for (const adapter of probeConfig.manifest.memory.adapters) probeEnv[adapter.urlEnv] = adapterUrl;
  let active = 0;
  let peak = 0;
  const fetchImpl = (async () => {
    active += 1;
    peak = Math.max(peak, active);
    await new Promise((resolve) => setTimeout(resolve, 15));
    active -= 1;
    return response(200, { records: [] });
  }) as typeof fetch;

  const probe = await probeMemoryAdapters(probeConfig, probeEnv, fetchImpl);
  assert(probe?.probedAt !== null, 'bounded operational probe did not complete');
  assert(peak === 2, `operational probe used ${peak} concurrent local readers instead of two`);

  peak = 0;
  const retrieval = await retrieveMemory(probeConfig, job, probeEnv, fetchImpl);
  assert(retrieval.results.every((result) => result.state === 'ok'), 'live retrieval failed after probe fanout bound');
  assert(peak === 3, 'live retrieval lost its configured concurrency');
}

async function main(): Promise<void> {
  await transientServerErrorRecovers();
  await timeoutRecovers();
  await finalFailureRemainsVisible();
  await permanentFailureDoesNotRetry();
  await exhaustedTimeoutStaysBounded();
  await readinessUsesTaskShapedRecall();
  await operationalProbeAvoidsSelfContention();
  console.log('apocrypha-worker-retrieval.test : OK · bounded transient retries preserve final adapter truth');
}

void main();
