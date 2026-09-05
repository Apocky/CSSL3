import { createHash } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';

import {
  APOCV4_PROXY_SCHEMA,
  RuntimeProxyError,
  publicRuntimeError,
  submitRuntimeObjective,
} from '@/lib/apocv4/runtime-proxy';
import { requireAdmin } from '@/lib/require-admin';
import { envelope } from '@/lib/response';
import { createServerTrace, emitOperationalTelemetry, traceparentFor } from '@/lib/telemetry/server';

export const maxDuration = 300;

export const config = {
  api: {
    bodyParser: { sizeLimit: '24kb' },
    responseLimit: '3mb',
  },
};

function exactObjectiveBody(value: unknown): string | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  const body = value as Record<string, unknown>;
  if (Object.keys(body).length !== 1 || !Object.hasOwn(body, 'objective')) return null;
  const objective = body.objective;
  if (
    typeof objective !== 'string'
    || objective !== objective.trim()
    || objective.length < 1
    || objective.length > 16_384
  ) return null;
  return objective;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  const trace = createServerTrace(req);
  const started = performance.now();
  res.setHeader('Cache-Control', 'no-store, max-age=0');
  res.setHeader('X-Apocky-Trace-Id', trace.traceId);
  res.setHeader('Traceparent', traceparentFor(trace));
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocv4.objective.denied', source: 'pages.api.admin.apocv4.objective', plane: 'security',
      severity: 'warn', outcome: 'denied', status: 405, durationMs: Math.round(performance.now() - started),
      message: 'Runtime objective method denied.', authority: 'owner-admin-required',
    });
    return;
  }
  if (!(await requireAdmin(req, res))) {
    await emitOperationalTelemetry({
      trace, kind: 'security.apocv4.objective.denied', source: 'pages.api.admin.apocv4.objective', plane: 'security',
      severity: 'warn', outcome: 'denied', status: res.statusCode, durationMs: Math.round(performance.now() - started),
      message: 'Runtime objective authorization denied.', authority: 'owner-admin-required',
    });
    return;
  }
  const objective = exactObjectiveBody(req.body);
  if (objective === null) {
    res.status(400).json({
      schema_version: APOCV4_PROXY_SCHEMA,
      error: 'objective_body_invalid',
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'runtime.objective.rejected', source: 'pages.api.admin.apocv4.objective', plane: 'security',
      severity: 'warn', outcome: 'denied', status: 400, durationMs: Math.round(performance.now() - started),
      message: 'Objective envelope rejected before runtime dispatch.', authority: 'owner-admin',
    });
    return;
  }
  const objectiveDigest = createHash('sha256').update(objective, 'utf8').digest('hex');
  try {
    const result = await submitRuntimeObjective(objective, traceparentFor(trace));
    res.status(200).json({ ...result, ...envelope() });
    const runtime = result.observed.runtime;
    const runtimeStatus = typeof runtime.status === 'string' ? runtime.status : 'unknown';
    const receiptRef = typeof runtime.accepted_candidate_digest === 'string'
      ? runtime.accepted_candidate_digest
      : typeof runtime.checkpoint_digest === 'string'
        ? runtime.checkpoint_digest
        : result.observed.receipt.binding_ref;
    await emitOperationalTelemetry({
      trace, kind: 'effect.runtime_objective.completed', source: 'apocv4-runtime-proxy', plane: 'effect',
      severity: runtimeStatus === 'ACCEPTED' ? 'info' : 'warn',
      outcome: runtimeStatus === 'ACCEPTED' ? 'accepted' : 'degraded',
      status: 200, durationMs: result.observed.receipt.latency_ms,
      message: `Runtime objective completed with ${runtimeStatus}.`,
      effectClass: 'apocv4.objective.submit', authority: 'owner-admin', receiptRef,
      attributes: {
        objective_digest: objectiveDigest,
        objective_bytes: Buffer.byteLength(objective, 'utf8'),
        runtime_status: runtimeStatus,
        terminal_reason: typeof runtime.terminal_reason === 'string' ? runtime.terminal_reason : null,
        iterations_completed: typeof runtime.iterations_completed === 'number' ? runtime.iterations_completed : null,
        upstream_status: result.observed.receipt.upstream_status,
        total_edge_ms: Math.round(performance.now() - started),
      },
    });
  } catch (error) {
    const status = error instanceof RuntimeProxyError ? error.publicStatus : 502;
    res.status(status).json({
      schema_version: APOCV4_PROXY_SCHEMA,
      ...publicRuntimeError(error),
      ...envelope(),
    });
    await emitOperationalTelemetry({
      trace, kind: 'effect.runtime_objective.failed', source: 'apocv4-runtime-proxy', plane: 'effect',
      severity: 'error', outcome: 'failed', status,
      durationMs: Math.round(performance.now() - started),
      message: error instanceof RuntimeProxyError ? error.code : 'runtime_proxy_failure',
      effectClass: 'apocv4.objective.submit', authority: 'owner-admin',
      attributes: {
        objective_digest: objectiveDigest,
        objective_bytes: Buffer.byteLength(objective, 'utf8'),
        upstream_status: error instanceof RuntimeProxyError ? error.upstreamStatus : null,
      },
    });
  }
}
