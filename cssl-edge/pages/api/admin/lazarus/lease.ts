import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { requireRunnerToken } from '@/lib/lazarus/auth';
import { leaseNextTask } from '@/lib/lazarus/store';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  }
  try {
    if (!requireRunnerToken(req, res)) return;
    const body = (req.body ?? {}) as { runner_id?: unknown };
    if (typeof body.runner_id !== 'string') {
      return res.status(400).json({ error: 'runner_id required', ...envelope() });
    }
    return res.status(200).json({ ...(await leaseNextTask(body.runner_id)), ...envelope() });
  } catch (err) {
    return res.status(500).json({ error: err instanceof Error ? err.message : String(err), ...envelope() });
  }
}
