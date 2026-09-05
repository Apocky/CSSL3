// apocky.com/api/admin/apocrypha/ma · proxy → Apocrypha /admin/cognition/ma

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { proxyToApocrypha } from '@/lib/apocrypha/proxy';

const actions = new Set(['pause', 'resume', 'drain', 'restore']);

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  const action = typeof req.query.action === 'string' ? req.query.action : null;
  if (req.method !== 'GET' && req.method !== 'POST') {
    res.setHeader('Allow', 'GET, POST');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  }
  if (action && !actions.has(action)) {
    return res.status(400).json({ error: 'action must be pause, drain, restore, or resume', ...envelope() });
  }
  await proxyToApocrypha(req, res, {
    method: req.method === 'GET' ? 'GET' : 'POST',
    upstreamPath: action ? `/admin/cognition/ma/${action}` : '/admin/cognition/ma',
  });
}
