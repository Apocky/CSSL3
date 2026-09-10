import type { NextPage } from 'next';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import AdminLayout from '../../components/AdminLayout';
import { authFetch } from '../../lib/browser-auth';

const REFRESH_INTERVAL_MS = 5_000;
const MAX_RESPONSE_CHARACTERS = 2_000_000;
const MAX_ROWS = 1_000;
const MAX_PAYLOAD_CHARACTERS = 16_384;

type TelemetrySummary = Record<string, unknown>;

export interface TelemetryRow {
  id: string;
  ts: string;
  eventId: string | null;
  traceId: string | null;
  spanId: string | null;
  parentSpanId: string | null;
  source: string;
  plane: string;
  severity: string;
  kind: string;
  outcome: string;
  route: string | null;
  status: number | null;
  durationMs: number | null;
  message: string | null;
  clusterSignature: string | null;
  deploymentId: string | null;
  effectClass: string | null;
  authority: string | null;
  receiptRef: string | null;
  privacyTier: string | null;
  payload: unknown;
}

export interface TelemetryResponse {
  schema_version: string;
  observed_at: string;
  cursor: string | number | null;
  hasMore: boolean;
  rows: TelemetryRow[];
  summary: TelemetrySummary;
  creationLedger: CreationLedgerRow[];
}

export interface CreationLedgerRow {
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

export interface TelemetryFilters {
  severity: string;
  source: string;
  kind: string;
  trace: string;
  search: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function boundedString(value: unknown, maximum = 4_096): string | null {
  return typeof value === 'string' && value.trim() ? value.slice(0, maximum) : null;
}

function finiteNumber(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function summaryRecord(summary: TelemetrySummary, key: string): Record<string, unknown> {
  const value = summary[key];
  return isRecord(value) ? value : {};
}

function summaryCount(summary: Record<string, unknown>, key: string): number {
  return finiteNumber(summary[key]) ?? 0;
}

function normalizeRow(value: unknown): TelemetryRow | null {
  if (!isRecord(value)) return null;
  const ts = boundedString(value.ts, 128);
  const id = boundedString(value.id, 512) ?? boundedString(value.eventId, 512);
  if (!ts || !id) return null;
  return {
    id,
    ts,
    eventId: boundedString(value.eventId, 512),
    traceId: boundedString(value.traceId, 512),
    spanId: boundedString(value.spanId, 512),
    parentSpanId: boundedString(value.parentSpanId, 512),
    source: boundedString(value.source, 256) ?? 'unknown',
    plane: boundedString(value.plane, 256) ?? 'unknown',
    severity: (boundedString(value.severity, 64) ?? 'INFO').toUpperCase(),
    kind: boundedString(value.kind, 256) ?? 'unknown',
    outcome: boundedString(value.outcome, 256) ?? 'unknown',
    route: boundedString(value.route, 2_048),
    status: finiteNumber(value.status),
    durationMs: finiteNumber(value.durationMs),
    message: boundedString(value.message, 8_192),
    clusterSignature: boundedString(value.clusterSignature, 512),
    deploymentId: boundedString(value.deploymentId, 512),
    effectClass: boundedString(value.effectClass, 256),
    authority: boundedString(value.authority, 512),
    receiptRef: boundedString(value.receiptRef, 512),
    privacyTier: boundedString(value.privacyTier, 128),
    payload: value.payload ?? null,
  };
}

function normalizeCreationLedgerRow(value: unknown): CreationLedgerRow | null {
  if (!isRecord(value) || value.contentRetained !== false) return null;
  const id = boundedString(value.id, 512);
  const ts = boundedString(value.ts, 128);
  const recordDigest = boundedString(value.recordDigest, 64);
  const creationKind = boundedString(value.creationKind, 128);
  const actorRef = boundedString(value.actorRef, 512);
  const requestRef = boundedString(value.requestRef, 512);
  if (!id || !ts || !recordDigest || !creationKind || !actorRef || !requestRef) return null;
  return {
    id,
    ts,
    kind: boundedString(value.kind, 128) ?? 'unknown',
    outcome: boundedString(value.outcome, 128) ?? 'unknown',
    recordDigest,
    creationKind,
    origin: boundedString(value.origin, 64) ?? 'unknown',
    stage: boundedString(value.stage, 64) ?? 'unknown',
    channel: boundedString(value.channel, 64) ?? 'unknown',
    actorRef,
    requestRef,
    artifactRef: boundedString(value.artifactRef, 512),
    modelId: boundedString(value.modelId, 256),
    toolId: boundedString(value.toolId, 256),
    effectAuthority: boundedString(value.effectAuthority, 256),
    inputDigest: boundedString(value.inputDigest, 64),
    outputDigest: boundedString(value.outputDigest, 64),
    inputBytes: finiteNumber(value.inputBytes) ?? 0,
    outputBytes: finiteNumber(value.outputBytes) ?? 0,
    safetyDisposition: boundedString(value.safetyDisposition, 64) ?? 'unknown',
    safetySignals: Array.isArray(value.safetySignals)
      ? value.safetySignals
        .map((signal) => boundedString(signal, 128))
        .filter((signal): signal is string => signal !== null)
        .slice(0, 16)
      : [],
    contentRetained: false,
  };
}

export function normalizeTelemetryResponse(value: unknown): TelemetryResponse {
  if (!isRecord(value) || !Array.isArray(value.rows)) {
    throw new Error('telemetry_schema_invalid');
  }
  const schemaVersion = boundedString(value.schema_version, 128);
  const observedAt = boundedString(value.observed_at, 128);
  if (!schemaVersion || !observedAt) throw new Error('telemetry_schema_invalid');
  const rows = value.rows
    .slice(0, MAX_ROWS)
    .map(normalizeRow)
    .filter((row): row is TelemetryRow => row !== null);
  const cursor = typeof value.cursor === 'string' || typeof value.cursor === 'number'
    ? value.cursor
    : null;
  return {
    schema_version: schemaVersion,
    observed_at: observedAt,
    cursor,
    hasMore: value.has_more === true,
    rows,
    summary: isRecord(value.summary) ? value.summary : {},
    creationLedger: Array.isArray(value.creation_ledger)
      ? value.creation_ledger
        .slice(0, MAX_ROWS)
        .map(normalizeCreationLedgerRow)
        .filter((row): row is CreationLedgerRow => row !== null)
      : [],
  };
}

export function matchesTelemetryFilters(row: TelemetryRow, filters: TelemetryFilters): boolean {
  if (filters.severity && row.severity !== filters.severity) return false;
  if (filters.source && row.source !== filters.source) return false;
  if (filters.kind && row.kind !== filters.kind) return false;
  if (filters.trace && !(row.traceId ?? '').toLowerCase().includes(filters.trace.toLowerCase())) return false;
  if (filters.search) {
    const needle = filters.search.toLowerCase();
    const haystack = [row.message, row.route, row.receiptRef, row.effectClass, row.outcome, row.authority]
      .filter((part): part is string => typeof part === 'string')
      .join('\n')
      .toLowerCase();
    if (!haystack.includes(needle)) return false;
  }
  return true;
}

function boundedJson(value: unknown): string {
  try {
    const text = JSON.stringify(value, null, 2) ?? 'null';
    if (text.length <= MAX_PAYLOAD_CHARACTERS) return text;
    return `${text.slice(0, MAX_PAYLOAD_CHARACTERS)}\n… [client display truncated at ${MAX_PAYLOAD_CHARACTERS.toLocaleString()} characters]`;
  } catch {
    return '{\n  "error": "payload_not_serializable"\n}';
  }
}

function formatTimestamp(value: string): string {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
}

function formatDuration(value: number | null): string | null {
  if (value === null) return null;
  if (value < 1) return `${Math.round(value * 1_000)}µs`;
  if (value < 1_000) return `${value.toFixed(value < 10 ? 1 : 0)}ms`;
  return `${(value / 1_000).toFixed(2)}s`;
}

function compactIdentifier(value: string | null): string {
  if (!value) return 'none';
  return value.length > 22 ? `${value.slice(0, 10)}…${value.slice(-8)}` : value;
}

function severityTone(severity: string): string {
  if (severity === 'FATAL' || severity === 'ERROR') return 'critical';
  if (severity === 'WARN' || severity === 'WARNING') return 'warning';
  if (severity === 'DEBUG' || severity === 'TRACE') return 'quiet';
  return 'normal';
}

function uniqueSorted(rows: TelemetryRow[], field: 'severity' | 'source' | 'kind'): string[] {
  return Array.from(new Set(rows.map((row) => row[field]).filter(Boolean))).sort((a, b) => a.localeCompare(b));
}

const Logs: NextPage = () => {
  const [authorized, setAuthorized] = useState(false);
  const [data, setData] = useState<TelemetryResponse | null>(null);
  const [busy, setBusy] = useState(false);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [copied, setCopied] = useState<string | null>(null);
  const [filters, setFilters] = useState<TelemetryFilters>({
    severity: '',
    source: '',
    kind: '',
    trace: '',
    search: '',
  });
  const requestInFlightRef = useRef(false);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);

  const refresh = useCallback(async () => {
    if (!authorized || requestInFlightRef.current) return;
    requestInFlightRef.current = true;
    setBusy(true);
    try {
      const response = await authFetch('/api/admin/logs?limit=240', { cache: 'no-store' });
      const rawBody = await response.text();
      if (rawBody.length > MAX_RESPONSE_CHARACTERS) throw new Error('telemetry_response_too_large');
      let body: unknown;
      try {
        body = JSON.parse(rawBody) as unknown;
      } catch {
        throw new Error('telemetry_response_not_json');
      }
      if (!response.ok) throw new Error(`telemetry_request_${response.status}`);
      const normalized = normalizeTelemetryResponse(body);
      if (!mountedRef.current) return;
      setData((current) => {
        if (!current) return normalized;
        const rows = [...normalized.rows, ...current.rows]
          .filter((row, index, all) => all.findIndex((candidate) => candidate.id === row.id) === index)
          .slice(0, MAX_ROWS);
        const creationLedger = [...normalized.creationLedger, ...current.creationLedger]
          .filter((row, index, all) => all.findIndex((candidate) => candidate.recordDigest === row.recordDigest) === index)
          .slice(0, MAX_ROWS);
        return {
          ...normalized,
          cursor: current.cursor ?? normalized.cursor,
          hasMore: current.hasMore && rows.length < MAX_ROWS,
          rows,
          creationLedger,
        };
      });
      setError(null);
    } catch (refreshError) {
      if (!mountedRef.current) return;
      setError(refreshError instanceof Error ? refreshError.message : 'telemetry_request_failed');
    } finally {
      requestInFlightRef.current = false;
      if (mountedRef.current) setBusy(false);
    }
  }, [authorized]);

  const loadOlder = useCallback(async () => {
    if (!authorized || !data?.hasMore || data.cursor === null || loadingOlder) return;
    setLoadingOlder(true);
    try {
      const response = await authFetch(
        `/api/admin/logs?limit=240&before_id=${encodeURIComponent(String(data.cursor))}`,
        { cache: 'no-store' },
      );
      const rawBody = await response.text();
      if (!response.ok) throw new Error(`telemetry_history_${response.status}`);
      if (rawBody.length > MAX_RESPONSE_CHARACTERS) throw new Error('telemetry_response_too_large');
      const older = normalizeTelemetryResponse(JSON.parse(rawBody) as unknown);
      setData((current) => {
        if (!current) return older;
        const rows = [...current.rows, ...older.rows]
          .filter((row, index, all) => all.findIndex((candidate) => candidate.id === row.id) === index)
          .slice(0, MAX_ROWS);
        const creationLedger = [...current.creationLedger, ...older.creationLedger]
          .filter((row, index, all) => all.findIndex((candidate) => candidate.recordDigest === row.recordDigest) === index)
          .slice(0, MAX_ROWS);
        return {
          ...current,
          cursor: older.cursor,
          hasMore: older.hasMore && rows.length < MAX_ROWS,
          rows,
          creationLedger,
        };
      });
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'telemetry_history_failed');
    } finally {
      setLoadingOlder(false);
    }
  }, [authorized, data, loadingOlder]);

  useEffect(() => {
    if (!authorized) return;
    void refresh();
    if (!autoRefresh) return;
    const interval = window.setInterval(() => void refresh(), REFRESH_INTERVAL_MS);
    return () => window.clearInterval(interval);
  }, [authorized, autoRefresh, refresh]);

  const filteredRows = useMemo(
    () => (data?.rows ?? []).filter((row) => matchesTelemetryFilters(row, filters)),
    [data, filters],
  );
  const severities = useMemo(() => uniqueSorted(data?.rows ?? [], 'severity'), [data]);
  const sources = useMemo(() => uniqueSorted(data?.rows ?? [], 'source'), [data]);
  const kinds = useMemo(() => uniqueSorted(data?.rows ?? [], 'kind'), [data]);
  const summaryEntries = useMemo(
    () => Object.entries(data?.summary ?? {})
      .filter(([key]) => !['visitors', 'interactions', 'creations'].includes(key))
      .slice(0, 8),
    [data],
  );
  const visitorSummary = summaryRecord(data?.summary ?? {}, 'visitors');
  const interactionSummary = summaryRecord(data?.summary ?? {}, 'interactions');
  const creationSummary = summaryRecord(data?.summary ?? {}, 'creations');
  const reviewRequired = data?.creationLedger.filter((row) => row.safetyDisposition === 'review_required') ?? [];

  const copyValue = useCallback(async (label: string, value: string | null) => {
    if (!value) return;
    try {
      await navigator.clipboard.writeText(value);
      setCopied(label);
      window.setTimeout(() => setCopied((current) => current === label ? null : current), 1_600);
    } catch {
      setError('clipboard_permission_denied');
    }
  }, []);

  const downloadVisibleJson = useCallback(() => {
    if (!data) return;
    const payload = JSON.stringify({
      schema_version: data.schema_version,
      observed_at: data.observed_at,
      exported_at: new Date().toISOString(),
      cursor: data.cursor,
      filters,
      summary: data.summary,
      rows: filteredRows,
    }, null, 2);
    const url = URL.createObjectURL(new Blob([payload], { type: 'application/json' }));
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `apocky-telemetry-${new Date().toISOString().replace(/[:.]/g, '-')}.json`;
    anchor.click();
    URL.revokeObjectURL(url);
  }, [data, filteredRows, filters]);

  const clearFilters = useCallback(() => {
    setFilters({ severity: '', source: '', kind: '', trace: '', search: '' });
  }, []);

  return (
    <AdminLayout
      title="Telemetry"
      hideHeading
      onAdminCheck={(check) => setAuthorized(check.authorized)}
    >
      <main className="telemetry-console">
        <header className="console-header">
          <div>
            <p className="eyebrow">OWNER OBSERVATORY · STRUCTURED EVENTS / EFFECTS</p>
            <h1>Telemetry nerve center</h1>
            <p className="intro">Correlated browser, edge, runtime, tool, and effect evidence. Payload display is bounded; authorization remains server-enforced.</p>
          </div>
          <div className="header-actions">
            <label className="auto-toggle">
              <input
                type="checkbox"
                checked={autoRefresh}
                onChange={(event) => setAutoRefresh(event.target.checked)}
              />
              <span aria-hidden="true" />
              auto · 5s
            </label>
            <button type="button" onClick={() => void refresh()} disabled={busy || !authorized}>
              {busy ? 'Reading…' : 'Refresh now'}
            </button>
            <button type="button" onClick={() => void loadOlder()} disabled={!data?.hasMore || loadingOlder}>
              {loadingOlder ? 'Loading…' : 'Load older'}
            </button>
            <button type="button" onClick={downloadVisibleJson} disabled={!data}>Download visible JSON</button>
          </div>
        </header>

        <section className="signal-strip" aria-label="Telemetry state">
          <div><span>stream</span><strong data-live={error ? 'false' : 'true'}>{error ? 'degraded' : data ? 'observed' : 'resolving'}</strong></div>
          <div><span>visible / received</span><strong>{filteredRows.length} / {data?.rows.length ?? 0}</strong></div>
          <div><span>cursor</span><strong title={String(data?.cursor ?? 'none')}>{compactIdentifier(data?.cursor === null || data?.cursor === undefined ? null : String(data.cursor))}</strong></div>
          <div><span>observed</span><strong>{data ? formatTimestamp(data.observed_at) : 'not yet'}</strong></div>
          <div><span>schema</span><strong>{data?.schema_version ?? 'unverified'}</strong></div>
        </section>

        {error && (
          <div className="error-banner" role="alert">
            <strong>Telemetry refresh degraded</strong>
            <span>{error}. The last valid snapshot remains visible.</span>
            <button type="button" onClick={() => void refresh()} disabled={busy}>Retry</button>
          </div>
        )}

        {data && (
          <>
            <section className="observatory-grid" aria-label="Apocky observatory overview">
              <article>
                <span>Consented visitors</span>
                <strong>{summaryCount(visitorSummary, 'consented_sessions')}</strong>
                <small>{summaryCount(visitorSummary, 'page_views')} page views · ephemeral sessions</small>
              </article>
              <article>
                <span>Runtime interactions</span>
                <strong>{summaryCount(interactionSummary, 'completed')}</strong>
                <small>{summaryCount(interactionSummary, 'started')} started · {summaryCount(interactionSummary, 'failed')} failed</small>
              </article>
              <article data-alert={reviewRequired.length > 0 ? 'true' : 'false'}>
                <span>Creation ledger</span>
                <strong>{summaryCount(creationSummary, 'results')}</strong>
                <small>{summaryCount(creationSummary, 'attempts')} attempts · {reviewRequired.length} need review</small>
              </article>
              <article>
                <span>Visible event health</span>
                <strong>{data.rows.length}</strong>
                <small>{String(data.summary.unique_traces ?? 0)} traces · newest bounded window</small>
              </article>
            </section>
            <p className="privacy-scope">
              Visitor totals cover only people who explicitly enabled first-party diagnostics, use tab-scoped identifiers,
              and exclude private content. Creation records retain digests and risk signals—not prompts, replies, media, IPs, or credentials.
            </p>
          </>
        )}

        <section className="filter-deck" aria-label="Telemetry filters">
          <label>
            <span>Severity</span>
            <select value={filters.severity} onChange={(event) => setFilters((current) => ({ ...current, severity: event.target.value }))}>
              <option value="">All severities</option>
              {severities.map((value) => <option key={value} value={value}>{value}</option>)}
            </select>
          </label>
          <label>
            <span>Source</span>
            <select value={filters.source} onChange={(event) => setFilters((current) => ({ ...current, source: event.target.value }))}>
              <option value="">All sources</option>
              {sources.map((value) => <option key={value} value={value}>{value}</option>)}
            </select>
          </label>
          <label>
            <span>Kind</span>
            <select value={filters.kind} onChange={(event) => setFilters((current) => ({ ...current, kind: event.target.value }))}>
              <option value="">All event kinds</option>
              {kinds.map((value) => <option key={value} value={value}>{value}</option>)}
            </select>
          </label>
          <label>
            <span>Trace contains</span>
            <input value={filters.trace} onChange={(event) => setFilters((current) => ({ ...current, trace: event.target.value }))} placeholder="trace id…" />
          </label>
          <label className="search-field">
            <span>Message / route / receipt / effect</span>
            <input value={filters.search} onChange={(event) => setFilters((current) => ({ ...current, search: event.target.value }))} placeholder="search current snapshot…" />
          </label>
          <button type="button" className="clear-button" onClick={clearFilters}>Clear filters</button>
        </section>

        {summaryEntries.length > 0 && (
          <section className="summary-grid" aria-label="Server telemetry summary">
            {summaryEntries.map(([label, value]) => (
              <div key={label}><span>{label.replace(/_/g, ' ')}</span><strong>{typeof value === 'object' ? boundedJson(value) : String(value)}</strong></div>
            ))}
          </section>
        )}

        {data && (
          <section className="creation-ledger" aria-label="Creation ledger">
            <header>
              <div>
                <span className="pulse" aria-hidden="true" />
                <div><h2>Creation ledger</h2><small>Prompted and system-originated generative records · newest first</small></div>
              </div>
              <strong data-alert={reviewRequired.length > 0 ? 'true' : 'false'}>
                {reviewRequired.length > 0 ? `${reviewRequired.length} review required` : 'No risk signals'}
              </strong>
            </header>
            {data.creationLedger.length === 0 ? (
              <div className="ledger-empty">
                <strong>No creation records in this event window</strong>
                <span>The next wired runtime response, background job, visual observation, code effect, or SMS reply will appear here.</span>
              </div>
            ) : (
              <div className="ledger-list">
                {data.creationLedger.slice(0, 80).map((row) => (
                  <article key={`${row.id}:${row.recordDigest}`} data-review={row.safetyDisposition === 'review_required' ? 'true' : 'false'}>
                    <header>
                      <div>
                        <strong>{row.creationKind}</strong>
                        <span>{row.stage}</span><span>{row.origin}</span><span>{row.channel}</span>
                      </div>
                      <time dateTime={row.ts}>{formatTimestamp(row.ts)}</time>
                    </header>
                    <div className="ledger-disposition">
                      <span>{row.safetyDisposition === 'review_required' ? 'Review required' : 'No first-party risk signal'}</span>
                      {row.safetySignals.map((signal) => <code key={signal}>{signal}</code>)}
                    </div>
                    <dl>
                      <div><dt>Record</dt><dd title={row.recordDigest}>{compactIdentifier(row.recordDigest)}</dd></div>
                      <div><dt>Actor</dt><dd title={row.actorRef}>{compactIdentifier(row.actorRef)}</dd></div>
                      <div><dt>Request</dt><dd title={row.requestRef}>{compactIdentifier(row.requestRef)}</dd></div>
                      <div><dt>Artifact / receipt</dt><dd title={row.artifactRef ?? 'none'}>{compactIdentifier(row.artifactRef)}</dd></div>
                      <div><dt>Input</dt><dd>{row.inputDigest ? `${compactIdentifier(row.inputDigest)} · ${row.inputBytes} B` : 'not recorded'}</dd></div>
                      <div><dt>Output</dt><dd>{row.outputDigest ? `${compactIdentifier(row.outputDigest)} · ${row.outputBytes} B` : 'not recorded'}</dd></div>
                    </dl>
                    <div className="ledger-actions">
                      <span>Content retained: no</span>
                      <button type="button" onClick={() => void copyValue(`ledger:${row.id}`, row.recordDigest)}>
                        {copied === `ledger:${row.id}` ? 'Digest copied' : 'Copy record digest'}
                      </button>
                    </div>
                  </article>
                ))}
              </div>
            )}
            <p className="ledger-boundary">
              “No risk signal” is not a claim of harmlessness. It means the bounded first-party scanner found no configured signal;
              flagged records require owner review through their correlated request and artifact receipts.
            </p>
          </section>
        )}

        <section className="timeline" aria-live="polite" aria-busy={busy}>
          <div className="timeline-heading">
            <div><span className="pulse" aria-hidden="true" /><h2>Recent event timeline</h2></div>
            <small>newest snapshot · expand a row for bounded structured evidence</small>
          </div>

          {!data && !error ? (
            <div className="empty-state"><strong>Resolving telemetry…</strong><span>The owner-authenticated event stream has not returned a valid snapshot yet.</span></div>
          ) : filteredRows.length === 0 ? (
            <div className="empty-state"><strong>No matching events</strong><span>{data?.rows.length ? 'The current filters exclude every received row.' : 'The current observed snapshot is quiet.'}</span></div>
          ) : (
            <div className="event-list">
              {filteredRows.map((row) => {
                const duration = formatDuration(row.durationMs);
                const tone = severityTone(row.severity);
                return (
                  <article key={row.id} className="event-card" data-tone={tone}>
                    <div className="event-rail" aria-hidden="true"><span /></div>
                    <div className="event-main">
                      <header>
                        <div className="event-identity">
                          <span className="severity">{row.severity}</span>
                          <strong>{row.kind}</strong>
                          <span className="source">{row.source}</span>
                          <span>{row.plane}</span>
                        </div>
                        <time dateTime={row.ts}>{formatTimestamp(row.ts)}</time>
                      </header>
                      <p className="event-message">{row.message ?? `${row.kind} · ${row.outcome}`}</p>
                      <div className="event-facts">
                        <span>outcome <strong>{row.outcome}</strong></span>
                        {row.status !== null && <span>status <strong>{row.status}</strong></span>}
                        {duration && <span>duration <strong>{duration}</strong></span>}
                        {row.route && <span>route <strong>{row.route}</strong></span>}
                        {row.effectClass && <span>effect <strong>{row.effectClass}</strong></span>}
                        {row.privacyTier && <span>privacy <strong>{row.privacyTier}</strong></span>}
                      </div>
                      <div className="correlation-row">
                        <code title={row.traceId ?? 'No trace identifier'}>trace / {compactIdentifier(row.traceId)}</code>
                        <code title={row.receiptRef ?? 'No receipt reference'}>receipt / {compactIdentifier(row.receiptRef)}</code>
                        <div className="row-actions">
                          <button type="button" onClick={() => void copyValue(`trace:${row.id}`, row.traceId)} disabled={!row.traceId}>{copied === `trace:${row.id}` ? 'Trace copied' : 'Copy trace'}</button>
                          <button type="button" onClick={() => void copyValue(`cluster:${row.id}`, row.clusterSignature)} disabled={!row.clusterSignature}>{copied === `cluster:${row.id}` ? 'Fingerprint copied' : 'Copy fingerprint'}</button>
                        </div>
                      </div>
                      <details>
                        <summary>Inspect structured event</summary>
                        <dl>
                          <div><dt>event</dt><dd>{row.eventId ?? row.id}</dd></div>
                          <div><dt>span / parent</dt><dd>{row.spanId ?? 'none'} / {row.parentSpanId ?? 'none'}</dd></div>
                          <div><dt>deployment</dt><dd>{row.deploymentId ?? 'none'}</dd></div>
                          <div><dt>authority</dt><dd>{row.authority ?? 'none'}</dd></div>
                          <div><dt>cluster</dt><dd>{row.clusterSignature ?? 'none'}</dd></div>
                        </dl>
                        <pre>{boundedJson(row.payload)}</pre>
                      </details>
                    </div>
                  </article>
                );
              })}
            </div>
          )}
        </section>

        <p className="boundary-note">This console renders the server-sanitized event envelope. It does not grant effect authority, and it does not treat telemetry as proof of model reasoning.</p>

        <style jsx>{`
          .telemetry-console { --line:#29263a; --muted:#8d8ba0; --violet:#b8a5ff; --cyan:#76e6ee; --rose:#ff7995; display:grid; gap:16px; max-width:1540px; margin:0 auto; color:#e8e6f1; }
          .console-header { position:relative; display:flex; justify-content:space-between; align-items:flex-end; gap:24px; overflow:hidden; padding:24px; border:1px solid rgba(184,165,255,.24); border-radius:20px; background:radial-gradient(circle at 86% -20%,rgba(118,230,238,.13),transparent 40%),radial-gradient(circle at 12% 0,rgba(176,103,255,.16),transparent 38%),linear-gradient(135deg,rgba(19,17,31,.96),rgba(10,10,17,.96)); box-shadow:0 22px 70px rgba(0,0,0,.3),inset 0 1px rgba(255,255,255,.04); }
          .console-header::after { content:''; position:absolute; inset:0; pointer-events:none; opacity:.18; background-image:linear-gradient(rgba(255,255,255,.05) 1px,transparent 1px),linear-gradient(90deg,rgba(255,255,255,.05) 1px,transparent 1px); background-size:38px 38px; mask-image:linear-gradient(90deg,black,transparent 75%); }
          .console-header > * { position:relative; z-index:1; }
          .eyebrow { margin:0 0 8px; color:var(--cyan); font:700 .68rem/1 ui-monospace,monospace; letter-spacing:.14em; }
          h1 { margin:0; font-size:clamp(1.75rem,4vw,3.15rem); line-height:1; letter-spacing:-.04em; }
          .intro { max-width:760px; margin:12px 0 0; color:#aaa8bb; font-size:.82rem; }
          .header-actions { display:flex; flex-wrap:wrap; justify-content:flex-end; gap:8px; }
          button,.auto-toggle { min-height:44px; border:1px solid #3a3651; border-radius:999px; color:#e8e6f1; background:rgba(20,18,31,.82); cursor:pointer; }
          button { padding:0 14px; }
          button:hover:not(:disabled) { border-color:var(--violet); background:rgba(184,165,255,.1); }
          button:focus-visible,input:focus-visible,select:focus-visible,summary:focus-visible { outline:2px solid var(--cyan); outline-offset:3px; }
          button:disabled { opacity:.38; cursor:not-allowed; }
          .auto-toggle { display:flex; align-items:center; gap:8px; padding:0 13px; color:#aaa8bb; font-size:.72rem; }
          .auto-toggle input { position:absolute; opacity:0; pointer-events:none; }
          .auto-toggle span { width:22px; height:12px; border-radius:999px; background:#383449; box-shadow:inset 0 0 0 1px #4b465f; }
          .auto-toggle span::after { content:''; display:block; width:8px; height:8px; margin:2px; border-radius:50%; background:#89859d; transition:transform .18s ease,background .18s ease; }
          .auto-toggle input:checked + span::after { transform:translateX(10px); background:var(--cyan); box-shadow:0 0 10px rgba(118,230,238,.7); }
          .auto-toggle:focus-within { outline:2px solid var(--cyan); outline-offset:3px; }
          .signal-strip { display:grid; grid-template-columns:repeat(5,minmax(0,1fr)); border:1px solid var(--line); border-radius:14px; overflow:hidden; background:rgba(12,11,19,.78); }
          .signal-strip div { min-width:0; padding:12px 14px; border-right:1px solid var(--line); }
          .signal-strip div:last-child { border:0; }
          .signal-strip span,.filter-deck label > span,.summary-grid span { display:block; margin-bottom:5px; color:#6f6c82; font:700 .58rem/1 ui-monospace,monospace; text-transform:uppercase; letter-spacing:.1em; }
          .signal-strip strong { display:block; overflow:hidden; color:#d8d5e2; font-size:.72rem; text-overflow:ellipsis; white-space:nowrap; }
          .signal-strip strong[data-live=true] { color:#79e7c2; }
          .signal-strip strong[data-live=false] { color:#ffb968; }
          .error-banner { display:flex; align-items:center; gap:12px; padding:12px 14px; border:1px solid rgba(255,185,104,.4); border-radius:12px; color:#ffd0a0; background:rgba(90,52,19,.22); font-size:.76rem; }
          .error-banner span { flex:1; color:#c3a98e; }
          .error-banner button { min-height:36px; }
          .observatory-grid { display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); gap:10px; }
          .observatory-grid article { min-width:0; padding:16px; border:1px solid var(--line); border-radius:14px; background:linear-gradient(145deg,rgba(18,17,28,.96),rgba(10,10,16,.96)); }
          .observatory-grid article[data-alert=true] { border-color:rgba(255,185,104,.48); background:linear-gradient(145deg,rgba(64,42,18,.42),rgba(13,11,17,.96)); }
          .observatory-grid span { display:block; color:#777386; font:700 .59rem/1 ui-monospace,monospace; text-transform:uppercase; letter-spacing:.1em; }
          .observatory-grid strong { display:block; margin:10px 0 6px; color:#f0edf6; font-size:1.8rem; line-height:1; }
          .observatory-grid small { display:block; color:#918da0; font-size:.66rem; line-height:1.45; }
          .privacy-scope { margin:-2px 2px 0; color:#706c7d; font-size:.65rem; line-height:1.5; }
          .filter-deck { display:grid; grid-template-columns:minmax(130px,.7fr) minmax(160px,1fr) minmax(170px,1fr) minmax(180px,1fr) minmax(220px,1.5fr) auto; gap:10px; align-items:end; padding:14px; border:1px solid var(--line); border-radius:14px; background:rgba(14,13,22,.84); }
          .filter-deck label { min-width:0; }
          input,select { width:100%; min-height:44px; padding:0 11px; border:1px solid #312e43; border-radius:9px; color:#e4e1ed; background:#0c0b13; font-size:.74rem; }
          select { color-scheme:dark; }
          .clear-button { border-radius:9px; white-space:nowrap; }
          .summary-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(140px,1fr)); gap:1px; overflow:hidden; border:1px solid var(--line); border-radius:12px; background:var(--line); }
          .summary-grid div { min-width:0; padding:11px 13px; background:#0d0c14; }
          .summary-grid strong { display:block; max-height:72px; overflow:auto; color:#d0cddd; font-size:.7rem; white-space:pre-wrap; overflow-wrap:anywhere; }
          .creation-ledger { overflow:hidden; border:1px solid rgba(184,165,255,.26); border-radius:18px; background:linear-gradient(180deg,rgba(17,15,27,.98),rgba(9,8,14,.98)); }
          .creation-ledger > header { display:flex; align-items:center; justify-content:space-between; gap:16px; padding:15px 18px; border-bottom:1px solid var(--line); }
          .creation-ledger > header > div { display:flex; align-items:center; gap:10px; }
          .creation-ledger h2 { margin:0; font-size:.78rem; text-transform:uppercase; letter-spacing:.09em; }
          .creation-ledger header small { display:block; margin-top:4px; color:#777386; font-size:.63rem; }
          .creation-ledger > header > strong { padding:7px 10px; border:1px solid rgba(123,233,196,.25); border-radius:999px; color:#7be9c4; font-size:.65rem; }
          .creation-ledger > header > strong[data-alert=true] { border-color:rgba(255,185,104,.4); color:#ffc783; background:rgba(255,185,104,.06); }
          .ledger-list { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:1px; background:#242130; }
          .ledger-list > article { min-width:0; padding:15px 17px; background:#0d0c14; }
          .ledger-list > article[data-review=true] { background:linear-gradient(145deg,rgba(62,39,17,.34),#0d0c14 62%); }
          .ledger-list article > header { display:flex; align-items:flex-start; justify-content:space-between; gap:12px; }
          .ledger-list article > header > div { display:flex; min-width:0; flex-wrap:wrap; align-items:center; gap:6px; }
          .ledger-list article > header strong { color:#dedbe7; font-size:.72rem; overflow-wrap:anywhere; }
          .ledger-list article > header span { padding:3px 6px; border:1px solid #302d40; border-radius:5px; color:#888497; font-size:.56rem; text-transform:uppercase; letter-spacing:.06em; }
          .ledger-list time { padding-top:3px; white-space:nowrap; }
          .ledger-disposition { display:flex; flex-wrap:wrap; gap:6px; margin:12px 0; }
          .ledger-disposition span,.ledger-disposition code { padding:4px 7px; border:1px solid rgba(118,230,238,.2); border-radius:999px; color:#8edbe1; background:rgba(118,230,238,.04); font-size:.58rem; }
          [data-review=true] .ledger-disposition span,[data-review=true] .ledger-disposition code { border-color:rgba(255,185,104,.35); color:#ffc783; background:rgba(255,185,104,.06); }
          .creation-ledger dl { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:7px; margin:0; }
          .creation-ledger dl div { min-width:0; padding:8px; border:1px solid #292637; border-radius:7px; background:#09080f; }
          .creation-ledger dd { white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
          .ledger-actions { display:flex; align-items:center; justify-content:space-between; gap:10px; margin-top:10px; color:#6f6b7c; font-size:.6rem; }
          .ledger-actions button { min-height:32px; padding:0 10px; font-size:.6rem; }
          .ledger-empty { display:grid; place-items:center; min-height:150px; padding:24px; text-align:center; }
          .ledger-empty span { max-width:640px; margin-top:6px; color:#777486; font-size:.69rem; }
          .ledger-boundary { margin:0; padding:11px 16px; border-top:1px solid var(--line); color:#716d7f; background:#0a0910; font-size:.62rem; line-height:1.5; }
          .timeline { overflow:hidden; border:1px solid var(--line); border-radius:18px; background:linear-gradient(180deg,rgba(15,14,23,.96),rgba(8,8,13,.96)); }
          .timeline-heading { display:flex; align-items:center; justify-content:space-between; gap:16px; padding:15px 18px; border-bottom:1px solid var(--line); }
          .timeline-heading > div { display:flex; align-items:center; gap:9px; }
          .timeline-heading h2 { margin:0; font-size:.76rem; text-transform:uppercase; letter-spacing:.09em; }
          .timeline-heading small { color:#747185; font-size:.65rem; }
          .pulse { width:7px; height:7px; border-radius:50%; background:#7be9c4; box-shadow:0 0 12px rgba(123,233,196,.8); }
          .event-list { display:grid; }
          .event-card { position:relative; display:grid; grid-template-columns:18px minmax(0,1fr); border-bottom:1px solid #211f2c; }
          .event-card:last-child { border-bottom:0; }
          .event-card:hover { background:rgba(184,165,255,.025); }
          .event-rail { display:flex; justify-content:center; padding-top:20px; }
          .event-rail::after { content:''; position:absolute; top:28px; bottom:-28px; width:1px; background:#2c293b; }
          .event-card:last-child .event-rail::after { display:none; }
          .event-rail span { position:relative; z-index:1; width:7px; height:7px; border:1px solid var(--violet); border-radius:50%; background:#13111d; box-shadow:0 0 8px rgba(184,165,255,.4); }
          .event-card[data-tone=critical] .event-rail span { border-color:var(--rose); box-shadow:0 0 11px rgba(255,121,149,.65); }
          .event-card[data-tone=warning] .event-rail span { border-color:#ffb968; box-shadow:0 0 10px rgba(255,185,104,.55); }
          .event-card[data-tone=quiet] { opacity:.72; }
          .event-main { min-width:0; padding:16px 18px 16px 4px; }
          .event-main > header { display:flex; justify-content:space-between; gap:16px; }
          .event-identity { display:flex; min-width:0; align-items:center; flex-wrap:wrap; gap:7px; }
          .event-identity > span,.event-identity > strong { padding:4px 7px; border:1px solid #302d40; border-radius:6px; color:#9d9aae; font:650 .62rem/1 ui-monospace,monospace; }
          .event-identity strong { color:#e4e1ed; }
          .event-identity .severity { color:#9fd8ff; border-color:rgba(118,230,238,.24); background:rgba(118,230,238,.05); }
          .event-card[data-tone=critical] .severity { color:#ff93a9; border-color:rgba(255,121,149,.35); background:rgba(255,121,149,.07); }
          .event-card[data-tone=warning] .severity { color:#ffc783; border-color:rgba(255,185,104,.3); }
          .event-identity .source { color:var(--violet); }
          time { flex:none; color:#777488; font-size:.64rem; }
          .event-message { margin:10px 0 8px; color:#cbc8d6; font-size:.78rem; overflow-wrap:anywhere; }
          .event-facts { display:flex; flex-wrap:wrap; gap:6px 14px; color:#716e81; font-size:.65rem; }
          .event-facts strong { color:#aaa7b8; font-weight:600; overflow-wrap:anywhere; }
          .correlation-row { display:flex; align-items:center; gap:8px; flex-wrap:wrap; margin-top:12px; }
          .correlation-row code { padding:5px 7px; border:1px solid #292638; border-radius:6px; color:#858296; background:#0a0910; font-size:.62rem; }
          .row-actions { display:flex; gap:6px; margin-left:auto; }
          .row-actions button { min-height:32px; padding:0 9px; font-size:.62rem; }
          details { margin-top:12px; border-top:1px dashed #2c2939; }
          summary { width:max-content; max-width:100%; padding:11px 0 0; color:#9d90d6; cursor:pointer; font-size:.68rem; }
          details dl { display:grid; grid-template-columns:repeat(auto-fit,minmax(180px,1fr)); gap:8px; margin:12px 0; }
          details dl div { min-width:0; padding:8px; border:1px solid #292637; border-radius:7px; background:#0a0910; }
          dt { color:#696678; font-size:.58rem; text-transform:uppercase; letter-spacing:.08em; }
          dd { margin:4px 0 0; color:#aaa7b7; font-size:.62rem; overflow-wrap:anywhere; }
          pre { max-height:420px; margin:0; padding:13px; overflow:auto; border:1px solid #292637; border-radius:8px; color:#a8a4b6; background:#08070d; font:500 .65rem/1.55 ui-monospace,monospace; white-space:pre-wrap; overflow-wrap:anywhere; }
          .empty-state { display:grid; place-items:center; min-height:240px; padding:30px; text-align:center; }
          .empty-state strong { color:#bcb8ca; }
          .empty-state span { margin-top:6px; color:#777486; font-size:.72rem; }
          .boundary-note { margin:0; color:#646173; font-size:.66rem; text-align:center; }
          @media (max-width:1100px) { .console-header { align-items:flex-start; flex-direction:column; } .header-actions { justify-content:flex-start; } .filter-deck { grid-template-columns:repeat(3,minmax(0,1fr)); } .search-field { grid-column:span 2; } .signal-strip { grid-template-columns:repeat(3,minmax(0,1fr)); } .observatory-grid { grid-template-columns:repeat(2,minmax(0,1fr)); } .creation-ledger dl { grid-template-columns:repeat(2,minmax(0,1fr)); } }
          @media (max-width:700px) { .console-header { padding:19px 16px; border-radius:15px; } .header-actions { display:grid; width:100%; grid-template-columns:1fr 1fr; } .auto-toggle { grid-column:1/-1; } .filter-deck { grid-template-columns:1fr 1fr; } .search-field { grid-column:1/-1; } .clear-button { width:100%; } .signal-strip { grid-template-columns:1fr 1fr; } .signal-strip div { border-bottom:1px solid var(--line); } .creation-ledger > header { align-items:flex-start; flex-direction:column; } .ledger-list { grid-template-columns:1fr; } .timeline-heading { align-items:flex-start; flex-direction:column; } .event-main > header { flex-direction:column; gap:8px; } .correlation-row { align-items:stretch; } .correlation-row code { width:100%; } .row-actions { width:100%; margin-left:0; } .row-actions button { flex:1; } }
          @media (max-width:430px) { .header-actions { grid-template-columns:1fr; } .auto-toggle { grid-column:auto; } .filter-deck { grid-template-columns:1fr; } .search-field { grid-column:auto; } .signal-strip,.observatory-grid { grid-template-columns:1fr; } .signal-strip div { border-right:0; } .creation-ledger dl { grid-template-columns:1fr; } .ledger-actions { align-items:stretch; flex-direction:column; } .event-card { grid-template-columns:14px minmax(0,1fr); } .event-main { padding-right:12px; } }
          @media (prefers-reduced-motion:reduce) { * { transition:none !important; } }
        `}</style>
      </main>
    </AdminLayout>
  );
};

export default Logs;
