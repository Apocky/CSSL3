// /api/auth/logout · clears Supabase session-cookie · returns success

import type { NextApiRequest, NextApiResponse } from 'next';
import { clearedSessionCookies, hasSameOrigin } from '../../../lib/auth-session';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    return res.status(405).json({ ok: false });
  }

  res.setHeader('Cache-Control', 'private, no-store, max-age=0');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Vary', 'Origin, Cookie');
  if (!hasSameOrigin(req)) return res.status(403).json({ ok: false });
  res.setHeader('Set-Cookie', clearedSessionCookies());

  return res.status(200).json({ ok: true });
}
