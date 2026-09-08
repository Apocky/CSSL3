import type { NextApiRequest, NextApiResponse } from 'next';

import { required, workerRpc } from '@/lib/apocrypha/worker-http';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  return workerRpc(req, res, 'apocrypha_append_chunk', (body, token) => ({
    p_node_id: required(body, 'node_id'), p_node_token: token,
    p_job_id: required(body, 'job_id'), p_attempt_id: required(body, 'attempt_id'),
    p_lease_epoch: required(body, 'lease_epoch'), p_lease_token: required(body, 'lease_token'),
    p_seq: required(body, 'seq'), p_chunk_kind: body.chunk_kind ?? 'text',
    p_delta: required(body, 'delta'), p_metadata: body.metadata ?? {},
    p_snapshot_no: body.snapshot_no ?? null, p_snapshot_body: body.snapshot_body ?? null,
    p_snapshot_state: body.snapshot_state ?? null,
  }));
}
