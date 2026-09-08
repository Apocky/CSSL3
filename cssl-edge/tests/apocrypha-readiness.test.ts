import { createHash, createHmac } from 'node:crypto';
import type { NextApiRequest, NextApiResponse } from 'next';

import {
  APOCRYPHA_MEMORY_MANIFEST_HASH,
  APOCRYPHA_MODEL_ALIAS,
  APOCRYPHA_PROFILE_HASH,
  APOCRYPHA_TOOL_REGISTRY_VERSION,
  resetApocryphaServiceClientForTests,
} from '../lib/apocrypha/job-control';
import { projectApocryphaReadiness } from '../lib/apocrypha/readiness';
import readinessHandler from '../pages/api/apocrypha/readiness';

function assert(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

const NOW = Date.parse('2026-09-08T04:10:00.000Z');
const expected = {
  model_alias: 'qwen35-35b-a3b-q4',
  profile_hash: 'profile-hash',
  tool_registry_version: 'apocrypha-readonly-v1',
  memory_manifest_hash: 'memory-hash',
};
const queue = { queued: 2, active: 1, failed_24h: 0 };
const healthyAdapters = {
  mempalace: 'ok',
  brainmonsoon: 'ok',
  anamnesis: 'ok',
  graphify: 'ok',
  mneme: 'ok',
  metaharness: 'ok',
};
const healthyOperations = {
  qwen_healthy: true,
  qwen_probe_at: '2026-09-08T04:09:45.000Z',
  adapter_probe_at: '2026-09-08T04:09:45.000Z',
  generation_deadline_ms: 2_700_000,
  adapter_states: healthyAdapters,
};

function node(overrides: Record<string, unknown> = {}) {
  return {
    status: 'active',
    allowed_capabilities: ['apocky_owner_chat', 'chaos_tarot_reading'],
    last_seen_at: '2026-09-08T04:09:45.000Z',
    model_profiles: {
      ...expected,
      phase: 'idle',
      ...healthyOperations,
      load: { private_host_detail: 'must-not-leak' },
    },
    id: 'private-node-id',
    node_key: 'private-node-key',
    display_name: 'private-display-name',
    token_hash: 'private-token-hash',
    ...overrides,
  };
}

const ready = projectApocryphaReadiness({ nodes: [node()], queue, expected, now: NOW });
assert(ready.ready && ready.code === 'READY', 'fresh compatible Qwen worker was not ready');
assert(ready.required_capability === 'chaos_tarot_reading', 'default capability changed');
assert(ready.worker.compatible_nodes === 1, 'compatible worker count was wrong');
assert(ready.worker.ready_nodes === 1, 'operational worker count was wrong');
assert(ready.operational.qwen_healthy && ready.operational.memory_ready, 'operational readiness was not projected');
assert(ready.operational.generation_ready, 'generation readiness was not projected');
assert(ready.configuration.observed?.model_alias === expected.model_alias, 'model alias was not projected');
const serialized = JSON.stringify(ready);
for (const forbidden of ['private-node-id', 'private-node-key', 'private-display-name', 'private-token-hash', 'private_host_detail']) {
  assert(!serialized.includes(forbidden), `readiness projection leaked ${forbidden}`);
}

const stale = projectApocryphaReadiness({
  nodes: [node({ last_seen_at: '2026-09-08T04:00:00.000Z' })],
  queue,
  expected,
  now: NOW,
});
assert(!stale.ready && stale.code === 'WORKER_HEARTBEAT_STALE', 'stale heartbeat was accepted');

const mismatch = projectApocryphaReadiness({
  nodes: [node({ model_profiles: {
    ...expected,
    memory_manifest_hash: 'wrong-memory-hash',
    phase: 'idle',
    ...healthyOperations,
  } })],
  queue,
  expected,
  now: NOW,
});
assert(!mismatch.ready && mismatch.status === 'degraded' && mismatch.code === 'WORKER_PROFILE_MISMATCH', 'profile mismatch was accepted');

const incapable = projectApocryphaReadiness({
  nodes: [node({ allowed_capabilities: ['apocky_owner_chat'] })],
  queue,
  expected,
  now: NOW,
});
assert(!incapable.ready && incapable.code === 'NO_ACTIVE_WORKER', 'worker without Chaos capability was accepted');

const ownerReady = projectApocryphaReadiness({
  nodes: [node({ allowed_capabilities: ['apocky_owner_chat'] })],
  queue,
  expected,
  requiredCapability: 'apocky_owner_chat',
  now: NOW,
});
assert(ownerReady.ready && ownerReady.required_capability === 'apocky_owner_chat', 'owner capability was not projected');

const qwenDown = projectApocryphaReadiness({
  nodes: [node({ model_profiles: { ...expected, phase: 'idle', ...healthyOperations, qwen_healthy: false } })],
  queue,
  expected,
  now: NOW,
});
assert(!qwenDown.ready && qwenDown.code === 'QWEN_UNHEALTHY', 'declared profile was accepted while Qwen was unhealthy');

const memoryDown = projectApocryphaReadiness({
  nodes: [node({ model_profiles: {
    ...expected,
    phase: 'idle',
    ...healthyOperations,
    adapter_states: { ...healthyAdapters, graphify: 'error' },
  } })],
  queue,
  expected,
  now: NOW,
});
assert(!memoryDown.ready && memoryDown.code === 'MEMORY_ADAPTERS_UNHEALTHY', 'failed required memory adapter was accepted');
assert(!memoryDown.operational.memory_ready, 'failed required memory adapter was projected as ready');
assert(memoryDown.operational.generation_ready, 'memory degradation incorrectly disabled healthy generation');

const staleMemoryProbe = projectApocryphaReadiness({
  nodes: [node({ model_profiles: {
    ...expected,
    phase: 'idle',
    ...healthyOperations,
    adapter_probe_at: '2026-09-08T04:04:00.000Z',
  } })],
  queue,
  expected,
  now: NOW,
});
assert(!staleMemoryProbe.ready && staleMemoryProbe.code === 'MEMORY_ADAPTERS_UNHEALTHY', 'stale memory probe was accepted');
assert(!staleMemoryProbe.operational.memory_ready, 'stale memory probe was projected as ready');

const generatingWithAdmittedMemory = projectApocryphaReadiness({
  nodes: [node({ model_profiles: {
    ...expected,
    phase: 'generating',
    ...healthyOperations,
    adapter_probe_at: '2026-09-08T04:04:00.000Z',
  } })],
  queue,
  expected,
  now: NOW,
});
assert(generatingWithAdmittedMemory.ready, 'active generation ignored its configured synthesis deadline');
assert(
  generatingWithAdmittedMemory.operational.memory_freshness_window_ms === 2_700_000,
  'active generation did not project its bounded memory freshness window',
);

const generatingWithoutDeadline = projectApocryphaReadiness({
  nodes: [node({ model_profiles: {
    ...expected,
    phase: 'generating',
    ...healthyOperations,
    generation_deadline_ms: null,
    adapter_probe_at: '2026-09-08T04:04:00.000Z',
  } })],
  queue,
  expected,
  now: NOW,
});
assert(!generatingWithoutDeadline.ready, 'generation without a validated deadline extended stale memory health');

const declarationsOnly = projectApocryphaReadiness({
  nodes: [node({ model_profiles: { ...expected, phase: 'idle' } })],
  queue,
  expected,
  now: NOW,
});
assert(!declarationsOnly.ready && declarationsOnly.code === 'QWEN_UNHEALTHY', 'declared hashes alone were accepted as operational health');

interface MockResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, number | string | readonly string[]>;
}

function response(): { res: NextApiResponse; out: MockResponse } {
  const out: MockResponse = { statusCode: 0, body: null, headers: {} };
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(body: unknown) { out.body = body; return this; },
    setHeader(name: string, value: number | string | readonly string[]) {
      out.headers[name.toLowerCase()] = value;
      return this;
    },
  } as unknown as NextApiResponse;
  return { res, out };
}

function signedRequest(secret: string): NextApiRequest {
  const timestamp = String(Date.now());
  const principal = `ct_health_${'a'.repeat(43)}`;
  const path = '/api/apocrypha/readiness';
  const bodyHash = createHash('sha256').update('').digest('hex');
  const signed = `${timestamp}\nGET\n${path}\n${principal}\n${bodyHash}`;
  const signature = `v1=${createHmac('sha256', secret).update(signed).digest('hex')}`;
  return {
    method: 'GET',
    url: path,
    headers: {
      authorization: `Bearer ${secret}`,
      'x-apocrypha-timestamp': timestamp,
      'x-apocrypha-principal': principal,
      'x-apocrypha-origin': 'chaos-tarot',
      'x-apocrypha-tenant': 'chaos-tarot',
      'x-apocrypha-content-sha256': bodyHash,
      'x-apocrypha-signature': signature,
    },
  } as unknown as NextApiRequest;
}

async function endpointContract(): Promise<void> {
  const originalFetch = globalThis.fetch;
  const originalEnv = {
    url: process.env.APOCKY_HUB_SUPABASE_URL,
    service: process.env.SUPABASE_SERVICE_ROLE_KEY,
    bridge: process.env.CHAOS_TAROT_BRIDGE_TOKEN,
  };
  const bridge = 'readiness-test-bridge-secret';
  process.env.APOCKY_HUB_SUPABASE_URL = 'https://supabase.test';
  process.env.SUPABASE_SERVICE_ROLE_KEY = 'service-role-test-key';
  process.env.CHAOS_TAROT_BRIDGE_TOKEN = bridge;
  let databaseCalls = 0;
  globalThis.fetch = async (input) => {
    databaseCalls += 1;
    const url = String(input);
    if (url.includes('/rest/v1/apocrypha_worker_node')) {
      return new Response(JSON.stringify([{
        status: 'active',
        allowed_capabilities: ['chaos_tarot_reading'],
        last_seen_at: new Date().toISOString(),
        model_profiles: {
          model_alias: APOCRYPHA_MODEL_ALIAS,
          profile_hash: APOCRYPHA_PROFILE_HASH,
          tool_registry_version: APOCRYPHA_TOOL_REGISTRY_VERSION,
          memory_manifest_hash: APOCRYPHA_MEMORY_MANIFEST_HASH,
          phase: 'idle',
          qwen_healthy: true,
          qwen_probe_at: new Date().toISOString(),
          adapter_probe_at: new Date().toISOString(),
          generation_deadline_ms: 2_700_000,
          adapter_states: healthyAdapters,
          load: { host_secret: 'must-not-leak' },
        },
      }]), { headers: { 'content-type': 'application/json' } });
    }
    return new Response(null, { status: 200, headers: { 'content-range': '0-0/0' } });
  };
  resetApocryphaServiceClientForTests();

  try {
    const rejected = response();
    await readinessHandler({ method: 'GET', url: '/api/apocrypha/readiness', headers: {} } as NextApiRequest, rejected.res);
    assert(rejected.out.statusCode === 401, `unsigned readiness returned ${rejected.out.statusCode}`);
    assert(databaseCalls === 0, 'unsigned readiness reached the database');

    const accepted = response();
    await readinessHandler(signedRequest(bridge), accepted.res);
    assert(accepted.out.statusCode === 200, `signed readiness returned ${accepted.out.statusCode}`);
    const body = accepted.out.body as Record<string, unknown>;
    assert(body.ready === true && body.code === 'READY', 'signed readiness did not report the compatible worker');
    assert(Number(databaseCalls) === 4, `signed readiness made ${databaseCalls} database calls instead of four bounded reads`);
    assert(!JSON.stringify(body).includes('host_secret'), 'endpoint leaked worker load or host details');
  } finally {
    globalThis.fetch = originalFetch;
    resetApocryphaServiceClientForTests();
    for (const [key, value] of Object.entries({
      APOCKY_HUB_SUPABASE_URL: originalEnv.url,
      SUPABASE_SERVICE_ROLE_KEY: originalEnv.service,
      CHAOS_TAROT_BRIDGE_TOKEN: originalEnv.bridge,
    })) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
}

endpointContract()
  .then(() => console.log('apocrypha-readiness.test: OK'))
  .catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
