// The worker answers on the lane the job was admitted to: 'flagship' goes to the hosted
// engine (Opus 5.5 behind the loopback gateway proxy) with the llama-only dials dropped and a
// cost receipt; without a hosted lane the turn still answers locally and the receipt says so.
// Attachments from migration 0057 reach the system prompt as evidence. A leaked thought is
// never stored as the answer.
import { createServer, type IncomingMessage, type ServerResponse } from 'node:http';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { once } from 'node:events';
import { loadHostedLane, loadManifest, memoryManifestHash } from '../scripts/apocrypha-worker/config';
import { AttemptJournal } from '../scripts/apocrypha-worker/journal';
import { ApocryphaWorker } from '../scripts/apocrypha-worker/worker';
import type { ClaimedJob, CompletionPayload, WorkerConfig } from '../scripts/apocrypha-worker/types';

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

async function engine(alias: string, reply: string, seen: Array<Record<string, unknown>>, nCtx = 4096): Promise<{ url: string; close: () => Promise<void> }> {
  const server = createServer((request, response) => void (async () => {
    if (request.url === '/health') return json(response, 200, { status: 'ok' });
    if (request.url === '/v1/models') return json(response, 200, { data: [{ id: alias }] });
    if (request.url === '/props') return json(response, 200, { default_generation_settings: { n_ctx: nCtx } });
    if (request.url === '/tokenize') return json(response, 404, {});
    if (request.url === '/v1/chat/completions') {
      seen.push(await body(request));
      response.statusCode = 200;
      response.setHeader('content-type', 'text/event-stream');
      response.write(`data: ${JSON.stringify({ model: alias, choices: [{ delta: { content: reply } }] })}\n\n`);
      response.write(`data: ${JSON.stringify({ choices: [], usage: { prompt_tokens: 1_000, completion_tokens: 500, total_tokens: 1_500 } })}\n\n`);
      response.end('data: [DONE]\n\n');
      return;
    }
    json(response, 404, {});
  })().catch((error) => json(response, 500, { detail: String(error) })));
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('no port');
  return { url: `http://127.0.0.1:${address.port}`, close: async () => { server.close(); await once(server, 'close'); } };
}

function config(localUrl: string, hostedUrl: string | null, journalDir: string): WorkerConfig {
  const manifest = loadManifest(join(process.cwd(), 'scripts', 'apocrypha-worker', 'manifest.production.json'));
  return {
    controlPlaneUrl: 'https://control.test',
    nodeId: 'lane-test-node',
    nodeToken: 'lane-test-token',
    qwenBaseUrl: `${localUrl}/v1`,
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
    chunkMaxChars: 4_096,
    qwenIdleTimeoutMs: 2_000,
    qwenMaxRuntimeMs: 10_000,
    contextWindowTokens: 4_096,
    maxOutputTokens: 512,
    journalDir,
    healthHost: '127.0.0.1',
    healthPort: 19_992,
    heartbeatIntervalMs: 1_000,
    heartbeatEnabled: false,
    memoryReadConcurrency: 1,
    memoryProbeTenantId: '11111111-1111-4111-8111-111111111111',
    memoryProbePrincipalId: '22222222-2222-4222-8222-222222222222',
    memoryProbeCapability: 'apocky_member_chat',
    hosted: hostedUrl ? loadHostedLane({ NODE_ENV: 'test',
      APOCRYPHA_HOSTED_ENABLED: '1',
      APOCRYPHA_HOSTED_BASE_URL: `${hostedUrl}/v1`,
      APOCRYPHA_HOSTED_MODEL_ALIAS: 'anthropic/claude-opus-5.5',
    }) : null,
    once: true,
    probeOnly: false,
    recoverOnly: false,
  };
}

function claim(workerConfig: WorkerConfig, lane: 'local' | 'flagship'): ClaimedJob {
  return {
    jobId: '10000000-0000-4000-8000-00000000000a',
    attemptId: '20000000-0000-4000-8000-00000000000a',
    attemptNo: 1,
    leaseEpoch: 1,
    leaseToken: 'lease-token-secret',
    leaseExpiresAt: new Date(Date.now() + 180_000).toISOString(),
    tenantId: '30000000-0000-4000-8000-000000000001',
    ownerPrincipalId: '40000000-0000-4000-8000-000000000001',
    kind: 'apocky_chat',
    capability: 'apocky_member_chat',
    request: {
      question: 'What does the attached note say about the launch date?',
      conversation_history: [],
      engine_lane: lane,
      attachments: [{ id: 'a1', name: 'notes.txt', mime: 'text/plain', bytes: 42, text: 'ATTACHMENT_MARKER launch moved to the 14th' }],
    },
    modelAlias: workerConfig.modelAlias,
    profileHash: workerConfig.profileHash,
    toolRegistryVersion: workerConfig.toolRegistryVersion,
    memoryManifestHash: workerConfig.memoryManifestHash,
  };
}

async function run(workerConfig: WorkerConfig, lane: 'local' | 'flagship'): Promise<CompletionPayload> {
  const journal = new AttemptJournal(workerConfig.journalDir, workerConfig.nodeToken, workerConfig.nodeId);
  await journal.initialize();
  let completion: CompletionPayload | null = null;
  const controlPlane = {
    appendChunk: async () => ({}),
    complete: async (_fence: unknown, payload: CompletionPayload) => { completion = payload; return {}; },
    fail: async (_fence: unknown, payload: unknown) => { throw new Error(`job failed: ${JSON.stringify(payload)}`); },
    renew: async () => ({ leaseExpiresAt: new Date(Date.now() + 180_000).toISOString(), cancelRequested: false }),
  };
  const worker = new ApocryphaWorker(workerConfig, { controlPlane: controlPlane as never, journal, env: { NODE_ENV: 'test' } });
  await (worker as unknown as { processClaim: (job: ClaimedJob) => Promise<void> }).processClaim(claim(workerConfig, lane));
  assert(completion, 'no completion delivered');
  return completion;
}

async function main(): Promise<void> {
  // Defaults: the hosted lane is off unless the owner turns it on (law L10).
  assert(loadHostedLane({ NODE_ENV: 'test' }) === null, 'hosted lane must be off by default');
  const hostedDefaults = loadHostedLane({ NODE_ENV: 'test', APOCRYPHA_HOSTED_ENABLED: 'true' });
  assert(hostedDefaults?.baseUrl === 'http://127.0.0.1:19135/v1' && hostedDefaults.modelAlias === 'anthropic/claude-opus-5.5',
    'hosted lane defaults are the loopback proxy and Opus 5.5');

  const localSeen: Array<Record<string, unknown>> = [];
  const hostedSeen: Array<Record<string, unknown>> = [];
  const local = await engine('qwen35-35b-a3b-q4', 'The note says the launch moved to the 14th.', localSeen);
  const hosted = await engine('anthropic/claude-opus-5.5', '<think>reading the note</think>The attached note moves the launch to the 14th.', hostedSeen, 1_000_000);
  const dir = await mkdtemp(join(tmpdir(), 'apocrypha-worker-lane-'));
  try {
    // 1. flagship job on a worker with the hosted lane on -> hosted engine, clean body, receipt.
    const flagship = await run(config(local.url, hosted.url, join(dir, 'a')), 'flagship');
    assert(hostedSeen.length === 1 && localSeen.length === 0, 'flagship job did not go to the hosted engine');
    const hostedBody = hostedSeen[0] as Record<string, unknown>;
    assert(hostedBody.model === 'anthropic/claude-opus-5.5', 'hosted request did not pin the flagship model');
    for (const dial of ['top_k', 'min_p', 'repeat_penalty', 'chat_template_kwargs']) {
      assert(!(dial in hostedBody), `hosted request carried the llama-only dial ${dial}`);
    }
    const messages = hostedBody.messages as Array<{ role: string; content: string }>;
    const system = String(messages[0]?.content ?? '');
    const evidence = String(messages.at(-1)?.content ?? '');
    // Attachments are this turn's evidence, so they ride after the final turn with the memory records
    // (prompt.ts) and the system message stays a byte-stable, cacheable prefix.
    assert(evidence.includes('ATTACHMENT_MARKER') && evidence.includes('name="notes.txt"'), 'attachment text did not reach the prompt');
    assert(!system.includes('ATTACHMENT_MARKER'), 'attachment text must not sit in the cacheable system prefix');
    assert(flagship.usage?.engine_lane === 'flagship' && flagship.usage?.model === 'anthropic/claude-opus-5.5', 'receipt lane/model wrong');
    assert(flagship.usage?.total_cost_usd === 0.014, `receipt cost wrong: ${String(flagship.usage?.total_cost_usd)}`);
    assert(flagship.provenance?.lane_fallback === undefined, 'no fallback should be recorded');
    assert(flagship.content === 'The attached note moves the launch to the 14th.', 'stored answer kept the thought');
    assert(flagship.usage?.withheld === false, 'closed thought must not be marked withheld');

    // 2. same job on a worker without the hosted lane -> local engine, visible fallback.
    const fallback = await run(config(local.url, null, join(dir, 'b')), 'flagship');
    assert((localSeen.length as number) === 1 && (hostedSeen.length as number) === 1, 'fallback did not run on the local engine');
    assert(fallback.usage?.engine_lane === 'flagship' ? false : fallback.usage?.engine_lane === 'local', 'fallback receipt lane wrong');
    assert(typeof fallback.provenance?.lane_fallback === 'string', 'fallback must be visible in provenance');
    assert(fallback.usage?.total_cost_usd === 0, 'local lane bills nothing');
    assert('top_k' in (localSeen[0] as Record<string, unknown>), 'local lane lost its llama dials');

    // 3. local job with the hosted lane on stays local.
    const plain = await run(config(local.url, hosted.url, join(dir, 'c')), 'local');
    assert((localSeen.length as number) === 2 && (hostedSeen.length as number) === 1, 'local job leaked onto the hosted lane');
    assert(plain.usage?.engine_lane === 'local' && plain.provenance?.lane_requested === 'local', 'local receipt wrong');
  } finally {
    await local.close();
    await hosted.close();
    await rm(dir, { recursive: true, force: true });
  }
  console.log('apocrypha-worker-lane.test : OK · flagship->hosted with clean dials + cost receipt, fallback visible, attachments in prompt, thought withheld from stored answer');
}

void main().catch((error) => { console.error(error); process.exit(1); });
