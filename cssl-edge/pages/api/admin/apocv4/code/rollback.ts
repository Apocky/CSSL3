import type { NextApiRequest, NextApiResponse } from 'next';

import { getAdminAuthorization } from '@/lib/admin-auth';
import {
  APOCV4_PROXY_SCHEMA,
  RuntimeProxyError,
  publicRuntimeError,
  submitRuntimeRollback,
} from '@/lib/apocv4/runtime-proxy';
import { publicMemberPrincipalRef } from '@/lib/apocv4/session-principal';
import { hasSameOrigin } from '@/lib/auth-session';
import {
  isOpaqueClientRequestId,
  isOpaqueConversationId,
  scopeRequestId,
  setPrivateNoStore,
} from '@/lib/apocrypha/proxy';
import { envelope } from '@/lib/response';
import { createServerTrace, emitOperationalTelemetry, traceparentFor } from '@/lib/telemetry/server';

export const maxDuration = 60;

export const config = {
  api: {
    bodyParser: { sizeLimit: '2kb' },
    responseLimit: '512kb',
  },
};

const SHA256_RE = /^[0-9a-f]{64}$/;
const OWNER_PRIVACY_PARTITION = 'owner:apocky';
let rollbackInFlight = false;

interface RollbackBody {
  promotionEventDigest: string;
  sessionId: string;
  requestId: string;
}

function exactRollbackBody(value: unknown): RollbackBody | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  const body = value as Record<string, unknown>;
  if (
    Object.keys(body).length !== 4
    || !Object.hasOwn(body, 'promotion_event_digest')
    || !Object.hasOwn(body, 'confirm_rollback')
    || !Object.hasOwn(body, 'session_id')
    || !Object.hasOwn(body, 'request_id')
    || body.confirm_rollback !== true
    || typeof body.promotion_event_digest !== 'string'
    || !SHA256_RE.test(body.promotion_event_digest)
    || !isOpaqueConversationId(body.session_id)
    || !isOpaqueClientRequestId(body.request_id)
  ) return null;
  return {
    promotionEventDigest: body.promotion_event_digest,
    sessionId: body.session_id.toLowerCase(),
    requestId: body.request_id.toLowerCase(),
  };
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  const trace = createServerTrace(req);
  const started = performance.now();
  setPrivateNoStore(res);
  res.setHeader('X-Apocky-Trace-Id', trace.traceId);
  res.setHeader('Traceparent', traceparentFor(trace));
  res.setHeader('Allow', 'POST');
  if (req.method !== 'POST') {
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocv4.code.rollback_method_denied', source: 'pages.api.admin.apocv4.code.rollback', plane: 'security',
      severity: 'warn', outcome: 'denied', status: 405, durationMs: Math.round(performance.now() - started),
      message: 'Owner code rollback method denied.', authority: 'owner-admin-required',
    });
    return;
  }
  if (!hasSameOrigin(req)) {
    res.status(403).json({ error: 'Same-origin request required', ...envelope() });
    await emitOperationalTelemetry({
      trace, kind: 'security.apocv4.code.rollback_origin_denied', source: 'pages.api.admin.apocv4.code.rollback', plane: 'security',
      severity: 'warn', outcome: 'denied', status: 403, durationMs: Math.round(performance.now() - started),
      message: 'Owner code rollback origin denied.', authority: 'same-origin-owner-admin',
    });
    return;
  }
  if (req.headers['content-type'] !== 'application/json') {
    res.status(415).json({ error: 'application/json required', ...envelope() });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocv4.code.rollback_content_type_denied', source: 'pages.api.admin.apocv4.code.rollback', plane: 'edge',
      severity: 'warn', outcome: 'denied', status: 415, durationMs: Math.round(performance.now() - started),
      message: 'Owner code rollback content type denied.', authority: 'same-origin-owner-admin',
    });
    return;
  }
  const authorization = await getAdminAuthorization(req);
  if (!authorization.authorized || !authorization.user) {
    res.status(authorization.user ? 403 : 401).json({
      error: authorization.reason ?? 'Owner authorization required.',
      authorized: false,
      ...envelope(),
    });
    return;
  }
  const sessionPrincipal = publicMemberPrincipalRef(authorization.user.id);
  const body = exactRollbackBody(req.body);
  if (!body) {
    res.status(400).json({
      schema_version: APOCV4_PROXY_SCHEMA,
      error: 'rollback_body_invalid',
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocv4.code.rollback_body_denied', source: 'pages.api.admin.apocv4.code.rollback', plane: 'edge',
      severity: 'warn', outcome: 'denied', status: 400, durationMs: Math.round(performance.now() - started),
      message: 'Owner code rollback body denied.', authority: 'owner-admin',
    });
    return;
  }
  if (rollbackInFlight) {
    res.setHeader('Retry-After', '5');
    res.status(429).json({
      schema_version: APOCV4_PROXY_SCHEMA,
      error: 'rollback_in_flight',
      retry_automatically: false,
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'security.apocv4.code.rollback_in_flight_denied', source: 'pages.api.admin.apocv4.code.rollback', plane: 'security',
      severity: 'warn', outcome: 'denied', status: 429, durationMs: Math.round(performance.now() - started),
      message: 'Concurrent owner code rollback denied.', authority: 'owner-admin-confirmed',
      attributes: { retry_after_seconds: 5 },
    });
    return;
  }

  rollbackInFlight = true;
  try {
    await emitOperationalTelemetry({
      trace, kind: 'effect.runtime_code.rollback_started', source: 'apocv4-runtime-proxy', plane: 'effect',
      severity: 'info', outcome: 'started', status: null, durationMs: Math.round(performance.now() - started),
      message: 'Owner-confirmed code rollback admitted for runtime dispatch.',
      effectClass: 'apocv4.code.rollback', authority: 'owner-admin-confirmed',
      receiptRef: body.promotionEventDigest,
      attributes: { principal_ref: sessionPrincipal },
    });
    const result = await submitRuntimeRollback({
      promotionEventDigest: body.promotionEventDigest,
      sessionId: body.sessionId,
      sessionPrincipal,
      requestId: scopeRequestId(sessionPrincipal, body.requestId),
      privacyPartition: OWNER_PRIVACY_PARTITION,
    }, traceparentFor(trace));
    res.status(200).json({
      ...result,
      observed: {
        ...result.observed,
        runtime: {
          ...result.observed.runtime,
          session_id: body.sessionId,
          request_id: body.requestId,
        },
      },
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'effect.runtime_code.rollback_completed', source: 'apocv4-runtime-proxy', plane: 'effect',
      severity: 'info', outcome: 'succeeded', status: 200,
      durationMs: result.observed.receipt.latency_ms,
      message: 'Owner-confirmed code effect rolled back.',
      effectClass: 'apocv4.code.rollback', authority: 'owner-admin-confirmed',
      receiptRef: result.observed.runtime.rollback_event_digest as string,
      attributes: {
        promotion_event_digest: body.promotionEventDigest,
        principal_ref: sessionPrincipal,
        upstream_status: result.observed.receipt.upstream_status,
      },
    });
  } catch (error) {
    const status = error instanceof RuntimeProxyError ? error.publicStatus : 502;
    res.status(status).json({
      schema_version: APOCV4_PROXY_SCHEMA,
      ...publicRuntimeError(error),
      retry_automatically: false,
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'effect.runtime_code.rollback_failed', source: 'apocv4-runtime-proxy', plane: 'effect',
      severity: 'error', outcome: 'failed', status,
      durationMs: Math.round(performance.now() - started),
      message: error instanceof RuntimeProxyError ? error.code : 'runtime_proxy_failure',
      effectClass: 'apocv4.code.rollback', authority: 'owner-admin-confirmed',
      attributes: {
        promotion_event_digest: body.promotionEventDigest,
        upstream_status: error instanceof RuntimeProxyError ? error.upstreamStatus : null,
      },
    });
  } finally {
    rollbackInFlight = false;
  }
}
