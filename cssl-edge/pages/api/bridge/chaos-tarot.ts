import type { NextApiRequest, NextApiResponse } from 'next';
import { timingSafeEqual } from 'node:crypto';
import { isOpaqueClientRequestId, isOpaqueConversationId, setPrivateNoStore } from '@/lib/apocrypha/proxy';
import { publicMemberPrincipalRef, RuntimeProxyError, submitOwnerBrainRuntimeChat } from '@/lib/apocv4/runtime-proxy';

const MAX_TEXT_BYTES = 16_384;

function bearer(req: NextApiRequest): string | null {
  const value = req.headers.authorization;
  return typeof value === 'string' && value.startsWith('Bearer ') ? value.slice(7).trim() : null;
}

function authorized(req: NextApiRequest): boolean {
  const supplied = bearer(req);
  const expected = process.env.CHAOS_TAROT_BRIDGE_TOKEN?.trim() ?? '';
  if (!supplied || !expected) return false;
  const a = Buffer.from(supplied);
  const b = Buffer.from(expected);
  return a.length === b.length && timingSafeEqual(a, b);
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setPrivateNoStore(res);
  res.setHeader('Allow', 'POST');
  if (req.method !== 'POST') { res.status(405).json({ error: 'Method not allowed' }); return; }
  if (!authorized(req)) { res.status(401).json({ error: 'Bridge authentication required' }); return; }
  const body = req.body as Record<string, unknown>;
  if (!body || Array.isArray(body) || Object.keys(body).sort().join(',') !== 'conversation_id,message,request_id') {
    res.status(400).json({ error: 'body must contain exactly message, conversation_id, and request_id' }); return;
  }
  const message = typeof body.message === 'string' ? body.message.trim() : '';
  if (!message || Buffer.byteLength(message, 'utf8') > MAX_TEXT_BYTES || !isOpaqueConversationId(body.conversation_id) || !isOpaqueClientRequestId(body.request_id)) {
    res.status(400).json({ error: 'invalid bridge request' }); return;
  }
  const owner = process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID?.trim();
  if (!owner) { res.status(503).json({ error: 'Bridge owner is not configured' }); return; }
  try {
    const projection = await submitOwnerBrainRuntimeChat({
      message,
      conversationId: body.conversation_id,
      requestId: body.request_id,
      sessionPrincipal: publicMemberPrincipalRef(owner),
      privacyPartition: 'owner:apocky',
      credentialProfile: 'owner',
    }, req.headers['traceparent'] as string | undefined);
    res.status(200).json({ text: projection.model_reported.text, conversation_id: body.conversation_id, request_id: body.request_id, provider: 'apocrypha' });
  } catch (error) {
    res.status(502).json({ error: 'Apocrypha runtime unavailable', code: error instanceof RuntimeProxyError ? error.code : 'runtime_upstream_error' });
  }
}
