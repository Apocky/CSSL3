import type { NextApiRequest, NextApiResponse } from 'next';

import { getRequestUser, type RequestUserResult } from '@/lib/admin-auth';
import {
  canonicalMemberChatVerifiedIdentity,
  getMemberChatJob,
  hasMemberChatReadOrigin,
  isMemberChatUuid,
  MemberChatStoreError,
  memberChatPublicError,
  setMemberChatPrivateHeaders,
  type MemberChatHistoryEntry,
} from '@/lib/apocrypha/member-chat';

export interface MemberChatJobReadDependencies {
  resolveUser(req: NextApiRequest): Promise<RequestUserResult>;
  getJob(input: {
    verifiedAuthUserId: string;
    jobId: string;
  }): Promise<MemberChatHistoryEntry | null>;
}

const DEFAULT_DEPENDENCIES: MemberChatJobReadDependencies = {
  resolveUser: getRequestUser,
  getJob: getMemberChatJob,
};

function authFailure(res: NextApiResponse, result: RequestUserResult): void {
  const unavailable = result.failureKind === 'upstream-unavailable'
    || result.failureKind === 'unconfigured';
  res.status(unavailable ? 503 : 401).json({
    ok: false,
    code: unavailable ? 'MEMBER_SESSION_UNAVAILABLE' : 'MEMBER_SESSION_REQUIRED',
    error: unavailable
      ? 'The sign-in service could not verify this member session.'
      : 'Sign in to read durable member chat.',
  });
}

export function createMemberChatJobReadHandler(
  dependencies: MemberChatJobReadDependencies = DEFAULT_DEPENDENCIES,
) {
  return async function memberChatJobReadHandler(
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
    if (Object.keys(req.query).length !== 1 || typeof req.query.id !== 'string') {
      res.status(400).json({ ok: false, code: 'MEMBER_CHAT_JOB_QUERY_INVALID' });
      return;
    }
    const jobId = req.query.id.toLowerCase();
    if (!isMemberChatUuid(jobId)) {
      res.status(400).json({ ok: false, code: 'MEMBER_CHAT_JOB_ID_INVALID' });
      return;
    }

    const session = await dependencies.resolveUser(req);
    if (!session.user) {
      authFailure(res, session);
      return;
    }

    try {
      const verifiedAuthUserId = canonicalMemberChatVerifiedIdentity(session.user.id);
      const job = await dependencies.getJob({
        verifiedAuthUserId,
        jobId,
      });
      if (!job) {
        // Foreign and absent jobs are deliberately indistinguishable.
        res.status(404).json({ ok: false, code: 'MEMBER_CHAT_JOB_NOT_FOUND' });
        return;
      }
      if (
        job.job_id.toLowerCase() !== jobId
        || job.conversation_id.toLowerCase() !== verifiedAuthUserId
      ) {
        throw new MemberChatStoreError(
          502,
          'MEMBER_CHAT_INVALID_PROJECTION',
          'The job projection escaped its verified member binding.',
        );
      }
      res.status(200).json({ ok: true, job });
    } catch (error) {
      const failure = memberChatPublicError(error);
      res.status(failure.status).json(failure.body);
    }
  };
}

export default createMemberChatJobReadHandler();
