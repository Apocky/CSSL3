import type { NextApiRequest, NextApiResponse } from 'next';

import {
  APOCRYPHA_MEMORY_MANIFEST_HASH,
  APOCRYPHA_MODEL_ALIAS,
  APOCRYPHA_PROFILE_HASH,
  APOCRYPHA_TOOL_REGISTRY_VERSION,
  getApocryphaServiceClient,
} from '@/lib/apocrypha/job-control';
import { noStore } from '@/lib/apocrypha/job-http';
import { projectApocryphaReadiness } from '@/lib/apocrypha/readiness';
import { requireAdmin } from '@/lib/require-admin';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  noStore(res);
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' });
  }
  if (!(await requireAdmin(req, res))) return;
  try {
    const client = getApocryphaServiceClient();
    const [nodesResult, queueResult, runningResult, failedResult] = await Promise.all([
      client.from('apocrypha_worker_node')
        .select('id,node_key,display_name,status,allowed_capabilities,max_concurrency,model_profiles,last_seen_at,updated_at')
        .neq('status', 'revoked')
        .order('last_seen_at', { ascending: false, nullsFirst: false }),
      client.from('apocrypha_job').select('id', { count: 'exact', head: true }).eq('status', 'queued'),
      client.from('apocrypha_job').select('id', { count: 'exact', head: true }).in('status', ['leased', 'running', 'cancel_requested']),
      client.from('apocrypha_job').select('id', { count: 'exact', head: true }).eq('status', 'failed')
        .gte('updated_at', new Date(Date.now() - 86_400_000).toISOString()),
    ]);
    if (nodesResult.error || queueResult.error || runningResult.error || failedResult.error) {
      throw nodesResult.error ?? queueResult.error ?? runningResult.error ?? failedResult.error;
    }
    const projection = projectApocryphaReadiness({
      nodes: nodesResult.data ?? [],
      queue: {
        queued: queueResult.count ?? 0,
        active: runningResult.count ?? 0,
        failed_24h: failedResult.count ?? 0,
      },
      expected: {
        model_alias: APOCRYPHA_MODEL_ALIAS,
        profile_hash: APOCRYPHA_PROFILE_HASH,
        tool_registry_version: APOCRYPHA_TOOL_REGISTRY_VERSION,
        memory_manifest_hash: APOCRYPHA_MEMORY_MANIFEST_HASH,
      },
      requiredCapability: 'apocky_owner_chat',
    });
    return res.status(projection.ready ? 200 : 503).json({
      ok: projection.ready,
      reachable: projection.worker.fresh_nodes > 0,
      ...projection,
    });
  } catch {
    return res.status(503).json({
      ok: false,
      reachable: false,
      rail: 'durable-outbound-qwen',
      code: 'CONTROL_PLANE_UNAVAILABLE',
      message: 'Apocrypha job control is temporarily unavailable.',
    });
  }
}
