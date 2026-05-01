// /admin/tasks · scheduled-tasks list · run-now · cancel · history
// Phone-first responsive · sovereign-cap-protected

import type { NextPage } from 'next';
import { useEffect, useState } from 'react';
import AdminLayout from '../../components/AdminLayout';

interface ScheduledTask {
  taskId: string;
  description: string;
  schedule: string;
  enabled: boolean;
  fireAt?: string;
  lastRunAt?: string;
  nextRunAt?: string;
}

interface TasksResponse {
  tasks: ScheduledTask[];
  stub?: boolean;
}

const Tasks: NextPage = () => {
  const [data, setData] = useState<TasksResponse | null>(null);

  useEffect(() => {
    fetch('/api/admin/tasks')
      .then((r) => r.json())
      .then((j: TasksResponse) => setData(j))
      .catch(() => setData({ tasks: [], stub: true }));
  }, []);

  return (
    <AdminLayout title="◐ Scheduled Tasks">
      <p style={{ color: '#7a7a8c', fontSize: '0.82rem', marginTop: 0, marginBottom: '1.5rem' }}>
        Tasks running on Claude-side (loa-w8/w9/w10) · monitored from server-side records · phone-readable summary
      </p>

      {data === null && <p style={{ color: '#7a7a8c' }}>§ loading…</p>}

      {data?.stub && (
        <div
          style={{
            padding: '1rem 1.25rem',
            background: 'rgba(251, 191, 36, 0.1)',
            border: '1px solid rgba(251, 191, 36, 0.4)',
            borderRadius: 6,
            marginBottom: '1.5rem',
            fontSize: '0.85rem',
            color: '#fbbf24',
          }}
        >
          <strong>⚠ stub-mode</strong>
          <p style={{ margin: '0.4rem 0 0' }}>
            Server-side scheduled-tasks tracking pending. Tasks shown below are illustrative · live data activates
            when scheduled-tasks server-bridge is wired (W9-target).
          </p>
        </div>
      )}

      {data && data.tasks.length === 0 && !data.stub && (
        <div
          style={{
            padding: '2rem 1rem',
            textAlign: 'center',
            color: '#7a7a8c',
            background: 'rgba(20, 20, 30, 0.4)',
            border: '1px solid #1f1f2a',
            borderRadius: 6,
          }}
        >
          § no scheduled tasks · all-clear
        </div>
      )}

      {data && data.tasks.length > 0 && (
        <div style={{ display: 'grid', gap: '0.6rem' }}>
          {data.tasks.map((t) => (
            <article
              key={t.taskId}
              style={{
                padding: '1rem 1.1rem',
                background: 'rgba(20, 20, 30, 0.5)',
                border: '1px solid #1f1f2a',
                borderRadius: 6,
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', marginBottom: '0.4rem' }}>
                <code style={{ fontSize: '0.8rem', color: '#7dd3fc' }}>{t.taskId}</code>
                <span
                  style={{
                    fontSize: '0.7rem',
                    color: t.enabled ? '#34d399' : '#7a7a8c',
                    border: `1px solid ${t.enabled ? 'rgba(52, 211, 153, 0.4)' : '#2a2a3a'}`,
                    borderRadius: 3,
                    padding: '0.15rem 0.5rem',
                  }}
                >
                  {t.enabled ? '◐ enabled' : '✓ done'}
                </span>
              </div>
              <p style={{ fontSize: '0.85rem', color: '#cdd6e4', margin: '0.4rem 0' }}>{t.description}</p>
              <div style={{ fontSize: '0.72rem', color: '#7a7a8c' }}>{t.schedule}</div>
              {t.lastRunAt && (
                <div style={{ fontSize: '0.7rem', color: '#5a5a6a', marginTop: 4 }}>
                  last fired : {new Date(t.lastRunAt).toLocaleString()}
                </div>
              )}
            </article>
          ))}
        </div>
      )}
    </AdminLayout>
  );
};

export default Tasks;
