import type { NextApiRequest, NextApiResponse } from 'next';

import { getAdminAllowlist, getRequestUser } from '@/lib/admin-auth';
import {
  isOpaqueClientRequestId,
  isOpaqueConversationId,
  scopeRequestId,
  setPrivateNoStore,
} from '@/lib/apocrypha/proxy';
import {
  publicMemberPrincipalRef,
  publicRuntimeError,
  RuntimeProxyError,
  submitRuntimeVision,
} from '@/lib/apocv4/runtime-proxy';
import { hasSameOrigin } from '@/lib/auth-session';
import { envelope } from '@/lib/response';
import { createServerTrace, traceparentFor } from '@/lib/telemetry/server';

const OWNER_RUNTIME_PRIVACY_PARTITION = 'owner:apocky';
const MAX_IMAGE_B64 = 5_600_000;
const BASE64_RE = /^[A-Za-z0-9+/]+={0,2}$/;
const MIME_TYPES = new Set(['image/jpeg', 'image/png', 'image/webp']);

interface VisionBody {
  session_id?: unknown;
  request_id?: unknown;
  image_b64?: unknown;
  mime_type?: unknown;
  question?: unknown;
}

export const config = {
  api: {
    bodyParser: {
      sizeLimit: '6mb',
    },
  },
};

function firstHeader(value: string | string[] | undefined): string | null {
  return (Array.isArray(value) ? value[0] : value)?.split(';')[0]?.trim().toLowerCase() || null;
}

function exactObject(value: unknown, expectedKeys: readonly string[]): value is Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return false;
  const keys = Object.keys(value).sort();
  const expected = [...expectedKeys].sort();
  return keys.length === expected.length && keys.every((key, index) => key === expected[index]);
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse,
): Promise<void> {
  const trace = createServerTrace(req);
  setPrivateNoStore(res);
  res.setHeader('X-Apocky-Trace-Id', trace.traceId);
  res.setHeader('Traceparent', traceparentFor(trace));
  res.setHeader('Allow', 'POST');
  if (req.method !== 'POST') {
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    return;
  }
  if (!hasSameOrigin(req)) {
    res.status(403).json({ error: 'Same-origin request required', ...envelope() });
    return;
  }
  if (firstHeader(req.headers['content-type']) !== 'application/json') {
    res.status(415).json({ error: 'Content-Type must be application/json', ...envelope() });
    return;
  }
  const member = await getRequestUser(req);
  if (!member.user) {
    const unavailable = member.failureKind === 'upstream-unavailable'
      || member.failureKind === 'unconfigured';
    res.status(unavailable ? 503 : 401).json({
      error: unavailable ? 'Sign-in verification is temporarily unavailable.' : 'Sign in to use vision.',
      authenticated: false,
      ...envelope(),
    });
    return;
  }
  if (!getAdminAllowlist().includes(member.user.email.toLowerCase())) {
    res.status(403).json({
      error: 'Vision is restricted to the owner partition until per-member visual isolation is deployed.',
      ...envelope(),
    });
    return;
  }
  const body = req.body as VisionBody;
  if (!exactObject(body, ['session_id', 'request_id', 'image_b64', 'mime_type', 'question'])) {
    res.status(400).json({ error: 'vision body must contain the exact required fields', ...envelope() });
    return;
  }
  if (
    !isOpaqueConversationId(body.session_id)
    || !isOpaqueClientRequestId(body.request_id)
    || typeof body.image_b64 !== 'string'
    || body.image_b64.length < 1
    || body.image_b64.length > MAX_IMAGE_B64
    || !BASE64_RE.test(body.image_b64)
    || typeof body.mime_type !== 'string'
    || !MIME_TYPES.has(body.mime_type)
    || typeof body.question !== 'string'
    || !body.question.trim()
    || body.question !== body.question.trim()
    || Buffer.byteLength(body.question, 'utf8') > 32_768
  ) {
    res.status(400).json({ error: 'vision request is malformed or exceeds its bounded media contract', ...envelope() });
    return;
  }

  const principalRef = publicMemberPrincipalRef(member.user.id);
  const sessionId = body.session_id.toLowerCase();
  const perceptId = scopeRequestId(principalRef, body.request_id.toLowerCase());
  try {
    const projection = await submitRuntimeVision({
      imageB64: body.image_b64,
      mimeType: body.mime_type as 'image/jpeg' | 'image/png' | 'image/webp',
      observedAt: new Date().toISOString(),
      perceptId,
      provenanceRef: `apocky.com:owner-vision:${principalRef}:${sessionId}`,
      question: body.question,
      privacyPartition: OWNER_RUNTIME_PRIVACY_PARTITION,
      credentialProfile: 'owner',
    }, traceparentFor(trace));
    res.status(200).json({
      observation: projection.observed.observation,
      perception_digest: projection.observed.perception_digest,
      upstream_status: projection.observed.receipt.upstream_status,
      ...envelope(),
    });
  } catch (error) {
    const status = error instanceof RuntimeProxyError ? error.publicStatus : 502;
    res.status(status).json({ ...publicRuntimeError(error), ...envelope() });
  }
}
