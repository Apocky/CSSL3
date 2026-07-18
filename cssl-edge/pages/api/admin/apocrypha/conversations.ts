// apocky.com/api/admin/apocrypha/conversations · proxy → Apocrypha /api/v1/conversations
// GET (no params) : list recent conversations
// GET ?id=N       : fetch one conversation's messages (delegates to /api/v1/conversations/N/messages)

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { proxyToApocrypha } from '@/lib/apocrypha/proxy';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET' && req.method !== 'PATCH') {
    res.setHeader('Allow', 'GET, PATCH');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  }
  const idRaw = typeof req.query.id === 'string' ? req.query.id : '';
  const id = idRaw && /^\d+$/.test(idRaw) ? Number(idRaw) : null;

  if (id !== null) {
    if (req.method === 'PATCH') {
      const body = req.body as { action?: string; expected_version?: number; title?: string };
      return proxyToApocrypha(req, res, {
        method: 'PATCH',
        upstreamPath: `/api/v1/conversations/${id}`,
        body: {
          action: body?.action,
          expected_version: body?.expected_version,
          ...(body?.title !== undefined ? { title: body.title } : {}),
        },
      });
    }
    return proxyToApocrypha(req, res, {
      method: 'GET',
      upstreamPath: `/api/v1/conversations/${id}/messages`,
    });
  }
  const requestedScope = typeof req.query.scope === 'string' && ['active', 'archived', 'trash', 'all'].includes(req.query.scope)
    ? req.query.scope
    : 'active';
  const requestedLimit = typeof req.query.limit === 'string' && /^\d+$/.test(req.query.limit)
    ? Number(req.query.limit)
    : undefined;
  return proxyToApocrypha(req, res, {
    method: 'GET',
    upstreamPath: '/api/v1/conversations',
    query: { scope: requestedScope, limit: requestedLimit },
  });
}
