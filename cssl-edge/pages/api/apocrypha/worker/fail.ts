import type { NextApiRequest, NextApiResponse } from 'next';

import { required, workerRpc } from '@/lib/apocrypha/worker-http';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  return workerRpc(req, res, 'apocrypha_fail_job', (body, token) => ({
    p_node_id: required(body, 'node_id'), p_node_token: token,
    p_job_id: required(body, 'job_id'), p_attempt_id: required(body, 'attempt_id'),
    p_lease_epoch: required(body, 'lease_epoch'), p_lease_token: required(body, 'lease_token'),
    p_error_code: required(body, 'error_code'), p_error_detail: String(body.error_detail ?? '').slice(0, 4000),
    p_retryable: body.retryable !== false, p_metrics: body.metrics ?? {},
  }));
}
