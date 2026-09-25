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
import { updateMemberThread, type MemberChatThread } from '@/lib/apocrypha/member-chat';

// PATCH /api/apocrypha/member/threads/:id {title?, pinned?, archived?}   rename / pin / archive
export interface MemberThreadUpdateDependencies {
  resolveUser(req: NextApiRequest): Promise<RequestUserResult>;
  update(input: { verifiedAuthUserId: string; threadId: string; title?: string | null; pinned?: boolean | null; archived?: boolean | null }): Promise<MemberChatThread>;
}

const DEFAULT_DEPENDENCIES: MemberThreadUpdateDependencies = { resolveUser: getRequestUser, update: updateMemberThread };
const ALLOWED = new Set(['title', 'pinned', 'archived']);

export function createMemberThreadUpdateHandler(dependencies: MemberThreadUpdateDependencies = DEFAULT_DEPENDENCIES) {
  return async function memberThreadUpdateHandler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
    setMemberChatPrivateHeaders(res);
    res.setHeader('Allow', 'PATCH');
    if (req.method !== 'PATCH') { res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' }); return; }
    if (!hasSameOrigin(req)) { res.status(403).json({ ok: false, code: 'MEMBER_ORIGIN_DENIED', error: 'Same-origin request required.' }); return; }
    const id = typeof req.query.id === 'string' ? req.query.id.toLowerCase() : '';
    if (!isMemberChatUuid(id)) { res.status(400).json({ ok: false, code: 'MEMBER_THREAD_ID_INVALID' }); return; }
    const body = req.body;
    if (body === null || typeof body !== 'object' || Array.isArray(body) || Object.keys(body).length === 0 || Object.keys(body).some((key) => !ALLOWED.has(key))) {
      res.status(400).json({ ok: false, code: 'MEMBER_THREAD_BODY_INVALID', error: 'Body may contain title, pinned, archived.' });
      return;
    }
    const { title, pinned, archived } = body as { title?: unknown; pinned?: unknown; archived?: unknown };
    if ((title !== undefined && typeof title !== 'string') || (pinned !== undefined && typeof pinned !== 'boolean') || (archived !== undefined && typeof archived !== 'boolean')) {
      res.status(400).json({ ok: false, code: 'MEMBER_THREAD_BODY_INVALID', error: 'title is a string; pinned and archived are booleans.' });
      return;
    }
    const session = await dependencies.resolveUser(req);
    if (!session.user) { authFailure(res, session); return; }
    try {
      const thread = await dependencies.update({ verifiedAuthUserId: session.user.id, threadId: id, title: title ?? null, pinned: pinned ?? null, archived: archived ?? null });
      res.status(200).json({ ok: true, thread });
    } catch (error) {
      const failure = memberChatPublicError(error);
      res.status(failure.status).json(failure.body);
    }
  };
}

export default createMemberThreadUpdateHandler();
