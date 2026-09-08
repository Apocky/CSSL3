import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { mkdtemp, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { once } from 'node:events';
import { loadManifest, memoryManifestHash } from '../scripts/apocrypha-worker/config';
import { AttemptJournal } from '../scripts/apocrypha-worker/journal';
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

async function listen(handler: (request: IncomingMessage, response: ServerResponse) => void | Promise<void>): Promise<{ url: string; close: () => Promise<void> }> {
  const server = createServer((request, response) => void Promise.resolve(handler(request, response)));
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('no address');
  return { url: `http://127.0.0.1:${address.port}`, close: async () => { server.close(); await once(server, 'close'); } };
}

function makeConfig(controlPlaneUrl: string, journalDir: string): WorkerConfig {
  const manifest = loadManifest(join(process.cwd(), 'scripts', 'apocrypha-worker', 'manifest.production.json'));
  return {
    controlPlaneUrl, nodeId: 'recovery-node', nodeToken: 'recovery-token', qwenBaseUrl: 'http://127.0.0.1:9/v1', runtimeProfilePath: null,
    modelAlias: manifest.model.alias, profileHash: manifest.model.profileHash,
    toolRegistryVersion: manifest.tools.registryVersion, memoryManifestHash: memoryManifestHash(manifest), manifest,
    pollIntervalMs: 10, claimLeaseSeconds: 180, leaseRenewIntervalMs: 10_000, leaseExpiryGraceMs: 5_000,
    controlPlaneTimeoutMs: 2_000, chunkFlushMs: 20, chunkMaxChars: 64, qwenIdleTimeoutMs: 2_000,
    qwenMaxRuntimeMs: 10_000, contextWindowTokens: 4_096, maxOutputTokens: 128, journalDir, healthHost: '127.0.0.1', healthPort: 19_992,
    heartbeatIntervalMs: 1_000, heartbeatEnabled: false,
    memoryProbeTenantId: '11111111-1111-4111-8111-111111111111',
    memoryProbePrincipalId: '22222222-2222-4222-8222-222222222222',
    memoryProbeCapability: 'chaos_tarot_reading',
    once: false, probeOnly: false, recoverOnly: true,
  };
}

function claim(config: WorkerConfig, suffix: string): ClaimedJob {
  return {
    jobId: `job-${suffix}`, attemptId: `attempt-${suffix}`, attemptNo: 1, leaseEpoch: 9,
    leaseToken: `lease-${suffix}`, leaseExpiresAt: new Date(Date.now() + 180_000).toISOString(),
    tenantId: 'tenant', ownerPrincipalId: 'principal', kind: 'apocky_chat', capability: 'apocky_owner_chat',
    request: { prompt: 'resume test' }, modelAlias: config.modelAlias, profileHash: config.profileHash,
    toolRegistryVersion: config.toolRegistryVersion, memoryManifestHash: config.memoryManifestHash,
  };
}

async function main(): Promise<void> {
  const directory = await mkdtemp(join(tmpdir(), 'apocrypha-recovery-'));
  const receivedChunks: string[] = [];
  let completed = 0;
  const control = await listen(async (request, response) => {
    const received = await body(request);
    response.setHeader('content-type', 'application/json');
    if (request.url === '/api/apocrypha/worker/lease') {
      response.end(JSON.stringify({ ok: true, data: new Date(Date.now() + 180_000).toISOString() }));
    } else if (request.url === '/api/apocrypha/worker/chunk') {
      receivedChunks.push(String(received.delta));
      response.end(JSON.stringify({ chunk_id: 1 }));
    } else if (request.url === '/api/apocrypha/worker/complete') {
      completed += 1;
      response.end(JSON.stringify({ job: { status: 'succeeded' } }));
    } else if (request.url === '/api/apocrypha/worker/fail') {
      response.end(JSON.stringify({ job: { status: 'queued' } }));
    } else {
      response.statusCode = 404;
      response.end('{}');
    }
  });
  try {
    const config = makeConfig(control.url, directory);
    const journal = new AttemptJournal(directory, config.nodeToken, config.nodeId);
    const state = await journal.create(claim(config, 'terminal'));
    await journal.addPendingChunk(state, { seq: 0, chunkKind: 'token', delta: 'uncertain-ack-chunk' });
    await journal.setCompletion(state, { content: 'uncertain-ack-chunk', revisionRole: 'primary' });
    const worker = new ApocryphaWorker(config);
    await worker.run();
    assert(receivedChunks.join('') === 'uncertain-ack-chunk', 'pending chunk was not replayed');
    assert(completed === 1, 'terminal completion was not replayed exactly once');
    assert(await worker.journal.pendingCount() === 0, 'acknowledged recovery journal was not removed');
    assert(worker.runtime.recoveredAttempts === 1, 'recovery was not counted');
  } finally {
    await control.close();
    await rm(directory, { recursive: true, force: true });
  }

  const fencedDirectory = await mkdtemp(join(tmpdir(), 'apocrypha-fenced-'));
  const fencedControl = await listen(async (_request, response) => {
    response.statusCode = 409;
    response.setHeader('content-type', 'application/json');
    response.end(JSON.stringify({ code: 'STALE_FENCE', detail: 'lease belongs to another epoch' }));
  });
  try {
    const config = makeConfig(fencedControl.url, fencedDirectory);
    const journal = new AttemptJournal(fencedDirectory, config.nodeToken, config.nodeId);
    await journal.create(claim(config, 'stale'));
    const worker = new ApocryphaWorker(config);
    await worker.run();
    assert(await worker.journal.pendingCount() === 0, 'stale fence remained active in journal');
    const orphaned = await readdir(join(fencedDirectory, 'orphaned'));
    assert(orphaned.length === 1, 'stale-fence journal was not preserved as an orphan');
  } finally {
    await fencedControl.close();
    await rm(fencedDirectory, { recursive: true, force: true });
  }

  console.log('apocrypha-worker-recovery.test : OK · pending ack replay, terminal replay, stale-fence rejection, orphan preservation');
}

void main();
