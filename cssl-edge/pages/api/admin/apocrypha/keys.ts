// apocky.com/api/admin/apocrypha/keys · CRUD → Apocrypha /api/v1/keys + /api/v1/keys/:id


// - GET   list
// - POST  create  (body : { label, principal, expires_at_iso? })
// - DELETE revoke (body : { key_id })

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { proxyToApocrypha } from '@/lib/apocrypha/proxy';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method === 'GET') {
    return proxyToApocrypha(req, res, { method: 'GET', upstreamPath: '/api/v1/keys' });
  }
  if (req.method === 'POST') {
    return proxyToApocrypha(req, res, {
      method: 'POST',
      upstreamPath: '/api/v1/keys',
      body: req.body,
    });
  }
  if (req.method === 'DELETE') {
    const keyId = typeof req.body === 'object' && req.body && 'key_id' in req.body
      ? String((req.body as { key_id: string }).key_id)
      : null;
    if (!keyId) {
      return res.status(400).json({ error: 'key_id required in body', ...envelope() });
    }
    return proxyToApocrypha(req, res, {
      method: 'DELETE',
      upstreamPath: `/api/v1/keys/${encodeURIComponent(keyId)}`,
    });
  }
  res.setHeader('Allow', 'GET, POST, DELETE');
  return res.status(405).json({ error: 'Method not allowed', ...envelope() });
}
