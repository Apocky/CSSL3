import type { NextApiRequest, NextApiResponse } from 'next';
import { timingSafeEqual } from 'node:crypto';
import { isOpaqueClientRequestId, isOpaqueConversationId, setPrivateNoStore } from '@/lib/apocrypha/proxy';
import { publicMemberPrincipalRef, RuntimeProxyError, submitOwnerBrainRuntimeChat } from '@/lib/apocv4/runtime-proxy';

const MAX_TEXT_BYTES = 16_384;
const ADMISSION_RETRY_LIMIT = 5;
const ADMISSION_RETRY_DELAY_MS = 1_500;

function sleep(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

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
  const keys = body && !Array.isArray(body) ? Object.keys(body).sort().join(',') : '';
  if (!body || Array.isArray(body) || (keys !== 'conversation_id,message,request_id' && keys !== 'conversation_id,message,model_routing,request_id')) {
    res.status(400).json({ error: 'body must contain exactly message, conversation_id, and request_id' }); return;
  }
  const message = typeof body.message === 'string' ? body.message.trim() : '';
  if (!message || Buffer.byteLength(message, 'utf8') > MAX_TEXT_BYTES || !isOpaqueConversationId(body.conversation_id) || !isOpaqueClientRequestId(body.request_id)) {
    res.status(400).json({ error: 'invalid bridge request' }); return;
  }
  const owner = process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID?.trim();
  if (!owner) { res.status(503).json({ error: 'Bridge owner is not configured' }); return; }
  try {
    const input = {
      message,
      conversationId: body.conversation_id,
      requestId: body.request_id,
      sessionPrincipal: publicMemberPrincipalRef(owner),
      privacyPartition: 'owner:apocky' as const,
      credentialProfile: 'owner' as const,
    };
    let projection;
    for (let attempt = 0; ; attempt += 1) {
      try {
        projection = await submitOwnerBrainRuntimeChat(
          input,
          req.headers['traceparent'] as string | undefined,
        );
        break;
      } catch (error) {
        if (!(error instanceof RuntimeProxyError)
          || error.code !== 'apex_admission_pending'
          || attempt >= ADMISSION_RETRY_LIMIT) {
          throw error;
        }
        await sleep(ADMISSION_RETRY_DELAY_MS);
      }
    }
    res.status(200).json({ text: projection.model_reported.text, conversation_id: body.conversation_id, request_id: body.request_id, provider: 'apocrypha' });
  } catch (error) {
    res.status(502).json({ error: 'Apocrypha runtime unavailable', code: error instanceof RuntimeProxyError ? error.code : 'runtime_upstream_error' });
  }
}
