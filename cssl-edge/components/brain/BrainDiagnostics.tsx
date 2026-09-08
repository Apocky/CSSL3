import { useEffect, useId, useRef, useState } from 'react';
import type { BrainObservation, ObservationView } from '@/lib/brain/observations';
import { authFetch } from '@/lib/browser-auth';
import { useSiteSession } from '@/components/hub/SiteSession';
import styles from './BrainDiagnostics.module.css';

const VIEWS: { value: ObservationView; label: string }[] = [
  { value: 'status', label: 'Status' }, { value: 'events', label: 'Activity' },
  { value: 'errors', label: 'Error details' }, { value: 'metrics', label: 'Timings' },
  { value: 'shards', label: 'Record integrity' }, { value: 'trace', label: 'Trace' },
];
const CODES = new Set(['OBSERVATION_METHOD_DENIED', 'OBSERVATION_ORIGIN_DENIED', 'OBSERVATION_REQUEST_INVALID',
  'OBSERVATION_AUTH_UNAVAILABLE', 'OBSERVATION_OWNER_REQUIRED', 'OBSERVATION_BRIDGE_UNCONFIGURED',
  'OBSERVATION_RESPONSE_UNVERIFIED', 'OBSERVATION_RESPONSE_TOO_LARGE', 'OBSERVATION_UPSTREAM_UNAVAILABLE', 'OBSERVATION_UNAVAILABLE']);
const UUID = /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/;
function object(value: unknown): value is Record<string, unknown> { return Boolean(value && typeof value === 'object' && !Array.isArray(value)); }
function text(value: unknown): string { return typeof value === 'string' ? value : ''; }
function count(value: unknown): string { return typeof value === 'number' && Number.isSafeInteger(value) ? value.toLocaleString() : 'Unavailable'; }
type Result = { account: string; observation?: BrainObservation; code?: string; trace?: string };
export default function BrainDiagnostics() {
  const { access, subjectKey } = useSiteSession();
  const [view, setView] = useState<ObservationView>('status');
  const [reference, setReference] = useState('');
  const [result, setResult] = useState<Result | null>(null);
  const [busy, setBusy] = useState(false);
  const helpId = useId();
  const controller = useRef<AbortController | null>(null);
  const current = useRef(subjectKey); current.current = subjectKey;
  useEffect(() => {
    controller.current?.abort(); setResult(null); setBusy(false); setReference('');
    return () => controller.current?.abort();
  }, [subjectKey, access]);
  if (access !== 'owner' || !subjectKey) return null;
  const active = result?.account === subjectKey ? result : null;
  const data = active?.observation?.data;
  const entries = data && Array.isArray(data.events) ? data.events : data && Array.isArray(data.occurrences) ? data.occurrences : [];
  const definition = data && object(data.definition) ? data.definition : null;
  async function refresh() {
    const account = subjectKey; if (!account) return;
    controller.current?.abort(); const abort = new AbortController(); controller.current = abort;
    const params = new URLSearchParams({ view });
    if (view === 'trace') params.set('trace_id', reference.trim());
    if (view === 'errors') params.set('error_code', reference.trim());
    setBusy(true); setResult(null);
    try {
      const response = await authFetch(`/api/brain/observe?${params.toString()}`, { cache: 'no-store', signal: abort.signal });
      const raw = await response.text();
      if (raw.length > 512 * 1024) throw new Error('bounded');
      const value: unknown = JSON.parse(raw);
      if (abort.signal.aborted || current.current !== account) return;
      if (!response.ok) {
        const code = object(value) && typeof value.code === 'string' && CODES.has(value.code) ? value.code : 'OBSERVATION_UNAVAILABLE';
        const trace = response.headers.get('x-apocky-trace-id') ?? '';
        setResult({ account, code, trace: UUID.test(trace) ? trace : undefined }); return;
      }
      if (!object(value) || value.schema_version !== 'apocky.brain.observation.v1' || value.view !== view
        || typeof value.observed_at !== 'string' || !Number.isFinite(Date.parse(value.observed_at))
        || typeof value.trace_id !== 'string' || !UUID.test(value.trace_id) || !object(value.data)) throw new Error('shape');
      setResult({ account, observation: value as unknown as BrainObservation });
    } catch {
      if (!abort.signal.aborted && current.current === account) setResult({ account, code: 'OBSERVATION_UNAVAILABLE' });
    } finally { if (!abort.signal.aborted && current.current === account) setBusy(false); }
  }
  function select(next: ObservationView) {
    controller.current?.abort(); setBusy(false); setView(next); setReference(''); setResult(null);
  }
  return <details className={styles.panel}>
    <summary>Desktop diagnostics</summary>
    <div className={styles.body}>
      <p className={styles.intro} id={helpId}>Read-only desktop records. Updated when you refresh.</p>
      <div className={styles.controls}>
        <label>View<select value={view} aria-describedby={helpId} title="Choose which desktop records to inspect" onChange={event => select(event.target.value as ObservationView)}>
          {VIEWS.map(item => <option key={item.value} value={item.value}>{item.label}</option>)}
        </select></label>
        {(view === 'trace' || view === 'errors') && <label>{view === 'trace' ? 'Trace ID' : 'Error code'}
          <input value={reference} maxLength={192} spellCheck={false} aria-describedby={helpId} title={view === 'trace' ? 'Paste a trace ID from an activity record' : 'Paste an error code from an activity record'} onChange={event => { controller.current?.abort(); setBusy(false); setReference(event.target.value); setResult(null); }} placeholder={view === 'trace' ? 'tr-…' : 'APOC-…-v1'} />
        </label>}
        <button type="button" title="Read the selected records from your desktop" aria-describedby={helpId} onClick={() => { void refresh(); }} disabled={busy || ((view === 'trace' || view === 'errors') && !reference.trim())}>{busy ? 'Refreshing…' : 'Refresh'}</button>
      </div>
      <div aria-live="polite">
        {active?.code && <div className={styles.failure} role="status"><p>Desktop diagnostics are unavailable. Reconnect, then try Refresh.</p>
          <code>{active.code}</code>{active.trace && <small>Reference: {active.trace}</small>}</div>}
        {active?.observation && data && <>
          <p className={styles.updated}>Read from desktop at {new Date(active.observation.observed_at).toLocaleString()}.</p>
          {(view === 'status' || view === 'shards') && <dl className={styles.counts}>
            <div><dt>Recorded events</dt><dd>{count(data.event_count)}</dd></div>
            {view === 'status' ? <div><dt>Recorded errors</dt><dd>{count(data.error_count)}</dd></div>
              : <div><dt>Verified record groups</dt><dd>{count(data.shard_count)}</dd></div>}
          </dl>}
          {definition && <div className={styles.failure}><code>{text(definition.code)}</code><p>{text(definition.public_message)}</p><small>Retry: {text(definition.retryability).replaceAll('_', ' ')}</small></div>}
          {(view === 'events' || view === 'trace' || view === 'errors') && <div className={styles.events}>
            {entries.length === 0 && <p>No matching records were returned.</p>}
            {entries.map((entry, index) => object(entry) ? <article key={text(entry.event_id) || index}>
              <header><strong>{text(entry.component)} · {text(entry.operation)}</strong><span>{text(entry.state)}</span></header>
              <small>{text(entry.occurred_at)}</small>{typeof entry.error_code === 'string' && <code>{entry.error_code}</code>}
              <small>Trace: {text(entry.trace_id)}</small>
            </article> : null)}
            {data.has_more === true && <p>More records are available.</p>}
          </div>}
          {view === 'metrics' && <p className={styles.intro}>Stage timings are in the record below, in nanoseconds.</p>}
          <details className={styles.record}><summary title="Inspect the verified fields returned by the desktop">Detailed record</summary><pre>{JSON.stringify(data, null, 2)}</pre></details>
        </>}
      </div>
    </div>
  </details>;
}
