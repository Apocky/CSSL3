import type { NextApiRequest, NextApiResponse } from 'next';

import { assertWorkerRequest, getApocryphaServiceClient, publicJobError } from '@/lib/apocrypha/job-control';
import { methodNotAllowed, noStore, objectField } from '@/lib/apocrypha/job-http';
import { required, workerDatabaseError } from '@/lib/apocrypha/worker-http';

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
    const { data, error } = await client
      .from('apocrypha_worker_node')
      .update({
        last_seen_at: new Date().toISOString(),
        model_profiles: {
          model_alias: body.model_alias,
          profile_hash: body.profile_hash,
          tool_registry_version: body.tool_registry_version,
          memory_manifest_hash: body.memory_manifest_hash,
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
