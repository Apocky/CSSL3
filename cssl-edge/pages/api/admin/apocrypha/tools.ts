// apocky.com/api/admin/apocrypha/tools · GET → Apocrypha tunnel /api/v1/tools

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { proxyToApocrypha } from '@/lib/apocrypha/proxy';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  }
  await proxyToApocrypha(req, res, { method: 'GET', upstreamPath: '/api/v1/tools' });
}
