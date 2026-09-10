// Every conversation the signed-in member owns.
//
// There is no conversation id in this request, and that is the point: the list
// is derived entirely from the verified session inside the RPC, so there is
// nothing here a caller could present that would widen what comes back.

import type { NextApiRequest, NextApiResponse } from 'next';

import { getRequestUser, type RequestUserResult } from '@/lib/admin-auth';
import {
  hasMemberChatReadOrigin,
  listMemberConversations,
  MemberChatStoreError,
  memberChatPublicError,
  setMemberChatPrivateHeaders,
  type MemberChatConversationSummary,
} from '@/lib/apocrypha/member-chat';

export interface MemberChatConversationsDependencies {
  resolveUser(req: NextApiRequest): Promise<RequestUserResult>;
  listConversations(input: {
    verifiedAuthUserId: string;
    limit?: number;
  }): Promise<MemberChatConversationSummary[]>;
}

const DEFAULT_DEPENDENCIES: MemberChatConversationsDependencies = {
  resolveUser: getRequestUser,
  listConversations: listMemberConversations,
};

function authFailure(res: NextApiResponse, result: RequestUserResult): void {
  const unavailable = result.failureKind === 'upstream-unavailable'
    || result.failureKind === 'unconfigured';
  res.status(unavailable ? 503 : 401).json({
    ok: false,
    code: unavailable ? 'MEMBER_SESSION_UNAVAILABLE' : 'MEMBER_SESSION_REQUIRED',
    error: unavailable
      ? 'The sign-in service could not verify this member session.'
      : 'Sign in to read your conversations.',
  });
}

export function createMemberChatConversationsHandler(
  dependencies: MemberChatConversationsDependencies = DEFAULT_DEPENDENCIES,
) {
  return async function memberChatConversationsHandler(
    req: NextApiRequest,
    res: NextApiResponse,
  ): Promise<void> {
    setMemberChatPrivateHeaders(res);
    res.setHeader('Allow', 'GET');

    if (req.method !== 'GET') {
      res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' });
      return;
    }
    if (!hasMemberChatReadOrigin(req)) {
      res.status(403).json({ ok: false, code: 'MEMBER_ORIGIN_DENIED', error: 'Same-origin request required.' });
      return;
    }

    const session = await dependencies.resolveUser(req);
    if (!session.user) {
      authFailure(res, session);
      return;
    }

    try {
      const conversations = await dependencies.listConversations({
        verifiedAuthUserId: session.user.id,
      });
      res.status(200).json({ ok: true, conversations });
    } catch (error) {
      if (error instanceof MemberChatStoreError) {
        res.status(error.publicStatus).json({
          ok: false,
          code: error.publicCode,
          error: error.message,
        });
        return;
      }
      const failure = memberChatPublicError(error);
      res.status(failure.status).json(failure.body);
    }
  };
}

export default createMemberChatConversationsHandler();
