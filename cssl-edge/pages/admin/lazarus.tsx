import type { NextPage } from 'next';
import { useEffect, useMemo, useState, type ReactNode } from 'react';

import AdminLayout from '../../components/AdminLayout';
import AdminTooltip from '../../components/AdminTooltip';
import { authFetch } from '../../lib/browser-auth';
import type {
  LazarusApproval,
  LazarusFleetConfig,
  LazarusHealth,
  LazarusModelMode,
  LazarusRun,
  LazarusRunner,
  LazarusTask,
  LazarusToolSpec,
} from '../../lib/lazarus/types';

type LoadState = 'loading' | 'ready' | 'error';

interface LazarusData {
  health: LazarusHealth | null;
  tasks: LazarusTask[];
  runs: LazarusRun[];
  runners: LazarusRunner[];
  approvals: LazarusApproval[];
  tools: LazarusToolSpec[];
  fleet: LazarusFleetConfig[];
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

const inputStyle = {
  width: '100%',
  marginTop: 4,
  padding: '0.7rem 0.85rem',
  background: 'rgba(10, 10, 16, 0.75)',
  border: '1px solid #2a2a3a',
  borderRadius: 4,
  color: '#e6e6f0',
  fontFamily: 'inherit',
  fontSize: '0.9rem',
  outline: 'none',
} as const;

function statusColor(status: string): string {
  if (status === 'completed' || status === 'online' || status === 'approved') return '#34d399';
  if (status === 'queued' || status === 'leased' || status === 'running' || status === 'pending') return '#7dd3fc';
  if (status === 'blocked') return '#fbbf24';
  return '#f87171';
}

function PanelTitle({ children, tip, color }: { children: ReactNode; tip: string; color: string }) {
  return (
    <h2 style={{ marginTop: 0, fontSize: '1rem', color, display: 'flex', alignItems: 'center', gap: '0.45rem' }}>
      {children}
      <AdminTooltip label={tip} />
    </h2>
  );
}

function FormLabel({ children, tip }: { children: ReactNode; tip: string }) {
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: '0.35rem' }}>
      {children}
      <AdminTooltip label={tip} />
    </span>
  );
}

function MetricCard({ label, value, tip }: { label: string; value: number; tip: string }) {
  return (
    <div style={cardStyle}>
      <div style={{ fontSize: '1.35rem', color: '#7dd3fc' }}>{value}</div>
      <div style={{ ...labelStyle, display: 'flex', alignItems: 'center', gap: '0.35rem' }}>
        {label}
        <AdminTooltip label={tip} />
      </div>
    </div>
  );
}

const metricTips: Record<string, string> = {
  queued: 'Tasks waiting for an online runner to lease them. If this grows while runners are online, runners may be blocked by approvals or auth.',
  active: 'Runs currently leased or running. This is live work in progress, not total historical runs.',
  approvals: 'Human review gates waiting for an admin decision before Lazarus can continue a risky action.',
  runners: 'Runner processes that recently checked in with the control plane and can lease work.',
  tools: 'Registered tool specs Lazarus can ask runners to use, grouped by build, test, sensorium, git, memory, and related capabilities.',
};

const LazarusConsole: NextPage = () => {
  const [state, setState] = useState<LoadState>('loading');
  const [data, setData] = useState<LazarusData>({
    health: null,
    tasks: [],
    runs: [],
    runners: [],
    approvals: [],
    tools: [],
    fleet: [],
    stub: true,
  });
  const [title, setTitle] = useState('LoA v14 work slice');
  const [prompt, setPrompt] = useState('Implement the next verified LoA v14 engine task with build/test evidence.');
  const [repoPath, setRepoPath] = useState('C:\\Users\\Apocky\\source\\repos\\LoA v14');
  const [modelMode, setModelMode] = useState<LazarusModelMode>('deepseek-v4-pro');
  const [costCeiling, setCostCeiling] = useState('2');
  const [sensoriumEnabled, setSensoriumEnabled] = useState(true);
  const [playtestEnabled, setPlaytestEnabled] = useState(true);
  const [taskFilter, setTaskFilter] = useState('all');
  const [lastUpdated, setLastUpdated] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  async function load(): Promise<void> {
    try {
      const [health, tasks, runs, runners, approvals, tools, fleet] = await Promise.all([
        authFetch('/api/admin/lazarus/health', { cache: 'no-store' }).then((r) => r.json()),
        authFetch('/api/admin/lazarus/tasks', { cache: 'no-store' }).then((r) => r.json()),
        authFetch('/api/admin/lazarus/runs', { cache: 'no-store' }).then((r) => r.json()),
        authFetch('/api/admin/lazarus/runners', { cache: 'no-store' }).then((r) => r.json()),
        authFetch('/api/admin/lazarus/approvals', { cache: 'no-store' }).then((r) => r.json()),
        authFetch('/api/admin/lazarus/tools', { cache: 'no-store' }).then((r) => r.json()),
        authFetch('/api/admin/lazarus/fleet', { cache: 'no-store' }).then((r) => r.json()),
      ]);
      setData({
        health: health.ok ? health : null,
        tasks: tasks.tasks ?? [],
        runs: runs.runs ?? [],
        runners: runners.runners ?? [],
        approvals: approvals.approvals ?? [],
        tools: tools.tools ?? [],
        fleet: fleet.fleet ?? [],
        stub: Boolean(health.stub || tasks.stub || runs.stub || runners.stub || approvals.stub || fleet.stub),
      });
      setLastUpdated(new Date().toLocaleTimeString());
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

  const pendingApprovals = useMemo(() => data.approvals.filter((a) => a.status === 'pending'), [data.approvals]);
  const visibleTasks = useMemo(
    () => data.tasks.filter((task) => taskFilter === 'all' || task.status === taskFilter),
    [data.tasks, taskFilter],
  );
  const toolGroups = useMemo(() => {
    const groups = new Map<string, number>();
    for (const tool of data.tools) groups.set(tool.group, (groups.get(tool.group) ?? 0) + 1);
    return Array.from(groups.entries());
  }, [data.tools]);

  async function createTask(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    if (submitting) return;
    setSubmitting(true);
    setNotice(null);
    try {
      const res = await authFetch('/api/admin/lazarus/tasks', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          title,
          prompt,
          repo_path: repoPath,
          model_mode: modelMode,
          cost_ceiling_usd: Number.parseFloat(costCeiling) || 0.25,
          sensorium_enabled: sensoriumEnabled,
          playtest_enabled: playtestEnabled,
        }),
      });
      const json = await res.json();
      if (!res.ok) throw new Error(json.error ?? 'task create failed');
      setNotice(`queued ${json.task.id}`);
      await load();
    } catch (err) {
      setNotice(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  }

  async function decide(approval: LazarusApproval, decision: 'approved' | 'denied'): Promise<void> {
    const res = await authFetch('/api/admin/lazarus/approvals', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ action: 'decide', approval_id: approval.id, decision, decided_by: 'admin-console' }),
    });
    const json = await res.json();
    if (!res.ok) setNotice(json.error ?? 'approval decision failed');
    else setNotice(`${decision} ${approval.gate}`);
    await load();
  }

  return (
    <AdminLayout title="Λ Lazarus Console">
      <p style={{ color: '#7a7a8c', fontSize: '0.82rem', marginTop: 0, marginBottom: '1rem' }}>
        LoA v14 autonomous coding control plane · DeepSeek fleet route · MNEME memory · Sensorium tools · approval gates
      </p>

      <div style={{ display: 'flex', alignItems: 'center', gap: '0.6rem', flexWrap: 'wrap', marginBottom: '1rem' }}>
        <button type="button" onClick={() => void load()} style={{ ...inputStyle, width: 'auto', minHeight: 36, marginTop: 0, cursor: 'pointer' }}>
          refresh
        </button>
        <span style={{ color: '#7a7a8c', fontSize: '0.78rem' }}>{lastUpdated ? `last updated ${lastUpdated}` : 'loading live control-plane state'}</span>
        <AdminTooltip label="This page polls every 10 seconds. Use refresh after queueing tasks or approving gates when you want an immediate state update." />
      </div>

      {data.stub && (
        <div style={{ ...cardStyle, borderColor: 'rgba(251, 191, 36, 0.4)', color: '#fbbf24', marginBottom: '1rem' }}>
          <strong>◐ stub-safe mode</strong>
          <div style={{ color: '#c9a94c', fontSize: '0.82rem', marginTop: 4 }}>
            Supabase service-role config is absent, so Lazarus is using an in-memory control loop. Set
            SUPABASE_SERVICE_ROLE_KEY after applying migration 0042 to persist state.
          </div>
        </div>
      )}

      {notice && (
        <div style={{ ...cardStyle, marginBottom: '1rem', color: notice.includes('failed') ? '#f87171' : '#7dd3fc' }}>
          {notice}
        </div>
      )}

      <section style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(130px, 1fr))', gap: '0.6rem' }}>
        {[
          ['queued', data.health?.queued_count ?? 0],
          ['active', data.health?.active_run_count ?? 0],
          ['approvals', data.health?.pending_approval_count ?? 0],
          ['runners', data.health?.online_runner_count ?? 0],
          ['tools', data.health?.tool_count ?? data.tools.length],
        ].map(([label, value]) => (
          <MetricCard key={label} label={String(label)} value={Number(value)} tip={metricTips[String(label)] ?? ''} />
        ))}
      </section>

      <section style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))', gap: '1rem', marginTop: '1rem' }}>
        <form onSubmit={createTask} style={cardStyle}>
          <PanelTitle color="#c084fc" tip="Create a unit of work for Lazarus. A runner leases queued tasks, emits events, and marks runs completed or failed.">
            New LoA v14 task
          </PanelTitle>
          <label style={labelStyle}>
            <FormLabel tip="Short name shown in the queue and run history.">title</FormLabel>
            <input value={title} onChange={(e) => setTitle(e.target.value)} style={inputStyle} />
          </label>
          <label style={{ ...labelStyle, display: 'block', marginTop: '0.75rem' }}>
            <FormLabel tip="The actual work order sent to the runner/model. Be concrete about files, expected evidence, and safety boundaries.">prompt</FormLabel>
            <textarea value={prompt} onChange={(e) => setPrompt(e.target.value)} rows={6} style={{ ...inputStyle, resize: 'vertical' }} />
          </label>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(160px, 1fr))', gap: '0.65rem', marginTop: '0.75rem' }}>
            <label style={labelStyle}>
              <FormLabel tip="Workspace path the runner should operate in.">workspace</FormLabel>
              <input value={repoPath} onChange={(e) => setRepoPath(e.target.value)} style={inputStyle} />
            </label>
            <label style={labelStyle}>
              <FormLabel tip="Model/fleet route requested for this task. The runner still enforces its own local safety and availability checks.">model</FormLabel>
              <select value={modelMode} onChange={(e) => setModelMode(e.target.value as LazarusModelMode)} style={inputStyle}>
                <option value="deepseek-v4-pro">deepseek-v4-pro</option>
                <option value="deepseek-v4-flash">deepseek-v4-flash</option>
                <option value="reviewer">reviewer</option>
              </select>
            </label>
            <label style={labelStyle}>
              <FormLabel tip="Maximum estimated external model spend allowed for one run before a cost gate should block continuation.">cost cap</FormLabel>
              <input type="number" min="0" step="0.25" value={costCeiling} onChange={(e) => setCostCeiling(e.target.value)} style={inputStyle} />
            </label>
          </div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: '0.8rem', marginTop: '0.75rem', color: '#cdd6e4', fontSize: '0.82rem' }}>
            <label style={{ display: 'inline-flex', alignItems: 'center', gap: '0.4rem' }}>
              <input type="checkbox" checked={sensoriumEnabled} onChange={(e) => setSensoriumEnabled(e.target.checked)} />
              sensorium
              <AdminTooltip label="Allows the runner to use perception tools such as screenshots, pixel diffs, frame stats, and other environment observations." />
            </label>
            <label style={{ display: 'inline-flex', alignItems: 'center', gap: '0.4rem' }}>
              <input type="checkbox" checked={playtestEnabled} onChange={(e) => setPlaytestEnabled(e.target.checked)} />
              playtest
              <AdminTooltip label="Allows the runner to launch or exercise the app/game and report build or UX evidence instead of only editing code." />
            </label>
          </div>
          <button
            type="submit"
            disabled={submitting}
            style={{
              marginTop: '0.75rem',
              width: '100%',
              minHeight: 44,
              border: 'none',
              borderRadius: 4,
              background: 'linear-gradient(135deg, #c084fc 0%, #7dd3fc 100%)',
              color: '#0a0a0f',
              fontWeight: 700,
              fontFamily: 'inherit',
              opacity: submitting ? 0.6 : 1,
            }}
          >
            {submitting ? '◐ queueing…' : '→ queue task'}
          </button>
        </form>

        <section style={cardStyle}>
          <PanelTitle color="#fbbf24" tip="Approvals are manual gates for risky actions such as destructive git, broad file deletion, unknown network egress, cost overruns, or persistent memory writes.">
            Approval gates
          </PanelTitle>
          {pendingApprovals.length === 0 ? (
            <p style={{ color: '#7a7a8c', fontSize: '0.84rem' }}>✓ no pending approvals</p>
          ) : (
            <div style={{ display: 'grid', gap: '0.5rem' }}>
              {pendingApprovals.map((approval) => (
                <article key={approval.id} style={{ borderTop: '1px solid #1f1f2a', paddingTop: '0.5rem' }}>
                  <code style={{ color: '#fbbf24' }}>{approval.gate}</code>
                  <p style={{ color: '#cdd6e4', fontSize: '0.82rem', margin: '0.35rem 0' }}>{approval.reason}</p>
                  <div style={{ display: 'flex', gap: '0.4rem' }}>
                    <button type="button" onClick={() => void decide(approval, 'approved')} style={{ flex: 1, minHeight: 38 }}>
                      approve
                    </button>
                    <button type="button" onClick={() => void decide(approval, 'denied')} style={{ flex: 1, minHeight: 38 }}>
                      deny
                    </button>
                  </div>
                </article>
              ))}
            </div>
          )}
        </section>
      </section>

      <section style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(260px, 1fr))', gap: '1rem', marginTop: '1rem' }}>
        <section style={cardStyle}>
          <PanelTitle color="#7dd3fc" tip="Queued tasks are work orders. Leased or running tasks have been picked up by a runner. Completed, failed, and cancelled tasks are historical state.">
            Task queue
          </PanelTitle>
          <select value={taskFilter} onChange={(e) => setTaskFilter(e.target.value)} style={{ ...inputStyle, marginTop: 0, marginBottom: '0.6rem' }}>
            <option value="all">all statuses</option>
            <option value="queued">queued</option>
            <option value="leased">leased</option>
            <option value="running">running</option>
            <option value="blocked">blocked</option>
            <option value="completed">completed</option>
            <option value="failed">failed</option>
            <option value="cancelled">cancelled</option>
          </select>
          {state === 'loading' && <p style={{ color: '#7a7a8c' }}>§ loading…</p>}
          {state === 'error' && <p style={{ color: '#f87171' }}>✗ load error</p>}
          {visibleTasks.slice(0, 6).map((task) => (
            <article key={task.id} style={{ borderTop: '1px solid #1f1f2a', padding: '0.55rem 0' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', gap: '0.5rem' }}>
                <strong style={{ color: '#e6e6f0', fontSize: '0.86rem' }}>{task.title}</strong>
                <span style={{ color: statusColor(task.status), fontSize: '0.72rem' }}>{task.status}</span>
              </div>
              <div style={{ color: '#7a7a8c', fontSize: '0.72rem' }}>{task.model_mode} · {task.repo_path}</div>
            </article>
          ))}
        </section>

        <section style={cardStyle}>
          <PanelTitle color="#34d399" tip="Runners are worker processes. Runs are task executions created when a runner leases work. A runner can be online even when no run is active.">
            Runs + runners
          </PanelTitle>
          {data.runners.length === 0 && <p style={{ color: '#7a7a8c', fontSize: '0.84rem' }}>○ no runner heartbeat yet</p>}
          {data.runners.map((runner) => (
            <article key={runner.id} style={{ borderTop: '1px solid #1f1f2a', padding: '0.55rem 0' }}>
              <div style={{ color: statusColor(runner.status) }}>{runner.label} · {runner.status}</div>
              <div style={{ color: '#7a7a8c', fontSize: '0.72rem' }}>{runner.capabilities.join(', ')}</div>
            </article>
          ))}
          {data.runs.slice(0, 4).map((run) => (
            <article key={run.id} style={{ borderTop: '1px solid #1f1f2a', padding: '0.55rem 0' }}>
              <code style={{ color: '#7dd3fc' }}>{run.id}</code>
              <div style={{ color: statusColor(run.status), fontSize: '0.72rem' }}>{run.status} · {run.model_mode}</div>
            </article>
          ))}
        </section>

        <section style={cardStyle}>
          <PanelTitle color="#c084fc" tip="Sensorium tools are runner capabilities for observing and testing the app: screenshots, pixel diffs, traces, frame stats, git, memory, tests, and related evidence.">
            Sensorium tools
          </PanelTitle>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: '0.35rem', marginBottom: '0.7rem' }}>
            {toolGroups.map(([group, count]) => (
              <span key={group} style={{ border: '1px solid #2a2a3a', borderRadius: 999, padding: '0.15rem 0.55rem', color: '#cdd6e4', fontSize: '0.72rem' }}>
                {group}:{count}
              </span>
            ))}
          </div>
          {data.tools.slice(0, 8).map((tool) => (
            <div key={tool.name} style={{ color: tool.approval_gate ? '#fbbf24' : '#7dd3fc', fontSize: '0.78rem', margin: '0.3rem 0' }}>
              {tool.name}
            </div>
          ))}
        </section>

        <section style={cardStyle}>
          <PanelTitle color="#a78bfa" tip="Fleet entries describe model routes and policy: privacy class, default model, cost cap, and whether human review is required.">
            Fleet
          </PanelTitle>
          {data.fleet.map((cfg) => (
            <article key={cfg.id}>
              <div style={{ color: '#e6e6f0' }}>{cfg.default_model_mode}</div>
              <div style={{ color: '#7a7a8c', fontSize: '0.78rem' }}>
                {cfg.privacy_class} · cap ${cfg.max_cost_usd_per_run} · review {cfg.review_required ? 'required' : 'optional'}
              </div>
            </article>
          ))}
        </section>
      </section>
    </AdminLayout>
  );
};

export function _testExportsAreFunctions(): boolean {
  return typeof LazarusConsole === 'function';
}

export default LazarusConsole;
