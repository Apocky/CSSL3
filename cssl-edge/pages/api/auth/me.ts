// /api/auth/me · returns current-user session info OR null
// Stub-mode safe : returns { user: null, stub: true } when hub Supabase not configured

import type { NextApiRequest, NextApiResponse } from 'next';
import {
  getAdminAuthorization,
  type AdminAuthorizationResult,
  type RequestUser,
} from '../../../lib/admin-auth';
import { usesOwnerRuntime } from '../../../lib/mobile/owner-runtime';

type AuthorizeRequest = (req: NextApiRequest) => Promise<AdminAuthorizationResult>;
type OwnerRuntime = (user: RequestUser) => boolean;

export function createAuthMeHandler(
  authorize: AuthorizeRequest = getAdminAuthorization,
  ownerRuntime: OwnerRuntime = usesOwnerRuntime,
) {
  return async function handler(req: NextApiRequest, res: NextApiResponse) {
    res.setHeader('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
    res.setHeader('Vary', 'Authorization, Cookie');
    if (req.method !== 'GET') {
      res.setHeader('Allow', 'GET');
      return res.status(405).json({ user: null });
    }

    // One provider verification supplies identity, member/owner classification,
    // and owner-runtime admission. The browser must not repeat the same remote
    // verification through /api/admin/check during initial page admission.
    const result = await authorize(req);
    return res.status(200).json({
      user: result.user,
      authorized: result.authorized,
      owner_conversation: Boolean(result.user && ownerRuntime(result.user)),
      stub: !result.authConfigured || undefined,
      failure_kind: result.failureKind,
      reason: result.user ? undefined : result.reason,
    });
  };
}

export default createAuthMeHandler();
