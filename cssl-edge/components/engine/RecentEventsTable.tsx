// § RecentEventsTable · last-50 events · scrollable · k-anon-respecting summaries
// W14-M live-engine-status-page · privacy-default · Σ-Chain anchor reveal optional

import type { CSSProperties } from 'react';
import { useMemo } from 'react';
import type { EngineEvent, EventKind } from '../../lib/engine-status';
import { fmtRelTime } from '../../lib/engine-status';

const C = {
  border: '#1f1f2a',
  cardBg: 'rgba(20, 20, 30, 0.5)',
  rowAlt: 'rgba(255, 255, 255, 0.015)',
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

const KIND_LABEL: Record<EventKind, string> = {
  self_author: 'self-author',
  playtest: 'playtest',
  kan_rollup: 'kan-rollup',
  mycelium_sync: 'mycelium-sync',
  sigma_anchor: 'sigma-anchor',
  idle_enter: 'idle-enter',
  idle_exit: 'idle-exit',
  sovereign_pause: 'sov-pause',
  sovereign_resume: 'sov-resume',
};

interface RecentEventsTableProps {
  events: EngineEvent[];
  now: number;
  stub?: boolean;
  limit?: number;
}

export default function RecentEventsTable(props: RecentEventsTableProps): JSX.Element {
  const { events, now, stub, limit = 50 } = props;

  const rows = useMemo(() => {
    if (!Array.isArray(events)) return [];
    // ⊑ defensive: most-recent-first · already sorted in API but enforce here
    return [...events].sort((a, b) => b.ts - a.ts).slice(0, limit);
  }, [events, limit]);

  const cardStyle: CSSProperties = {
    background: C.cardBg,
    border: `1px solid ${stub ? 'rgba(251, 191, 36, 0.3)' : C.border}`,
    borderRadius: 6,
    padding: '0.85rem 0.95rem',
  };

  return (
    <div style={cardStyle}>
      <div
        style={{
          fontSize: '0.65rem',
          textTransform: 'uppercase',
          letterSpacing: '0.18em',
          color: C.textDim,
          marginBottom: 8,
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
        }}
      >
        <span>
          <span style={{ color: C.accentLavender, marginRight: 6 }}>⌖</span>
          recent events · last {limit}
        </span>
        <span style={{ color: C.textDim, opacity: 0.6 }}>
          {stub ? '◐ stub' : `${rows.length} entries`}
        </span>
      </div>

      {stub ? (
        <div style={{ fontSize: '0.78rem', color: C.textDim, fontStyle: 'italic', padding: '0.5rem 0' }}>
          ◐ engine-orchestrator (W14-J/K) not deployed · zero-state · ¬ ghost-events
        </div>
      ) : rows.length === 0 ? (
        <div style={{ fontSize: '0.78rem', color: C.textDim, padding: '0.5rem 0' }}>
          ○ no events yet · engine waking up
        </div>
      ) : (
        <div
          style={{
            maxHeight: 480,
            overflowY: 'auto',
            border: `1px solid ${C.border}`,
            borderRadius: 4,
          }}
        >
          <table
            style={{
              width: '100%',
              borderCollapse: 'collapse',
              fontSize: '0.75rem',
              tableLayout: 'fixed',
            }}
          >
            <thead
              style={{
                position: 'sticky',
                top: 0,
                background: 'rgba(15, 15, 22, 0.96)',
                backdropFilter: 'blur(4px)',
                WebkitBackdropFilter: 'blur(4px)',
              }}
            >
              <tr>
                <th
                  style={{
                    textAlign: 'left',
                    padding: '6px 8px',
                    color: C.textDim,
                    fontWeight: 400,
                    fontSize: '0.68rem',
                    textTransform: 'uppercase',
                    letterSpacing: '0.1em',
                    borderBottom: `1px solid ${C.border}`,
                    width: '6.5rem',
                  }}
                >
                  when
                </th>
                <th
                  style={{
                    textAlign: 'left',
                    padding: '6px 8px',
                    color: C.textDim,
                    fontWeight: 400,
                    fontSize: '0.68rem',
                    textTransform: 'uppercase',
                    letterSpacing: '0.1em',
                    borderBottom: `1px solid ${C.border}`,
                    width: '7rem',
                  }}
                >
                  kind
                </th>
                <th
                  style={{
                    textAlign: 'left',
                    padding: '6px 8px',
                    color: C.textDim,
                    fontWeight: 400,
                    fontSize: '0.68rem',
                    textTransform: 'uppercase',
                    letterSpacing: '0.1em',
                    borderBottom: `1px solid ${C.border}`,
                  }}
                >
                  summary
                </th>
                <th
                  style={{
                    textAlign: 'right',
                    padding: '6px 8px',
                    color: C.textDim,
                    fontWeight: 400,
                    fontSize: '0.68rem',
                    textTransform: 'uppercase',
                    letterSpacing: '0.1em',
                    borderBottom: `1px solid ${C.border}`,
                    width: '6rem',
                  }}
                >
                  Σ
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((ev, idx) => (
                <tr
                  key={`${ev.ts}-${idx}`}
                  style={{
                    background: idx % 2 === 0 ? 'transparent' : C.rowAlt,
                  }}
                >
                  <td
                    style={{
                      padding: '6px 8px',
                      color: C.textDim,
                      whiteSpace: 'nowrap',
                      fontFamily: 'inherit',
                    }}
                  >
                    {fmtRelTime(ev.ts, now)}
                  </td>
                  <td style={{ padding: '6px 8px' }}>
                    <span style={{ color: KIND_COLOR[ev.kind] ?? C.text, marginRight: 6 }}>
                      {KIND_GLYPH[ev.kind] ?? '·'}
                    </span>
                    <span style={{ color: C.text, fontSize: '0.7rem' }}>
                      {KIND_LABEL[ev.kind] ?? ev.kind}
                    </span>
                  </td>
                  <td
                    style={{
                      padding: '6px 8px',
                      color: C.text,
                      overflow: 'hidden',
                      textOverflow: 'ellipsis',
                      whiteSpace: 'nowrap',
                    }}
                    title={ev.summary || ''}
                  >
                    {ev.summary || <span style={{ color: C.textDim }}>—</span>}
                  </td>
                  <td
                    style={{
                      padding: '6px 8px',
                      textAlign: 'right',
                      color: ev.sigma_chain_anchor ? C.accentMint : C.textDim,
                      fontFamily: 'inherit',
                      fontSize: '0.68rem',
                    }}
                  >
                    {ev.sigma_chain_anchor ? (
                      <span title={`Σ-Chain anchor: ${ev.sigma_chain_anchor}`}>
                        ◇{ev.sigma_chain_anchor.slice(0, 6)}
                      </span>
                    ) : (
                      <span aria-hidden="true">·</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <div style={{ marginTop: 8, fontSize: '0.65rem', color: C.textDim, lineHeight: 1.4 }}>
        ◇ k-anon-respecting summaries · ¬ PII · Σ-Chain anchors visible per-event when present
      </div>
    </div>
  );
}
