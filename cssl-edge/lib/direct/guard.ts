// Every /api/direct route: 404 off direct mode or to anyone but the owner; POSTs also same-origin.
import type { NextApiRequest, NextApiResponse } from 'next';

import { directMode, directSameOrigin, directViewer } from './mode';

export function directGuard(req: NextApiRequest, res: NextApiResponse, methods: readonly string[]): boolean {
  res.setHeader('Cache-Control', 'no-store');
  if (!directMode() || !directViewer(req).owner) { res.status(404).json({ ok: false, code: 'NOT_FOUND' }); return false; }
  if (!methods.includes(req.method ?? '')) { res.setHeader('Allow', methods.join(', ')); res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' }); return false; }
  if (req.method !== 'GET' && !directSameOrigin(req)) { res.status(403).json({ ok: false, code: 'ORIGIN_REQUIRED' }); return false; }
  return true;
}
