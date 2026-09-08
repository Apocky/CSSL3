// Owner-only V2 diagnostic console. Every panel reads a canonical V2 proxy;
// no predecessor conversation, telemetry, or model surface is used.

import type { NextPage } from 'next';
import { useCallback, useEffect, useRef, useState } from 'react';

import AdminLayout from '../../components/AdminLayout';
import { authFetch } from '../../lib/browser-auth';
import { validateTelemetryCursor } from '../../lib/apocrypha/telemetry-cursor';

const SURFACES = ['health', 'capabilities', 'telemetry', 'diagnostics', 'organarium'] as const;
type Surface = typeof SURFACES[number];
const TRANSIENT_GATEWAY_STATUSES = new Set([502, 503, 504]);
const TRANSIENT_RETRY_DELAY_MS = 400;

interface SurfaceState {
  status: 'loading' | 'live' | 'unavailable';
  httpStatus: number | null;
  data: unknown;
}

function initialStates(): Record<Surface, SurfaceState> {
  return Object.fromEntries(SURFACES.map((surface) => [surface, {
    status: 'loading',
    httpStatus: null,
    data: null,
  }])) as Record<Surface, SurfaceState>;
}

function capabilityText(value: unknown, path: string[]): string {
  let cursor = value;
  for (const key of path) {
    if (!cursor || typeof cursor !== 'object' || Array.isArray(cursor)) return 'unverified';
    cursor = (cursor as Record<string, unknown>)[key];
  }
  return typeof cursor === 'string' ? cursor : 'unverified';
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function readProjection(path: string): Promise<{ response: Response; data: unknown }> {
  for (let attempt = 0; attempt < 2; attempt += 1) {
    const response = await authFetch(path, { cache: 'no-store' });
    const rawBody = await response.text();
    let body: unknown;
    try {
      body = JSON.parse(rawBody) as unknown;
    } catch {
      body = {
        error: 'Projection returned a non-JSON gateway response.',
        content_type: response.headers.get('content-type') ?? 'unknown',
      };
    }
    if (attempt === 0 && TRANSIENT_GATEWAY_STATUSES.has(response.status)) {
      await delay(TRANSIENT_RETRY_DELAY_MS);
      continue;
    }
    const record = body && typeof body === 'object' && !Array.isArray(body)
      ? body as { data?: unknown }
      : null;
    return { response, data: record?.data ?? body };
  }
  throw new Error('Projection retry loop ended without a response.');
}

const Diagnostics: NextPage = () => {
  const [authorized, setAuthorized] = useState(false);
  const [states, setStates] = useState<Record<Surface, SurfaceState>>(initialStates);
  const telemetryCursorRef = useRef(0);
  const refreshInFlightRef = useRef(false);

  const refresh = useCallback(async () => {
    if (refreshInFlightRef.current) return;
    refreshInFlightRef.current = true;
    const next = initialStates();
    try {
      // The cloud body is a single resident entity. Read its projections in
      // canonical order instead of stampeding the tunnel with parallel calls.
      for (const surface of SURFACES) {
        try {
        const cursor = telemetryCursorRef.current;
        const path = surface === 'telemetry'
          ? `/api/admin/apocrypha/telemetry?limit=100&after_event_seq=${cursor}`
          : `/api/admin/apocrypha/${surface}`;
        const { response, data } = await readProjection(path);
        if (surface === 'telemetry' && response.ok) {
          const validatedCursor = validateTelemetryCursor(data, cursor);
          if (validatedCursor === null) {
            next[surface] = {
              status: 'unavailable',
              httpStatus: response.status,
              data: { error: 'Telemetry cursor envelope failed monotonic validation.' },
            };
            setStates({ ...next });
            continue;
          }
          telemetryCursorRef.current = validatedCursor;
        }
        next[surface] = {
          status: response.ok ? 'live' : 'unavailable',
          httpStatus: response.status,
          data,
        };
        } catch {
          next[surface] = {
            status: 'unavailable',
            httpStatus: null,
            data: { error: 'Projection request failed before a valid response was received.' },
          };
        }
        setStates({ ...next });
      }
    } finally {
      refreshInFlightRef.current = false;
    }
  }, []);

  useEffect(() => {
    if (!authorized) return;
    void refresh();
    const interval = setInterval(() => void refresh(), 15_000);
    return () => clearInterval(interval);
  }, [authorized, refresh]);

  const capabilityData = states.capabilities.data;
  const expression = capabilityText(capabilityData, ['chat', 'expression']);
  const learnedField = capabilityText(capabilityData, ['chat', 'learned_native_whole_field']);
  const audioInput = capabilityText(capabilityData, ['voice', 'audio_input']);

  return (
    <AdminLayout title="⌬ Apocrypha V2 diagnostics" onAdminCheck={(check) => setAuthorized(check.authorized)}>
      {authorized ? (
        <main className="diagnostic-console">
          <header className="diagnostic-header">
            <div>
              <h1>Canonical V2 body</h1>
              <p>Owner-only · private/no-store · live upstream projections</p>
            </div>
            <button type="button" onClick={() => void refresh()}>Refresh</button>
          </header>

          <section className="capability-strip" aria-label="Current capability boundary">
            <span>Expression · {expression}</span>
            <span>Learned field · {learnedField}</span>
            <span>Audio · {audioInput === 'absent' ? 'unavailable' : audioInput}</span>
          </section>

          <div className="diagnostic-grid">
            {SURFACES.map((surface) => {
              const state = states[surface];
              return (
                <section key={surface} className="diagnostic-card">
                  <div className="card-title">
                    <h2>{surface}</h2>
                    <span data-status={state.status}>
                      {state.status}{state.httpStatus ? ` · ${state.httpStatus}` : ''}
                    </span>
                  </div>
                  <pre>{state.data === null ? 'No verified response.' : JSON.stringify(state.data, null, 2)}</pre>
                </section>
              );
            })}
          </div>

          <style jsx>{`
            .diagnostic-console { display:grid; gap:16px; color:#dfe1ec; }
            .diagnostic-header { display:flex; align-items:center; justify-content:space-between; gap:16px; }
            h1 { margin:0; font-size:1.35rem; }
            .diagnostic-header p { margin:5px 0 0; color:#85879a; font-size:.8rem; }
            button { min-height:44px; padding:0 16px; border:1px solid #37364a; border-radius:999px; color:#e7e4f4; background:#171621; cursor:pointer; }
            button:focus-visible { outline:2px solid #b8a5ff; outline-offset:3px; }
            .capability-strip { display:flex; flex-wrap:wrap; gap:8px; }
            .capability-strip span { padding:7px 10px; border:1px solid #2e2d40; border-radius:999px; color:#aaa8bc; background:#11111a; font:600 .72rem/1 ui-monospace,monospace; }
            .diagnostic-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(min(100%,360px),1fr)); gap:14px; }
            .diagnostic-card { min-width:0; border:1px solid #272633; border-radius:14px; background:#0e0e16; overflow:hidden; }
            .card-title { display:flex; justify-content:space-between; gap:12px; padding:12px 14px; border-bottom:1px solid #242330; }
            h2 { margin:0; font:700 .78rem/1 ui-monospace,monospace; text-transform:uppercase; letter-spacing:.08em; }
            .card-title span { color:#f2b86b; font:600 .68rem/1 ui-monospace,monospace; }
            .card-title span[data-status=live] { color:#6ce0bd; }
            pre { max-height:360px; margin:0; padding:14px; overflow:auto; color:#a9abba; font:500 .69rem/1.55 ui-monospace,monospace; white-space:pre-wrap; overflow-wrap:anywhere; }
          `}</style>
        </main>
      ) : (
        <p style={{ padding: '2rem', color: '#a0a0b0' }}>Diagnostics require owner authentication.</p>
      )}
    </AdminLayout>
  );
};

export default Diagnostics;
