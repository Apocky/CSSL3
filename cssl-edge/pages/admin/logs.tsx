// /admin/logs · live audit-event stream from cssl-host-attestation
// Stub-mode-aware · activates with Apocky-Hub Supabase

import type { NextPage } from 'next';
import { useEffect, useState } from 'react';
import AdminLayout from '../../components/AdminLayout';

interface AuditRow {
  ts: string;
  source: string;
  level: string;
  kind: string;
  message: string;
}

interface LogsResponse {
  rows: AuditRow[];
  stub?: boolean;
}

const Logs: NextPage = () => {
  const [data, setData] = useState<LogsResponse | null>(null);
  const [filter, setFilter] = useState('');

  useEffect(() => {
    fetch('/api/admin/logs')
      .then((r) => r.json())
      .then((j: LogsResponse) => setData(j))
      .catch(() => setData({ rows: [], stub: true }));
  }, []);

  const filtered = (data?.rows ?? []).filter((r) =>
    !filter ? true : (r.kind + r.message).toLowerCase().includes(filter.toLowerCase()),
  );

  return (
    <AdminLayout title="✓ Audit Logs">
      <p style={{ color: '#7a7a8c', fontSize: '0.82rem', marginTop: 0, marginBottom: '1rem' }}>
        Live audit-event stream from cssl-host-attestation · Σ-Chain TIER-1 (LOCAL) · readable from phone via bridge
      </p>

      {data?.stub && (
        <div
          style={{
            padding: '1rem 1.25rem',
            background: 'rgba(251, 191, 36, 0.1)',
            border: '1px solid rgba(251, 191, 36, 0.4)',
            borderRadius: 6,
            marginBottom: '1rem',
            fontSize: '0.82rem',
            color: '#fbbf24',
          }}
        >
          <strong>⚠ stub-mode</strong>
          <p style={{ margin: '0.4rem 0 0' }}>
            Live audit-events live in cssl-host-attestation rt-trace ring (LOCAL). Bridge activates with
            Apocky-Hub Supabase + desktop relay.
          </p>
        </div>
      )}

      <input
        type="text"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        placeholder="filter logs…"
        style={{
          width: '100%',
          padding: '0.65rem 0.85rem',
          background: 'rgba(20, 20, 30, 0.7)',
          border: '1px solid #2a2a3a',
          borderRadius: 4,
          color: '#e6e6f0',
          fontSize: '0.92rem',
          outline: 'none',
          marginBottom: '1rem',
          minHeight: 44,
          fontFamily: 'inherit',
        }}
      />

      {filtered.length === 0 ? (
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
          § no events {filter ? `matching "${filter}"` : '· stream is quiet'}
        </div>
      ) : (
        <div style={{ display: 'grid', gap: '0.35rem' }}>
          {filtered.map((r, i) => (
            <article
              key={i}
              style={{
                padding: '0.65rem 0.85rem',
                background: 'rgba(20, 20, 30, 0.4)',
                border: '1px solid #1f1f2a',
                borderRadius: 4,
                fontSize: '0.78rem',
              }}
            >
              <div style={{ display: 'flex', gap: '0.5rem', marginBottom: 4, flexWrap: 'wrap' }}>
                <span style={{ color: '#7a7a8c' }}>{new Date(r.ts).toLocaleTimeString()}</span>
                <code style={{ color: '#a78bfa' }}>{r.source}</code>
                <code style={{ color: r.level === 'ERROR' ? '#f87171' : r.level === 'WARN' ? '#fbbf24' : '#7dd3fc' }}>
                  {r.level}
                </code>
                <code style={{ color: '#cdd6e4' }}>{r.kind}</code>
              </div>
              <div style={{ color: '#cdd6e4', wordBreak: 'break-word' }}>{r.message}</div>
            </article>
          ))}
        </div>
      )}
    </AdminLayout>
  );
};

export default Logs;
