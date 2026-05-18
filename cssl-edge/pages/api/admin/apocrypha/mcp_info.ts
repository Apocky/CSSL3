// apocky.com/api/admin/apocrypha/mcp_info · proxy → Apocrypha /api/v1/mcp/info
// Returns the MCP-exposed tool subset (vs blocked) + transport info.

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { proxyToApocrypha } from '@/lib/apocrypha/proxy';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  }
  await proxyToApocrypha(req, res, { method: 'GET', upstreamPath: '/api/v1/mcp/info' });
}
