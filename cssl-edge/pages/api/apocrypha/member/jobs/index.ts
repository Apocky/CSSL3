import type { NextApiRequest, NextApiResponse } from 'next';

import { getRequestUser, type RequestUserResult } from '@/lib/admin-auth';
import { hasSameOrigin } from '@/lib/auth-session';
import {
  canonicalMemberChatMessage,
  enqueueMemberChat,
  isMemberChatUuid,
  MemberChatStoreError,
  memberChatPublicError,
  requireMemberChatConversationBinding,
  setMemberChatPrivateHeaders,
  type MemberChatJobReceipt,
} from '@/lib/apocrypha/member-chat';

interface MemberChatSubmitBody {
  conversation_id?: unknown;
  request_id?: unknown;
  message?: unknown;
}

export interface MemberChatSubmitDependencies {
  resolveUser(req: NextApiRequest): Promise<RequestUserResult>;
  enqueue(input: {
    verifiedAuthUserId: string;
    conversationId: string;
    requestId: string;
    message: string;
  }): Promise<MemberChatJobReceipt>;
}

const DEFAULT_DEPENDENCIES: MemberChatSubmitDependencies = {
  resolveUser: getRequestUser,
  enqueue: enqueueMemberChat,
};

export const config = {
  api: {
    bodyParser: {
      sizeLimit: '128kb',
    },
  },
};

function firstHeader(value: string | string[] | undefined): string | null {
  return (Array.isArray(value) ? value[0] : value)?.split(';')[0]?.trim().toLowerCase() || null;
}

function exactSubmitBody(value: unknown): value is Required<MemberChatSubmitBody> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return false;
  const keys = Object.keys(value).sort();
  return keys.length === 3
    && keys[0] === 'conversation_id'
    && keys[1] === 'message'
    && keys[2] === 'request_id';
}

function authFailure(res: NextApiResponse, result: RequestUserResult): void {
  const unavailable = result.failureKind === 'upstream-unavailable'
    || result.failureKind === 'unconfigured';
  res.status(unavailable ? 503 : 401).json({
    ok: false,
    code: unavailable ? 'MEMBER_SESSION_UNAVAILABLE' : 'MEMBER_SESSION_REQUIRED',
    error: unavailable
      ? 'The sign-in service could not verify this member session.'
      : 'Sign in to use durable member chat.',
  });
}

export function createMemberChatSubmitHandler(
  dependencies: MemberChatSubmitDependencies = DEFAULT_DEPENDENCIES,
) {
  return async function memberChatSubmitHandler(
    req: NextApiRequest,
    res: NextApiResponse,
  ): Promise<void> {
    setMemberChatPrivateHeaders(res);
    res.setHeader('Allow', 'POST');

    if (req.method !== 'POST') {
      res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' });
      return;
    }
    if (!hasSameOrigin(req)) {
      res.status(403).json({ ok: false, code: 'MEMBER_ORIGIN_DENIED', error: 'Same-origin request required.' });
      return;
    }
    if (firstHeader(req.headers['content-type']) !== 'application/json') {
      res.status(415).json({ ok: false, code: 'JSON_REQUIRED', error: 'Content-Type must be application/json.' });
      return;
    }
    if (!exactSubmitBody(req.body)) {
      res.status(400).json({
        ok: false,
        code: 'MEMBER_CHAT_BODY_INVALID',
        error: 'Body must contain only conversation_id, request_id, and message.',
      });
      return;
    }

    const conversationId = typeof req.body.conversation_id === 'string'
      ? req.body.conversation_id.toLowerCase()
      : '';
    const requestId = typeof req.body.request_id === 'string'
      ? req.body.request_id.toLowerCase()
      : '';
    const message = canonicalMemberChatMessage(req.body.message);
    if (!isMemberChatUuid(conversationId) || !isMemberChatUuid(requestId) || message === null) {
      res.status(400).json({
        ok: false,
        code: 'MEMBER_CHAT_INPUT_INVALID',
        error: 'Conversation, request, or message input is invalid.',
      });
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
      const job = await dependencies.enqueue({
        verifiedAuthUserId,
        conversationId: authoritativeConversationId,
        requestId,
        message,
      });
      if (
        job.conversation_id.toLowerCase() !== authoritativeConversationId
        || job.request_id.toLowerCase() !== requestId
      ) {
        throw new MemberChatStoreError(
          502,
          'MEMBER_CHAT_INVALID_PROJECTION',
          'The job receipt escaped its verified member binding.',
        );
      }
      res.status(job.replayed ? 200 : 202).json({
        ok: true,
        accepted: true,
        replayed: job.replayed,
        job,
      });
    } catch (error) {
      const failure = memberChatPublicError(error);
      if (failure.retryAfterSeconds !== undefined) {
        res.setHeader('Retry-After', String(failure.retryAfterSeconds));
      }
      res.status(failure.status).json(failure.body);
    }
  };
}

export default createMemberChatSubmitHandler();
