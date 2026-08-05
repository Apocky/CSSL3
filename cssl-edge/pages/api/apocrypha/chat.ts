// Signed-in public conversation boundary for the Apocv4 runtime.
//
// This route deliberately does not reuse the owner authorization surface.
// A verified site member receives a server-owned pseudonymous principal, and
// every client conversation/request UUID is deterministically scoped to that
// principal before it reaches the direct private runtime transport.

import type { NextApiRequest, NextApiResponse } from 'next';

import { getAdminAllowlist, getRequestUser } from '@/lib/admin-auth';
import {
  isOpaqueClientRequestId,
  isOpaqueConversationId,
  scopeConversationId,
  scopeRequestId,
  setPrivateNoStore,
} from '@/lib/apocrypha/proxy';
import {
  publicMemberPrincipalRef,
  publicRuntimeError,
  RuntimeProxyError,
  streamRuntimeChat,
  submitRuntimeChat,
} from '@/lib/apocv4/runtime-proxy';
import { hasSameOrigin } from '@/lib/auth-session';
import { envelope } from '@/lib/response';
import { createServerTrace, emitOperationalTelemetry, traceparentFor } from '@/lib/telemetry/server';

const MAX_TEXT_BYTES = 16_384;
const OWNER_RUNTIME_PRIVACY_PARTITION = 'owner:apocky';
const PUBLIC_RUNTIME_PRIVACY_PARTITION = 'public:apocrypha';
const RATE_WINDOW_MS = 60_000;
const RATE_WINDOW_TURNS = 8;
const MAX_RATE_BUCKETS = 10_000;
const PUBLIC_CHAT_STREAM_SCHEMA = 'apocky.apocrypha-chat-stream.v1';

interface TurnBody {
  text?: unknown;
  session_id?: unknown;
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

function exactObject(value: unknown, expectedKeys: readonly string[]): value is Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return false;
  const keys = Object.keys(value).sort();
  const expected = [...expectedKeys].sort();
  return keys.length === expected.length && keys.every((key, index) => key === expected[index]);
}

function readClientSessionId(body: TurnBody | null): unknown {
  if (body === null) return undefined;
  const canonical = exactObject(body, ['text', 'session_id', 'request_id']);
  const legacy = exactObject(body, ['text', 'conversation_id', 'request_id']);
  if (canonical === legacy) return undefined;
  return canonical ? body.session_id : body.conversation_id;
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
  const trace = createServerTrace(req);
  const started = performance.now();
  setPrivateNoStore(res);
  res.setHeader('X-Apocky-Trace-Id', trace.traceId);
  res.setHeader('Traceparent', traceparentFor(trace));
  res.setHeader('Allow', 'POST');
  if (req.method !== 'POST') {
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocrypha.turn.denied', source: 'pages.api.apocrypha.chat', plane: 'security',
      severity: 'warn', outcome: 'denied', status: 405, durationMs: Math.round(performance.now() - started),
      message: 'Public turn method denied.', authority: 'authenticated-member-required',
    });
    return;
  }
  if (!hasSameOrigin(req)) {
    res.status(403).json({ error: 'Same-origin request required', ...envelope() });
    await emitOperationalTelemetry({
      trace, kind: 'security.apocrypha.turn.origin_denied', source: 'pages.api.apocrypha.chat', plane: 'security',
      severity: 'warn', outcome: 'denied', status: 403, durationMs: Math.round(performance.now() - started),
      message: 'Public turn origin denied.', authority: 'same-origin',
    });
    return;
  }
  if (firstHeader(req.headers['content-type']) !== 'application/json') {
    res.status(415).json({ error: 'Content-Type must be application/json', ...envelope() });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocrypha.turn.rejected', source: 'pages.api.apocrypha.chat', plane: 'edge',
      severity: 'warn', outcome: 'denied', status: 415, durationMs: Math.round(performance.now() - started),
      message: 'Public turn content type rejected.', authority: 'same-origin',
    });
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
    await emitOperationalTelemetry({
      trace, kind: 'security.apocrypha.turn.session_denied', source: 'pages.api.apocrypha.chat', plane: 'security',
      severity: unavailable ? 'error' : 'warn', outcome: unavailable ? 'degraded' : 'denied',
      status: unavailable ? 503 : 401, durationMs: Math.round(performance.now() - started),
      message: unavailable ? 'Member session verification unavailable.' : 'Member session required.',
      authority: 'authenticated-member-required',
      attributes: { failure_kind: session.failureKind ?? 'unauthenticated' },
    });
    return;
  }

  const body = bodyRecord(req.body);
  const rawSessionId = readClientSessionId(body);
  if (rawSessionId === undefined) {
    res.status(400).json({
      error: 'body must contain exactly text, session_id, and request_id',
      ...envelope(),
    });
    return;
  }
  const text = boundedText(body?.text);
  if (!text) {
    res.status(400).json({
      error: `text must contain 1-${MAX_TEXT_BYTES} UTF-8 bytes after trimming`,
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocrypha.turn.rejected', source: 'pages.api.apocrypha.chat', plane: 'edge',
      severity: 'warn', outcome: 'denied', status: 400, durationMs: Math.round(performance.now() - started),
      message: 'Public turn text envelope rejected.', authority: 'authenticated-member',
    });
    return;
  }
  if (!isOpaqueConversationId(rawSessionId)) {
    res.status(400).json({
      error: 'session_id must be an opaque UUIDv4 minted by this client',
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocrypha.turn.rejected', source: 'pages.api.apocrypha.chat', plane: 'edge',
      severity: 'warn', outcome: 'denied', status: 400, durationMs: Math.round(performance.now() - started),
      message: 'Conversation identifier rejected.', authority: 'authenticated-member',
    });
    return;
  }
  if (!isOpaqueClientRequestId(body?.request_id)) {
    res.status(400).json({
      error: 'request_id must be an opaque UUIDv4 minted once for this client turn',
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocrypha.turn.rejected', source: 'pages.api.apocrypha.chat', plane: 'edge',
      severity: 'warn', outcome: 'denied', status: 400, durationMs: Math.round(performance.now() - started),
      message: 'Request identifier rejected.', authority: 'authenticated-member',
    });
    return;
  }

  const principalRef = publicMemberPrincipalRef(session.user.id);
  const ownerProfile = getAdminAllowlist().includes(session.user.email.toLowerCase());
  const runtimePrivacyPartition = ownerProfile
    ? OWNER_RUNTIME_PRIVACY_PARTITION
    : PUBLIC_RUNTIME_PRIVACY_PARTITION;
  const credentialProfile = ownerProfile ? 'owner' : 'public';
  const budget = rateDecision(principalRef);
  if (!budget.allowed) {
    res.setHeader('Retry-After', String(budget.retryAfterSeconds));
    res.status(429).json({
      error: 'This conversation reached its short turn budget. Try again shortly.',
      retry_after_seconds: budget.retryAfterSeconds,
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'security.apocrypha.turn.rate_denied', source: 'pages.api.apocrypha.chat', plane: 'security',
      severity: 'warn', outcome: 'denied', status: 429, durationMs: Math.round(performance.now() - started),
      message: 'Public turn rate budget exhausted.', authority: 'authenticated-member',
      attributes: { retry_after_seconds: budget.retryAfterSeconds },
    });
    return;
  }

  const clientSessionId = rawSessionId.toLowerCase();
  const clientRequestId = body.request_id.toLowerCase();
  const scopedConversationId = scopeConversationId(principalRef, clientSessionId);
  const scopedRequestId = scopeRequestId(principalRef, clientRequestId);
  const streaming = firstHeader(req.headers.accept) === 'application/x-ndjson';
  await emitOperationalTelemetry({
    trace, kind: 'inference.apocrypha.turn.started', source: 'apocv4-runtime-proxy', plane: 'runtime',
    severity: 'info', outcome: 'started', status: null, durationMs: Math.round(performance.now() - started),
    message: 'Restricted public turn admitted for response-only runtime dispatch.',
    effectClass: 'apocrypha.public.turn.no-effects', authority: 'authenticated-member',
    attributes: {
      text_bytes: Buffer.byteLength(text, 'utf8'),
      privacy_class: 'restricted',
      memory_scope: ownerProfile ? 'owner_partitioned_retrieval' : 'public_safe_retrieval',
      training_consent: false,
    },
  });
  let projection;
  try {
    const runtimeInput = {
      message: text,
      conversationId: scopedConversationId,
      requestId: scopedRequestId,
      sessionId: clientSessionId,
      sessionPrincipal: principalRef,
      privacyPartition: runtimePrivacyPartition,
      credentialProfile,
    } as const;
    if (streaming) {
      res.status(200);
      res.setHeader('Content-Type', 'application/x-ndjson; charset=utf-8');
      res.setHeader('X-Accel-Buffering', 'no');
      res.flushHeaders();
      projection = await streamRuntimeChat(
        runtimeInput,
        (delta) => {
          res.write(`${JSON.stringify({
            schema_version: PUBLIC_CHAT_STREAM_SCHEMA,
            type: 'delta',
            text: delta,
          })}\n`);
        },
        traceparentFor(trace),
      );
    } else {
      projection = await submitRuntimeChat(runtimeInput, traceparentFor(trace));
    }
  } catch (error) {
    const status = error instanceof RuntimeProxyError ? error.publicStatus : 502;
    if (streaming) {
      res.write(`${JSON.stringify({
        schema_version: PUBLIC_CHAT_STREAM_SCHEMA,
        type: 'error',
        ...publicRuntimeError(error),
      })}\n`);
      res.end();
    } else {
      res.status(status).json({
        ...publicRuntimeError(error),
        session_id: clientSessionId,
        conversation_id: clientSessionId,
        request_id: clientRequestId,
        duplicate_effect_protection: 'not_applicable_no_effect_authority',
        ...envelope(),
      });
    }
    await emitOperationalTelemetry({
      trace, kind: 'inference.apocrypha.turn.failed', source: 'apocv4-runtime-proxy', plane: 'runtime',
      severity: 'error', outcome: 'failed', status,
      durationMs: Math.round(performance.now() - started),
      message: 'Apocv4 public turn did not return a valid response.',
      effectClass: 'apocrypha.public.turn.no-effects', authority: 'authenticated-member',
      attributes: { failure_kind: publicRuntimeError(error).error },
    });
    return;
  }

  const modelId = stringField(projection.model_reported, 'model_id');
  const responseId = stringField(projection.model_reported, 'response_id');
  const responseDigest = stringField(projection.model_reported, 'response_digest');
  const servingProfileDigest = stringField(projection.model_reported, 'serving_profile_digest');
  if (!modelId || !responseId || !responseDigest || !servingProfileDigest) {
    if (streaming) {
      res.write(`${JSON.stringify({
        schema_version: PUBLIC_CHAT_STREAM_SCHEMA,
        type: 'error',
        error: 'runtime_response_invalid',
      })}\n`);
      res.end();
    } else {
      res.status(502).json({
        error: 'The Apocv4 runtime did not return the exact public-turn evidence envelope.',
        session_id: clientSessionId,
        conversation_id: clientSessionId,
        request_id: clientRequestId,
        duplicate_effect_protection: 'not_applicable_no_effect_authority',
        ...envelope(),
      });
    }
    await emitOperationalTelemetry({
      trace, kind: 'inference.apocrypha.turn.envelope_rejected', source: 'apocv4-runtime-proxy', plane: 'runtime',
      severity: 'error', outcome: 'failed', status: 502,
      durationMs: Math.round(performance.now() - started),
      message: 'Apocv4 public-turn evidence envelope failed exact validation.',
      effectClass: 'apocrypha.public.turn.no-effects', authority: 'authenticated-member',
    });
    return;
  }

  const publicResult = {
    text: projection.model_reported.text,
    session_id: clientSessionId,
    conversation_id: clientSessionId,
    request_id: clientRequestId,
    model_id: modelId,
    response_id: responseId,
    response_digest: responseDigest,
    serving_profile_digest: servingProfileDigest,
    effect_authority: projection.authority.effect_authority,
    tool_authority: projection.authority.tool_authority,
    outcome: 'completed',
    learned_faculty_used: true,
    memory_scope: projection.authority.memory_scope,
    conversation_history: projection.authority.conversation_history === 'durable_principal_bound'
      ? 'durable_principal_bound'
      : 'not_retained_by_public_interface',
    training_consent: false,
    identity: projection.identity,
    context: projection.context,
    duplicate_effect_protection: 'not_applicable_no_effect_authority',
    upstream_status: projection.observed.receipt.upstream_status,
    ...envelope(),
  };
  if (streaming) {
    res.write(`${JSON.stringify({
      schema_version: PUBLIC_CHAT_STREAM_SCHEMA,
      type: 'completed',
      result: publicResult,
    })}\n`);
    res.end();
  } else {
    res.status(200).json(publicResult);
  }
  await emitOperationalTelemetry({
    trace, kind: 'inference.apocrypha.turn.completed', source: 'apocv4-runtime-proxy', plane: 'runtime',
    severity: 'info', outcome: 'succeeded', status: 200,
    durationMs: Math.round(performance.now() - started),
    message: 'Apocv4 public turn completed with exact evidence envelope.',
    effectClass: 'apocrypha.public.turn.no-effects', authority: 'authenticated-member',
    receiptRef: responseDigest,
    attributes: {
      upstream_status: projection.observed.receipt.upstream_status,
      model_id: modelId,
      effect_authority: projection.authority.effect_authority,
      tool_authority: projection.authority.tool_authority,
      memory_scope: projection.authority.memory_scope,
      training_consent: false,
    },
  });
}
