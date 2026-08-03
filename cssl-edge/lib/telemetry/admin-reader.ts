import { createHash } from 'node:crypto';

import { createClient, type SupabaseClient } from '@supabase/supabase-js';

import { OPERATIONAL_TELEMETRY_SCHEMA, sanitizeTelemetryAttributes } from './server';

export const ADMIN_LOG_SCHEMA = 'apocky.admin-telemetry-log.v1' as const;

export interface AdminTelemetryRow {
  id: string;
  ts: string;
  eventId: string;
  traceId: string;
  spanId: string;
  parentSpanId: string | null;
  source: string;
  plane: string;
  severity: 'debug' | 'info' | 'warn' | 'error' | 'critical';
  kind: string;
  outcome: string;
  route: string | null;
  status: number | null;
  durationMs: number | null;
  message: string | null;
  fingerprint: string;
  clusterSignature: string | null;
  deploymentId: string;
  effectClass: string | null;
  authority: string | null;
  receiptRef: string | null;
  privacyTier: string;
  payload: Record<string, unknown>;
}

interface AkashicRow {
  id: number | string;
  cell_id: string;
  ts_iso: string;
  sigma_mask: number;
  dpl_id: string;
  commit_sha: string;
  kind: string;
  payload: unknown;
  session_id: string;
  cluster_signature: string | null;
}

let serviceClient: SupabaseClient | null | undefined;

function getServiceClient(): SupabaseClient | null {
  if (serviceClient !== undefined) return serviceClient;
  const url = process.env.APOCKY_HUB_SUPABASE_URL ?? process.env.NEXT_PUBLIC_SUPABASE_URL;
  const key = process.env.APOCKY_HUB_SUPABASE_SERVICE_ROLE_KEY ?? process.env.SUPABASE_SERVICE_ROLE_KEY;
  serviceClient = url && key
    ? createClient(url, key, { auth: { persistSession: false, autoRefreshToken: false } })
    : null;
  return serviceClient;
}

function stringAt(payload: Record<string, unknown>, key: string, max = 2_048): string | null {
  const value = payload[key];
  return typeof value === 'string' && value.length > 0 ? value.slice(0, max) : null;
}

function numberAt(payload: Record<string, unknown>, key: string): number | null {
  const value = payload[key];
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function safeRoute(value: string | null): string | null {
  if (!value) return null;
  try {
    const parsed = value.startsWith('http') ? new URL(value) : null;
    return (parsed?.pathname ?? value.split('?', 1)[0] ?? '/').slice(0, 256);
  } catch {
    return value.split('?', 1)[0]?.slice(0, 256) ?? null;
  }
}

function severityFor(kind: string, payload: Record<string, unknown>): AdminTelemetryRow['severity'] {
  const stated = stringAt(payload, 'severity', 16);
  if (stated === 'debug' || stated === 'info' || stated === 'warn' || stated === 'error' || stated === 'critical') return stated;
  if (/critical|security\.breach/.test(kind)) return 'critical';
  if (/error|fail|unhandled|denied/.test(kind)) return 'error';
  if (/warn|slow|degraded|retry/.test(kind)) return 'warn';
  return 'info';
}

function stableHex(value: string, length: 16 | 32): string {
  return createHash('sha256').update(value).digest('hex').slice(0, length);
}

function normalize(row: AkashicRow): AdminTelemetryRow {
  const rawPayload = row.payload && typeof row.payload === 'object' && !Array.isArray(row.payload)
    ? row.payload as Record<string, unknown>
    : {};
  const payload = sanitizeTelemetryAttributes(rawPayload);
  const operational = stringAt(payload, 'schema_version', 128) === OPERATIONAL_TELEMETRY_SCHEMA;
  const traceId = stringAt(payload, 'trace_id', 32);
  const spanId = stringAt(payload, 'span_id', 16);
  const message = stringAt(payload, 'message') ?? stringAt(payload, 'error');
  const fingerprint = stringAt(payload, 'fingerprint', 64)
    ?? row.cluster_signature
    ?? stableHex(`${row.kind}|${message ?? ''}|${row.dpl_id}`, 16);
  return {
    id: String(row.id),
    ts: row.ts_iso,
    eventId: stringAt(payload, 'event_id', 64) ?? row.cell_id,
    traceId: traceId && /^[0-9a-f]{32}$/i.test(traceId) ? traceId : stableHex(row.session_id, 32),
    spanId: spanId && /^[0-9a-f]{16}$/i.test(spanId) ? spanId : stableHex(row.cell_id, 16),
    parentSpanId: stringAt(payload, 'parent_span_id', 16),
    source: stringAt(payload, 'telemetry_source', 128)
      ?? (operational ? stringAt(payload, 'source', 128) : null)
      ?? (operational ? 'edge' : 'browser'),
    plane: stringAt(payload, 'plane', 32) ?? (operational ? 'edge' : 'browser'),
    severity: severityFor(row.kind, payload),
    kind: row.kind,
    outcome: stringAt(payload, 'outcome', 64) ?? (/error|fail|unhandled/.test(row.kind) ? 'failed' : 'observed'),
    route: safeRoute(stringAt(payload, 'route', 256) ?? stringAt(payload, 'url', 2_048)),
    status: numberAt(payload, 'status'),
    durationMs: numberAt(payload, 'duration_ms'),
    message,
    fingerprint,
    clusterSignature: row.cluster_signature,
    deploymentId: stringAt(payload, 'deployment_id', 256) ?? row.dpl_id,
    effectClass: stringAt(payload, 'effect_class', 128),
    authority: stringAt(payload, 'authority', 256),
    receiptRef: stringAt(payload, 'receipt_ref', 512),
    privacyTier: stringAt(payload, 'privacy_tier', 64) ?? `sigma-${row.sigma_mask}`,
    payload,
  };
}

export async function readAdminTelemetry(limit: number, beforeId?: number): Promise<{
  rows: AdminTelemetryRow[];
  cursor: string | null;
  hasMore: boolean;
  source: 'supabase' | 'unconfigured' | 'failed';
}> {
  const client = getServiceClient();
  if (!client) return { rows: [], cursor: null, hasMore: false, source: 'unconfigured' };
  let query = client
    .from('akashic_events')
    .select('id,cell_id,ts_iso,sigma_mask,dpl_id,commit_sha,kind,payload,session_id,cluster_signature')
    .order('id', { ascending: false })
    .limit(limit);
  if (beforeId !== undefined) query = query.lt('id', beforeId);
  const { data, error } = await query;
  if (error || !data) return { rows: [], cursor: null, hasMore: false, source: 'failed' };
  const rows = (data as AkashicRow[]).map(normalize);
  return {
    rows,
    cursor: rows.at(-1)?.id ?? null,
    hasMore: rows.length === limit,
    source: 'supabase',
  };
}

export function summarizeTelemetry(rows: AdminTelemetryRow[]): Record<string, unknown> {
  const counts = (key: 'severity' | 'plane' | 'outcome'): Record<string, number> => rows.reduce<Record<string, number>>((all, row) => {
    all[row[key]] = (all[row[key]] ?? 0) + 1;
    return all;
  }, {});
  return {
    total: rows.length,
    severity: counts('severity'),
    plane: counts('plane'),
    outcome: counts('outcome'),
    unique_traces: new Set(rows.map((row) => row.traceId)).size,
    unique_fingerprints: new Set(rows.map((row) => row.fingerprint)).size,
    latest_at: rows[0]?.ts ?? null,
  };
}

export function _resetAdminTelemetryForTests(): void {
  serviceClient = undefined;
}
