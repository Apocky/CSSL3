// § LiveFeed · streaming "happening now" snippet · low-emphasis · auto-shifts on new events
// W14-M live-engine-status-page · privacy-respecting summaries only

import type { CSSProperties } from 'react';
import { useMemo } from 'react';
import type { EngineEvent, EventKind } from '../../lib/engine-status';
import { fmtRelTime } from '../../lib/engine-status';

const C = {
  border: '#1f1f2a',
  cardBg: 'rgba(20, 20, 30, 0.5)',
  textDim: '#7a7a8c',
  text: '#cdd6e4',
  accentPurple: '#c084fc',
  accentBlue: '#7dd3fc',
  accentAmber: '#fbbf24',
  accentMint: '#34d399',
  accentLavender: '#a78bfa',
  accentRose: '#f87171',
};

const KIND_GLYPH: Record<EventKind, string> = {
  self_author: '✍',
  playtest: '⊑',
  kan_rollup: '∂',
  mycelium_sync: '⌬',
  sigma_anchor: '◇',
  idle_enter: '○',
  idle_exit: '◐',
  sovereign_pause: '⊘',
  sovereign_resume: '✓',
};

const KIND_COLOR: Record<EventKind, string> = {
  self_author: C.accentPurple,
  playtest: C.accentBlue,
  kan_rollup: C.accentAmber,
  mycelium_sync: C.accentLavender,
  sigma_anchor: C.accentMint,
  idle_enter: C.textDim,
  idle_exit: C.text,
  sovereign_pause: C.accentRose,
  sovereign_resume: C.accentMint,
};

interface LiveFeedProps {
  events: EngineEvent[];
  now: number;
  stub?: boolean;
  glyph?: string;
  title?: string;
  windowMs?: number; // ⊑ cap shown window for "happening now" emphasis
}

export default function LiveFeed(props: LiveFeedProps): JSX.Element {
  const { events, now, stub, glyph = '∿', title = 'live feed', windowMs = 60_000 } = props;

  const recent = useMemo(() => {
    if (!Array.isArray(events)) return [];
    const cutoff = now - windowMs;
    return events.filter((e) => e.ts >= cutoff).slice(0, 5);
  }, [events, now, windowMs]);

  const cardStyle: CSSProperties = {
    background: C.cardBg,
    border: `1px solid ${stub ? 'rgba(251, 191, 36, 0.3)' : C.border}`,
    borderRadius: 6,
    padding: '0.85rem 0.95rem',
  };

  return (
    <div style={cardStyle} aria-live="polite" aria-atomic="false">
      <div
        style={{
          fontSize: '0.65rem',
          textTransform: 'uppercase',
          letterSpacing: '0.18em',
          color: C.textDim,
          marginBottom: 8,
        }}
      >
        <span style={{ color: C.accentLavender, marginRight: 6 }}>{glyph}</span>
        {title}
        <span style={{ float: 'right', color: C.textDim, opacity: 0.6 }}>
          last {Math.floor(windowMs / 1000)}s · {recent.length} events
        </span>
      </div>

      {stub ? (
        <div style={{ fontSize: '0.78rem', color: C.textDim, fontStyle: 'italic' }}>
          ◐ engine-orchestrator (W14-J/K) not deployed · zero-state
        </div>
      ) : recent.length === 0 ? (
        <div style={{ fontSize: '0.78rem', color: C.textDim }}>
          ○ idle · no recent activity
        </div>
      ) : (
        <ul style={{ margin: 0, padding: 0, listStyle: 'none' }}>
          {recent.map((ev, idx) => (
            <li
              key={`${ev.ts}-${idx}`}
              style={{
                display: 'grid',
                gridTemplateColumns: 'auto 1fr auto',
                gap: 8,
                padding: '4px 0',
                fontSize: '0.78rem',
                borderTop: idx > 0 ? `1px solid ${C.border}` : 'none',
                alignItems: 'baseline',
              }}
            >
              <span style={{ color: KIND_COLOR[ev.kind] ?? C.text, fontSize: '0.85rem' }}>
                {KIND_GLYPH[ev.kind] ?? '·'}
              </span>
              <span
                style={{
                  color: C.text,
                  whiteSpace: 'nowrap',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                }}
              >
                {ev.summary || ev.kind.replace('_', '-')}
              </span>
              <span style={{ color: C.textDim, fontSize: '0.7rem' }}>
                {fmtRelTime(ev.ts, now)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
