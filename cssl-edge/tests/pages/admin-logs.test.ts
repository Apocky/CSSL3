import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import {
  matchesTelemetryFilters,
  normalizeTelemetryResponse,
  type TelemetryFilters,
  type TelemetryRow,
} from '../../pages/admin/logs';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function sampleRow(overrides: Partial<TelemetryRow> = {}): TelemetryRow {
  return {
    id: 'row-1',
    ts: '2026-08-02T08:00:00.000Z',
    eventId: 'evt-1',
    traceId: 'trace-alpha',
    spanId: 'span-1',
    parentSpanId: null,
    source: 'edge',
    plane: 'effect',
    severity: 'ERROR',
    kind: 'effect.denied',
    outcome: 'denied',
    route: '/api/admin/apocv4/objective',
    status: 403,
    durationMs: 4.5,
    message: 'Effect was denied by the authority gate.',
    clusterSignature: 'cluster-7',
    deploymentId: 'deployment-9',
    effectClass: 'workspace_write',
    authority: 'deny_all',
    receiptRef: 'receipt-3',
    privacyTier: 'operational',
    payload: { reason: 'unknown_effect' },
    ...overrides,
  };
}

const normalized = normalizeTelemetryResponse({
  schema_version: 'apocky.telemetry.v1',
  observed_at: '2026-08-02T08:00:01.000Z',
  cursor: 17,
  rows: [
    { ...sampleRow(), severity: 'error' },
    null,
    { id: 'missing-ts' },
  ],
  summary: { errors: 1 },
});

assert(normalized.rows.length === 1, 'invalid rows are rejected');
const normalizedRow = normalized.rows[0];
assert(normalizedRow !== undefined, 'one normalized row is present');
assert(normalizedRow.severity === 'ERROR', 'severity is canonicalized');
assert(normalized.cursor === 17, 'cursor is preserved');
assert(normalized.summary.errors === 1, 'summary is preserved');

let invalidSchemaRejected = false;
try {
  normalizeTelemetryResponse({ rows: [] });
} catch (error) {
  invalidSchemaRejected = error instanceof Error && error.message === 'telemetry_schema_invalid';
}
assert(invalidSchemaRejected, 'missing envelope identity is rejected');

const baseFilters: TelemetryFilters = { severity: '', source: '', kind: '', trace: '', search: '' };
const row = sampleRow();
assert(matchesTelemetryFilters(row, baseFilters), 'empty filters preserve row');
assert(matchesTelemetryFilters(row, { ...baseFilters, severity: 'ERROR' }), 'severity matches exactly');
assert(!matchesTelemetryFilters(row, { ...baseFilters, severity: 'WARN' }), 'severity mismatch rejects row');
assert(matchesTelemetryFilters(row, { ...baseFilters, source: 'edge', kind: 'effect.denied' }), 'source and kind compose');
assert(matchesTelemetryFilters(row, { ...baseFilters, trace: 'ALPHA' }), 'trace matching is case-insensitive');
assert(matchesTelemetryFilters(row, { ...baseFilters, search: 'workspace_write' }), 'effect class is searchable');
assert(matchesTelemetryFilters(row, { ...baseFilters, search: 'receipt-3' }), 'receipt reference is searchable');
assert(!matchesTelemetryFilters(row, { ...baseFilters, search: 'raw prompt text' }), 'unrelated text rejects row');

const source = readFileSync(resolve(process.cwd(), 'pages/admin/logs.tsx'), 'utf8');
for (const token of [
  "authFetch('/api/admin/logs?limit=240'",
  'onAdminCheck={(check) => setAuthorized(check.authorized)}',
  'window.setInterval(() => void refresh(), REFRESH_INTERVAL_MS)',
  'Download visible JSON',
  'Copy trace',
  'Copy fingerprint',
  '<details>',
  'MAX_PAYLOAD_CHARACTERS',
]) {
  assert(source.includes(token), `owner telemetry console contract missing: ${token}`);
}
assert(!source.includes('dangerouslySetInnerHTML'), 'telemetry UI must not render raw HTML');

console.log('admin-logs.test : OK · schema, filters, owner polling, bounded inspection, copy, and export passed');
