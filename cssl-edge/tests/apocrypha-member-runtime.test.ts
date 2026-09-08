import { loadConfig, loadManifest, memoryManifestHash } from '../scripts/apocrypha-worker/config';
import { probeMemoryAdapters, retrieveMemory } from '../scripts/apocrypha-worker/retrieval';
import type { ClaimedJob, WorkerConfig } from '../scripts/apocrypha-worker/types';
import { loadGatewayConfig } from '../scripts/apocrypha-memory-gateway/config';
import { GatewayError, validateSearchRequest } from '../scripts/apocrypha-memory-gateway/security';
import {
  APOCRYPHA_REQUIRED_MEMORY_ADAPTERS,
  APOCRYPHA_RUNTIME_CAPABILITIES,
  APOCRYPHA_RUNTIME_CONFIGURATION,
  projectApocryphaReadiness,
} from '../lib/apocrypha/readiness';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

const MEMBER_TENANT = '10000000-0000-4000-8000-000000000001';
const MEMBER_PRINCIPAL = '20000000-0000-4000-8000-000000000001';
const OWNER_TENANT = '30000000-0000-4000-8000-000000000001';
const OWNER_PRINCIPAL = '40000000-0000-4000-8000-000000000001';
const CHAOS_TENANT = '50000000-0000-4000-8000-000000000001';
const CHAOS_PRINCIPAL = '60000000-0000-4000-8000-000000000001';

function workerConfig(): WorkerConfig {
  const manifest = loadManifest();
  return {
    controlPlaneUrl: 'https://example.test', nodeId: OWNER_PRINCIPAL, nodeToken: 'test-worker-token',
    qwenBaseUrl: 'http://127.0.0.1:19124/v1', runtimeProfilePath: null,
    modelAlias: manifest.model.alias, profileHash: manifest.model.profileHash,
    toolRegistryVersion: manifest.tools.registryVersion, memoryManifestHash: memoryManifestHash(manifest), manifest,
    pollIntervalMs: 1_000, claimLeaseSeconds: 180, leaseRenewIntervalMs: 10_000, leaseExpiryGraceMs: 5_000,
    controlPlaneTimeoutMs: 15_000, chunkFlushMs: 1_000, chunkMaxChars: 256,
    qwenIdleTimeoutMs: 180_000, qwenMaxRuntimeMs: 2_700_000,
    contextWindowTokens: 4_096, maxOutputTokens: 2_048, journalDir: '.',
    healthHost: '127.0.0.1', healthPort: 19_126, heartbeatIntervalMs: 15_000, heartbeatEnabled: false,
    memoryReadConcurrency: 6,
    memoryProbeTenantId: CHAOS_TENANT,
    memoryProbePrincipalId: CHAOS_PRINCIPAL,
    memoryProbeCapability: 'chaos_tarot_reading',
    memoryAdditionalProbeScopes: [
      { tenantId: OWNER_TENANT, principalId: OWNER_PRINCIPAL, capability: 'apocky_owner_chat' },
      { tenantId: MEMBER_TENANT, principalId: MEMBER_PRINCIPAL, capability: 'apocky_member_chat' },
    ],
    once: true, probeOnly: false, recoverOnly: false,
  };
}

function search(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    operation: 'search', read_only: true, query: 'member continuity', limit: 4,
    tenant_id: MEMBER_TENANT, principal_id: MEMBER_PRINCIPAL, capability: 'apocky_member_chat',
    memory_manifest_hash: APOCRYPHA_RUNTIME_CONFIGURATION.memory_manifest_hash,
    ...overrides,
  };
}

function assertGatewayDenied(config: ReturnType<typeof loadGatewayConfig>, body: Record<string, unknown>, code: string): void {
  let observed = '';
  try {
    validateSearchRequest(body, config);
  } catch (error) {
    if (error instanceof GatewayError) observed = error.code;
  }
  assert(observed === code, `gateway returned ${observed || 'success'} instead of ${code}`);
}

async function main(): Promise<void> {
  const manifest = loadManifest();
  assert(manifest.model.alias === APOCRYPHA_RUNTIME_CONFIGURATION.model_alias, 'Qwen alias drifted');
  assert(manifest.model.profileHash === APOCRYPHA_RUNTIME_CONFIGURATION.profile_hash, 'Qwen profile drifted');
  assert(manifest.tools.registryVersion === APOCRYPHA_RUNTIME_CONFIGURATION.tool_registry_version, 'tool registry drifted');
  assert(memoryManifestHash(manifest) === APOCRYPHA_RUNTIME_CONFIGURATION.memory_manifest_hash, 'memory manifest drifted');
  assert(JSON.stringify(manifest.capabilities) === JSON.stringify(APOCRYPHA_RUNTIME_CAPABILITIES), 'capability manifest drifted');
  assert(JSON.stringify(manifest.memory.adapters.map((adapter) => adapter.name))
    === JSON.stringify(APOCRYPHA_REQUIRED_MEMORY_ADAPTERS), 'six-adapter manifest drifted');

  const baseConfigEnv: NodeJS.ProcessEnv = {
    NODE_ENV: 'test', APOCRYPHA_CONTROL_PLANE_URL: 'https://example.test',
    APOCRYPHA_WORKER_NODE_ID: OWNER_PRINCIPAL, APOCRYPHA_WORKER_TOKEN: 'test-worker-token',
    APOCRYPHA_MEMORY_PROBE_TENANT_ID: CHAOS_TENANT,
    APOCRYPHA_MEMORY_PROBE_PRINCIPAL_ID: CHAOS_PRINCIPAL,
    APOCRYPHA_MEMORY_PROBE_CAPABILITY: 'chaos_tarot_reading',
  };
  let missingMemberProbeRejected = false;
  try {
    loadConfig({
      ...baseConfigEnv,
      APOCRYPHA_MEMORY_ADDITIONAL_PROBE_SCOPES: `${OWNER_TENANT}:${OWNER_PRINCIPAL}:apocky_owner_chat`,
    }, []);
  } catch (error) {
    missingMemberProbeRejected = error instanceof Error && error.message.includes('apocky_member_chat');
  }
  assert(missingMemberProbeRejected, 'worker admitted member capability without a member-specific probe');
  const loaded = loadConfig({
    ...baseConfigEnv,
    APOCRYPHA_MEMORY_ADDITIONAL_PROBE_SCOPES:
      `${OWNER_TENANT}:${OWNER_PRINCIPAL}:apocky_owner_chat,${MEMBER_TENANT}:${MEMBER_PRINCIPAL}:apocky_member_chat`,
  }, []);
  assert(loaded.memoryAdditionalProbeScopes?.some((scope) => scope.capability === 'apocky_member_chat'),
    'worker did not retain the exact member probe scope');

  const gatewayConfig = loadGatewayConfig({
    NODE_ENV: 'test', APOCRYPHA_MEMORY_GATEWAY_TOKEN: 'x'.repeat(32),
    APOCRYPHA_MEMORY_GATEWAY_ALLOWED_TENANTS: `${OWNER_TENANT},${CHAOS_TENANT},${MEMBER_TENANT}`,
    APOCRYPHA_MEMORY_GATEWAY_ALLOWED_PRINCIPALS: OWNER_PRINCIPAL,
    APOCRYPHA_MEMORY_GATEWAY_ALLOWED_CAPABILITIES: APOCRYPHA_RUNTIME_CAPABILITIES.join(','),
    APOCRYPHA_MEMORY_GATEWAY_OWNER_SCOPES: `${OWNER_TENANT}:${OWNER_PRINCIPAL}`,
    APOCRYPHA_MEMORY_GATEWAY_DYNAMIC_MEMBER_SCOPES:
      `${CHAOS_TENANT}:chaos_tarot_reading,${MEMBER_TENANT}:apocky_member_chat`,
  });
  assert(validateSearchRequest(search(), gatewayConfig).principal_id === MEMBER_PRINCIPAL,
    'exact member tenant/principal scope was denied');
  assertGatewayDenied(gatewayConfig, search({ tenant_id: CHAOS_TENANT }), 'PRINCIPAL_DENIED');
  assertGatewayDenied(gatewayConfig, search({ principal_id: 'member-not-a-uuid' }), 'PRINCIPAL_DENIED');
  assertGatewayDenied(gatewayConfig, search({ capability: 'chaos_tarot_reading' }), 'PRINCIPAL_DENIED');
  assertGatewayDenied(gatewayConfig, search({
    tenant_id: OWNER_TENANT, principal_id: MEMBER_PRINCIPAL, capability: 'apocky_owner_chat',
  }), 'PRINCIPAL_DENIED');

  const config = workerConfig();
  const adapterEnv: NodeJS.ProcessEnv = { NODE_ENV: 'test' };
  for (const adapter of manifest.memory.adapters) adapterEnv[adapter.urlEnv] = 'http://127.0.0.1:19127/read';
  const requests: Array<Record<string, unknown>> = [];
  const fetchImpl = (async (_input: RequestInfo | URL, init?: RequestInit) => {
    requests.push(JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>);
    return new Response(JSON.stringify({ records: [] }), { status: 200, headers: { 'content-type': 'application/json' } });
  }) as typeof fetch;
  const memberJob: ClaimedJob = {
    jobId: '70000000-0000-4000-8000-000000000001', attemptId: '80000000-0000-4000-8000-000000000001',
    attemptNo: 1, leaseEpoch: 1, leaseToken: 'lease', leaseExpiresAt: new Date(Date.now() + 60_000).toISOString(),
    tenantId: MEMBER_TENANT, ownerPrincipalId: MEMBER_PRINCIPAL, kind: 'apocky_chat', capability: 'apocky_member_chat',
    request: { question: 'What should I remember?', conversation_history: [] },
    modelAlias: config.modelAlias, profileHash: config.profileHash,
    toolRegistryVersion: config.toolRegistryVersion, memoryManifestHash: config.memoryManifestHash,
  };
  const retrieval = await retrieveMemory(config, memberJob, adapterEnv, fetchImpl);
  assert(retrieval.results.length === 6 && retrieval.results.every((result) => result.state === 'ok'),
    'member retrieval did not exercise all six adapters');
  assert(requests.every((body) => body.tenant_id === MEMBER_TENANT && body.principal_id === MEMBER_PRINCIPAL
    && body.capability === 'apocky_member_chat'
    && body.memory_manifest_hash === APOCRYPHA_RUNTIME_CONFIGURATION.memory_manifest_hash),
  'member retrieval lost tenant, principal, capability, or manifest isolation');
  requests.length = 0;
  const probe = await probeMemoryAdapters(config, adapterEnv, fetchImpl);
  assert(probe?.capabilityProbes?.apocky_member_chat?.probedAt !== null, 'member probe did not mint scoped evidence');
  assert(probe?.capabilityProbes?.apocky_member_chat?.results.length === 6, 'member probe omitted an adapter');
  assert(Object.keys(probe?.capabilityProbes ?? {}).length === 3, 'owner or Chaos scoped evidence was lost');

  const now = Date.parse('2026-09-08T12:00:00.000Z');
  const healthyStates = Object.fromEntries(APOCRYPHA_REQUIRED_MEMORY_ADAPTERS.map((name) => [name, 'ok']));
  const row = {
    status: 'active', allowed_capabilities: [...APOCRYPHA_RUNTIME_CAPABILITIES],
    last_seen_at: '2026-09-08T11:59:45.000Z',
    model_profiles: {
      ...APOCRYPHA_RUNTIME_CONFIGURATION, phase: 'idle', qwen_healthy: true,
      qwen_probe_at: '2026-09-08T11:59:45.000Z', adapter_probe_at: '2026-09-08T11:59:45.000Z',
      generation_deadline_ms: 2_700_000, adapter_states: healthyStates,
    },
  };
  const missingScopedEvidence = projectApocryphaReadiness({
    nodes: [row], queue: { queued: 0, active: 0, failed_24h: 0 },
    expected: APOCRYPHA_RUNTIME_CONFIGURATION, requiredCapability: 'apocky_member_chat', now,
  });
  assert(!missingScopedEvidence.ready && missingScopedEvidence.operational.memory_evidence === 'missing',
    'member readiness inherited aggregate owner or Chaos evidence');
  const memberReady = projectApocryphaReadiness({
    nodes: [{ ...row, model_profiles: { ...row.model_profiles, capability_memory: {
      apocky_member_chat: { adapter_probe_at: '2026-09-08T11:59:45.000Z', adapter_states: healthyStates },
    } } }],
    queue: { queued: 0, active: 0, failed_24h: 0 },
    expected: APOCRYPHA_RUNTIME_CONFIGURATION, requiredCapability: 'apocky_member_chat', now,
  });
  assert(memberReady.ready && memberReady.operational.memory_evidence === 'capability',
    'fresh six-adapter member evidence was not ready');

  console.log('apocrypha-member-runtime.test: OK');
}

void main();
