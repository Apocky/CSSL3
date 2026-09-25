import type { NextApiRequest, NextApiResponse } from 'next';

import { getRequestUser, type RequestUserResult } from '@/lib/admin-auth';
import { hasSameOrigin } from '@/lib/auth-session';
import {
  hasMemberChatReadOrigin,
  isMemberChatUuid,
  memberChatPublicError,
  setMemberChatPrivateHeaders,
} from '@/lib/apocrypha/member-chat';

function authFailure(res: NextApiResponse, result: RequestUserResult): void {
  const unavailable = result.failureKind === 'upstream-unavailable' || result.failureKind === 'unconfigured';
  res.status(unavailable ? 503 : 401).json({
    ok: false,
    code: unavailable ? 'MEMBER_SESSION_UNAVAILABLE' : 'MEMBER_SESSION_REQUIRED',
    error: unavailable ? 'The sign-in service could not verify this member session.' : 'Sign in to use your conversations.',
  });
}
import { getMemberPlan, type MemberPlan } from '@/lib/apocrypha/member-chat';

// GET /api/apocrypha/member/plan -> { flagship, default_lane, product_id }
// The premium plan (Stripe product apocrypha-premium) unlocks the flagship lane; the server
// resolves it from the entitlement, and the enqueue RPC enforces it again on every turn.
export interface MemberPlanDependencies {
  resolveUser(req: NextApiRequest): Promise<RequestUserResult>;
  plan(input: { verifiedAuthUserId: string }): Promise<MemberPlan>;
}
const DEFAULT_DEPENDENCIES: MemberPlanDependencies = { resolveUser: getRequestUser, plan: getMemberPlan };

export function createMemberPlanHandler(dependencies: MemberPlanDependencies = DEFAULT_DEPENDENCIES) {
  return async function memberPlanHandler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
    setMemberChatPrivateHeaders(res);
    res.setHeader('Allow', 'GET');
    if (req.method !== 'GET') { res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' }); return; }
    if (!hasMemberChatReadOrigin(req)) { res.status(403).json({ ok: false, code: 'MEMBER_ORIGIN_DENIED', error: 'Same-origin request required.' }); return; }
    const session = await dependencies.resolveUser(req);
    if (!session.user) { authFailure(res, session); return; }
    try {
      const plan = await dependencies.plan({ verifiedAuthUserId: session.user.id });
      res.status(200).json({ ok: true, plan });
    } catch (error) {
      const failure = memberChatPublicError(error);
      res.status(failure.status).json(failure.body);
    }
  };
}

export default createMemberPlanHandler();
