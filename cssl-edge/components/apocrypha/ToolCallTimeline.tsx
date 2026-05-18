// ToolCallTimeline — live cockpit feed of recent tool calls.
//
// Polls /api/admin/apocrypha/tool_calls every 2s ; renders newest-first ; filter
// by tool-name substring. v0 uses polling per A4 scope ; WS path /ws/tools exists
// on the Apocrypha backend for desktop-app subscribers.

import { useMemo, useState } from 'react';

import { useToolCallPoll } from '../../lib/apocrypha/useToolCallPoll';

export function ToolCallTimeline() {
  const { records, loading, error, lastFetch } = useToolCallPoll(2000, 200);
  const [filter, setFilter] = useState('');
  const [okFilter, setOkFilter] = useState<'all' | 'ok' | 'err'>('all');

  const filtered = useMemo(() => {
    const f = filter.trim().toLowerCase();
    return records.filter((r) => {
      if (f && !r.tool_name.toLowerCase().includes(f)) return false;
      if (okFilter === 'ok' && !r.ok) return false;
      if (okFilter === 'err' && r.ok) return false;
      return true;
    });
  }, [records, filter, okFilter]);

  return (
    <div style={{
      display: 'flex',
      flexDirection: 'column',
      gap: '0.6rem',
      color: '#cdd6e4',
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
      height: '100%',
    }}>
      <div style={{
        display: 'flex',
        gap: '0.5rem',
        alignItems: 'center',
        padding: '0.35rem 0.6rem',
        borderBottom: '1px solid #2a2a3a',
        fontSize: '0.85rem',
      }}>
        <span>§ Tool-call timeline</span>
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="filter by tool-name …"
          style={{
            flex: 1,
            background: '#0a0a10',
            border: '1px solid #2a2a3a',
            color: '#cdd6e4',
            padding: '0.25rem 0.5rem',
            fontSize: '0.82rem',
            borderRadius: 3,
            fontFamily: 'inherit',
          }}
        />
        <select
          value={okFilter}
          onChange={(e) => setOkFilter(e.target.value as 'all' | 'ok' | 'err')}
          style={{
            background: '#0a0a10',
            border: '1px solid #2a2a3a',
            color: '#cdd6e4',
            padding: '0.25rem 0.4rem',
            fontSize: '0.82rem',
            borderRadius: 3,
            fontFamily: 'inherit',
          }}
        >
          <option value="all">all</option>
          <option value="ok">ok only</option>
          <option value="err">err only</option>
        </select>
        <span style={{ color: '#7a7a8c', fontSize: '0.75rem' }}>
          {filtered.length}/{records.length}{loading ? ' · loading' : ''}
        </span>
      </div>

      {error && (
        <div style={{ color: '#ff8888', fontSize: '0.85rem', padding: '0.3rem 0.6rem' }}>
          error : {error}
        </div>
      )}

      <div style={{
        flex: 1,
        overflowY: 'auto',
        padding: '0 0.6rem',
        fontSize: '0.82rem',
      }}>
        {filtered.length === 0 && !loading && (
          <div style={{ color: '#7a7a8c', padding: '0.5rem 0' }}>
            no tool calls yet · trigger one via /chat or wait for autonomous activity
          </div>
        )}
        {filtered.map((r) => (
          <div key={r.id} style={{
            display: 'grid',
            gridTemplateColumns: '32px 1fr auto auto',
            gap: '0.5rem',
            padding: '0.35rem 0.3rem',
            borderBottom: '1px solid #1a1a26',
            alignItems: 'baseline',
          }}>
            <span style={{
              color: r.ok ? '#7fd17f' : '#ff8888',
              fontWeight: 600,
              fontSize: '0.95rem',
            }}>
              {r.ok ? '✓' : '✗'}
            </span>
            <span style={{ color: r.ok ? '#cdd6e4' : '#ff8888' }}>
              {r.tool_name}
              {!r.ok && r.error && (
                <span style={{ color: '#aa6060', marginLeft: '0.5rem' }}>· {r.error.slice(0, 80)}</span>
              )}
            </span>
            <span style={{ color: '#7a7a8c' }}>{r.elapsed_ms}ms</span>
            <span style={{ color: '#7a7a8c' }}>
              ${r.cost_usd.toFixed(4)}
            </span>
          </div>
        ))}
      </div>

      {lastFetch && (
        <div style={{
          color: '#5a5a6c',
          fontSize: '0.7rem',
          padding: '0.25rem 0.6rem',
          borderTop: '1px solid #2a2a3a',
        }}>
          last fetch : {lastFetch.toLocaleTimeString()}
        </div>
      )}
    </div>
  );
}
