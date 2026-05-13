import type { NextPage } from 'next';
import { useEffect, useMemo, useState } from 'react';

import AdminLayout from '../../components/AdminLayout';
import { authFetch } from '../../lib/browser-auth';
import type {
  JsonValue,
  LazarusApproval,
  LazarusEvent,
  LazarusFleetConfig,
  LazarusHealth,
  LazarusRun,
  LazarusRunner,
  LazarusTask,
} from '../../lib/lazarus/types';

type LoadState = 'loading' | 'ready' | 'error';

interface TesseraCockpitData {
  health: LazarusHealth | null;
  tasks: LazarusTask[];
  runs: LazarusRun[];
  runners: LazarusRunner[];
  approvals: LazarusApproval[];
  fleet: LazarusFleetConfig[];
  events: LazarusEvent[];
  stub: boolean;
}

const cardStyle = {
  padding: '0.85rem',
  background: 'rgba(20, 20, 30, 0.5)',
  border: '1px solid #1f1f2a',
  borderRadius: 6,
} as const;

const labelStyle = {
  fontSize: '0.66rem',
  color: '#7a7a8c',
  textTransform: 'uppercase',
  letterSpacing: '0.15em',
} as const;

function statusColor(status: string): string {
  if (status === 'completed' || status === 'online' || status === 'approved' || status === 'succeeded') return '#34d399';
  if (status === 'queued' || status === 'leased' || status === 'running' || status === 'pending') return '#7dd3fc';
  if (status === 'blocked' || status === 'waiting_approval') return '#fbbf24';
  return '#f87171';
}

function jsonFlag(value: JsonValue | undefined): boolean {
  return value === true || value === 'true' || value === 1;
}

function isTesseraEvent(event: LazarusEvent): boolean {
  return event.kind.startsWith('tessera.') || event.kind.startsWith('lr.') || event.kind.startsWith('goal.');
}

function shortId(id: string): string {
  return id.length > 18 ? `${id.slice(0, 10)}…${id.slice(-6)}` : id;
}

const TesseraOmnimindCockpit: NextPage = () => {
  const [state, setState] = useState<LoadState>('loading');
  const [notice, setNotice] = useState<string | null>(null);
  const [data, setData] = useState<TesseraCockpitData>({
    health: null,
    tasks: [],
    runs: [],
    runners: [],
    approvals: [],
    fleet: [],
    events: [],
    stub: true,
  });

  async function load(): Promise<void> {
    try {
      const [health, tasks, runs, runners, approvals, fleet, events] = await Promise.all([
        authFetch('/api/admin/lazarus/health', { cache: 'no-store' }).then((response) => response.json()),
        authFetch('/api/admin/lazarus/tasks', { cache: 'no-store' }).then((response) => response.json()),
        authFetch('/api/admin/lazarus/runs', { cache: 'no-store' }).then((response) => response.json()),
        authFetch('/api/admin/lazarus/runners', { cache: 'no-store' }).then((response) => response.json()),
        authFetch('/api/admin/lazarus/approvals', { cache: 'no-store' }).then((response) => response.json()),
        authFetch('/api/admin/lazarus/fleet', { cache: 'no-store' }).then((response) => response.json()),
        authFetch('/api/admin/lazarus/events', { cache: 'no-store' }).then((response) => response.json()),
      ]);

      setData({
        health: health.ok ? health : null,
        tasks: tasks.tasks ?? [],
        runs: runs.runs ?? [],
        runners: runners.runners ?? [],
        approvals: approvals.approvals ?? [],
        fleet: fleet.fleet ?? [],
        events: events.events ?? [],
        stub: Boolean(health.stub || tasks.stub || runs.stub || runners.stub || approvals.stub || fleet.stub || events.stub),
      });
      setState('ready');
    } catch (err) {
      setNotice(`load failed: ${err instanceof Error ? err.message : String(err)}`);
      setState('error');
    }
  }

  useEffect(() => {
    void load();
    const timer = setInterval(() => void load(), 10_000);
    return () => clearInterval(timer);
  }, []);

  const tesseraEvents = useMemo(() => data.events.filter(isTesseraEvent), [data.events]);
  const bridgeRunners = useMemo(
    () => data.runners.filter((runner) => runner.capabilities.includes('tessera-bridge') || jsonFlag(runner.metadata.tessera_bridge_enabled)),
    [data.runners],
  );
  const activeGoals = useMemo(
    () => data.tasks.filter((task) => ['queued', 'leased', 'running', 'blocked'].includes(task.status)).slice(0, 8),
    [data.tasks],
  );
  const pendingApprovals = useMemo(() => data.approvals.filter((approval) => approval.status === 'pending'), [data.approvals]);
  const recentRuns = useMemo(() => data.runs.slice(0, 6), [data.runs]);
  const eventRuns = useMemo(() => {
    const runIds = new Set(tesseraEvents.map((event) => event.run_id));
    return data.runs.filter((run) => runIds.has(run.id)).slice(0, 5);
  }, [data.runs, tesseraEvents]);

  const bridgeMode = bridgeRunners.length > 0 ? 'dry-run' : tesseraEvents.length > 0 ? 'trace-only' : 'idle';
  const costTotal = data.runs.reduce((sum, run) => sum + run.cost_usd_estimate, 0);

  return (
    <AdminLayout title="Ψ Tessera Omnimind">
      <section style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(130px, 1fr))', gap: '0.6rem' }}>
        {[
          ['mode', bridgeMode],
          ['goals', activeGoals.length],
          ['events', tesseraEvents.length],
          ['approvals', pendingApprovals.length],
          ['cost', `$${costTotal.toFixed(2)}`],
        ].map(([label, value]) => (
          <div key={label} style={cardStyle}>
            <div style={{ fontSize: '1.25rem', color: label === 'approvals' && value !== 0 ? '#fbbf24' : '#c084fc' }}>{value}</div>
            <div style={labelStyle}>{label}</div>
          </div>
        ))}
      </section>

      {data.stub && (
        <div style={{ ...cardStyle, borderColor: 'rgba(251, 191, 36, 0.4)', color: '#fbbf24', marginTop: '1rem' }}>
          <strong>◐ stub-safe</strong>
          <div style={{ color: '#c9a94c', fontSize: '0.82rem', marginTop: 4 }}>state is in-memory; persistence gate not proven</div>
        </div>
      )}

      {notice && (
        <div style={{ ...cardStyle, marginTop: '1rem', color: notice.includes('failed') ? '#f87171' : '#7dd3fc' }}>
          {notice}
        </div>
      )}

      <section style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))', gap: '1rem', marginTop: '1rem' }}>
        <section style={cardStyle}>
          <h2 style={{ marginTop: 0, fontSize: '1rem', color: '#c084fc' }}>Goal Stack</h2>
          {state === 'loading' && <p style={{ color: '#7a7a8c' }}>§ loading…</p>}
          {state === 'error' && <p style={{ color: '#f87171' }}>✗ load error</p>}
          {activeGoals.length === 0 && state === 'ready' && <p style={{ color: '#7a7a8c', fontSize: '0.84rem' }}>○ empty</p>}
          {activeGoals.map((task) => (
            <article key={task.id} style={{ borderTop: '1px solid #1f1f2a', padding: '0.55rem 0' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', gap: '0.5rem' }}>
                <strong style={{ color: '#e6e6f0', fontSize: '0.86rem' }}>{task.title}</strong>
                <span style={{ color: statusColor(task.status), fontSize: '0.72rem' }}>{task.status}</span>
              </div>
              <div style={{ color: '#7a7a8c', fontSize: '0.72rem' }}>{shortId(task.id)} · {task.model_mode}</div>
            </article>
          ))}
        </section>

        <section style={cardStyle}>
          <h2 style={{ marginTop: 0, fontSize: '1rem', color: '#7dd3fc' }}>Bridge Runners</h2>
          {bridgeRunners.length === 0 && <p style={{ color: '#7a7a8c', fontSize: '0.84rem' }}>○ no bridge heartbeat</p>}
          {bridgeRunners.map((runner) => (
            <article key={runner.id} style={{ borderTop: '1px solid #1f1f2a', padding: '0.55rem 0' }}>
              <div style={{ color: statusColor(runner.status) }}>{runner.label}</div>
              <div style={{ color: '#7a7a8c', fontSize: '0.72rem' }}>{runner.status} · {shortId(runner.id)}</div>
            </article>
          ))}
          {data.fleet.slice(0, 2).map((fleetConfig) => (
            <div key={fleetConfig.id} style={{ marginTop: '0.7rem', color: '#7a7a8c', fontSize: '0.78rem' }}>
              {fleetConfig.privacy_class} · {fleetConfig.default_model_mode} · ${fleetConfig.max_cost_usd_per_run}
            </div>
          ))}
        </section>

        <section style={cardStyle}>
          <h2 style={{ marginTop: 0, fontSize: '1rem', color: '#34d399' }}>Run Trace</h2>
          {eventRuns.length === 0 && <p style={{ color: '#7a7a8c', fontSize: '0.84rem' }}>○ no Tessera trace</p>}
          {eventRuns.map((run) => (
            <details key={run.id} style={{ borderTop: '1px solid #1f1f2a', padding: '0.55rem 0' }}>
              <summary style={{ cursor: 'pointer', color: statusColor(run.status), listStyle: 'none' }}>
                {shortId(run.id)} · {run.status}
              </summary>
              {tesseraEvents.filter((event) => event.run_id === run.id).slice(0, 8).map((event) => (
                <div key={event.id} style={{ marginTop: '0.45rem', color: event.level === 'error' ? '#f87171' : '#cdd6e4', fontSize: '0.76rem' }}>
                  <code style={{ color: '#7dd3fc' }}>{event.kind}</code>
                  <div style={{ color: '#7a7a8c' }}>{event.message}</div>
                </div>
              ))}
            </details>
          ))}
        </section>

        <section style={cardStyle}>
          <h2 style={{ marginTop: 0, fontSize: '1rem', color: '#fbbf24' }}>Approvals</h2>
          {pendingApprovals.length === 0 ? (
            <p style={{ color: '#7a7a8c', fontSize: '0.84rem' }}>✓ clear</p>
          ) : (
            pendingApprovals.slice(0, 6).map((approval) => (
              <article key={approval.id} style={{ borderTop: '1px solid #1f1f2a', padding: '0.55rem 0' }}>
                <code style={{ color: '#fbbf24' }}>{approval.gate}</code>
                <div style={{ color: '#cdd6e4', fontSize: '0.78rem' }}>{approval.reason}</div>
              </article>
            ))
          )}
        </section>
      </section>

      <section style={{ ...cardStyle, marginTop: '1rem' }}>
        <h2 style={{ marginTop: 0, fontSize: '1rem', color: '#a78bfa' }}>Event Replay</h2>
        {tesseraEvents.length === 0 && <p style={{ color: '#7a7a8c', fontSize: '0.84rem' }}>○ waiting</p>}
        <div style={{ display: 'grid', gap: '0.4rem' }}>
          {tesseraEvents.slice(0, 12).map((event) => (
            <div key={event.id} style={{ display: 'grid', gridTemplateColumns: 'minmax(120px, 180px) 1fr', gap: '0.6rem', fontSize: '0.76rem' }}>
              <code style={{ color: event.level === 'error' ? '#f87171' : '#7dd3fc' }}>{event.kind}</code>
              <span style={{ color: '#cdd6e4', overflowWrap: 'anywhere' }}>{event.message}</span>
            </div>
          ))}
        </div>
      </section>

      <section style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))', gap: '1rem', marginTop: '1rem' }}>
        {recentRuns.map((run) => (
          <article key={run.id} style={cardStyle}>
            <div style={{ ...labelStyle, marginBottom: 4 }}>{shortId(run.id)}</div>
            <div style={{ color: statusColor(run.status), fontSize: '0.88rem' }}>{run.status}</div>
            <div style={{ color: '#7a7a8c', fontSize: '0.72rem', marginTop: 4 }}>{run.model_mode} · ${run.cost_usd_estimate.toFixed(4)}</div>
          </article>
        ))}
      </section>
    </AdminLayout>
  );
};

export function _testExportsAreFunctions(): boolean {
  return typeof TesseraOmnimindCockpit === 'function';
}

export default TesseraOmnimindCockpit;