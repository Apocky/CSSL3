// apocky.com/api/admin/apocrypha/conversations · proxy → Apocrypha /api/v1/conversations
// GET (no params) : list recent conversations
// GET ?id=N       : fetch one conversation's messages (delegates to /api/v1/conversations/N/messages)

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { proxyToApocrypha } from '@/lib/apocrypha/proxy';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  }
  const idRaw = typeof req.query.id === 'string' ? req.query.id : '';
  const id = idRaw && /^\d+$/.test(idRaw) ? Number(idRaw) : null;

  if (id !== null) {
    return proxyToApocrypha(req, res, {
      method: 'GET',
      upstreamPath: `/api/v1/conversations/${id}/messages`,
    });
  }
  return proxyToApocrypha(req, res, {
    method: 'GET',
    upstreamPath: '/api/v1/conversations',
  });
}
