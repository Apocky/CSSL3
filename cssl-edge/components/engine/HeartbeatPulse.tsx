// § HeartbeatPulse · LOCAL/CLOUD node status with last-seen + uptime + animated pulse
// Phone-first · pure-CSS pulse · ¬ engagement-bait · subtle-by-default
// W14-M live-engine-status-page

import type { CSSProperties } from 'react';
import type { NodeHeartbeat } from '../../lib/engine-status';
import { fmtRelTime, fmtUptime } from '../../lib/engine-status';

const C = {
  border: '#1f1f2a',
  cardBg: 'rgba(20, 20, 30, 0.5)',
  textDim: '#7a7a8c',
  text: '#cdd6e4',
  online: '#34d399',
  stale: '#fbbf24',
  offline: '#7a7a8c',
  accentLavender: '#a78bfa',
};

interface PulseProps {
  label: 'LOCAL' | 'CLOUD';
  node: NodeHeartbeat;
  now: number;
  stub?: boolean;
}

function statusColor(node: NodeHeartbeat, now: number): string {
  if (!node || node.last_seen === 0) return C.offline;
  const dt = now - node.last_seen;
  if (dt < 30_000) return C.online;
  if (dt < 120_000) return C.stale;
  return C.offline;
}

function statusGlyph(node: NodeHeartbeat, now: number): string {
  if (!node || node.last_seen === 0) return '○';
  const dt = now - node.last_seen;
  if (dt < 30_000) return '✓';
  if (dt < 120_000) return '◐';
  return '○';
}

function statusText(node: NodeHeartbeat, now: number, stub?: boolean): string {
  if (stub) return 'stub';
  if (!node || node.last_seen === 0) return 'never-seen';
  const dt = now - node.last_seen;
  if (dt < 30_000) return 'online';
  if (dt < 120_000) return 'stale';
  return 'offline';
}

export default function HeartbeatPulse(props: PulseProps): JSX.Element {
  const { label, node, now, stub } = props;
  const color = stub ? C.stale : statusColor(node, now);
  const glyph = stub ? '◐' : statusGlyph(node, now);
  const status = statusText(node, now, stub);
  const isOnline = !stub && node.last_seen > 0 && now - node.last_seen < 30_000;

  const cardStyle: CSSProperties = {
    background: C.cardBg,
    border: `1px solid ${stub ? 'rgba(251, 191, 36, 0.3)' : C.border}`,
    borderRadius: 6,
    padding: '0.85rem 0.95rem',
    display: 'grid',
    gridTemplateColumns: 'auto 1fr auto',
    gap: '0.75rem',
    alignItems: 'center',
    minHeight: 64,
  };

  return (
    <div style={cardStyle}>
      <style>{`
        @keyframes engine-pulse {
          0% { transform: scale(1); opacity: 1; }
          50% { transform: scale(1.5); opacity: 0.5; }
          100% { transform: scale(1); opacity: 1; }
        }
        .engine-pulse-dot { animation: engine-pulse 2s ease-in-out infinite; }
        @media (prefers-reduced-motion: reduce) {
          .engine-pulse-dot { animation: none !important; }
        }
      `}</style>

      {/* status orb */}
      <div
        style={{
          width: 16,
          height: 16,
          borderRadius: '50%',
          background: color,
          boxShadow: isOnline ? `0 0 8px ${color}` : 'none',
        }}
        className={isOnline ? 'engine-pulse-dot' : ''}
        aria-hidden="true"
      />

      {/* label + status */}
      <div style={{ minWidth: 0 }}>
        <div
          style={{
            fontSize: '0.65rem',
            textTransform: 'uppercase',
            letterSpacing: '0.18em',
            color: C.textDim,
            marginBottom: 4,
          }}
        >
          <span style={{ color: C.accentLavender, marginRight: 6 }}>{glyph}</span>
          {label}
        </div>
        <div style={{ fontSize: '1.05rem', color, fontWeight: 700, lineHeight: 1 }}>
          {status}
        </div>
        <div style={{ fontSize: '0.7rem', color: C.textDim, marginTop: 4 }}>
          last-seen {stub ? '—' : fmtRelTime(node.last_seen, now)}
        </div>
      </div>

      {/* uptime */}
      <div style={{ textAlign: 'right', minWidth: 70 }}>
        <div
          style={{
            fontSize: '0.6rem',
            color: C.textDim,
            textTransform: 'uppercase',
            letterSpacing: '0.12em',
            marginBottom: 2,
          }}
        >
          uptime
        </div>
        <div style={{ fontSize: '0.85rem', color: C.text, fontFamily: 'inherit' }}>
          {stub ? '—' : fmtUptime(node.uptime_secs)}
        </div>
      </div>
    </div>
  );
}
