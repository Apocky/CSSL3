import type { NextApiRequest, NextApiResponse } from 'next';

import { required, workerRpc } from '@/lib/apocrypha/worker-http';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  return workerRpc(req, res, 'apocrypha_complete_job', (body, token) => ({
    p_node_id: required(body, 'node_id'), p_node_token: token,
    p_job_id: required(body, 'job_id'), p_attempt_id: required(body, 'attempt_id'),
    p_lease_epoch: required(body, 'lease_epoch'), p_lease_token: required(body, 'lease_token'),
    p_content: required(body, 'content'), p_revision_role: body.revision_role ?? 'primary',
    p_provenance: body.provenance ?? {}, p_usage: body.usage ?? {},
  }));
}
