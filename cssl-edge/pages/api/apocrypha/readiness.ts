import type { NextApiRequest, NextApiResponse } from 'next';

import {
  APOCRYPHA_MEMORY_MANIFEST_HASH,
  APOCRYPHA_MODEL_ALIAS,
  APOCRYPHA_PROFILE_HASH,
  APOCRYPHA_TOOL_REGISTRY_VERSION,
  assertChaosBridgeSignature,
  getApocryphaServiceClient,
  publicJobError,
} from '@/lib/apocrypha/job-control';
import { methodNotAllowed, noStore } from '@/lib/apocrypha/job-http';
import { projectApocryphaReadiness } from '@/lib/apocrypha/readiness';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  noStore(res);
  if (req.method !== 'GET') return methodNotAllowed(res, ['GET']);

  try {
    assertChaosBridgeSignature({
      authorization: req.headers.authorization,
      method: req.method,
      url: req.url,
      body: undefined,
      headers: req.headers,
    });

    const client = getApocryphaServiceClient();
    const failedSince = new Date(Date.now() - 86_400_000).toISOString();
    const [nodesResult, queuedResult, activeResult, failedResult] = await Promise.all([
      client.from('apocrypha_worker_node')
        .select('status,allowed_capabilities,model_profiles,last_seen_at')
        .eq('status', 'active')
        .order('last_seen_at', { ascending: false, nullsFirst: false }),
      client.from('apocrypha_job').select('id', { count: 'exact', head: true }).eq('status', 'queued'),
      client.from('apocrypha_job').select('id', { count: 'exact', head: true }).in('status', ['leased', 'running', 'cancel_requested']),
      client.from('apocrypha_job').select('id', { count: 'exact', head: true }).eq('status', 'failed').gte('updated_at', failedSince),
    ]);
    const databaseError = nodesResult.error ?? queuedResult.error ?? activeResult.error ?? failedResult.error;
    if (databaseError) throw new Error(`READINESS_DATABASE_FAILED:${databaseError.code ?? 'unknown'}`);

    const projection = projectApocryphaReadiness({
      nodes: nodesResult.data ?? [],
      queue: {
        queued: queuedResult.count ?? 0,
        active: activeResult.count ?? 0,
        failed_24h: failedResult.count ?? 0,
      },
      expected: {
        model_alias: APOCRYPHA_MODEL_ALIAS,
        profile_hash: APOCRYPHA_PROFILE_HASH,
        tool_registry_version: APOCRYPHA_TOOL_REGISTRY_VERSION,
        memory_manifest_hash: APOCRYPHA_MEMORY_MANIFEST_HASH,
      },
    });

    return res.status(projection.ready ? 200 : 503).json({ ok: projection.ready, ...projection });
  } catch (error) {
    const safe = publicJobError(error);
    return res.status(safe.status).json({
      ok: false,
      ready: false,
      rail: 'durable-outbound-qwen',
      status: 'unavailable',
      code: safe.code,
      error: safe.message,
    });
  }
}
