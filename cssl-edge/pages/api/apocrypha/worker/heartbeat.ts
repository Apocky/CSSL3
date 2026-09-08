import type { NextApiRequest, NextApiResponse } from 'next';

import { assertWorkerRequest, getApocryphaServiceClient, publicJobError } from '@/lib/apocrypha/job-control';
import { methodNotAllowed, noStore, objectField } from '@/lib/apocrypha/job-http';
import { required, workerDatabaseError } from '@/lib/apocrypha/worker-http';
import { APOCRYPHA_RUNTIME_CAPABILITIES, APOCRYPHA_RUNTIME_CONFIGURATION } from '@/lib/apocrypha/readiness';

const ADAPTERS = ['mempalace', 'brainmonsoon', 'anamnesis', 'graphify', 'mneme', 'metaharness'] as const;
const ADAPTER_STATES = new Set(['ok', 'unconfigured', 'timeout', 'error', 'denied']);

function operationalAdapterStates(value: unknown): Record<string, string> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('INVALID_ADAPTER_STATES');
  const body = value as Record<string, unknown>;
  if (Object.keys(body).some((name) => !ADAPTERS.includes(name as typeof ADAPTERS[number]))) {
    throw new Error('INVALID_ADAPTER_STATES');
  }
  const result: Record<string, string> = {};
  for (const name of ADAPTERS) {
    if (typeof body[name] !== 'string' || !ADAPTER_STATES.has(body[name])) throw new Error('INVALID_ADAPTER_STATES');
    result[name] = body[name];
  }
  return result;
}

function runtimeCapabilities(value: unknown, ordered: boolean): string[] {
  if (!Array.isArray(value) || value.length !== APOCRYPHA_RUNTIME_CAPABILITIES.length
    || value.some((item) => typeof item !== 'string')) {
    throw new Error('WORKER_CAPABILITY_ADMISSION_MISMATCH');
  }
  const capabilities = value as string[];
  const matches = ordered
    ? capabilities.every((capability, index) => capability === APOCRYPHA_RUNTIME_CAPABILITIES[index])
    : APOCRYPHA_RUNTIME_CAPABILITIES.every((capability) => capabilities.includes(capability));
  if (!matches || new Set(capabilities).size !== capabilities.length) {
    throw new Error('WORKER_CAPABILITY_ADMISSION_MISMATCH');
  }
  return [...capabilities];
}

function capabilityMemory(value: unknown): Record<string, {
  adapter_states: Record<string, string>;
  adapter_probe_at: string | null;
}> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('INVALID_CAPABILITY_MEMORY');
  }
  const body = value as Record<string, unknown>;
  const names = Object.keys(body);
  if (names.length !== APOCRYPHA_RUNTIME_CAPABILITIES.length
    || !APOCRYPHA_RUNTIME_CAPABILITIES.every((capability) => names.includes(capability))) {
    throw new Error('INVALID_CAPABILITY_MEMORY');
  }
  return Object.fromEntries(APOCRYPHA_RUNTIME_CAPABILITIES.map((capability) => {
    const entry = body[capability];
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) throw new Error('INVALID_CAPABILITY_MEMORY');
    const scoped = entry as Record<string, unknown>;
    if (Object.keys(scoped).some((key) => !['adapter_states', 'adapter_probe_at'].includes(key))) {
      throw new Error('INVALID_CAPABILITY_MEMORY');
    }
    return [capability, {
      adapter_states: operationalAdapterStates(scoped.adapter_states),
      adapter_probe_at: probeTimestamp(scoped.adapter_probe_at, 'INVALID_CAPABILITY_PROBE_AT', true),
    }];
  }));
}

function probeTimestamp(value: unknown, code: string, nullable = false): string | null {
  if (nullable && value === null) return null;
  const parsed = typeof value === 'string' ? Date.parse(value) : Number.NaN;
  if (typeof value !== 'string' || value.length > 40 || !Number.isFinite(parsed) || parsed > Date.now() + 30_000) {
    throw new Error(code);
  }
  return new Date(parsed).toISOString();
}

function generationDeadline(value: unknown): number {
  if (!Number.isInteger(value) || Number(value) < 60_000 || Number(value) > 7_200_000) {
    throw new Error('INVALID_GENERATION_DEADLINE');
  }
  return Math.min(7_200_000, Math.max(60_000, Number(value)));
}

function memoryProbeDiagnostics(value: unknown): {
  started_at: string | null;
  last_success_at: string | null;
  consecutive_failures: number;
  last_failure: { code: string; detail: string; at: string } | null;
} {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return { started_at: null, last_success_at: null, consecutive_failures: 0, last_failure: null };
  }
  const source = value as Record<string, unknown>;
  const failure = source.last_failure && typeof source.last_failure === 'object' && !Array.isArray(source.last_failure)
    ? source.last_failure as Record<string, unknown>
    : null;
  const failureAt = failure && typeof failure.at === 'string'
    ? probeTimestamp(failure.at, 'INVALID_MEMORY_PROBE_AT')
    : null;
  return {
    started_at: source.started_at == null ? null : probeTimestamp(source.started_at, 'INVALID_MEMORY_PROBE_AT', true),
    last_success_at: source.last_success_at == null ? null : probeTimestamp(source.last_success_at, 'INVALID_MEMORY_PROBE_AT', true),
    consecutive_failures: Number.isInteger(source.consecutive_failures) && Number(source.consecutive_failures) >= 0
      ? Math.min(100_000, Number(source.consecutive_failures)) : 0,
    last_failure: failure && typeof failure.code === 'string' && typeof failure.detail === 'string' && failureAt
      ? { code: failure.code.slice(0, 96), detail: failure.detail.slice(0, 300), at: failureAt }
      : null,
  };
}

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  noStore(res);
  if (req.method !== 'POST') return methodNotAllowed(res, ['POST']);
  try {
    const token = assertWorkerRequest(req.headers.authorization);
    const body = objectField(req.body, 'body');
    const nodeId = required(body, 'node_id');
    const client = getApocryphaServiceClient();
    const { data: authData, error: authError } = await client.rpc('apocrypha_require_worker', {
      p_node_id: nodeId,
      p_node_token: token,
      p_require_active: true,
    });
    if (authError) throw workerDatabaseError(authError, 'HEARTBEAT_AUTH_FAILED');
    const authNode = (Array.isArray(authData) ? authData[0] : authData) as Record<string, unknown> | null;
    runtimeCapabilities(authNode?.allowed_capabilities, false);
    const capabilities = runtimeCapabilities(body.capabilities, true);
    for (const [field, expected] of Object.entries(APOCRYPHA_RUNTIME_CONFIGURATION)) {
      if (body[field] !== expected) throw new Error('WORKER_MANIFEST_MISMATCH');
    }
    if (typeof body.qwen_healthy !== 'boolean') throw new Error('INVALID_QWEN_HEALTHY');
    const qwenProbeAt = probeTimestamp(body.qwen_probe_at, 'INVALID_QWEN_PROBE_AT');
    const adapterStates = operationalAdapterStates(body.adapter_states);
    const adapterProbeAt = probeTimestamp(body.adapter_probe_at, 'INVALID_ADAPTER_PROBE_AT', true);
    const scopedMemory = capabilityMemory(body.capability_memory);
    const memoryProbe = memoryProbeDiagnostics(body.memory_probe);
    const generationDeadlineMs = generationDeadline(body.generation_deadline_ms);
    const { data, error } = await client
      .from('apocrypha_worker_node')
      .update({
        last_seen_at: new Date().toISOString(),
        model_profiles: {
          model_alias: body.model_alias,
          profile_hash: body.profile_hash,
          tool_registry_version: body.tool_registry_version,
          memory_manifest_hash: body.memory_manifest_hash,
          capabilities,
          qwen_healthy: body.qwen_healthy,
          qwen_probe_at: qwenProbeAt,
          adapter_states: adapterStates,
          adapter_probe_at: adapterProbeAt,
          capability_memory: scopedMemory,
          generation_deadline_ms: generationDeadlineMs,
          memory_probe: memoryProbe,
          phase: body.status,
          load: body.load,
        },
      })
      .eq('id', nodeId)
      .eq('status', 'active')
      .select('id,last_seen_at')
      .maybeSingle();
    if (error || !data) throw new Error(`HEARTBEAT_FAILED:${error?.code ?? 'node_not_found'}`);
    return res.status(200).json({ ok: true, node_id: data.id, last_seen_at: data.last_seen_at });
  } catch (error) {
    const safe = publicJobError(error);
    return res.status(safe.status).json({ ok: false, code: safe.code, error: safe.message });
  }
}
