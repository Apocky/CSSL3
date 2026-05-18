// apocky.com/api/admin/apocrypha/tool_calls · GET → Apocrypha /api/v1/tool_calls/recent

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { proxyToApocrypha } from '@/lib/apocrypha/proxy';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  }
  const limit = typeof req.query.limit === 'string' ? Number(req.query.limit) : 50;
  await proxyToApocrypha(req, res, {
    method: 'GET',
    upstreamPath: '/api/v1/tool_calls/recent',
    query: { limit: Number.isFinite(limit) ? limit : 50 },
  });
}
