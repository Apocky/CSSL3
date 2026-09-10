import type { NextApiRequest, NextApiResponse } from 'next';

import { proxyV2ToApocrypha, setPrivateNoStore } from '@/lib/apocrypha/proxy';
import { isVisionSessionRef } from '@/lib/apocrypha/vision';
import { envelope } from '@/lib/response';

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setPrivateNoStore(res);
  res.setHeader('Allow', 'GET');
  if (req.method !== 'GET') {
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    return;
  }
  const sessionRef = req.query.session_ref;
  if (!isVisionSessionRef(sessionRef)) {
    res.status(400).json({ error: 'session_ref must be an opaque UUIDv4 value', ...envelope() });
    return;
  }
  await proxyV2ToApocrypha(req, res, { upstreamPath: `/v2/vision/session/${sessionRef}` });
}
