import type { NextApiRequest, NextApiResponse } from 'next';

import {
  APOCV4_PROXY_SCHEMA,
  RuntimeProxyError,
  fetchRuntimeHealth,
  publicRuntimeError,
} from '@/lib/apocv4/runtime-proxy';
import { requireAdmin } from '@/lib/require-admin';
import { envelope } from '@/lib/response';
import { createServerTrace, emitOperationalTelemetry, traceparentFor } from '@/lib/telemetry/server';

export const maxDuration = 20;

export const config = {
  api: {
    responseLimit: '512kb',
  },
};

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  const trace = createServerTrace(req);
  const started = performance.now();
  res.setHeader('Cache-Control', 'no-store, max-age=0');
  res.setHeader('X-Apocky-Trace-Id', trace.traceId);
  res.setHeader('Traceparent', traceparentFor(trace));
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    res.status(405).json({ error: 'Method not allowed', ...envelope() });
    await emitOperationalTelemetry({
      trace, kind: 'api.apocv4.health.denied', source: 'pages.api.admin.apocv4.health', plane: 'security',
      severity: 'warn', outcome: 'denied', status: 405, durationMs: Math.round(performance.now() - started),
      message: 'Runtime health method denied.', authority: 'owner-admin-required',
    });
    return;
  }
  if (!(await requireAdmin(req, res))) {
    await emitOperationalTelemetry({
      trace, kind: 'security.apocv4.health.denied', source: 'pages.api.admin.apocv4.health', plane: 'security',
      severity: 'warn', outcome: 'denied', status: res.statusCode, durationMs: Math.round(performance.now() - started),
      message: 'Runtime health authorization denied.', authority: 'owner-admin-required',
    });
    return;
  }
  try {
    const result = await fetchRuntimeHealth(traceparentFor(trace));
    res.status(200).json({ ...result, ...envelope() });
    await emitOperationalTelemetry({
      trace, kind: 'runtime.health.checked', source: 'apocv4-runtime-proxy', plane: 'runtime',
      severity: 'info', outcome: 'succeeded', status: 200,
      durationMs: result.observed.receipt.latency_ms,
      message: 'Apocv4 runtime health observed.', authority: 'owner-admin',
      receiptRef: result.observed.receipt.binding_ref ?? result.observed.receipt.auth_registry_ref,
      attributes: {
        upstream_status: result.observed.receipt.upstream_status,
        auth_mode: result.observed.receipt.auth_mode,
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
      trace, kind: 'runtime.health.failed', source: 'apocv4-runtime-proxy', plane: 'runtime',
      severity: 'error', outcome: 'failed', status,
      durationMs: Math.round(performance.now() - started),
      message: error instanceof RuntimeProxyError ? error.code : 'runtime_proxy_failure',
      authority: 'owner-admin',
      attributes: { upstream_status: error instanceof RuntimeProxyError ? error.upstreamStatus : null },
    });
  }
}
