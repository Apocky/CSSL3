// /api/auth/me · returns current-user session info OR null
// Stub-mode safe : returns { user: null, stub: true } when hub Supabase not configured

import type { NextApiRequest, NextApiResponse } from 'next';
import { getRequestUser } from '../../../lib/admin-auth';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ user: null });
  }

  const result = await getRequestUser(req);
  return res.status(200).json({
    user: result.user,
    stub: !result.authConfigured || undefined,
    reason: result.user ? undefined : result.reason,
  });
}
