import type { NextApiRequest, NextApiResponse } from 'next';

import type { BrainRuntimeTurn } from '@/lib/brain/contracts';
import { requireBrainOwner, respondBrainOwnerFailure, setBrainPrivateHeaders } from '@/lib/brain/owner';
import { ownerBrainRuntimeConfigured, sendOwnerBrainTurn } from '@/lib/brain/runtime-provider';
import { isOpaqueClientRequestId, isOpaqueConversationId } from '@/lib/apocrypha/proxy';
import { RuntimeProxyError } from '@/lib/apocv4/runtime-proxy';
import { hasSameOrigin } from '@/lib/auth-session';
import { envelope } from '@/lib/response';

const MAX_TEXT_BYTES = 16_384;

function isExactBody(value: unknown): value is { text: string; session_id: string; request_id: string } {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const body = value as Record<string, unknown>;
  const keys = Object.keys(body).sort();
  return keys.join(',') === 'request_id,session_id,text'
    && typeof body.text === 'string'
    && body.text === body.text.trim()
    && Buffer.byteLength(body.text, 'utf8') >= 1
    && Buffer.byteLength(body.text, 'utf8') <= MAX_TEXT_BYTES
    && isOpaqueConversationId(body.session_id)
    && isOpaqueClientRequestId(body.request_id);
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setBrainPrivateHeaders(res);
  res.setHeader('Allow', 'POST');
  if (req.method !== 'POST') {
    res.status(405).json({ error: 'Method not allowed', code: 'BRAIN_METHOD_NOT_ALLOWED', ...envelope() });
    return;
  }
  if (!hasSameOrigin(req)) {
    res.status(403).json({ error: 'Same-origin request required', code: 'BRAIN_ORIGIN_DENIED', ...envelope() });
    return;
  }
  const contentType = (Array.isArray(req.headers['content-type']) ? req.headers['content-type'][0] : req.headers['content-type'])
    ?.split(';', 1)[0]
    ?.trim()
    .toLowerCase();
  if (contentType !== 'application/json') {
    res.status(415).json({ error: 'Content-Type must be application/json', code: 'BRAIN_CONTENT_TYPE_REQUIRED', ...envelope() });
    return;
  }
  const owner = await requireBrainOwner(req);
  if (!owner.ok) {
    respondBrainOwnerFailure(res, owner);
    return;
  }
  if (!ownerBrainRuntimeConfigured()) {
    res.status(503).json({
      error: 'The free local Apocv4 provider is not connected. No message was sent.',
      code: 'BRAIN_LOCAL_PROVIDER_DISABLED',
      ...envelope(),
    });
    return;
  }
  if (!isExactBody(req.body)) {
    res.status(400).json({
      error: 'Body must contain exactly text, session_id, and request_id with opaque UUIDs.',
      code: 'BRAIN_TURN_INVALID',
      ...envelope(),
    });
    return;
  }

  try {
    const projection = await sendOwnerBrainTurn({
      userId: owner.user.id,
      text: req.body.text,
      sessionId: req.body.session_id.toLowerCase(),
      requestId: req.body.request_id.toLowerCase(),
    });
    const modelId = typeof projection.model_reported.model_id === 'string'
      ? projection.model_reported.model_id
      : 'unreported';
    const responseDigest = typeof projection.model_reported.response_digest === 'string'
      ? projection.model_reported.response_digest
      : '';
    const env = envelope();
    const body: BrainRuntimeTurn = {
      schema_version: 'apocky.owner-brain.turn.v1',
      status: 'completed',
      text: projection.model_reported.text,
      session_id: req.body.session_id.toLowerCase(),
      request_id: req.body.request_id.toLowerCase(),
      model_id: modelId,
      response_digest: responseDigest,
      memory: projection.context ? {
        status: projection.context.memory.status,
        records_used: projection.context.memory.records_used,
        refs: projection.context.memory.refs,
      } : null,
      served_by: env.served_by,
      ts: env.ts,
    };
    res.status(200).json(body);
  } catch (error) {
    const code = error instanceof RuntimeProxyError ? error.code : 'runtime_unreachable';
    const status = error instanceof RuntimeProxyError && error.publicStatus === 400
      ? 400
      : error instanceof RuntimeProxyError && error.publicStatus === 504
        ? 504
        : 502;
    res.status(status).json({
      error: 'The local Apocv4 provider did not return a verified turn. The message was not represented as completed.',
      code: `BRAIN_${code.toUpperCase()}`,
      ...envelope(),
    });
  }
}
