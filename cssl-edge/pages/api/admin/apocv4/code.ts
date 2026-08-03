import { createHash } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';

import { getAdminAuthorization } from '@/lib/admin-auth';
import {
  APOCV4_PROXY_SCHEMA,
  RuntimeProxyError,
  publicRuntimeError,
  submitRuntimeCode,
  validateRuntimeCodePaths,
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

export const maxDuration = 300;

export const config = {
  api: {
    bodyParser: { sizeLimit: '48kb' },
    responseLimit: '4mb',
  },
};

const OWNER_PRIVACY_PARTITION = 'owner:apocky';
const MAX_RUNTIME_BODY_BYTES = 22 * 1024;
let codeEffectInFlight = false;

interface CodeBody {
  objective: string;
  allowedPaths: string[];
  sessionId: string;
  requestId: string;
}

function exactCodeBody(value: unknown): CodeBody | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  const body = value as Record<string, unknown>;
  if (
    Object.keys(body).length !== 5
    || !Object.hasOwn(body, 'objective')
    || !Object.hasOwn(body, 'allowed_paths')
    || !Object.hasOwn(body, 'confirm_apply')
    || !Object.hasOwn(body, 'session_id')
    || !Object.hasOwn(body, 'request_id')
    || body.confirm_apply !== true
  ) return null;
  const objective = body.objective;
  if (
    typeof objective !== 'string'
    || objective !== objective.trim()
    || objective.length < 1
    || Buffer.byteLength(objective, 'utf8') > 32_768
  ) return null;
  if (!isOpaqueConversationId(body.session_id) || !isOpaqueClientRequestId(body.request_id)) {
    return null;
  }
  try {
    return {
      objective,
      allowedPaths: validateRuntimeCodePaths(body.allowed_paths),
      sessionId: body.session_id.toLowerCase(),
      requestId: body.request_id.toLowerCase(),
    };
  } catch {
    return null;
  }
}

function sha256(value: string): string {
  return createHash('sha256').update(value, 'utf8').digest('hex');
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
      trace, kind: 'api.apocv4.code.method_denied', source: 'pages.api.admin.apocv4.code', plane: 'security',
      severity: 'warn', outcome: 'denied', status: 405, durationMs: Math.round(performance.now() - started),
      message: 'Owner code-effect method denied.', authority: 'owner-admin-required',
    });
    return;
  }
  if (!hasSameOrigin(req)) {
    res.status(403).json({ error: 'Same-origin request required', ...envelope() });
    await emitOperationalTelemetry({
      trace, kind: 'security.apocv4.code.origin_denied', source: 'pages.api.admin.apocv4.code', plane: 'security',
      severity: 'warn', outcome: 'denied', status: 403, durationMs: Math.round(performance.now() - started),
      message: 'Owner code-effect origin denied.', authority: 'same-origin-owner-admin',
    });
    return;
  }
  if (req.headers['content-type'] !== 'application/json') {
    res.status(415).json({ error: 'application/json required', ...envelope() });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocv4.code.content_type_denied', source: 'pages.api.admin.apocv4.code', plane: 'edge',
      severity: 'warn', outcome: 'denied', status: 415, durationMs: Math.round(performance.now() - started),
      message: 'Owner code-effect content type denied.', authority: 'same-origin-owner-admin',
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
  const body = exactCodeBody(req.body);
  if (!body) {
    res.status(400).json({
      schema_version: APOCV4_PROXY_SCHEMA,
      error: 'code_body_invalid',
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocv4.code.body_denied', source: 'pages.api.admin.apocv4.code', plane: 'edge',
      severity: 'warn', outcome: 'denied', status: 400, durationMs: Math.round(performance.now() - started),
      message: 'Owner code-effect body denied.', authority: 'owner-admin',
    });
    return;
  }
  const runtimeBodyBytes = Buffer.byteLength(JSON.stringify({
    objective: body.objective,
    privacy_partition: OWNER_PRIVACY_PARTITION,
    allowed_paths: body.allowedPaths,
    session_id: body.sessionId,
    session_principal: sessionPrincipal,
    request_id: scopeRequestId(sessionPrincipal, body.requestId),
    session_binding_mac: '0'.repeat(64),
  }), 'utf8');
  if (runtimeBodyBytes > MAX_RUNTIME_BODY_BYTES) {
    res.status(413).json({
      schema_version: APOCV4_PROXY_SCHEMA,
      error: 'code_body_too_large',
      maximum_runtime_bytes: MAX_RUNTIME_BODY_BYTES,
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocv4.code.size_denied', source: 'pages.api.admin.apocv4.code', plane: 'edge',
      severity: 'warn', outcome: 'denied', status: 413, durationMs: Math.round(performance.now() - started),
      message: 'Owner code-effect runtime envelope exceeded its byte bound.', authority: 'owner-admin',
      attributes: {
        runtime_body_bytes: runtimeBodyBytes,
        maximum_runtime_bytes: MAX_RUNTIME_BODY_BYTES,
      },
    });
    return;
  }
  if (codeEffectInFlight) {
    res.setHeader('Retry-After', '5');
    res.status(429).json({
      schema_version: APOCV4_PROXY_SCHEMA,
      error: 'code_effect_in_flight',
      retry_automatically: false,
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'security.apocv4.code.in_flight_denied', source: 'pages.api.admin.apocv4.code', plane: 'security',
      severity: 'warn', outcome: 'denied', status: 429, durationMs: Math.round(performance.now() - started),
      message: 'Concurrent owner code effect denied.', authority: 'owner-admin-confirmed',
      attributes: { retry_after_seconds: 5 },
    });
    return;
  }

  const objectiveDigest = sha256(body.objective);
  const pathSetDigest = sha256(JSON.stringify(body.allowedPaths));
  codeEffectInFlight = true;
  await emitOperationalTelemetry({
    trace, kind: 'effect.runtime_code.started', source: 'apocv4-runtime-proxy', plane: 'effect',
    severity: 'info', outcome: 'started', status: null, durationMs: Math.round(performance.now() - started),
    message: 'Owner-confirmed bounded code effect admitted for runtime dispatch.',
    effectClass: 'apocv4.code.generate_test_apply', authority: 'owner-admin-confirmed',
    attributes: {
      objective_digest: objectiveDigest,
      path_set_digest: pathSetDigest,
      path_count: body.allowedPaths.length,
      principal_ref: sessionPrincipal,
    },
  });
  try {
    const result = await submitRuntimeCode({
      objective: body.objective,
      allowedPaths: body.allowedPaths,
      privacyPartition: OWNER_PRIVACY_PARTITION,
      sessionId: body.sessionId,
      sessionPrincipal,
      requestId: scopeRequestId(sessionPrincipal, body.requestId),
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
    const state = typeof result.observed.runtime.state === 'string'
      ? result.observed.runtime.state
      : 'UNKNOWN';
    await emitOperationalTelemetry({
      trace, kind: 'effect.runtime_code.completed', source: 'apocv4-runtime-proxy', plane: 'effect',
      severity: state === 'PROMOTED' ? 'info' : 'warn',
      outcome: state === 'PROMOTED' ? 'accepted' : 'degraded',
      status: 200, durationMs: result.observed.receipt.latency_ms,
      message: `Bounded code effect completed with ${state}.`,
      effectClass: 'apocv4.code.generate_test_apply', authority: 'owner-admin-confirmed',
      receiptRef: typeof result.observed.runtime.promotion_event_digest === 'string'
        ? result.observed.runtime.promotion_event_digest
        : result.observed.runtime.terminal_event_digest as string | null,
      attributes: {
        objective_digest: objectiveDigest,
        path_set_digest: pathSetDigest,
        path_count: body.allowedPaths.length,
        runtime_state: state,
        test_passed: result.observed.test?.passed ?? null,
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
      trace, kind: 'effect.runtime_code.failed', source: 'apocv4-runtime-proxy', plane: 'effect',
      severity: 'error', outcome: 'failed', status,
      durationMs: Math.round(performance.now() - started),
      message: error instanceof RuntimeProxyError ? error.code : 'runtime_proxy_failure',
      effectClass: 'apocv4.code.generate_test_apply', authority: 'owner-admin-confirmed',
      attributes: {
        objective_digest: objectiveDigest,
        path_set_digest: pathSetDigest,
        path_count: body.allowedPaths.length,
        upstream_status: error instanceof RuntimeProxyError ? error.upstreamStatus : null,
      },
    });
  } finally {
    codeEffectInFlight = false;
  }
}
