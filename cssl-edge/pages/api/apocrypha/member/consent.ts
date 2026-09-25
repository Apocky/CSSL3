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
import { setMemberConsent } from '@/lib/apocrypha/member-chat';
import { getApocryphaServiceClient } from '@/lib/apocrypha/job-control';

// POST /api/apocrypha/member/consent {analytics: boolean}
// Tracking that actually collects usable data does so only after this switch is on; the
// database drops member events without it (apocrypha_record_analytics_event).
export interface MemberConsentDependencies {
  resolveUser(req: NextApiRequest): Promise<RequestUserResult>;
  set(input: { verifiedAuthUserId: string; analytics: boolean }): Promise<{ analytics: boolean; updated_at: string }>;
  get?(input: { verifiedAuthUserId: string }): Promise<{ analytics: boolean; updated_at: string | null }>;
}

// Read the member's current choice; no row means the default, which is off.
async function readConsent(input: { verifiedAuthUserId: string }): Promise<{ analytics: boolean; updated_at: string | null }> {
  const { data, error } = await getApocryphaServiceClient()
    .from('apocrypha_member_consent').select('analytics,updated_at')
    .eq('auth_user_id', input.verifiedAuthUserId).maybeSingle();
  if (error) throw new Error(`MEMBER_CONSENT_READ_FAILED:${error.code ?? 'unknown'}`);
  return { analytics: data?.analytics === true, updated_at: typeof data?.updated_at === 'string' ? data.updated_at : null };
}
const DEFAULT_DEPENDENCIES: MemberConsentDependencies = { resolveUser: getRequestUser, set: setMemberConsent, get: readConsent };

export function createMemberConsentHandler(dependencies: MemberConsentDependencies = DEFAULT_DEPENDENCIES) {
  return async function memberConsentHandler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
    setMemberChatPrivateHeaders(res);
    res.setHeader('Allow', 'GET, POST');
    if (req.method === 'GET') {
      if (!hasMemberChatReadOrigin(req)) { res.status(403).json({ ok: false, code: 'MEMBER_ORIGIN_DENIED', error: 'Same-origin request required.' }); return; }
      const session = await dependencies.resolveUser(req);
      if (!session.user) { authFailure(res, session); return; }
      try {
        res.status(200).json({ ok: true, consent: await (dependencies.get ?? readConsent)({ verifiedAuthUserId: session.user.id }) });
      } catch (error) {
        const failure = memberChatPublicError(error);
        res.status(failure.status).json(failure.body);
      }
      return;
    }
    if (req.method !== 'POST') { res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' }); return; }
    if (!hasSameOrigin(req)) { res.status(403).json({ ok: false, code: 'MEMBER_ORIGIN_DENIED', error: 'Same-origin request required.' }); return; }
    const body = req.body;
    if (body === null || typeof body !== 'object' || Array.isArray(body) || Object.keys(body).join() !== 'analytics' || typeof (body as { analytics: unknown }).analytics !== 'boolean') {
      res.status(400).json({ ok: false, code: 'MEMBER_CONSENT_BODY_INVALID', error: 'Body must be {analytics: boolean}.' });
      return;
    }
    const session = await dependencies.resolveUser(req);
    if (!session.user) { authFailure(res, session); return; }
    try {
      const consent = await dependencies.set({ verifiedAuthUserId: session.user.id, analytics: (body as { analytics: boolean }).analytics });
      res.status(200).json({ ok: true, consent });
    } catch (error) {
      const failure = memberChatPublicError(error);
      res.status(failure.status).json(failure.body);
    }
  };
}

export default createMemberConsentHandler();
