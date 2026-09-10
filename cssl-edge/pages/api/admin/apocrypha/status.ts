import type { NextApiRequest, NextApiResponse } from 'next';

import { getApocryphaServiceClient } from '@/lib/apocrypha/job-control';
import { noStore } from '@/lib/apocrypha/job-http';
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
    const staleBefore = new Date(Date.now() - 90_000).toISOString();
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
    const nodes = (nodesResult.data ?? []).map((node) => ({
      ...node,
      reachable: Boolean(node.last_seen_at && node.last_seen_at >= staleBefore && node.status === 'active'),
    }));
    return res.status(200).json({
      ok: true,
      rail: 'durable-outbound-qwen',
      reachable: nodes.some((node) => node.reachable),
      model_alias: process.env.APOCRYPHA_MODEL_ALIAS ?? 'qwen35-35b-a3b-q4',
      profile_hash: process.env.APOCRYPHA_PROFILE_HASH ?? 'qwen35-35b-a3b-q4-vulkan-hybrid-v1',
      tool_registry_version: process.env.APOCRYPHA_TOOL_REGISTRY_VERSION ?? 'apocrypha-read-v1',
      memory_manifest_hash: process.env.APOCRYPHA_MEMORY_MANIFEST_HASH ?? 'apocrypha-memory-fabric-v1',
      queue: { queued: queueResult.count ?? 0, active: runningResult.count ?? 0, failed_24h: failedResult.count ?? 0 },
      nodes,
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
