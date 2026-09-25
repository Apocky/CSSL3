// Service control panel: GET status of the PC's services; POST {key} restarts one, hidden.
import type { NextApiRequest, NextApiResponse } from 'next';

import { directGuard } from '@/lib/direct/guard';
import { restartService, serviceStatus } from '@/lib/direct/services';

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  if (!directGuard(req, res, ['GET', 'POST'])) return;
  if (req.method === 'GET') { res.status(200).json({ ok: true, services: await serviceStatus() }); return; }
  const key = typeof req.body?.key === 'string' ? req.body.key : '';
  const result = await restartService(key);
  res.status(result.ok ? 200 : 400).json(result);
}
