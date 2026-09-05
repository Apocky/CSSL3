import type { NextApiRequest, NextApiResponse } from 'next';

import { requireAdmin } from '@/lib/require-admin';
import { envelope } from '@/lib/response';
import {
  ADMIN_LOG_SCHEMA,
  readAdminTelemetry,
  summarizeTelemetry,
} from '@/lib/telemetry/admin-reader';
import {
  createServerTrace,
  emitOperationalTelemetry,
  traceparentFor,
} from '@/lib/telemetry/server';

const DEFAULT_LIMIT = 200;
const MAX_LIMIT = 500;

function decimal(value: string | string[] | undefined, allowZero: boolean): number | undefined | null {
  if (value === undefined) return undefined;
  if (typeof value !== 'string' || !/^(?:0|[1-9][0-9]{0,15})$/.test(value)) return null;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < (allowZero ? 0 : 1)) return null;
  return parsed;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  const trace = createServerTrace(req);
  const started = performance.now();
  res.setHeader('Cache-Control', 'private, no-store, max-age=0');
  res.setHeader('X-Apocky-Trace-Id', trace.traceId);
  res.setHeader('Traceparent', traceparentFor(trace));
  res.setHeader('Allow', 'GET');

  if (req.method !== 'GET') {
    res.status(405).json({ schema_version: ADMIN_LOG_SCHEMA, error: 'Method not allowed', ...envelope() });
    return;
  }
  if (!(await requireAdmin(req, res))) {
    await emitOperationalTelemetry({
      trace,
      kind: 'security.admin_logs.denied',
      source: 'pages.api.admin.logs',
      plane: 'security',
      severity: 'warn',
      outcome: 'denied',
      status: res.statusCode,
      durationMs: Math.round(performance.now() - started),
      message: 'Admin telemetry read denied.',
      authority: 'owner-admin-required',
    });
    return;
  }

  const rawLimit = decimal(req.query.limit, false);
  const beforeId = decimal(req.query.before_id, false);
  if (rawLimit === null || beforeId === null) {
    res.status(400).json({
      schema_version: ADMIN_LOG_SCHEMA,
      error: 'limit and before_id must be positive decimal integers',
      ...envelope(),
    });
    return;
  }
  const limit = Math.min(rawLimit ?? DEFAULT_LIMIT, MAX_LIMIT);
  const result = await readAdminTelemetry(limit, beforeId);
  const status = result.source === 'failed' ? 'degraded' : 'live';
  res.status(200).json({
    schema_version: ADMIN_LOG_SCHEMA,
    observed_at: new Date().toISOString(),
    status,
    source: result.source,
    cursor: result.cursor,
    has_more: result.hasMore,
    rows: result.rows,
    summary: summarizeTelemetry(result.rows),
    trace_id: trace.traceId,
    ...envelope(),
  });

  await emitOperationalTelemetry({
    trace,
    kind: 'admin.telemetry.read',
    source: 'pages.api.admin.logs',
    plane: 'storage',
    severity: result.source === 'failed' ? 'error' : 'info',
    outcome: result.source === 'supabase' ? 'succeeded' : 'degraded',
    status: 200,
    durationMs: Math.round(performance.now() - started),
    message: result.source === 'supabase' ? 'Telemetry page read.' : 'Telemetry storage unavailable.',
    authority: 'owner-admin',
    attributes: { rows: result.rows.length, has_more: result.hasMore, source: result.source },
  });
}
