// Apocrypha OrganRack — right rail (280px) showing all 10 organs' state
// Per spec 11 §LIVE-PANE + spec 12 D020 (unified : Ω1..Ω8 + Ω9 OPERATOR + Ω10 REASONER)
// Phase-2 : status-only with mock readings ; Phase-3 wires /ws/live for real ticks.

import type { ApocryphaStatus } from '../../lib/apocrypha/useApocryphaStatus';

type OrganState = 'alive' | 'idle' | 'offline' | 'pending';

interface Organ {
  id: string;
  glyph: string;
  name: string;
  spec: string;
  layer: 'Λ-C' | 'Λ-S' | 'Λ-U';
}

const ORGANS: ReadonlyArray<Organ> = [
  { id: 'O1', glyph: '◐', name: 'CfC swarm', spec: '01', layer: 'Λ-S' },
  { id: 'O2', glyph: '⊗', name: 'HRR memory', spec: '02', layer: 'Λ-S' },
  { id: 'O3', glyph: 'Λ', name: 'Language', spec: '03', layer: 'Λ-C' },
  { id: 'O4', glyph: '◉', name: 'Forage', spec: '04', layer: 'Λ-S' },
  { id: 'O5', glyph: '⟲', name: 'Self-mod', spec: '05', layer: 'Λ-U' },
  { id: 'O6', glyph: '∞', name: 'Dream', spec: '06', layer: 'Λ-U' },
  { id: 'O7', glyph: '✺', name: 'Swarm bus', spec: '07', layer: 'Λ-S' },
  { id: 'O8', glyph: '⊡', name: 'Dispatch', spec: '08', layer: 'Λ-C' },
  { id: 'O9', glyph: '⊞', name: 'Operator', spec: '12 (absorbed Lazarus)', layer: 'Λ-C' },
  { id: 'O10', glyph: 'Ψ', name: 'Reasoner', spec: '12 (Tessera bridge)', layer: 'Λ-C' },
];

const LAYER_COLOR: Record<Organ['layer'], string> = {
  'Λ-C': '#ff7733',
  'Λ-S': '#4488dd',
  'Λ-U': '#aa66dd',
};

const STATE_COLOR: Record<OrganState, string> = {
  alive: '#34d399',
  idle: '#7a7a8c',
  offline: '#3a3a4a',
  pending: '#fbbf24',
};

function organState(o: Organ, s: ApocryphaStatus | null): OrganState {
  if (!s?.reachable) return 'offline';
  const tiers = s.upstream_payload?.tiers_available;
  switch (o.id) {
    case 'O1':
    case 'O2':
    case 'O7':
    case 'O8':
      return 'alive'; // Λ-S core always-on when dispatch reachable
    case 'O3':
      if (tiers?.tier_a || tiers?.tier_b) return 'alive';
      return tiers?.tier0 ? 'idle' : 'offline';
    case 'O4':
    case 'O5':
    case 'O6':
      return 'idle'; // wired-but-not-running until invoked
    case 'O9':
    case 'O10':
      return 'pending'; // Phase-5 will bring these online
    default:
      return 'idle';
  }
}

interface Props {
  status: ApocryphaStatus | null;
}

export function OrganRack({ status }: Props) {
  return (
    <aside
      aria-label="Apocrypha organ rack"
      style={{
        width: 280,
        background: 'rgba(15, 15, 24, 0.6)',
        borderLeft: '1px solid #1f1f2a',
        padding: '0.75rem',
        overflowY: 'auto',
        flexShrink: 0,
        color: '#cdd6e4',
        fontSize: '0.78rem',
        fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
      }}
    >
      <h2
        style={{
          fontSize: '0.7rem',
          letterSpacing: '0.18em',
          textTransform: 'uppercase',
          color: '#7a7a8c',
          margin: '0 0 0.6rem',
        }}
      >
        § organ rack
      </h2>

      <div style={{ display: 'grid', gap: '0.3rem' }}>
        {ORGANS.map((o) => {
          const s = organState(o, status);
          return (
            <div
              key={o.id}
              style={{
                display: 'grid',
                gridTemplateColumns: 'auto auto 1fr auto',
                gap: '0.4rem',
                alignItems: 'center',
                padding: '0.35rem 0.5rem',
                border: '1px solid #1f1f2a',
                borderLeft: `2px solid ${LAYER_COLOR[o.layer]}`,
                borderRadius: 4,
                background: 'rgba(20, 20, 30, 0.5)',
              }}
              title={`${o.name} · spec ${o.spec} · layer ${o.layer}`}
            >
              <span style={{ color: '#5a5a6a', fontSize: '0.66rem', minWidth: 22 }}>{o.id}</span>
              <span aria-hidden="true" style={{ fontSize: '0.95rem', color: LAYER_COLOR[o.layer] }}>
                {o.glyph}
              </span>
              <span style={{ color: '#cdd6e4' }}>{o.name}</span>
              <span
                aria-hidden="true"
                style={{
                  display: 'inline-block',
                  width: 8,
                  height: 8,
                  borderRadius: '50%',
                  background: STATE_COLOR[s],
                  boxShadow: s === 'alive' ? `0 0 4px ${STATE_COLOR[s]}` : 'none',
                }}
              />
            </div>
          );
        })}
      </div>

      <div style={{ marginTop: '1rem', color: '#5a5a6a', fontSize: '0.66rem' }}>
        <p style={{ margin: '0 0 0.4rem' }}>
          <span style={{ color: '#ff7733' }}>■</span> Λ-C conscious (bursty GPU)
        </p>
        <p style={{ margin: '0 0 0.4rem' }}>
          <span style={{ color: '#4488dd' }}>■</span> Λ-S subconscious (10Hz CPU always-on)
        </p>
        <p style={{ margin: '0 0 0.4rem' }}>
          <span style={{ color: '#aa66dd' }}>■</span> Λ-U unconscious (batch opportunistic)
        </p>
        <p style={{ margin: '0.6rem 0 0', color: '#7a7a8c' }}>
          Phase-3 wires <code>/ws/live</code> for real tick-stream + raster-of-rings.
        </p>
      </div>
    </aside>
  );
}
