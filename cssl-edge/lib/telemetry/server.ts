import { createHash, randomBytes } from 'node:crypto';

import { createClient, type SupabaseClient } from '@supabase/supabase-js';
import type { NextApiRequest } from 'next';

export const OPERATIONAL_TELEMETRY_SCHEMA = 'apocky.operational-telemetry.v1' as const;

export type TelemetryPlane = 'browser' | 'edge' | 'runtime' | 'tool' | 'effect' | 'security' | 'storage';
export type TelemetrySeverity = 'debug' | 'info' | 'warn' | 'error' | 'critical';
export type TelemetryOutcome = 'started' | 'accepted' | 'succeeded' | 'denied' | 'failed' | 'degraded' | 'unknown';

export interface ServerTraceContext {
  traceId: string;
  spanId: string;
  parentSpanId: string | null;
  route: string;
  method: string;
}

export interface OperationalTelemetryInput {
  trace: ServerTraceContext;
  kind: string;
  source: string;
  plane: TelemetryPlane;
  severity: TelemetrySeverity;
  outcome: TelemetryOutcome;
  status?: number | null;
  durationMs?: number | null;
  message?: string | null;
  clusterSignature?: string | null;
  effectClass?: string | null;
  authority?: string | null;
  receiptRef?: string | null;
  attributes?: Record<string, unknown>;
}

export interface OperationalTelemetryReceipt {
  schemaVersion: typeof OPERATIONAL_TELEMETRY_SCHEMA;
  eventId: string;
  traceId: string;
  spanId: string;
  fingerprint: string;
  persisted: boolean;
  persistence: 'supabase' | 'console-only' | 'failed';
}

const TRACEPARENT_RX = /^[0-9a-f]{2}-([0-9a-f]{32})-([0-9a-f]{16})-[0-9a-f]{2}$/i;
const TRACE_ID_RX = /^[0-9a-f]{32}$/i;
const KIND_RX = /^[a-z][a-z0-9._-]{2,63}$/;
const SECRET_VALUE_RX = /(?:bearer\s+[a-z0-9._~+\/-]+=*|eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,})/gi;
const EMAIL_RX = /[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}/gi;
const QUERY_SECRET_RX = /([?&](?:token|key|secret|api[_-]?key|password|auth)=)([^&\s]+)/gi;
const SENSITIVE_KEY_RX = /^(?:authorization|cookie|set-cookie|token|secret|password|api[_-]?key|access[_-]?token|refresh[_-]?token|headers?|raw|body|prompt|text|content|email|ip|address|user)$/i;
const MAX_STRING = 2_048;
const MAX_KEYS = 64;
const MAX_ARRAY = 32;
const MAX_DEPTH = 4;
const PERSIST_TIMEOUT_MS = 1_200;

let serviceClient: SupabaseClient | null | undefined;

function first(value: string | string[] | undefined): string | null {
  return (Array.isArray(value) ? value[0] : value)?.trim() || null;
}

function randomHex(bytes: number): string {
  return randomBytes(bytes).toString('hex');
}

function safePath(url: string | undefined): string {
  const pathname = (url ?? '/').split('?', 1)[0] || '/';
  return pathname.slice(0, 256);
}

function safeKind(kind: string): string {
  return KIND_RX.test(kind) ? kind : 'telemetry.invalid_kind';
}

export function createServerTrace(req: Pick<NextApiRequest, 'headers' | 'url' | 'method'>): ServerTraceContext {
  const traceparent = first(req.headers.traceparent);
  const parentMatch = traceparent ? TRACEPARENT_RX.exec(traceparent) : null;
  const direct = first(req.headers['x-apocky-trace-id']);
  const traceId = parentMatch?.[1]?.toLowerCase()
    ?? (direct && TRACE_ID_RX.test(direct) ? direct.toLowerCase() : randomHex(16));
  return {
    traceId,
    spanId: randomHex(8),
    parentSpanId: parentMatch?.[2]?.toLowerCase() ?? null,
    route: safePath(req.url),
    method: (req.method ?? 'UNKNOWN').slice(0, 16).toUpperCase(),
  };
}

export function traceparentFor(trace: ServerTraceContext): string {
  return `00-${trace.traceId}-${trace.spanId}-01`;
}

function redactString(value: string): string {
  return value
    .slice(0, MAX_STRING)
    .replace(SECRET_VALUE_RX, '«secret»')
    .replace(EMAIL_RX, '«email»')
    .replace(QUERY_SECRET_RX, '$1«redacted»');
}

function sanitize(value: unknown, depth = 0): unknown {
  if (depth > MAX_DEPTH) return '«depth-limit»';
  if (typeof value === 'string') return redactString(value);
  if (typeof value === 'number') return Number.isFinite(value) ? value : null;
  if (typeof value === 'boolean' || value === null) return value;
  if (Array.isArray(value)) return value.slice(0, MAX_ARRAY).map((item) => sanitize(item, depth + 1));
  if (typeof value === 'object') {
    const result: Record<string, unknown> = {};
    for (const [key, item] of Object.entries(value as Record<string, unknown>).slice(0, MAX_KEYS)) {
      result[key.slice(0, 64)] = SENSITIVE_KEY_RX.test(key) ? '«redacted-field»' : sanitize(item, depth + 1);
    }
    return result;
  }
  return String(value).slice(0, 128);
}

export function sanitizeTelemetryAttributes(value: Record<string, unknown> | undefined): Record<string, unknown> {
  return (sanitize(value ?? {}) as Record<string, unknown>) ?? {};
}

function fingerprint(input: OperationalTelemetryInput): string {
  return createHash('sha256')
    .update([
      safeKind(input.kind),
      input.source,
      input.outcome,
      input.status ?? '',
      input.clusterSignature ?? '',
      input.message ? redactString(input.message) : '',
    ].join('|'))
    .digest('hex')
    .slice(0, 16);
}

function getServiceClient(): SupabaseClient | null {
  if (serviceClient !== undefined) return serviceClient;
  const url = process.env.APOCKY_HUB_SUPABASE_URL ?? process.env.NEXT_PUBLIC_SUPABASE_URL;
  const key = process.env.APOCKY_HUB_SUPABASE_SERVICE_ROLE_KEY ?? process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!url || !key) {
    serviceClient = null;
    return null;
  }
  serviceClient = createClient(url, key, { auth: { persistSession: false, autoRefreshToken: false } });
  return serviceClient;
}

async function persist(row: Record<string, unknown>): Promise<'supabase' | 'console-only' | 'failed'> {
  const client = getServiceClient();
  if (!client) return 'console-only';
  try {
    const write = client.from('akashic_events').insert(row).then(({ error }) => error ? 'failed' as const : 'supabase' as const);
    return await Promise.race([
      write,
      new Promise<'failed'>((resolve) => setTimeout(() => resolve('failed'), PERSIST_TIMEOUT_MS)),
    ]);
  } catch {
    return 'failed';
  }
}

export async function emitOperationalTelemetry(input: OperationalTelemetryInput): Promise<OperationalTelemetryReceipt> {
  const eventId = randomHex(16);
  const ts = new Date().toISOString();
  const eventKind = safeKind(input.kind);
  const eventFingerprint = fingerprint(input);
  const payload = {
    schema_version: OPERATIONAL_TELEMETRY_SCHEMA,
    event_id: eventId,
    trace_id: input.trace.traceId,
    span_id: input.trace.spanId,
    parent_span_id: input.trace.parentSpanId,
    source: redactString(input.source),
    plane: input.plane,
    severity: input.severity,
    outcome: input.outcome,
    route: input.trace.route,
    method: input.trace.method,
    status: input.status ?? null,
    duration_ms: input.durationMs ?? null,
    message: input.message ? redactString(input.message) : null,
    fingerprint: eventFingerprint,
    effect_class: input.effectClass ? redactString(input.effectClass) : null,
    authority: input.authority ? redactString(input.authority) : null,
    receipt_ref: input.receiptRef ? redactString(input.receiptRef) : null,
    privacy_tier: 'operational_metadata',
    redacted: true,
    deployment_id: process.env.VERCEL_DEPLOYMENT_ID ?? process.env.VERCEL_URL ?? 'local',
    commit_sha: process.env.VERCEL_GIT_COMMIT_SHA ?? process.env.GIT_COMMIT_SHA ?? 'unknown',
    attributes: sanitizeTelemetryAttributes(input.attributes),
  };
  const persistence = await persist({
    cell_id: eventId,
    ts_iso: ts,
    sigma_mask: 2,
    cap_witness_hash: null,
    dpl_id: payload.deployment_id,
    commit_sha: payload.commit_sha,
    build_time: process.env.NEXT_PUBLIC_BUILD_TIME ?? 'unknown',
    kind: eventKind,
    payload,
    session_id: `server-${input.trace.traceId.slice(0, 32)}`,
    user_cap_hash: null,
    cluster_signature: input.clusterSignature && input.clusterSignature.length >= 8
      ? input.clusterSignature.slice(0, 64)
      : null,
  });

  const log = JSON.stringify({
    ...payload,
    ts,
    kind: eventKind,
    persistence,
  });
  if (input.severity === 'error' || input.severity === 'critical') console.error(log);
  else if (input.severity === 'warn') console.warn(log);
  else console.log(log);

  return {
    schemaVersion: OPERATIONAL_TELEMETRY_SCHEMA,
    eventId,
    traceId: input.trace.traceId,
    spanId: input.trace.spanId,
    fingerprint: eventFingerprint,
    persisted: persistence === 'supabase',
    persistence,
  };
}

export function _resetOperationalTelemetryForTests(): void {
  serviceClient = undefined;
}
