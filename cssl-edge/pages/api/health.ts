// cssl-edge · /api/health
// Liveness ping. Always returns 200. Carries commit SHA so deploys are auditable.

import type { NextApiRequest, NextApiResponse } from 'next';
import { commitSha, envelope, logHit } from '@/lib/response';

export interface HealthResponse {
  ok: true;
  sha: string;
  served_by: string;
  ts: string;
  version: string;
}

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<HealthResponse>
): void {
  logHit('health', { method: req.method ?? 'GET' });

  const env = envelope();
  const body: HealthResponse = {
    ok: true,
    sha: commitSha(),
    served_by: env.served_by,
    ts: env.ts,
    version: process.env.CSSL_EDGE_VERSION ?? '0.1.0',
  };

  res.status(200).json(body);
}
