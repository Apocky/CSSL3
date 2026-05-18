// apocky.com/api/admin/apocrypha/sub_minds · proxy → Apocrypha /api/v1/sub_minds/health
// Returns combined Lazarus (Ω9 operator) + Tessera (Ω10 reasoner) health snapshot.

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { proxyToApocrypha } from '@/lib/apocrypha/proxy';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  }
  await proxyToApocrypha(req, res, {
    method: 'GET',
    upstreamPath: '/api/v1/sub_minds/health',
  });
}
