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
import { randomUUID } from 'node:crypto';
import {
  attachmentTextFrom,
  MEMBER_CHAT_ATTACHMENT_BUCKET,
  MEMBER_CHAT_ATTACHMENT_MAX_BYTES,
  registerMemberAttachment,
  type MemberChatAttachment,
} from '@/lib/apocrypha/member-chat';
import { getApocryphaServiceClient } from '@/lib/apocrypha/job-control';

// POST /api/apocrypha/member/attachments {thread_id?, file_name, mime_type, data_base64}
// The plus icon. Bytes go to the private bucket; the row keeps the text the model may read.
export const config = { api: { bodyParser: { sizeLimit: '36mb' } } };

const MIME_RE = /^[a-z0-9!#$&^_.+-]{1,64}\/[a-z0-9!#$&^_.+-]{1,120}$/i;

export interface MemberAttachmentDependencies {
  resolveUser(req: NextApiRequest): Promise<RequestUserResult>;
  store(input: { verifiedAuthUserId: string; storagePath: string; mimeType: string; bytes: Buffer }): Promise<void>;
  register(input: { verifiedAuthUserId: string; threadId: string | null; fileName: string; mimeType: string; byteSize: number; storagePath: string; extractedText: string | null }): Promise<MemberChatAttachment>;
}

async function storeInBucket(input: { verifiedAuthUserId: string; storagePath: string; mimeType: string; bytes: Buffer }): Promise<void> {
  const client = getApocryphaServiceClient();
  const { error } = await client.storage.from(MEMBER_CHAT_ATTACHMENT_BUCKET).upload(input.storagePath, input.bytes, { contentType: input.mimeType, upsert: false });
  if (error) throw new Error(`attachment storage: ${error.message}`);
}

const DEFAULT_DEPENDENCIES: MemberAttachmentDependencies = { resolveUser: getRequestUser, store: storeInBucket, register: registerMemberAttachment };

export function createMemberAttachmentHandler(dependencies: MemberAttachmentDependencies = DEFAULT_DEPENDENCIES) {
  return async function memberAttachmentHandler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
    setMemberChatPrivateHeaders(res);
    res.setHeader('Allow', 'POST');
    if (req.method !== 'POST') { res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' }); return; }
    if (!hasSameOrigin(req)) { res.status(403).json({ ok: false, code: 'MEMBER_ORIGIN_DENIED', error: 'Same-origin request required.' }); return; }
    const body = req.body as { thread_id?: unknown; file_name?: unknown; mime_type?: unknown; data_base64?: unknown } | null;
    if (!body || typeof body !== 'object' || Array.isArray(body)
      || Object.keys(body).some((key) => !['thread_id', 'file_name', 'mime_type', 'data_base64'].includes(key))
      || typeof body.file_name !== 'string' || body.file_name.trim().length === 0 || body.file_name.length > 255
      || typeof body.mime_type !== 'string' || !MIME_RE.test(body.mime_type)
      || typeof body.data_base64 !== 'string' || body.data_base64.length === 0
      || (body.thread_id !== undefined && body.thread_id !== null && (typeof body.thread_id !== 'string' || !isMemberChatUuid(body.thread_id.toLowerCase())))) {
      res.status(400).json({ ok: false, code: 'MEMBER_ATTACHMENT_BODY_INVALID', error: 'Body must be {thread_id?, file_name, mime_type, data_base64}.' });
      return;
    }
    let bytes: Buffer;
    try { bytes = Buffer.from(body.data_base64, 'base64'); } catch { bytes = Buffer.alloc(0); }
    if (bytes.length === 0 || bytes.length > MEMBER_CHAT_ATTACHMENT_MAX_BYTES) {
      res.status(413).json({ ok: false, code: 'MEMBER_ATTACHMENT_TOO_LARGE', error: 'Attachments are 1 byte to 25 MB.' });
      return;
    }
    const session = await dependencies.resolveUser(req);
    if (!session.user) { authFailure(res, session); return; }
    const fileName = body.file_name.trim().replace(/[\u0000-\u001f\u007f]/g, '');
    const mimeType = body.mime_type.toLowerCase();
    const threadId = typeof body.thread_id === 'string' ? body.thread_id.toLowerCase() : null;
    const storagePath = `${session.user.id}/${threadId ?? 'unfiled'}/${randomUUID()}`;
    try {
      await dependencies.store({ verifiedAuthUserId: session.user.id, storagePath, mimeType, bytes });
      const attachment = await dependencies.register({
        verifiedAuthUserId: session.user.id, threadId, fileName, mimeType, byteSize: bytes.length, storagePath,
        extractedText: attachmentTextFrom(mimeType, bytes),
      });
      res.status(201).json({ ok: true, attachment });
    } catch (error) {
      const failure = memberChatPublicError(error);
      res.status(failure.status).json(failure.body);
    }
  };
}

export default createMemberAttachmentHandler();
