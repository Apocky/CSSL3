// The desktop app asks this before choosing the direct connection over apocky.com.
import type { NextApiRequest, NextApiResponse } from 'next';

import { directGuard } from '@/lib/direct/guard';

export default function handler(req: NextApiRequest, res: NextApiResponse): void {
  if (!directGuard(req, res, ['GET'])) return;
  res.status(200).json({ ok: true, direct: true, at: new Date().toISOString() });
}
