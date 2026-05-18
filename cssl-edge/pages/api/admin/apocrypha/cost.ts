// apocky.com/api/admin/apocrypha/cost · proxy → Apocrypha /api/v1/cost
// Cost-tracker snapshot : today's spend, daily cap, per-model breakdown, recent calls.

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { proxyToApocrypha } from '@/lib/apocrypha/proxy';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  }
  await proxyToApocrypha(req, res, { method: 'GET', upstreamPath: '/api/v1/cost' });
}
