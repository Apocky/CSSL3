import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { requireAdmin, requireRunnerToken } from '@/lib/lazarus/auth';
import { listRunners, registerRunner } from '@/lib/lazarus/store';
import type { RegisterRunnerInput } from '@/lib/lazarus/types';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  try {
    if (req.method === 'GET') {
      if (!(await requireAdmin(req, res))) return;
      return res.status(200).json({ ...(await listRunners()), ...envelope() });
    }
    if (req.method === 'POST') {
      if (!requireRunnerToken(req, res)) return;
      const body = (req.body ?? {}) as RegisterRunnerInput;
      return res.status(200).json({ ...(await registerRunner(body)), ...envelope() });
    }
    res.setHeader('Allow', 'GET, POST');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  } catch (err) {
    return res.status(500).json({ error: err instanceof Error ? err.message : String(err), ...envelope() });
  }
}
