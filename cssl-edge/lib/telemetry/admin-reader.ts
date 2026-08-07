import { createHash } from 'node:crypto';

import { createClient, type SupabaseClient } from '@supabase/supabase-js';

import { CREATION_LEDGER_SCHEMA } from './creation-ledger';
import { OPERATIONAL_TELEMETRY_SCHEMA, sanitizeTelemetryAttributes } from './server';

export const ADMIN_LOG_SCHEMA = 'apocky.admin-telemetry-log.v1' as const;
const TRUSTED_CREATION_EVENT_RX = /^(?:inference\.apocrypha\.turn\.(?:started|completed)|runtime\.chat\.(?:started|completed)|effect\.runtime_code\.(?:started|completed)|creation\.apocrypha\.[a-z0-9._-]+)$/;

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
  sessionRef: string;
  payload: Record<string, unknown>;
}

export interface AdminCreationLedgerRow {
  id: string;
  ts: string;
  kind: string;
  outcome: string;
  recordDigest: string;
  creationKind: string;
  origin: string;
  stage: string;
  channel: string;
  actorRef: string;
  requestRef: string;
  artifactRef: string | null;
  modelId: string | null;
  toolId: string | null;
  effectAuthority: string | null;
  inputDigest: string | null;
  outputDigest: string | null;
  inputBytes: number;
  outputBytes: number;
  safetyDisposition: string;
  safetySignals: string[];
  contentRetained: false;
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

function recordAt(payload: Record<string, unknown>, key: string): Record<string, unknown> | null {
  const value = payload[key];
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
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
  const projectedRoute = safeRoute(
    stringAt(rawPayload, 'route', 256) ?? stringAt(rawPayload, 'url', 2_048),
  );
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
    route: projectedRoute,
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
    sessionRef: stableHex(row.session_id, 16),
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
  const pageViews = rows.filter((row) => row.kind === 'page.view');
  const routeCounts = pageViews.reduce<Record<string, number>>((all, row) => {
    const route = row.route ?? 'unknown';
    all[route] = (all[route] ?? 0) + 1;
    return all;
  }, {});
  const interactions = rows.filter((row) => (
    row.kind.includes('apocrypha.turn')
    || row.kind === 'runtime.chat.started'
    || row.kind === 'runtime.chat.completed'
    || row.kind === 'runtime.chat.failed'
    || row.kind === 'runtime.chat.contract_rejected'
  ));
  const creations = creationLedgerEntries(rows);
  const byCreationKind = creations.reduce<Record<string, number>>((all, row) => {
    all[row.creationKind] = (all[row.creationKind] ?? 0) + 1;
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
    visitors: {
      consented_sessions: new Set(pageViews.map((row) => row.sessionRef)).size,
      page_views: pageViews.length,
      top_routes: Object.entries(routeCounts)
        .sort((a, b) => b[1] - a[1])
        .slice(0, 8)
        .map(([route, views]) => ({ route, views })),
      scope: 'consented_ephemeral_sessions_in_visible_window',
    },
    interactions: {
      total: interactions.length,
      started: interactions.filter((row) => row.outcome === 'started').length,
      completed: interactions.filter((row) => row.outcome === 'succeeded').length,
      denied: interactions.filter((row) => row.outcome === 'denied').length,
      failed: interactions.filter((row) => row.outcome === 'failed' || row.outcome === 'degraded').length,
    },
    creations: {
      total: creations.length,
      results: creations.filter((row) => row.stage === 'result').length,
      attempts: creations.filter((row) => row.stage === 'attempt').length,
      review_required: creations.filter((row) => row.safetyDisposition === 'review_required').length,
      no_signal: creations.filter((row) => row.safetyDisposition === 'no_signal').length,
      by_kind: byCreationKind,
      screening_basis: 'first_party_signal_scan_not_safety_proof',
    },
  };
}

export function creationLedgerEntries(rows: AdminTelemetryRow[]): AdminCreationLedgerRow[] {
  const entries: AdminCreationLedgerRow[] = [];
  for (const row of rows) {
    if (
      stringAt(row.payload, 'schema_version', 128) !== OPERATIONAL_TELEMETRY_SCHEMA
      || !TRUSTED_CREATION_EVENT_RX.test(row.kind)
    ) continue;
    const attributes = recordAt(row.payload, 'attributes');
    const ledger = attributes ? recordAt(attributes, 'creation_ledger') : null;
    if (!ledger || stringAt(ledger, 'schema_version', 128) !== CREATION_LEDGER_SCHEMA) continue;
    const recordDigest = stringAt(ledger, 'record_digest', 64);
    const creationKind = stringAt(ledger, 'creation_kind', 64);
    const actorRef = stringAt(ledger, 'actor_ref', 512);
    const requestRef = stringAt(ledger, 'request_ref', 512);
    if (!recordDigest || !creationKind || !actorRef || !requestRef) continue;
    const rawSignals = ledger.safety_signals;
    entries.push({
      id: row.id,
      ts: row.ts,
      kind: row.kind,
      outcome: row.outcome,
      recordDigest,
      creationKind,
      origin: stringAt(ledger, 'origin', 64) ?? 'unknown',
      stage: stringAt(ledger, 'stage', 64) ?? 'unknown',
      channel: stringAt(ledger, 'channel', 64) ?? 'unknown',
      actorRef,
      requestRef,
      artifactRef: stringAt(ledger, 'artifact_ref', 512),
      modelId: stringAt(ledger, 'model_id', 256),
      toolId: stringAt(ledger, 'tool_id', 256),
      effectAuthority: stringAt(ledger, 'effect_authority', 256),
      inputDigest: stringAt(ledger, 'input_digest', 64),
      outputDigest: stringAt(ledger, 'output_digest', 64),
      inputBytes: numberAt(ledger, 'input_bytes') ?? 0,
      outputBytes: numberAt(ledger, 'output_bytes') ?? 0,
      safetyDisposition: stringAt(ledger, 'safety_disposition', 64) ?? 'unknown',
      safetySignals: Array.isArray(rawSignals)
        ? rawSignals.filter((value): value is string => typeof value === 'string').slice(0, 16)
        : [],
      contentRetained: false,
    });
  }
  return entries;
}

export function _resetAdminTelemetryForTests(): void {
  serviceClient = undefined;
}
