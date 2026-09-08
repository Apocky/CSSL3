import assert from 'node:assert/strict';
import { FrontierClient, FrontierError } from '../scripts/apocrypha-worker/frontier';
import type { QwenMessage } from '../scripts/apocrypha-worker/qwen';
import type { WorkerConfig } from '../scripts/apocrypha-worker/types';

function config(overrides: Partial<WorkerConfig> = {}): WorkerConfig {
  return {
    controlPlaneUrl: 'https://example.test', nodeId: 'node', nodeToken: 'token',
    qwenBaseUrl: 'http://127.0.0.1:19124/v1', runtimeProfilePath: null,
    modelAlias: 'qwen35-35b-a3b-q4', profileHash: 'profile', toolRegistryVersion: 'tools',
    memoryManifestHash: 'memory', manifest: { } as WorkerConfig['manifest'],
    pollIntervalMs: 1_000, claimLeaseSeconds: 180, leaseRenewIntervalMs: 10_000, leaseExpiryGraceMs: 5_000,
    controlPlaneTimeoutMs: 15_000, chunkFlushMs: 1_000, chunkMaxChars: 256,
    qwenIdleTimeoutMs: 180_000, qwenMaxRuntimeMs: 2_700_000, contextWindowTokens: 4_096, maxOutputTokens: 2_048,
    journalDir: '.', healthHost: '127.0.0.1', healthPort: 19_126, heartbeatIntervalMs: 15_000, heartbeatEnabled: false,
    memoryReadConcurrency: 3, memoryProbeTenantId: null, memoryProbePrincipalId: 'node', memoryProbeCapability: 'chaos_tarot_reading',
    once: false, probeOnly: false, recoverOnly: false,
    frontierProvider: 'openai', frontierBaseUrl: 'https://api.openai.com/v1', frontierApiKey: 'test-key', frontierModel: 'frontier-test',
    frontierTimeoutMs: 5_000, frontierCooldownMs: 30_000,
    ...overrides,
  };
}

const messages: QwenMessage[] = [
  { role: 'system', content: 'You are Apocrypha.' },
  { role: 'user', content: 'Give a grounded reading.' },
];

async function main(): Promise<void> {
  let calls = 0;
  const openAi = new FrontierClient(config(), async (_url, init) => {
    calls += 1;
    assert.equal(init?.headers && (init.headers as Record<string, string>).authorization, 'Bearer test-key');
    return new Response(JSON.stringify({ model: 'frontier-test', choices: [{ message: { content: 'A useful answer.' } }], usage: { prompt_tokens: 4, completion_tokens: 3 } }), { status: 200 });
  });
  const openResult = await openAi.generate(messages, { maxTokens: 64 });
  assert.equal(openResult.content, 'A useful answer.');
  assert.equal(openResult.usage.totalTokens, 7);
  assert.equal(calls, 1);

  const anthropicRequest: { value: Record<string, unknown> | null } = { value: null };
  const anthropic = new FrontierClient(config({
    frontierProvider: 'anthropic', frontierBaseUrl: 'https://api.anthropic.com', frontierModel: 'fable-test',
  }), async (_url, init) => {
    anthropicRequest.value = JSON.parse(String(init?.body)) as Record<string, unknown>;
    return new Response(JSON.stringify({ content: [{ type: 'text', text: 'Fable answer.' }], usage: { input_tokens: 5, output_tokens: 4 } }), { status: 200 });
  });
  const anthropicResult = await anthropic.generate(messages, { maxTokens: 64 });
  assert.equal(anthropicResult.content, 'Fable answer.');
  assert.equal(anthropicRequest.value?.system, 'You are Apocrypha.');
  assert.deepEqual(anthropicRequest.value?.messages, [{ role: 'user', content: 'Give a grounded reading.' }]);

  const rateLimited = new FrontierClient(config(), async () => new Response('rate limited', { status: 429 }));
  await assert.rejects(() => rateLimited.generate(messages, { maxTokens: 64 }), (error: unknown) => error instanceof FrontierError && error.code === 'FRONTIER_HTTP_429');
  assert.equal(rateLimited.status().available, false);
  await assert.rejects(() => rateLimited.generate(messages, { maxTokens: 64 }), (error: unknown) => error instanceof FrontierError && error.code === 'FRONTIER_COOLDOWN');

  const unconfigured = new FrontierClient(config({ frontierProvider: null, frontierApiKey: null, frontierModel: null }));
  assert.equal(unconfigured.status().configured, false);
  await assert.rejects(() => unconfigured.generate(messages, { maxTokens: 64 }), (error: unknown) => error instanceof FrontierError && error.code === 'FRONTIER_UNCONFIGURED');
  console.log('apocrypha frontier tests: 4 passed');
}

void main().catch((error) => { console.error(error); process.exitCode = 1; });
