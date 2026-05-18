// Apocrypha StatusBar — 32px sticky top bar
// Per Apocrypha/specs/11_DISPATCH_UI_UX.csl §LAYOUT + spec 12 unified-brand (D020)

import type { ApocryphaStatus } from '../../lib/apocrypha/useApocryphaStatus';

interface Props {
  status: ApocryphaStatus | null;
  loading: boolean;
  error: string | null;
}

function aliveLabel(s: ApocryphaStatus | null, loading: boolean): { text: string; color: string } {
  if (loading && !s) return { text: 'CONNECTING', color: '#7a7a8c' };
  if (!s) return { text: 'OFFLINE', color: '#f87171' };
  if (s.reachable && s.upstream_payload) return { text: 'ALIVE', color: '#34d399' };
  if (s.phase === 'stub') return { text: 'STUB', color: '#fbbf24' };
  return { text: 'OFFLINE', color: '#f87171' };
}

export function StatusBar({ status, loading, error }: Props) {
  const alive = aliveLabel(status, loading);
  const u = status?.upstream_payload;
  const cost = u?.spent_today_usd ?? 0;
  const cap = u?.daily_cap_usd ?? 0;
  const costRatio = cap > 0 ? Math.min(1, cost / cap) : 0;
  const tiers = u?.tiers_available;

  return (
    <div
      style={{
        height: 32,
        display: 'flex',
        alignItems: 'center',
        gap: '0.75rem',
        padding: '0 0.75rem',
        background: 'rgba(10, 10, 20, 0.85)',
        borderBottom: '1px solid #1f1f2a',
        fontSize: '0.72rem',
        color: '#cdd6e4',
        fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
      }}
    >
      <span style={{ color: '#ffaa55', fontWeight: 700 }}>§ apocrypha</span>
      <span style={{ color: '#5a5a6a' }}>v{u?.version ?? '?'}</span>

      <span
        title={status?.note ?? ''}
        style={{ display: 'inline-flex', alignItems: 'center', gap: '0.35rem' }}
      >
        <span
          style={{
            display: 'inline-block',
            width: 8,
            height: 8,
            borderRadius: '50%',
            background: alive.color,
            boxShadow: alive.text === 'ALIVE' ? `0 0 6px ${alive.color}` : 'none',
          }}
        />
        <span style={{ color: alive.color }}>{alive.text}</span>
      </span>

      {tiers && (
        <span style={{ display: 'inline-flex', gap: '0.25rem' }} title="Tiers available">
          <span style={{ color: tiers.tier0 ? '#888899' : '#3a3a4a' }}>T0</span>
          <span style={{ color: tiers.tier_a ? '#66aaff' : '#3a3a4a' }}>T-A</span>
          <span style={{ color: tiers.tier_b ? '#ff5566' : '#3a3a4a' }}>T-B</span>
        </span>
      )}

      <span
        title={`$${cost.toFixed(4)} of $${cap.toFixed(2)} daily cap`}
        style={{ display: 'inline-flex', alignItems: 'center', gap: '0.35rem' }}
      >
        <span style={{ color: '#5a5a6a' }}>$</span>
        <span
          aria-hidden="true"
          style={{
            display: 'inline-block',
            width: 60,
            height: 6,
            background: '#1f1f2a',
            borderRadius: 2,
            overflow: 'hidden',
          }}
        >
          <span
            style={{
              display: 'block',
              width: `${costRatio * 100}%`,
              height: '100%',
              background: costRatio > 0.8 ? '#f87171' : costRatio > 0.5 ? '#fbbf24' : '#34d399',
            }}
          />
        </span>
        <span style={{ color: '#cdd6e4', minWidth: 64 }}>
          ${cost.toFixed(2)}/${cap.toFixed(0)}
        </span>
      </span>

      <span style={{ flex: 1 }} />

      {error && (
        <span style={{ color: '#f87171', fontSize: '0.68rem' }}>{error.slice(0, 80)}</span>
      )}

      {status?.served_by && (
        <span style={{ color: '#5a5a6a' }}>served-by {status.served_by}</span>
      )}

      <button
        type="button"
        title="Kill switch (creates state/kill-switch file → next-tick halt)"
        aria-label="Kill switch"
        style={{
          border: '1px solid #2a2a3a',
          background: 'transparent',
          color: '#f87171',
          borderRadius: 4,
          padding: '2px 8px',
          fontSize: '0.7rem',
          cursor: 'pointer',
          fontFamily: 'inherit',
        }}
      >
        ⏻ kill
      </button>
    </div>
  );
}
