// apocky.com/api/admin/apocrypha/chat · POST → Apocrypha tunnel /api/v1/chat
// Per HANDOFF_v10 § TRACK-A A4 (three-faces UI ; chat-face backend).

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { proxyToApocrypha } from '@/lib/apocrypha/proxy';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  }
  await proxyToApocrypha(req, res, {
    method: 'POST',
    upstreamPath: '/api/v1/chat',
    body: req.body,
  });
}
