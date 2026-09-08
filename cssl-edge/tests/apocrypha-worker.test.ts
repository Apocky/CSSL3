import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { once } from 'node:events';
import { loadManifest, memoryManifestHash } from '../scripts/apocrypha-worker/config';
import { AttemptJournal } from '../scripts/apocrypha-worker/journal';
import { probeMemoryAdapters } from '../scripts/apocrypha-worker/retrieval';
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
    request: { prompt: 'Interpret the Tower crossing the Star with practical specificity.', max_tokens: 512 },
    modelAlias: workerConfig.modelAlias,
    profileHash: workerConfig.profileHash,
    toolRegistryVersion: workerConfig.toolRegistryVersion,
    memoryManifestHash: workerConfig.memoryManifestHash,
  };
}

async function main(): Promise<void> {
  const journalDir = await mkdtemp(join(tmpdir(), 'apocrypha-worker-test-'));
  const qwenRequests: Array<Record<string, unknown>> = [];
  const output = 'The Tower names the break already underway; the Star asks what remains worth carrying through it. '.repeat(5);
  const qwen = await listen(async (request, response) => {
    if (request.url === '/health') return json(response, 200, { status: 'ok' });
    if (request.url === '/v1/models') return json(response, 200, { data: [{ id: 'qwen35-35b-a3b-q4' }] });
    if (request.url === '/memory') {
      const received = await body(request);
      assert(received.read_only === true, 'memory request was not read-only');
      assert([
        '30000000-0000-4000-8000-000000000001',
        '11111111-1111-4111-8111-111111111111',
      ].includes(String(received.tenant_id)), 'memory request lost tenant boundary');
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
      response.statusCode = 200;
      response.setHeader('content-type', 'text/event-stream');
      for (const delta of [output.slice(0, 90), output.slice(90, 260), output.slice(260)]) {
        response.write(`data: ${JSON.stringify({ model: 'qwen35-35b-a3b-q4', choices: [{ delta: { content: delta } }] })}\n\n`);
        await new Promise((resolve) => setTimeout(resolve, 5));
      }
      response.write(`data: ${JSON.stringify({ choices: [], usage: { prompt_tokens: 100, completion_tokens: 80, total_tokens: 180 } })}\n\n`);
      response.end('data: [DONE]\n\n');
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
    workerConfig = config(control.url, qwen.url, journalDir);
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

    assert(worker.runtime.completedJobs === 1, 'worker did not complete claimed job');
    assert(completed !== null, 'completion was not delivered');
    const completion = completed as Record<string, unknown>;
    assert(completion.content === output, 'completion content differs from streamed Qwen output');
    assert(completion.revision_role === 'primary', 'Qwen completion was not committed as primary');
    assert([...chunks.values()].join('') === output, 'buffered chunks do not reconstruct the final output');
    assert([...chunks.values()].every((value) => value.length <= 64), 'chunk exceeded configured buffer size');
    assert(authorizations.every((value) => value === 'Bearer test-node-token-never-log'), 'worker bearer authentication missing');
    assert(qwenRequests.length === 1, 'Qwen was invoked more than once');
    const qwenRequest = qwenRequests[0] as Record<string, unknown>;
    assert(qwenRequest.model === 'qwen35-35b-a3b-q4', 'worker did not use accepted Qwen alias');
    assert((qwenRequest.chat_template_kwargs as Record<string, unknown>).enable_thinking === false, 'ordinary reading left model thinking enabled');
    const messages = qwenRequest.messages as Array<{ role: string; content: string }>;
    assert(messages[0]?.content.includes('tarot:tower-star'), 'admitted memory provenance was not supplied to Qwen');
    assert(worker.runtime.adapterStates.brainmonsoon === 'unconfigured', 'gateway unconfigured state was flattened to a generic error');
    assert(worker.runtime.adapterStates.anamnesis === 'timeout', 'per-adapter timeout override was not enforced');
    assert(worker.runtime.adapterProbeAt === null, 'partial adapter configuration minted fresh operational evidence');
    assert(await worker.journal.pendingCount() === 0, 'journal remained after terminal server acknowledgement');

    const probeEnv: NodeJS.ProcessEnv = { ...env };
    for (const adapter of workerConfig.manifest.memory.adapters) probeEnv[adapter.urlEnv] = `${qwen.url}/memory`;
    const operationalProbe = await probeMemoryAdapters(workerConfig, probeEnv);
    assert(operationalProbe?.probedAt !== null, 'five real adapter reads did not mint probe freshness');
    assert(operationalProbe?.results.length === 5 && operationalProbe.results.every((result) => result.state === 'ok'),
      'periodic adapter probe did not report all five runtime states');
  } finally {
    await Promise.all([control.close(), qwen.close()]);
    await rm(journalDir, { recursive: true, force: true });
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

  console.log('apocrypha-worker.test : OK · authenticated claim, tenant-scoped memory, Qwen stream, bounded chunks, terminal ack, encrypted journal');
}

void main();
