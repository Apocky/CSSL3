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
import { createMemberThread, listMemberThreads, type MemberChatThread } from '@/lib/apocrypha/member-chat';

// GET  /api/apocrypha/member/threads[?archived=1]   the sidebar
// POST /api/apocrypha/member/threads {title?}        new chat
export interface MemberThreadsDependencies {
  resolveUser(req: NextApiRequest): Promise<RequestUserResult>;
  list(input: { verifiedAuthUserId: string; includeArchived?: boolean }): Promise<MemberChatThread[]>;
  create(input: { verifiedAuthUserId: string; title?: string | null }): Promise<MemberChatThread>;
}

const DEFAULT_DEPENDENCIES: MemberThreadsDependencies = { resolveUser: getRequestUser, list: listMemberThreads, create: createMemberThread };

export function createMemberThreadsHandler(dependencies: MemberThreadsDependencies = DEFAULT_DEPENDENCIES) {
  return async function memberThreadsHandler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
    setMemberChatPrivateHeaders(res);
    res.setHeader('Allow', 'GET, POST');
    if (req.method !== 'GET' && req.method !== 'POST') {
      res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' });
      return;
    }
    if (req.method === 'GET' ? !hasMemberChatReadOrigin(req) : !hasSameOrigin(req)) {
      res.status(403).json({ ok: false, code: 'MEMBER_ORIGIN_DENIED', error: 'Same-origin request required.' });
      return;
    }
    const session = await dependencies.resolveUser(req);
    if (!session.user) { authFailure(res, session); return; }
    try {
      if (req.method === 'GET') {
        const includeArchived = req.query.archived === '1' || req.query.archived === 'true';
        const threads = await dependencies.list({ verifiedAuthUserId: session.user.id, includeArchived });
        res.status(200).json({ ok: true, threads, count: threads.length });
        return;
      }
      const body = (req.body ?? {}) as { title?: unknown };
      if (body !== null && typeof body === 'object' && Object.keys(body).some((key) => key !== 'title')) {
        res.status(400).json({ ok: false, code: 'MEMBER_THREAD_BODY_INVALID', error: 'Body may contain only title.' });
        return;
      }
      const thread = await dependencies.create({ verifiedAuthUserId: session.user.id, title: typeof body.title === 'string' ? body.title : null });
      res.status(201).json({ ok: true, thread });
    } catch (error) {
      const failure = memberChatPublicError(error);
      res.status(failure.status).json(failure.body);
    }
  };
}

export default createMemberThreadsHandler();
