// § CycleCounter · single counter card with glyph + label + value + delta-since-cycle
// Used for self-author/playtest/kan-rollup/mycelium-sync/sigma-anchors counts
// W14-M live-engine-status-page

import type { CSSProperties } from 'react';
import { fmtCompact, fmtRelTime } from '../../lib/engine-status';

interface CounterProps {
  glyph: string;
  label: string;
  value: number | null | undefined;
  accent: string;
  lastEventTs?: number | null;
  now: number;
  subline?: string;
  stub?: boolean;
  bytesPerCycle?: number | null;
}

const C = {
  border: '#1f1f2a',
  cardBg: 'rgba(20, 20, 30, 0.5)',
  textDim: '#7a7a8c',
  text: '#cdd6e4',
  accentLavender: '#a78bfa',
};

export default function CycleCounter(props: CounterProps): JSX.Element {
  const { glyph, label, value, accent, lastEventTs, now, subline, stub, bytesPerCycle } = props;

  const cardStyle: CSSProperties = {
    background: C.cardBg,
    border: `1px solid ${stub ? 'rgba(251, 191, 36, 0.3)' : C.border}`,
    borderRadius: 6,
    padding: '0.85rem 0.95rem',
    minHeight: 96,
    display: 'flex',
    flexDirection: 'column',
    justifyContent: 'space-between',
  };

  return (
    <div style={cardStyle} role="group" aria-label={`${label} counter`}>
      <div>
        <div
          style={{
            fontSize: '0.65rem',
            textTransform: 'uppercase',
            letterSpacing: '0.18em',
            color: C.textDim,
            marginBottom: 6,
            whiteSpace: 'nowrap',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
          }}
        >
          <span style={{ color: C.accentLavender, marginRight: 6 }}>{glyph}</span>
          {label}
        </div>
        <div
          style={{
            fontSize: '1.6rem',
            color: stub ? C.textDim : accent,
            fontWeight: 700,
            lineHeight: 1,
            fontFamily: 'inherit',
          }}
        >
          {stub ? '◐ —' : fmtCompact(value ?? 0)}
        </div>
        {subline && (
          <div style={{ fontSize: '0.7rem', color: C.textDim, marginTop: 5 }}>
            {subline}
          </div>
        )}
      </div>

      <div
        style={{
          marginTop: 8,
          fontSize: '0.65rem',
          color: C.textDim,
          display: 'flex',
          justifyContent: 'space-between',
          gap: 6,
        }}
      >
        <span>
          {lastEventTs !== undefined && lastEventTs !== null && lastEventTs > 0
            ? `Δ last: ${fmtRelTime(lastEventTs, now)}`
            : 'Δ last: —'}
        </span>
        {bytesPerCycle !== undefined && bytesPerCycle !== null && bytesPerCycle > 0 && (
          <span style={{ color: accent, opacity: 0.65 }}>
            ω {bytesPerCycle.toFixed(0)} B/cy
          </span>
        )}
      </div>
    </div>
  );
}
