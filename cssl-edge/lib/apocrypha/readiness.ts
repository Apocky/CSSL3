export const APOCRYPHA_WORKER_FRESHNESS_MS = 90_000;
export const APOCRYPHA_MEMORY_IDLE_FRESHNESS_MS = 90_000;
export const APOCRYPHA_GENERATION_DEADLINE_MAX_MS = 7_200_000;
export const APOCRYPHA_RUNTIME_CAPABILITIES = [
  'apocky_owner_chat',
  'chaos_tarot_reading',
  'apocky_member_chat',
] as const;
export const APOCRYPHA_RUNTIME_CONFIGURATION: ApocryphaExpectedConfiguration = {
  model_alias: 'qwen35-35b-a3b-q4',
  profile_hash: '5d390055297aed74dbba092eb313dc8c4bf4e551ca4bf2c50fed16c8cb3a21a9',
  tool_registry_version: 'apocrypha-readonly-v1',
  memory_manifest_hash: '307a86ce2ec83a37ad30f86327195e47259167728cf32e4276af377f08988273',
};

export const APOCRYPHA_REQUIRED_MEMORY_ADAPTERS = [
  'mempalace',
  'brainmonsoon',
  'anamnesis',
  'graphify',
  'mneme',
  'metaharness',
] as const;

export type ApocryphaRequiredMemoryAdapter = typeof APOCRYPHA_REQUIRED_MEMORY_ADAPTERS[number];

export interface ApocryphaExpectedConfiguration {
  model_alias: string;
  profile_hash: string;
  tool_registry_version: string;
  memory_manifest_hash: string;
}

export interface ApocryphaWorkerReadinessRow {
  status?: unknown;
  allowed_capabilities?: unknown;
  model_profiles?: unknown;
  last_seen_at?: unknown;
}

export interface ApocryphaQueueReadiness {
  queued: number;
  active: number;
  failed_24h: number;
}

interface SafeWorkerProfile {
  model_alias: string | null;
  profile_hash: string | null;
  tool_registry_version: string | null;
  memory_manifest_hash: string | null;
  phase: string | null;
  qwen_healthy: boolean;
  qwen_probe_at: string | null;
  adapter_probe_at: string | null;
  generation_deadline_ms: number | null;
  adapter_states: Record<ApocryphaRequiredMemoryAdapter, string | null>;
  capability_memory: Record<string, {
    adapter_probe_at: string | null;
    adapter_states: Record<ApocryphaRequiredMemoryAdapter, string | null>;
  }>;
}

export interface ApocryphaReadinessProjection {
  rail: 'durable-outbound-qwen';
  required_capability: string;
  status: 'ready' | 'degraded' | 'unavailable';
  ready: boolean;
  code:
    | 'READY'
    | 'NO_ACTIVE_WORKER'
    | 'WORKER_HEARTBEAT_STALE'
    | 'WORKER_PROFILE_MISMATCH'
    | 'QWEN_UNHEALTHY'
    | 'MEMORY_ADAPTERS_UNHEALTHY';
  checked_at: string;
  freshness_window_ms: number;
  memory_probe_freshness_ms: number;
  configuration: {
    compatible: boolean;
    expected: ApocryphaExpectedConfiguration;
    observed: SafeWorkerProfile | null;
  };
  operational: {
    generation_ready: boolean;
    qwen_healthy: boolean;
    qwen_probe_at: string | null;
    memory_ready: boolean;
    memory_evidence: 'capability' | 'aggregate' | 'missing';
    adapter_probe_at: string | null;
    generation_deadline_ms: number | null;
    memory_freshness_window_ms: number;
    required_memory_adapters: readonly ApocryphaRequiredMemoryAdapter[];
    adapter_states: Record<ApocryphaRequiredMemoryAdapter, string | null>;
  };
  worker: {
    active_nodes: number;
    capable_nodes: number;
    fresh_nodes: number;
    compatible_nodes: number;
    ready_nodes: number;
    last_heartbeat_at: string | null;
    phase: string | null;
  };
  queue: ApocryphaQueueReadiness;
}

function record(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
}

function safeString(value: unknown, maxLength = 160): string | null {
  if (typeof value !== 'string') return null;
  const clean = value.trim();
  return clean && clean.length <= maxLength ? clean : null;
}

function safeInteger(value: unknown, minimum: number, maximum: number): number | null {
  return Number.isInteger(value) && Number(value) >= minimum && Number(value) <= maximum
    ? Number(value)
    : null;
}

function safeAdapterStates(value: unknown): Record<ApocryphaRequiredMemoryAdapter, string | null> {
  const source = record(value);
  return Object.fromEntries(
    APOCRYPHA_REQUIRED_MEMORY_ADAPTERS.map((name) => [name, safeString(source[name], 64)]),
  ) as Record<ApocryphaRequiredMemoryAdapter, string | null>;
}

function profile(row: ApocryphaWorkerReadinessRow): SafeWorkerProfile {
  const source = record(row.model_profiles);
  const rawCapabilityMemory = record(source.capability_memory);
  const capabilityMemory = Object.fromEntries(APOCRYPHA_RUNTIME_CAPABILITIES.flatMap((capability) => {
    const capabilitySource = record(rawCapabilityMemory[capability]);
    return Object.keys(capabilitySource).length === 0 ? [] : [[capability, {
      adapter_probe_at: safeString(capabilitySource.adapter_probe_at, 64),
      adapter_states: safeAdapterStates(capabilitySource.adapter_states),
    }]];
  }));
  return {
    model_alias: safeString(source.model_alias),
    profile_hash: safeString(source.profile_hash),
    tool_registry_version: safeString(source.tool_registry_version),
    memory_manifest_hash: safeString(source.memory_manifest_hash),
    phase: safeString(source.phase, 64),
    qwen_healthy: source.qwen_healthy === true,
    qwen_probe_at: safeString(source.qwen_probe_at, 64),
    adapter_probe_at: safeString(source.adapter_probe_at, 64),
    generation_deadline_ms: safeInteger(source.generation_deadline_ms, 60_000, APOCRYPHA_GENERATION_DEADLINE_MAX_MS),
    adapter_states: safeAdapterStates(source.adapter_states),
    capability_memory: capabilityMemory,
  };
}

function heartbeat(row: ApocryphaWorkerReadinessRow): { text: string | null; milliseconds: number | null } {
  const text = safeString(row.last_seen_at, 64);
  const milliseconds = text === null ? Number.NaN : Date.parse(text);
  return { text, milliseconds: Number.isFinite(milliseconds) ? milliseconds : null };
}

function hasCapability(row: ApocryphaWorkerReadinessRow, requiredCapability: string): boolean {
  return Array.isArray(row.allowed_capabilities)
    && row.allowed_capabilities.some((item) => item === '*' || item === requiredCapability);
}

function matches(actual: SafeWorkerProfile, expected: ApocryphaExpectedConfiguration): boolean {
  return actual.model_alias === expected.model_alias
    && actual.profile_hash === expected.profile_hash
    && actual.tool_registry_version === expected.tool_registry_version
    && actual.memory_manifest_hash === expected.memory_manifest_hash;
}

function recentProbe(timestamp: string | null, now: number, freshnessWindowMs: number): boolean {
  if (!timestamp) return false;
  const observedAt = Date.parse(timestamp);
  return Number.isFinite(observedAt) && observedAt <= now && now - observedAt <= freshnessWindowMs;
}

function boundedFreshnessWindow(value: number | undefined, fallback: number, maximum: number): number {
  return Number.isFinite(value)
    ? Math.min(maximum, Math.max(1_000, Math.trunc(value as number)))
    : fallback;
}

function qwenReady(actual: SafeWorkerProfile, now: number, freshnessWindowMs: number): boolean {
  return actual.qwen_healthy && recentProbe(actual.qwen_probe_at, now, freshnessWindowMs);
}

function memoryFreshnessWindow(actual: SafeWorkerProfile, idleFreshnessWindowMs: number): number {
  return (actual.phase === 'generating' || actual.phase === 'delivering') && actual.generation_deadline_ms !== null
    ? Math.max(idleFreshnessWindowMs, actual.generation_deadline_ms)
    : idleFreshnessWindowMs;
}

function memoryEvidence(actual: SafeWorkerProfile, capability: string): {
  source: 'capability' | 'aggregate';
  adapter_probe_at: string | null;
  adapter_states: Record<ApocryphaRequiredMemoryAdapter, string | null>;
} | null {
  const scoped = actual.capability_memory[capability];
  if (scoped) return { source: 'capability', ...scoped };
  if (capability === 'apocky_member_chat') return null;
  return { source: 'aggregate', adapter_probe_at: actual.adapter_probe_at, adapter_states: actual.adapter_states };
}

function memoryReady(
  actual: SafeWorkerProfile,
  capability: string,
  now: number,
  idleFreshnessWindowMs: number,
): boolean {
  const evidence = memoryEvidence(actual, capability);
  return evidence !== null
    && recentProbe(evidence.adapter_probe_at, now, memoryFreshnessWindow(actual, idleFreshnessWindowMs))
    && APOCRYPHA_REQUIRED_MEMORY_ADAPTERS.every((name) => evidence.adapter_states[name] === 'ok');
}

function operationallyReady(
  actual: SafeWorkerProfile,
  capability: string,
  now: number,
  workerFreshnessWindowMs: number,
  memoryProbeFreshnessWindowMs: number,
): boolean {
  return qwenReady(actual, now, workerFreshnessWindowMs)
    && memoryReady(actual, capability, now, memoryProbeFreshnessWindowMs);
}

export function projectApocryphaReadiness(input: {
  nodes: ApocryphaWorkerReadinessRow[];
  queue: ApocryphaQueueReadiness;
  expected: ApocryphaExpectedConfiguration;
  requiredCapability?: string;
  now?: number;
  freshnessWindowMs?: number;
  memoryProbeFreshnessWindowMs?: number;
}): ApocryphaReadinessProjection {
  const now = input.now ?? Date.now();
  const requiredCapability = safeString(input.requiredCapability, 128) ?? 'chaos_tarot_reading';
  const freshnessWindowMs = boundedFreshnessWindow(
    input.freshnessWindowMs,
    APOCRYPHA_WORKER_FRESHNESS_MS,
    APOCRYPHA_WORKER_FRESHNESS_MS,
  );
  const memoryProbeFreshnessWindowMs = boundedFreshnessWindow(
    input.memoryProbeFreshnessWindowMs,
    APOCRYPHA_MEMORY_IDLE_FRESHNESS_MS,
    APOCRYPHA_MEMORY_IDLE_FRESHNESS_MS,
  );
  const active = input.nodes.filter((node) => node.status === 'active');
  const capable = active.filter((node) => hasCapability(node, requiredCapability));
  const ordered = [...capable].sort((left, right) =>
    (heartbeat(right).milliseconds ?? -1) - (heartbeat(left).milliseconds ?? -1));
  const fresh = ordered.filter((node) => {
    const seen = heartbeat(node).milliseconds;
    return seen !== null && seen <= now && now - seen <= freshnessWindowMs;
  });
  const compatible = fresh.filter((node) => matches(profile(node), input.expected));
  const qwenHealthy = compatible.filter((node) => qwenReady(profile(node), now, freshnessWindowMs));
  const readyNodes = compatible.filter((node) => operationallyReady(
    profile(node), requiredCapability, now, freshnessWindowMs, memoryProbeFreshnessWindowMs,
  ));
  const selected = readyNodes[0] ?? qwenHealthy[0] ?? compatible[0] ?? fresh[0] ?? ordered[0] ?? null;
  const selectedProfile = selected ? profile(selected) : null;
  const selectedMemoryEvidence = selectedProfile ? memoryEvidence(selectedProfile, requiredCapability) : null;
  const latestHeartbeat = ordered
    .map((node) => heartbeat(node).text)
    .find((value): value is string => value !== null) ?? null;

  let code: ApocryphaReadinessProjection['code'];
  let status: ApocryphaReadinessProjection['status'];
  if (active.length === 0 || capable.length === 0) {
    code = 'NO_ACTIVE_WORKER';
    status = 'unavailable';
  } else if (fresh.length === 0) {
    code = 'WORKER_HEARTBEAT_STALE';
    status = 'unavailable';
  } else if (compatible.length === 0) {
    code = 'WORKER_PROFILE_MISMATCH';
    status = 'degraded';
  } else if (qwenHealthy.length === 0) {
    code = 'QWEN_UNHEALTHY';
    status = 'unavailable';
  } else if (readyNodes.length === 0) {
    code = 'MEMORY_ADAPTERS_UNHEALTHY';
    status = 'degraded';
  } else {
    code = 'READY';
    status = 'ready';
  }

  const ready = code === 'READY';
  return {
    rail: 'durable-outbound-qwen',
    required_capability: requiredCapability,
    status,
    ready,
    code,
    checked_at: new Date(now).toISOString(),
    freshness_window_ms: freshnessWindowMs,
    memory_probe_freshness_ms: memoryProbeFreshnessWindowMs,
    configuration: {
      compatible: selectedProfile ? matches(selectedProfile, input.expected) : false,
      expected: input.expected,
      observed: selectedProfile,
    },
    operational: {
      generation_ready: qwenHealthy.length > 0,
      qwen_healthy: selectedProfile ? qwenReady(selectedProfile, now, freshnessWindowMs) : false,
      qwen_probe_at: selectedProfile?.qwen_probe_at ?? null,
      memory_ready: selectedProfile
        ? memoryReady(selectedProfile, requiredCapability, now, memoryProbeFreshnessWindowMs)
        : false,
      memory_evidence: selectedMemoryEvidence?.source ?? 'missing',
      adapter_probe_at: selectedMemoryEvidence?.adapter_probe_at ?? null,
      generation_deadline_ms: selectedProfile?.generation_deadline_ms ?? null,
      memory_freshness_window_ms: selectedProfile
        ? memoryFreshnessWindow(selectedProfile, memoryProbeFreshnessWindowMs)
        : memoryProbeFreshnessWindowMs,
      required_memory_adapters: APOCRYPHA_REQUIRED_MEMORY_ADAPTERS,
      adapter_states: selectedMemoryEvidence?.adapter_states ?? Object.fromEntries(
        APOCRYPHA_REQUIRED_MEMORY_ADAPTERS.map((name) => [name, null]),
      ) as Record<ApocryphaRequiredMemoryAdapter, string | null>,
    },
    worker: {
      active_nodes: active.length,
      capable_nodes: capable.length,
      fresh_nodes: fresh.length,
      compatible_nodes: compatible.length,
      ready_nodes: readyNodes.length,
      last_heartbeat_at: latestHeartbeat,
      phase: selectedProfile?.phase ?? null,
    },
    queue: input.queue,
  };
}
