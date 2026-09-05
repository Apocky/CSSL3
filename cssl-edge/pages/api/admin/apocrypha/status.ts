import type { NextApiRequest, NextApiResponse } from 'next';
import { getAuthClient } from '../../../../lib/auth';
import { apocryphaOriginHeaders } from '../../../../lib/apocryphaOrigin';

const ALLOWLIST = () =>
  (process.env.APOCKY_ADMIN_EMAILS ?? 'apocky13@gmail.com')
    .split(',')
    .map((value) => value.trim().toLowerCase())
    .filter(Boolean);

async function isAdmin(req: NextApiRequest): Promise<boolean> {
  const client = getAuthClient();
  if (!client) return false;
  const token = req.headers.cookie?.match(/sb-access-token=([^;]+)/)?.[1];
  if (!token) return false;
  const { data } = await client.auth.getUser(decodeURIComponent(token));
  return Boolean(data.user?.email && ALLOWLIST().includes(data.user.email.toLowerCase()));
}

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET') return res.status(405).json({ ok: false, reason: 'method_not_allowed' });
  if (!(await isAdmin(req))) return res.status(403).json({ ok: false, reason: 'admin_required' });

  const origin = process.env.APOCRYPHA_ORIGIN?.replace(/\/$/, '');
  if (!origin) {
    return res.status(503).json({ ok: false, state: 'degraded', reason: 'APOCRYPHA_ORIGIN_not_configured' });
  }

  try {
    const response = await fetch(`${origin}/api/status`, {
      headers: apocryphaOriginHeaders(),
      signal: AbortSignal.timeout(5000),
    });
    const body = await response.json();
    return res.status(response.ok ? 200 : 502).json({ ok: response.ok, state: response.ok ? 'live' : 'degraded', body });
  } catch (error) {
    return res.status(502).json({ ok: false, state: 'degraded', reason: error instanceof Error ? error.name : 'upstream_unreachable' });
  }
}
