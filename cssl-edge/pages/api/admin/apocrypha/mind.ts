// apocky.com/api/admin/apocrypha/mind · ContinuousMind controls + health
// GET   /api/admin/apocrypha/mind                  → snapshot
// POST  /api/admin/apocrypha/mind?action=dream     → trigger one dream cycle

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { proxyToApocrypha } from '@/lib/apocrypha/proxy';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method === 'GET') {
    return proxyToApocrypha(req, res, { method: 'GET', upstreamPath: '/api/v1/mind/health' });
  }
  if (req.method === 'POST') {
    const action = typeof req.query.action === 'string' ? req.query.action : 'dream';
    if (action === 'dream') {
      return proxyToApocrypha(req, res, {
        method: 'POST',
        upstreamPath: '/api/v1/mind/dream/trigger',
      });
    }
    return res.status(400).json({ error: `unknown action: ${action}`, ...envelope() });
  }
  res.setHeader('Allow', 'GET, POST');
  return res.status(405).json({ error: 'Method not allowed', ...envelope() });
}
