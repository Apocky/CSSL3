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
  MEMBER_CHAT_ENGINE_LANES,
  type MemberChatEngineLane,
  type MemberChatJobReceipt,
} from '@/lib/apocrypha/member-chat';

interface MemberChatSubmitBody {
  conversation_id?: unknown;
  request_id?: unknown;
  message?: unknown;
  thread_id?: unknown;
  engine_lane?: unknown;
  attachment_ids?: unknown;
}

export interface MemberChatSubmitDependencies {
  resolveUser(req: NextApiRequest): Promise<RequestUserResult>;
  enqueue(input: {
    verifiedAuthUserId: string;
    conversationId: string;
    requestId: string;
    message: string;
    threadId?: string | null;
    engineLane?: MemberChatEngineLane;
    attachmentIds?: string[];
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

const REQUIRED_SUBMIT_KEYS = ['conversation_id', 'message', 'request_id'] as const;
const OPTIONAL_SUBMIT_KEYS = new Set(['thread_id', 'engine_lane', 'attachment_ids']);

// The three original keys are required; the 0057 keys (thread, lane, attachments) are optional;
// anything else is refused, as before.
function exactSubmitBody(value: unknown): value is MemberChatSubmitBody & Required<Pick<MemberChatSubmitBody, 'conversation_id' | 'message' | 'request_id'>> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return false;
  const keys = Object.keys(value);
  if (!REQUIRED_SUBMIT_KEYS.every((key) => keys.includes(key))) return false;
  return keys.every((key) => (REQUIRED_SUBMIT_KEYS as readonly string[]).includes(key) || OPTIONAL_SUBMIT_KEYS.has(key));
}

function optionalSubmitFields(body: MemberChatSubmitBody): { threadId: string | null; engineLane: MemberChatEngineLane; attachmentIds: string[] } | null {
  const threadId = body.thread_id === undefined || body.thread_id === null
    ? null
    : typeof body.thread_id === 'string' && isMemberChatUuid(body.thread_id.toLowerCase()) ? body.thread_id.toLowerCase() : undefined;
  if (threadId === undefined) return null;
  const engineLane = body.engine_lane === undefined ? 'local' : typeof body.engine_lane === 'string' && MEMBER_CHAT_ENGINE_LANES.has(body.engine_lane) ? body.engine_lane as MemberChatEngineLane : undefined;
  if (engineLane === undefined) return null;
  const rawIds = body.attachment_ids === undefined ? [] : body.attachment_ids;
  if (!Array.isArray(rawIds) || rawIds.length > 8 || rawIds.some((id) => typeof id !== 'string' || !isMemberChatUuid(id.toLowerCase()))) return null;
  return { threadId, engineLane, attachmentIds: (rawIds as string[]).map((id) => id.toLowerCase()) };
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
        error: 'Body must contain conversation_id, request_id, and message, plus optional thread_id, engine_lane, attachment_ids.',
      });
      return;
    }
    const optional = optionalSubmitFields(req.body);
    if (optional === null) {
      res.status(400).json({ ok: false, code: 'MEMBER_CHAT_INPUT_INVALID', error: 'Thread, lane, or attachment input is invalid.' });
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
      const authoritativeConversationId = requireMemberChatConversationBinding(
        session.user.id,
        conversationId,
      );
      const job = await dependencies.enqueue({
        verifiedAuthUserId: authoritativeConversationId,
        conversationId: authoritativeConversationId,
        requestId,
        message,
        threadId: optional.threadId,
        engineLane: optional.engineLane,
        attachmentIds: optional.attachmentIds,
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
