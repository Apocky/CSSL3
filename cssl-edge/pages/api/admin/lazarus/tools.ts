import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { requireAdmin } from '@/lib/lazarus/auth';
import { listLazarusTools } from '@/lib/lazarus/store';
import { LAZARUS_APPROVAL_GATES } from '@/lib/lazarus/types';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  }
  if (!(await requireAdmin(req, res))) return;
  return res.status(200).json({
    tools: listLazarusTools(),
    approval_gates: LAZARUS_APPROVAL_GATES,
    ...envelope(),
  });
}
