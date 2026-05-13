// /api/admin/check · verifies signed-in user is on admin allowlist
// Allowlist via APOCKY_ADMIN_EMAILS env-var (comma-separated) · falls-back to apocky13@gmail.com

import type { NextApiRequest, NextApiResponse } from 'next';
import { getAdminAuthorization } from '../../../lib/admin-auth';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ authorized: false, reason: 'Method not allowed' });
  }

  const result = await getAdminAuthorization(req);

  return res.status(200).json({
    authorized: result.authorized,
    email: result.user?.email,
    stub: !result.authConfigured || undefined,
    reason: result.authorized ? undefined : result.reason,
  });
}
