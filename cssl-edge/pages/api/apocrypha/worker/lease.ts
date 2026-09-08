import type { NextApiRequest, NextApiResponse } from 'next';

import { required, workerRpc } from '@/lib/apocrypha/worker-http';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  return workerRpc(req, res, 'apocrypha_renew_lease', (body, token) => ({
    p_node_id: required(body, 'node_id'), p_node_token: token,
    p_job_id: required(body, 'job_id'), p_attempt_id: required(body, 'attempt_id'),
    p_lease_epoch: required(body, 'lease_epoch'), p_lease_token: required(body, 'lease_token'),
    p_lease_seconds: Math.min(600, Math.max(60, Number(body.lease_seconds) || 180)),
  }), (data) => {
    const row = Array.isArray(data) ? data[0] : data;
    return row && typeof row === 'object'
      ? row as Record<string, unknown>
      : { lease_expires_at: row, cancel_requested: false };
  });
}
