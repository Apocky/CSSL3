import type { NextPage } from 'next';
import { useCallback, useEffect, useState, type FormEvent } from 'react';

import AdminLayout from '../../components/AdminLayout';
import { authFetch } from '../../lib/browser-auth';
import type {
  RuntimeHealthProjection,
  RuntimeObjectiveProjection,
} from '../../lib/apocv4/runtime-proxy';

type ApiFailure = { error?: string; upstream_status?: number };

const panel: React.CSSProperties = {
  background: 'rgba(10, 10, 16, 0.62)',
  border: '1px solid #29293a',
  borderRadius: 8,
  padding: '1rem',
};

const pre: React.CSSProperties = {
  background: '#08080d',
  border: '1px solid #20202d',
  borderRadius: 6,
  color: '#cdd6e4',
  fontSize: '0.74rem',
  lineHeight: 1.5,
  margin: '0.75rem 0 0',
  maxHeight: 420,
  overflow: 'auto',
  padding: '0.75rem',
  whiteSpace: 'pre-wrap',
  wordBreak: 'break-word',
};

function pretty(value: unknown): string {
  return JSON.stringify(value, null, 2);
}

function errorMessage(payload: ApiFailure, status: number): string {
  const upstream = typeof payload.upstream_status === 'number'
    ? ` · runtime HTTP ${payload.upstream_status}`
    : '';
  return `${payload.error ?? `request_failed_${status}`}${upstream}`;
}

const Apex: NextPage = () => {
  const [adminAuthorized, setAdminAuthorized] = useState(false);
  const [health, setHealth] = useState<RuntimeHealthProjection | null>(null);
  const [healthError, setHealthError] = useState<string | null>(null);
  const [healthBusy, setHealthBusy] = useState(false);
  const [objective, setObjective] = useState('');
  const [objectiveBusy, setObjectiveBusy] = useState(false);
  const [objectiveError, setObjectiveError] = useState<string | null>(null);
  const [objectiveResult, setObjectiveResult] = useState<RuntimeObjectiveProjection | null>(null);

  const refreshHealth = useCallback(async () => {
    setHealthBusy(true);
    setHealthError(null);
    try {
      const response = await authFetch('/api/admin/apocv4/health', { cache: 'no-store' });
      const payload = await response.json() as RuntimeHealthProjection & ApiFailure;
      if (!response.ok) throw new Error(errorMessage(payload, response.status));
      setHealth(payload);
    } catch (error) {
      setHealth(null);
      setHealthError(error instanceof Error ? error.message : 'health_request_failed');
    } finally {
      setHealthBusy(false);
    }
  }, []);

  useEffect(() => {
    if (adminAuthorized) void refreshHealth();
  }, [adminAuthorized, refreshHealth]);

  const submitObjective = useCallback(async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const canonical = objective.trim();
    if (!canonical || canonical !== objective || canonical.length > 16_384) {
      setObjectiveError('Objective must be 1–16,384 characters with no outer whitespace.');
      return;
    }
    setObjectiveBusy(true);
    setObjectiveError(null);
    setObjectiveResult(null);
    try {
      const response = await authFetch('/api/admin/apocv4/objective', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ objective: canonical }),
      });
      const payload = await response.json() as RuntimeObjectiveProjection & ApiFailure;
      if (!response.ok) throw new Error(errorMessage(payload, response.status));
      setObjectiveResult(payload);
      await refreshHealth();
    } catch (error) {
      setObjectiveError(error instanceof Error ? error.message : 'objective_request_failed');
    } finally {
      setObjectiveBusy(false);
    }
  }, [objective, refreshHealth]);

  const runtimeReady = health?.observed.runtime.status === 'READY';

  return (
    <AdminLayout
      title="Apex · Apocv4"
      onAdminCheck={(check) => setAdminAuthorized(check.authorized)}
    >
      {!adminAuthorized ? (
        <div style={{ ...panel, color: '#a0a0b0' }}>
          Apex runtime access requires owner/admin authentication.
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '1rem', maxWidth: 1100 }}>
          <header>
            <div style={{ color: '#a78bfa', fontSize: '0.72rem', letterSpacing: '0.14em' }}>
              § APOCV4 · OWNER SURFACE
            </div>
            <h1 style={{ fontSize: '1.35rem', margin: '0.35rem 0' }}>Apex runtime</h1>
            <p style={{ color: '#8b8b9e', fontSize: '0.82rem', margin: 0 }}>
              Server-mediated RunPod access. Browser requests never receive the runtime credential or choose a privacy partition.
            </p>
            <p style={{ color: '#6f6f82', fontSize: '0.74rem', margin: '0.35rem 0 0' }}>
              Synchronous RunPod proxy bound: 95 seconds. The API function ceiling is 300 seconds, but RunPod&apos;s Cloudflare proxy closes at approximately 100 seconds.
            </p>
          </header>

          <section style={panel} aria-live="polite">
            <div style={{ alignItems: 'center', display: 'flex', justifyContent: 'space-between', gap: 12 }}>
              <div>
                <div style={{ color: '#7dd3fc', fontSize: '0.72rem', letterSpacing: '0.12em' }}>
                  ✓ OBSERVED · RUNTIME HTTP
                </div>
                <strong style={{ color: runtimeReady ? '#6ee7b7' : '#fca5a5' }}>
                  {healthBusy ? '◐ checking' : runtimeReady ? '✓ READY' : '✗ unavailable'}
                </strong>
              </div>
              <button type="button" onClick={() => void refreshHealth()} disabled={healthBusy} style={buttonStyle}>
                {healthBusy ? 'checking…' : 'refresh'}
              </button>
            </div>
            {healthError && <p style={{ color: '#fca5a5' }}>{healthError}</p>}
            {health && <pre style={pre}>{pretty(health.observed)}</pre>}
          </section>

          <section style={panel}>
            <div style={{ color: '#7dd3fc', fontSize: '0.72rem', letterSpacing: '0.12em' }}>
              → OBJECTIVE · CREDENTIAL-BOUND PARTITION
            </div>
            <form onSubmit={(event) => void submitObjective(event)}>
              <label htmlFor="apex-objective" style={{ display: 'block', margin: '0.75rem 0 0.35rem' }}>
                One bounded objective
              </label>
              <textarea
                id="apex-objective"
                value={objective}
                onChange={(event) => setObjective(event.target.value)}
                maxLength={16_384}
                rows={6}
                disabled={objectiveBusy}
                placeholder="Describe the exact outcome Apocv4 should pursue and verify."
                style={{
                  background: '#08080d',
                  border: '1px solid #303044',
                  borderRadius: 6,
                  color: '#e6e6f0',
                  padding: '0.75rem',
                  resize: 'vertical',
                  width: '100%',
                }}
              />
              <div style={{ alignItems: 'center', display: 'flex', justifyContent: 'space-between', marginTop: 8 }}>
                <span style={{ color: '#686879', fontSize: '0.72rem' }}>{objective.length} / 16,384</span>
                <button type="submit" disabled={objectiveBusy || !runtimeReady} style={buttonStyle}>
                  {objectiveBusy ? 'Apocv4 is working…' : 'submit objective'}
                </button>
              </div>
            </form>
            {objectiveError && <p style={{ color: '#fca5a5' }}>{objectiveError}</p>}
          </section>

          {objectiveResult && (
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(320px, 1fr))', gap: '1rem' }}>
              <section style={{ ...panel, borderColor: '#24566b' }}>
                <div style={{ color: '#7dd3fc', fontSize: '0.72rem', letterSpacing: '0.12em' }}>
                  ✓ OBSERVED · TRANSPORT + TEST RECEIPTS
                </div>
                <p style={{ color: '#8b8b9e', fontSize: '0.78rem' }}>
                  HTTP receipt, checkpoint state, test outcomes, evidence digests, and terminal status observed by the governed runtime.
                </p>
                <pre style={pre}>{pretty(objectiveResult.observed)}</pre>
              </section>
              <section style={{ ...panel, borderColor: '#634c82' }}>
                <div style={{ color: '#c4b5fd', fontSize: '0.72rem', letterSpacing: '0.12em' }}>
                  ◐ MODEL-REPORTED · NOT OBSERVED FACT
                </div>
                <p style={{ color: '#a89aba', fontSize: '0.78rem' }}>
                  Faculty routes, candidate digests, and council reports. This is visible model output—not hidden chain-of-thought and not independent proof.
                </p>
                <pre style={pre}>{pretty(objectiveResult.model_reported)}</pre>
              </section>
            </div>
          )}
        </div>
      )}
    </AdminLayout>
  );
};

const buttonStyle: React.CSSProperties = {
  background: '#24243a',
  border: '1px solid #464663',
  borderRadius: 5,
  color: '#d9d9e8',
  cursor: 'pointer',
  padding: '0.48rem 0.8rem',
};

export default Apex;
