import type { NextApiRequest, NextApiResponse } from 'next';

import { assertWorkerRequest, getApocryphaServiceClient, publicJobError } from '@/lib/apocrypha/job-control';
import { methodNotAllowed, noStore, objectField } from '@/lib/apocrypha/job-http';
import { required, workerDatabaseError } from '@/lib/apocrypha/worker-http';

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

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  noStore(res);
  if (req.method !== 'POST') return methodNotAllowed(res, ['POST']);
  try {
    const token = assertWorkerRequest(req.headers.authorization);
    const body = objectField(req.body, 'body');
    const nodeId = required(body, 'node_id');
    const client = getApocryphaServiceClient();
    const { error: authError } = await client.rpc('apocrypha_require_worker', {
      p_node_id: nodeId,
      p_node_token: token,
      p_require_active: true,
    });
    if (authError) throw workerDatabaseError(authError, 'HEARTBEAT_AUTH_FAILED');
    if (typeof body.qwen_healthy !== 'boolean') throw new Error('INVALID_QWEN_HEALTHY');
    const qwenProbeAt = probeTimestamp(body.qwen_probe_at, 'INVALID_QWEN_PROBE_AT');
    const adapterStates = operationalAdapterStates(body.adapter_states);
    const adapterProbeAt = probeTimestamp(body.adapter_probe_at, 'INVALID_ADAPTER_PROBE_AT', true);
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
          qwen_healthy: body.qwen_healthy,
          qwen_probe_at: qwenProbeAt,
          adapter_states: adapterStates,
          adapter_probe_at: adapterProbeAt,
          generation_deadline_ms: generationDeadlineMs,
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
