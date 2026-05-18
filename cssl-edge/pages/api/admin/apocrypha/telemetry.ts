// apocky.com/api/admin/apocrypha/telemetry · proxy → Apocrypha /api/v1/telemetry/recent
// Returns recent TelemetryEvents for the cockpit poll-loop.

import type { NextApiRequest, NextApiResponse } from 'next';

import { envelope } from '@/lib/response';
import { proxyToApocrypha } from '@/lib/apocrypha/proxy';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ error: 'Method not allowed', ...envelope() });
  }
  const limit = typeof req.query.limit === 'string' ? Number(req.query.limit) : 200;
  const prefix = typeof req.query.prefix === 'string' ? req.query.prefix : undefined;
  const query: Record<string, string | number | undefined> = {
    limit: Number.isFinite(limit) ? limit : 200,
  };
  if (prefix) query.prefix = prefix;
  await proxyToApocrypha(req, res, {
    method: 'GET',
    upstreamPath: '/api/v1/telemetry/recent',
    query,
  });
}
