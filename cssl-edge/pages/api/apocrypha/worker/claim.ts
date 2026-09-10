import type { NextApiRequest, NextApiResponse } from 'next';

import { required, workerRpc } from '@/lib/apocrypha/worker-http';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  return workerRpc(req, res, 'apocrypha_claim_job', (body, token) => ({
    p_node_id: required(body, 'node_id'),
    p_node_token: token,
    p_claim_key: required(
      { claim_key: Array.isArray(req.headers['idempotency-key'])
        ? req.headers['idempotency-key'][0]
        : req.headers['idempotency-key'] },
      'claim_key',
    ),
    p_lease_seconds: Math.min(600, Math.max(60, Number(body.lease_seconds) || 180)),
  }), (data) => ({ job: Array.isArray(data) ? (data[0] ?? null) : (data ?? null) }));
}
