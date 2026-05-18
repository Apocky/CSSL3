// Shared requireAdmin guard for admin-only API routes.
// Extracted from lib/lazarus/auth.ts when D043 absorbed Lazarus into Apocrypha —
// the guard itself is project-generic, not Lazarus-specific.

import type { NextApiRequest, NextApiResponse } from 'next';

import { getAdminAuthorization } from '@/lib/admin-auth';
import { envelope } from '@/lib/response';

export async function requireAdmin(req: NextApiRequest, res: NextApiResponse): Promise<boolean> {
  const result = await getAdminAuthorization(req);
  if (result.authorized) return true;
  const status = result.user ? 403 : 401;
  res.status(status).json({
    error: result.reason ?? 'Admin authorization required.',
    authorized: false,
    ...envelope(),
  });
  return false;
}
