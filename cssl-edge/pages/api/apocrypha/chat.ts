// Signed-in public conversation boundary for the native Apocrypha V2 body.
//
// This route deliberately does not reuse the owner authorization surface.
// A verified site member receives a server-owned pseudonymous principal, and
// every client conversation/request UUID is deterministically scoped to that
// principal before it reaches the private tunnel.

import { createHash } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';

import { getRequestUser } from '@/lib/admin-auth';
import {
  expectedConversationRef,
  fetchApocryphaV2,
  isOpaqueClientRequestId,
  isOpaqueConversationId,
  scopeConversationId,
  scopeRequestId,
  setPrivateNoStore,
} from '@/lib/apocrypha/proxy';
import { hasSameOrigin } from '@/lib/auth-session';
import { envelope } from '@/lib/response';

const UPSTREAM_DEADLINE_MS = 25_000;
const MAX_TEXT_BYTES = 16_384;
const TURN_SOURCE_REF = 'public:apocky.com/apocrypha';
const EXPECTED_EXPRESSION_MODE = 'bootstrap_shallow';
const EXPECTED_RESPONSE_SCHEMA = 'apocrypha.v2.turn-response.v1';
const EXPECTED_EFFECT_AUTHORITY = 'deny_all_O10_membrane';
const RATE_WINDOW_MS = 60_000;
const RATE_WINDOW_TURNS = 8;
const MAX_RATE_BUCKETS = 10_000;

interface TurnBody {
  text?: unknown;
  conversation_id?: unknown;
  request_id?: unknown;
}

interface RateBucket {
  count: number;
  resetAt: number;
}

const rateBuckets = new Map<string, RateBucket>();

export const config = {
  api: {
    bodyParser: {
      sizeLimit: '24kb',
    },
  },
};

function firstHeader(value: string | string[] | undefined): string | null {
  return (Array.isArray(value) ? value[0] : value)?.split(';')[0]?.trim().toLowerCase() || null;
}

function boundedText(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  const text = value.trim();
  if (!text || Buffer.byteLength(text, 'utf8') > MAX_TEXT_BYTES) return null;
  return text;
}

function stringField(body: Record<string, unknown>, key: string): string | null {
  const value = body[key];
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function bodyRecord(value: unknown): TurnBody | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as TurnBody
    : null;
}

export function publicMemberPrincipalRef(userId: string): string {
  const digest = createHash('sha256')
    .update('APOCRYPHA-V2-PUBLIC-MEMBER-PRINCIPAL-v1\0', 'utf8')
    .update(userId, 'utf8')
    .digest('hex');
  return `principal:apocky-member:${digest}`;
}

function rateDecision(
  principalRef: string,
  now = Date.now(),
): { allowed: boolean; retryAfterSeconds: number } {
  const current = rateBuckets.get(principalRef);
  if (!current || current.resetAt <= now) {
    if (rateBuckets.size >= MAX_RATE_BUCKETS) {
      for (const [key, bucket] of rateBuckets) {
        if (bucket.resetAt <= now) rateBuckets.delete(key);
      }
      if (rateBuckets.size >= MAX_RATE_BUCKETS) {
        const oldest = rateBuckets.keys().next().value as string | undefined;
        if (oldest) rateBuckets.delete(oldest);
      }
    }
    rateBuckets.set(principalRef, { count: 1, resetAt: now + RATE_WINDOW_MS });
    return { allowed: true, retryAfterSeconds: 0 };
  }
  if (current.count >= RATE_WINDOW_TURNS) {
    return {
      allowed: false,
      retryAfterSeconds: Math.max(1, Math.ceil((current.resetAt - now) / 1000)),
    };
  }
  current.count += 1;
  return { allowed: true, retryAfterSeconds: 0 };
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse,
): Promise<void> {
  setPrivateNoStore(res);
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

  const session = await getRequestUser(req);
  if (!session.user) {
    const unavailable = session.failureKind === 'upstream-unavailable'
      || session.failureKind === 'unconfigured';
    res.status(unavailable ? 503 : 401).json({
      error: unavailable
        ? 'Sign-in verification is temporarily unavailable.'
        : 'Sign in to speak with Apocrypha.',
      authenticated: false,
      ...envelope(),
    });
    return;
  }

  const body = bodyRecord(req.body);
  const text = boundedText(body?.text);
  if (!text) {
    res.status(400).json({
      error: `text must contain 1-${MAX_TEXT_BYTES} UTF-8 bytes after trimming`,
      ...envelope(),
    });
    return;
  }
  if (!isOpaqueConversationId(body?.conversation_id)) {
    res.status(400).json({
      error: 'conversation_id must be an opaque UUIDv4 minted by this client',
      ...envelope(),
    });
    return;
  }
  if (!isOpaqueClientRequestId(body?.request_id)) {
    res.status(400).json({
      error: 'request_id must be an opaque UUIDv4 minted once for this client turn',
      ...envelope(),
    });
    return;
  }

  const principalRef = publicMemberPrincipalRef(session.user.id);
  const budget = rateDecision(principalRef);
  if (!budget.allowed) {
    res.setHeader('Retry-After', String(budget.retryAfterSeconds));
    res.status(429).json({
      error: 'This conversation reached its short turn budget. Try again shortly.',
      retry_after_seconds: budget.retryAfterSeconds,
      ...envelope(),
    });
    return;
  }

  const clientConversationId = body.conversation_id.toLowerCase();
  const clientRequestId = body.request_id.toLowerCase();
  const scopedConversationId = scopeConversationId(principalRef, clientConversationId);
  const scopedRequestId = scopeRequestId(principalRef, clientRequestId);
  const requiredConversationRef = expectedConversationRef(
    scopedConversationId,
    TURN_SOURCE_REF,
  );
  const upstream = await fetchApocryphaV2({
    method: 'POST',
    upstreamPath: '/v2/turn',
    deadlineMs: UPSTREAM_DEADLINE_MS,
    body: {
      text,
      request_id: scopedRequestId,
      idempotency_key: scopedRequestId,
      conversation_id: scopedConversationId,
      source_ref: TURN_SOURCE_REF,
      authority_ref: `authority:authenticated-member:${principalRef}`,
      consent_ref: `consent:single-public-turn:${principalRef}`,
      privacy_class: 'restricted',
      modality: 'text',
      memory_scope: 'ephemeral',
    },
  });

  if (
    !upstream.ok
    || !upstream.payload
    || typeof upstream.payload !== 'object'
    || Array.isArray(upstream.payload)
  ) {
    res.status(upstream.status).json({
      error: 'Apocrypha could not complete this native V2 turn.',
      upstream_status: upstream.status,
      conversation_id: clientConversationId,
      request_id: clientRequestId,
      duplicate_commit_protection: 'active',
      ...envelope(),
    });
    return;
  }

  const payload = upstream.payload as Record<string, unknown>;
  const responseText = stringField(payload, 'text');
  const upstreamRequestId = stringField(payload, 'request_id');
  const conversationRef = stringField(payload, 'conversation_ref');
  const transitionId = stringField(payload, 'transition_id');
  const stateRoot = stringField(payload, 'state_root');
  const expressionMode = stringField(payload, 'expression_mode');
  const effectAuthority = stringField(payload, 'effect_authority');
  const outcome = stringField(payload, 'outcome');
  const responseSchema = stringField(payload, 'schema');
  const committedEnvelope = Boolean(
    responseText
      && transitionId
      && stateRoot
      && upstreamRequestId === scopedRequestId
      && conversationRef === requiredConversationRef
      && expressionMode === EXPECTED_EXPRESSION_MODE
      && effectAuthority === EXPECTED_EFFECT_AUTHORITY
      && responseSchema === EXPECTED_RESPONSE_SCHEMA
      && outcome === 'committed'
      && payload.external_inference === false,
  );
  if (!committedEnvelope) {
    res.status(502).json({
      error: 'The V2 body did not return the exact committed public-turn envelope.',
      conversation_id: clientConversationId,
      request_id: clientRequestId,
      duplicate_commit_protection: 'active',
      ...envelope(),
    });
    return;
  }

  res.status(200).json({
    text: responseText,
    conversation_id: clientConversationId,
    request_id: clientRequestId,
    transition_id: transitionId,
    state_root: stateRoot,
    expression_mode: expressionMode,
    external_inference: false,
    effect_authority: effectAuthority,
    outcome,
    memory_scope: 'ephemeral',
    conversation_history: 'not_retained_by_public_interface',
    training_consent: false,
    duplicate_commit_protection: 'active',
    upstream_status: upstream.status,
    ...envelope(),
  });
}
