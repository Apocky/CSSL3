import type { NextApiRequest, NextApiResponse } from 'next';

import { getAuthClient } from '../../../lib/auth';
import { bearerFromRequest, hasSameOrigin, jwtLifetimeSeconds, sessionCookies } from '../../../lib/auth-session';

type VerifySession = (token: string) => Promise<'valid' | 'invalid' | 'unavailable'>;

async function defaultVerify(token: string): Promise<'valid' | 'invalid' | 'unavailable'> {
  const client = getAuthClient();
  if (!client) return 'unavailable';
  try {
    const result = await Promise.race([
      client.auth.getUser(token),
      new Promise<never>((_, reject) => setTimeout(() => reject(new Error('timeout')), 5000)),
    ]);
    if (!result.error && result.data.user) return 'valid';
    return result.error?.status === 401 || result.error?.status === 403 ? 'invalid' : 'unavailable';
  } catch {
    return 'unavailable';
  }
}

export function createSessionHandler(
  verify: VerifySession = defaultVerify,
  options: { production?: boolean; now?: () => number } = {},
) {
  return async function sessionHandler(req: NextApiRequest, res: NextApiResponse) {
    res.setHeader('Cache-Control', 'private, no-store, max-age=0');
    res.setHeader('Pragma', 'no-cache');
    res.setHeader('Vary', 'Origin');

    if (req.method !== 'POST') {
      res.setHeader('Allow', 'POST');
      return res.status(405).json({ ok: false });
    }
    if (!hasSameOrigin(req)) return res.status(403).json({ ok: false });

    const token = bearerFromRequest(req);
    if (!token) return res.status(401).json({ ok: false });
    const maxAge = jwtLifetimeSeconds(token, options.now?.() ?? Date.now());
    if (!maxAge) return res.status(401).json({ ok: false });

    const verdict = await verify(token);
    if (verdict === 'unavailable') return res.status(503).json({ ok: false });
    if (verdict !== 'valid') return res.status(401).json({ ok: false });

    res.setHeader('Set-Cookie', sessionCookies(token, maxAge, options.production ?? process.env.NODE_ENV === 'production'));
    return res.status(200).json({ ok: true, expiresIn: maxAge });
  };
}

export default createSessionHandler();
