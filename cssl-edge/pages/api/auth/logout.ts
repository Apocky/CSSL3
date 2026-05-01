// /api/auth/logout · clears Supabase session-cookie · returns success

import type { NextApiRequest, NextApiResponse } from 'next';
import { signOut } from '../../../lib/auth';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    return res.status(405).json({ ok: false });
  }

  await signOut();

  // Clear common Supabase cookies (names vary by helper · clear conservatively)
  res.setHeader('Set-Cookie', [
    'sb-access-token=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax',
    'sb-refresh-token=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax',
  ]);

  return res.status(200).json({ ok: true });
}
