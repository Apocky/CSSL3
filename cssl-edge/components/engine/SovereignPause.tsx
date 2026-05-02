// § SovereignPause · Apocky-only toggle to halt-engine globally · cap-gated · public-readable status
// W14-M live-engine-status-page · transparency-mandate

import type { CSSProperties } from 'react';
import { useState } from 'react';
import type { SovereignPauseState } from '../../lib/engine-status';
import { fmtRelTime, postSovereignPause, sanitizePubkey } from '../../lib/engine-status';

const C = {
  border: '#1f1f2a',
  cardBg: 'rgba(20, 20, 30, 0.5)',
  textDim: '#7a7a8c',
  text: '#cdd6e4',
  accentRose: '#f87171',
  accentMint: '#34d399',
  accentAmber: '#fbbf24',
  accentLavender: '#a78bfa',
};

interface SovereignPauseProps {
  state: SovereignPauseState | null;
  now: number;
  // I> caller passes cap-mask · 0 means visitor (read-only) · positive means sovereign-authorized
  cap?: number;
  onToggle?: (paused: boolean) => void;
  stub?: boolean;
}

export default function SovereignPause(props: SovereignPauseProps): JSX.Element {
  const { state, now, cap = 0, onToggle, stub } = props;
  const [busy, setBusy] = useState(false);
  const [lastResult, setLastResult] = useState<string | null>(null);

  const paused = state?.paused === true;
  const hasCap = cap > 0; // ⊑ any sovereign-cap bit allows toggle attempt

  const accent = paused ? C.accentRose : C.accentMint;
  const headline = stub
    ? 'pause-state unknown'
    : paused
      ? 'engine paused'
      : 'engine running';

  const cardStyle: CSSProperties = {
    background: C.cardBg,
    border: `1px solid ${
      stub ? 'rgba(251, 191, 36, 0.3)' : paused ? 'rgba(248, 113, 113, 0.4)' : C.border
    }`,
    borderRadius: 6,
    padding: '0.85rem 0.95rem',
  };

  const buttonStyle: CSSProperties = {
    width: '100%',
    padding: '0.65rem 0.85rem',
    marginTop: 12,
    background: hasCap ? (paused ? 'rgba(52, 211, 153, 0.12)' : 'rgba(248, 113, 113, 0.12)') : 'transparent',
    border: `1px solid ${hasCap ? (paused ? 'rgba(52, 211, 153, 0.5)' : 'rgba(248, 113, 113, 0.5)') : C.border}`,
    borderRadius: 4,
    color: hasCap ? (paused ? C.accentMint : C.accentRose) : C.textDim,
    fontFamily: 'inherit',
    fontSize: '0.82rem',
    cursor: hasCap && !busy ? 'pointer' : 'not-allowed',
    transition: 'background 0.15s',
    letterSpacing: '0.04em',
  };

  async function onClick(): Promise<void> {
    if (!hasCap || busy) return;
    setBusy(true);
    setLastResult(null);
    try {
      const next = !paused;
      const r = await postSovereignPause({ cap, pause: next });
      if (r.ok) {
        setLastResult(`✓ ${r.paused ? 'paused' : 'resumed'}`);
        onToggle?.(r.paused);
      } else {
        setLastResult(`✗ ${r.reason ?? 'failed'}`);
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <div style={cardStyle}>
      <div
        style={{
          fontSize: '0.65rem',
          textTransform: 'uppercase',
          letterSpacing: '0.18em',
          color: C.textDim,
          marginBottom: 6,
        }}
      >
        <span style={{ color: C.accentLavender, marginRight: 6 }}>⊘</span>
        sovereign-pause
      </div>
      <div
        style={{
          fontSize: '1.1rem',
          color: stub ? C.textDim : accent,
          fontWeight: 700,
          lineHeight: 1.1,
        }}
      >
        {paused ? '⊘ ' : '✓ '}{headline}
      </div>

      {state && state.paused && state.by && (
        <div style={{ fontSize: '0.7rem', color: C.textDim, marginTop: 4 }}>
          paused by 0x{sanitizePubkey(state.by)}…
          {state.since ? ` · ${fmtRelTime(state.since, now)}` : ''}
        </div>
      )}
      {state && state.reason && (
        <div style={{ fontSize: '0.7rem', color: C.textDim, marginTop: 4, fontStyle: 'italic' }}>
          {state.reason}
        </div>
      )}

      <button
        type="button"
        onClick={onClick}
        disabled={!hasCap || busy}
        style={buttonStyle}
        aria-label={paused ? 'resume engine' : 'pause engine'}
        title={hasCap ? 'sovereign-cap detected · toggle enabled' : 'sovereign-cap required · view-only'}
      >
        {busy
          ? '◐ working…'
          : !hasCap
            ? '○ view-only · sovereign-cap required'
            : paused
              ? '✓ resume engine'
              : '⊘ pause engine'}
      </button>

      {lastResult && (
        <div
          style={{
            marginTop: 8,
            fontSize: '0.72rem',
            color: lastResult.startsWith('✓') ? C.accentMint : C.accentAmber,
          }}
        >
          {lastResult}
        </div>
      )}

      <div style={{ marginTop: 10, fontSize: '0.65rem', color: C.textDim, lineHeight: 1.4 }}>
        ◇ public-readable · only Apocky w/ sovereign-cap may toggle · all-actions Σ-Chain-anchored
      </div>
    </div>
  );
}
