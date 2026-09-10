import type { NextApiRequest, NextApiResponse } from 'next';

import { getRequestUser, type RequestUserResult } from '@/lib/admin-auth';
import {
  canonicalMemberChatCursor,
  hasMemberChatReadOrigin,
  isMemberChatUuid,
  listMemberChatHistory,
  MEMBER_CHAT_HISTORY_LIMIT,
  MEMBER_CHAT_HISTORY_WIRE_MAX_BYTES,
  MemberChatStoreError,
  memberChatPublicError,
  requireMemberChatConversationBinding,
  setMemberChatPrivateHeaders,
  type MemberChatHistoryPage,
} from '@/lib/apocrypha/member-chat';

export interface MemberChatHistoryDependencies {
  resolveUser(req: NextApiRequest): Promise<RequestUserResult>;
  listHistory(input: {
    verifiedAuthUserId: string;
    conversationId: string;
    beforeCursor?: string | null;
  }): Promise<MemberChatHistoryPage>;
}

const DEFAULT_DEPENDENCIES: MemberChatHistoryDependencies = {
  resolveUser: getRequestUser,
  listHistory: listMemberChatHistory,
};

function authFailure(res: NextApiResponse, result: RequestUserResult): void {
  const unavailable = result.failureKind === 'upstream-unavailable'
    || result.failureKind === 'unconfigured';
  res.status(unavailable ? 503 : 401).json({
    ok: false,
    code: unavailable ? 'MEMBER_SESSION_UNAVAILABLE' : 'MEMBER_SESSION_REQUIRED',
    error: unavailable
      ? 'The sign-in service could not verify this member session.'
      : 'Sign in to read durable member chat history.',
  });
}

export function createMemberChatHistoryHandler(
  dependencies: MemberChatHistoryDependencies = DEFAULT_DEPENDENCIES,
) {
  return async function memberChatHistoryHandler(
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
    const queryKeys = Object.keys(req.query).sort();
    if (
      queryKeys.length < 1
      || queryKeys.length > 2
      || !queryKeys.includes('conversation_id')
      || queryKeys.some((key) => key !== 'conversation_id' && key !== 'before')
      || typeof req.query.conversation_id !== 'string'
      || (req.query.before !== undefined && typeof req.query.before !== 'string')
    ) {
      res.status(400).json({ ok: false, code: 'MEMBER_CHAT_HISTORY_QUERY_INVALID' });
      return;
    }
    const conversationId = req.query.conversation_id.toLowerCase();
    if (!isMemberChatUuid(conversationId)) {
      res.status(400).json({ ok: false, code: 'MEMBER_CHAT_CONVERSATION_ID_INVALID' });
      return;
    }
    const beforeCursor = req.query.before === undefined
      ? null
      : canonicalMemberChatCursor(req.query.before);
    if (req.query.before !== undefined && beforeCursor === null) {
      res.status(400).json({ ok: false, code: 'MEMBER_CHAT_HISTORY_CURSOR_INVALID' });
      return;
    }

    const session = await dependencies.resolveUser(req);
    if (!session.user) {
      authFailure(res, session);
      return;
    }

    try {
      // These are no longer the same value. They were, while a member had one
      // conversation whose id WAS their auth user id - so passing one for both
      // was harmless then and would be a privilege bug now.
      const verifiedAuthUserId = session.user.id.toLowerCase();
      const authoritativeConversationId = requireMemberChatConversationBinding(
        session.user.id,
        conversationId,
      );
      const page = await dependencies.listHistory({
        verifiedAuthUserId,
        conversationId: authoritativeConversationId,
        beforeCursor,
      });
      if (
        !Array.isArray(page.history)
        || page.history.length > MEMBER_CHAT_HISTORY_LIMIT
        || (page.nextCursor !== null && canonicalMemberChatCursor(page.nextCursor) !== page.nextCursor)
        || (beforeCursor !== null && page.nextCursor !== null
          && BigInt(page.nextCursor) >= BigInt(beforeCursor))
        || page.history.some(
          (entry) => entry.conversation_id.toLowerCase() !== authoritativeConversationId,
        )
      ) {
        throw new MemberChatStoreError(
          502,
          'MEMBER_CHAT_INVALID_PROJECTION',
          'The history projection escaped its verified member binding.',
        );
      }
      const body = {
        ok: true,
        conversation_id: authoritativeConversationId,
        history: page.history,
        count: page.history.length,
        next_cursor: page.nextCursor,
      } as const;
      if (Buffer.byteLength(JSON.stringify(body), 'utf8') > MEMBER_CHAT_HISTORY_WIRE_MAX_BYTES) {
        throw new MemberChatStoreError(
          502,
          'MEMBER_CHAT_INVALID_PROJECTION',
          'The serialized history response exceeded its byte bound.',
        );
      }
      res.status(200).json(body);
    } catch (error) {
      const failure = memberChatPublicError(error);
      res.status(failure.status).json(failure.body);
    }
  };
}

export default createMemberChatHistoryHandler();
