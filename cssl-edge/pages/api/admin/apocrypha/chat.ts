import type { NextApiRequest, NextApiResponse } from 'next';
import { getAuthClient } from '../../../../lib/auth';
import { apocryphaOriginHeaders } from '../../../../lib/apocryphaOrigin';

async function isAdmin(req: NextApiRequest): Promise<boolean> {
  const client = getAuthClient();
  if (!client) return false;
  const token = req.headers.cookie?.match(/sb-access-token=([^;]+)/)?.[1];
  if (!token) return false;
  const { data } = await client.auth.getUser(decodeURIComponent(token));
  const allowlist = (process.env.APOCKY_ADMIN_EMAILS ?? 'apocky13@gmail.com').split(',').map((v) => v.trim().toLowerCase());
  return Boolean(data.user?.email && allowlist.includes(data.user.email.toLowerCase()));
}

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'POST') return res.status(405).json({ ok: false, reason: 'method_not_allowed' });
  if (!(await isAdmin(req))) return res.status(403).json({ ok: false, reason: 'admin_required' });
  const origin = process.env.APOCRYPHA_ORIGIN?.replace(/\/$/, '');
  if (!origin) return res.status(503).json({ ok: false, state: 'degraded', reason: 'APOCRYPHA_ORIGIN_not_configured' });
  const message = typeof req.body?.message === 'string' ? req.body.message.trim() : '';
  if (!message || message.length > 20_000) return res.status(400).json({ ok: false, reason: 'invalid_message' });

  try {
    const upstream = await fetch(`${origin}/api/v1/chat`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', ...apocryphaOriginHeaders() },
      body: JSON.stringify({ message }),
      signal: AbortSignal.timeout(60_000),
    });
    const body = await upstream.json();
    return res.status(upstream.ok ? 200 : 502).json({ ok: upstream.ok, state: upstream.ok ? 'live' : 'degraded', body });
  } catch (error) {
    return res.status(502).json({ ok: false, state: 'degraded', reason: error instanceof Error ? error.name : 'upstream_unreachable' });
  }
}
