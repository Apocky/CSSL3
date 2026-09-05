import type { NextApiRequest, NextApiResponse } from 'next';

import { proxyV2ToApocrypha, setPrivateNoStore } from '@/lib/apocrypha/proxy';
import { envelope } from '@/lib/response';

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setPrivateNoStore(res);
  res.setHeader('Allow', 'GET');
  if (req.method !== 'GET') {
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    return;
  }
  await proxyV2ToApocrypha(req, res, { upstreamPath: '/v2/health' });
}
