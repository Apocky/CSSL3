import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { requireAdmin } from '@/lib/lazarus/auth';
import { getLazarusHealth } from '@/lib/lazarus/store';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  }
  try {
    if (!(await requireAdmin(req, res))) return;
    return res.status(200).json({ ...(await getLazarusHealth()), ...envelope() });
  } catch (err) {
    return res.status(500).json({ error: err instanceof Error ? err.message : String(err), ...envelope() });
  }
}
