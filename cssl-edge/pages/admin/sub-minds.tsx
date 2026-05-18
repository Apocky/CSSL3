// /admin/sub-minds · Lazarus (Ω9 operator) + Tessera (Ω10 reasoner) health.
//
// Replaces the old /admin/tasks (LoA scheduling content). Apocrypha doesn't run
// user-scheduled tasks ; it runs SUB-MINDS (per D043 absorption). The closest
// equivalent to a "task queue" is Lazarus's operator task queue.

import type { NextPage } from 'next';
import { useEffect, useState } from 'react';

import AdminLayout from '../../components/AdminLayout';
import { subMindsHealth, type SubMindsHealth } from '../../lib/apocrypha/client';

const SubMinds: NextPage = () => {
  const [adminAuthorized, setAdminAuthorized] = useState(false);
  const [data, setData] = useState<SubMindsHealth | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    if (!adminAuthorized) return;
    const fetchOnce = async () => {
      try {
        setData(await subMindsHealth());
        setError(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setLoading(false);
      }
    };
    void fetchOnce();
    const t = setInterval(() => void fetchOnce(), 5000);
    return () => clearInterval(t);
  }, [adminAuthorized]);

  return (
    <AdminLayout title="Ω Sub-Minds" onAdminCheck={(c) => setAdminAuthorized(c.authorized)}>
      {!adminAuthorized ? (
        <div style={{ padding: '2rem', color: '#a0a0b0' }}>
          <p>Sub-mind status requires admin authentication.</p>
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '1rem', color: '#cdd6e4' }}>
          <p style={{ fontSize: '0.82rem', color: '#7a7a8c', marginTop: 0 }}>
            Apocrypha runs two sub-minds in-process (per D043 structural absorption — no IPC, no
            remote services). Auto-refresh every 5 seconds.
          </p>

          {loading && <div style={{ color: '#7a7a8c' }}>§ loading…</div>}
          {error && (
            <div style={{
              padding: '0.6rem 0.9rem',
              border: '1px solid rgba(255, 136, 136, 0.4)',
              background: 'rgba(255, 136, 136, 0.08)',
              color: '#ff8888',
              borderRadius: 6,
              fontSize: '0.85rem',
            }}>
              error : {error}
            </div>
          )}

          {data && (
            <div style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fit, minmax(360px, 1fr))',
              gap: '1rem',
            }}>
              <section
                title="Ω9 Lazarus : drains the task queue, leases tasks, dispatches workers"
                style={cardStyle}>
                <div style={{ display: 'flex', alignItems: 'baseline', gap: '0.5rem', marginBottom: '0.6rem' }}>
                  <span style={{ fontSize: '0.7rem', color: '#a78bfa', letterSpacing: '0.1em' }}>Ω9</span>
                  <h2 style={{ margin: 0, fontSize: '1.05rem', color: '#ffaa55' }}>
                    Lazarus
                  </h2>
                  <span style={{
                    marginLeft: 'auto',
                    fontSize: '0.7rem',
                    padding: '0.15rem 0.5rem',
                    borderRadius: 999,
                    background: data.lazarus.runner_loop_running
                      ? 'rgba(127, 209, 127, 0.18)'
                      : 'rgba(255, 136, 136, 0.18)',
                    color: data.lazarus.runner_loop_running ? '#9ddb9d' : '#ff8888',
                    border: `1px solid ${data.lazarus.runner_loop_running ? 'rgba(127, 209, 127, 0.3)' : 'rgba(255, 136, 136, 0.3)'}`,
                  }}>
                    {data.lazarus.runner_loop_running ? '● running' : '○ stopped'}
                  </span>
                </div>
                <div style={{ fontSize: '0.78rem', color: '#9aa0a6', marginBottom: '0.8rem' }}>
                  {data.lazarus.label} · Tier-2 proactive · drains tasks at 1Hz
                </div>
                <KV k="Runner ID" v={data.lazarus.runner_id ?? '—'} tip="In-process runner registered with the store" />
                <KV k="Tasks (total)" v={data.lazarus.task_count} tip="Total rows in lazarus_task table" />
                <KV k="Queued" v={data.lazarus.queued_count} tip="Tasks awaiting lease" highlight={data.lazarus.queued_count > 0} />
                <KV k="Active runs" v={data.lazarus.active_run_count} tip="Leased + currently running" />
                <KV k="Pending approvals" v={data.lazarus.pending_approval_count} tip="Approval gates awaiting decision" highlight={data.lazarus.pending_approval_count > 0} />
                <KV k="Online runners" v={data.lazarus.online_runner_count} tip="External + in-process runners online" />
                <KV k="In-flight dispatches" v={data.lazarus.in_flight_dispatches} tip="Currently executing (capped at max_concurrent_dispatches)" />
                <KV k="Tool catalog size" v={data.lazarus.tool_count} tip="LoA-tool catalog Lazarus can invoke per task" />
              </section>

              <section
                title="Ω10 Tessera : reactive HRR reasoning chains, optional Mamba CoT escape"
                style={cardStyle}>
                <div style={{ display: 'flex', alignItems: 'baseline', gap: '0.5rem', marginBottom: '0.6rem' }}>
                  <span style={{ fontSize: '0.7rem', color: '#a78bfa', letterSpacing: '0.1em' }}>Ω10</span>
                  <h2 style={{ margin: 0, fontSize: '1.05rem', color: '#c084fc' }}>
                    Tessera
                  </h2>
                  <span style={{
                    marginLeft: 'auto',
                    fontSize: '0.7rem',
                    padding: '0.15rem 0.5rem',
                    borderRadius: 999,
                    background: data.tessera.started
                      ? 'rgba(127, 209, 127, 0.18)'
                      : 'rgba(255, 136, 136, 0.18)',
                    color: data.tessera.started ? '#9ddb9d' : '#ff8888',
                    border: `1px solid ${data.tessera.started ? 'rgba(127, 209, 127, 0.3)' : 'rgba(255, 136, 136, 0.3)'}`,
                  }}>
                    {data.tessera.started ? '● ready' : '○ stopped'}
                  </span>
                </div>
                <div style={{ fontSize: '0.78rem', color: '#9aa0a6', marginBottom: '0.8rem' }}>
                  {data.tessera.label} · Tier-1 reactive · {'<'}200ms HRR chains
                </div>
                <KV k="Codebook size" v={data.tessera.codebook_size} tip="Number of (term, hypervector) entries available for HRR cleanup" />
                <KV k="Episodes" v={data.tessera.episode_count} tip="Episodic-memory rows the reasoner can recall" />
                <KV k="Escape configured" v={data.tessera.escape_configured ? 'yes (Mamba CoT)' : 'no'} tip="Whether low-confidence HRR chains escalate to Mamba" />
                <div style={{ marginTop: '0.6rem', fontSize: '0.72rem', color: '#5a5a6a' }}>
                  Reactive — no background loop ; activates per reason() call.
                </div>
              </section>
            </div>
          )}
        </div>
      )}
    </AdminLayout>
  );
};

function KV({ k, v, tip, highlight }: { k: string; v: string | number; tip?: string; highlight?: boolean }) {
  return (
    <div title={tip} style={{
      display: 'flex',
      justifyContent: 'space-between',
      padding: '0.3rem 0',
      borderBottom: '1px solid #1a1a26',
      fontSize: '0.82rem',
    }}>
      <span style={{ color: '#7a7a8c' }}>{k}</span>
      <span style={{ color: highlight ? '#fbbf24' : '#cdd6e4', fontWeight: highlight ? 600 : 400 }}>
        {v}
      </span>
    </div>
  );
}

const cardStyle: React.CSSProperties = {
  padding: '1rem 1.2rem',
  border: '1px solid #2a2a3a',
  borderRadius: 6,
  background: 'rgba(20, 20, 30, 0.4)',
};

export default SubMinds;
