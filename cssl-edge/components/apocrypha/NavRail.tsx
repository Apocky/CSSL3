// Apocrypha NavRail — 64px left rail with view-switcher
// Spec 11 §NAV-RAIL + spec 12 §Tessera-unification (D020) :
// Unified nav surfaces : Chat / Memory / Dream / Evolve / Forage / Operator (was Lazarus) /
//                        Reasoner (was Tessera) / REPL + Settings

export type ApocryphaView =
  | 'chat'
  | 'memory'
  | 'dream'
  | 'evolve'
  | 'forage'
  | 'operator'
  | 'reasoner'
  | 'repl';

interface ViewSpec {
  id: ApocryphaView;
  glyph: string;
  label: string;
  hint: string;
}

const VIEWS: ReadonlyArray<ViewSpec> = [
  { id: 'chat', glyph: '💬', label: 'Chat', hint: '⌘1 · talk with Apocrypha' },
  { id: 'memory', glyph: '🧠', label: 'Memory', hint: '⌘2 · episodic + provenance browser' },
  { id: 'dream', glyph: '💭', label: 'Dream', hint: '⌘3 · AIF rollouts + hypotheses' },
  { id: 'evolve', glyph: '🔧', label: 'Evolve', hint: '⌘4 · self-modifier cockpit (Ω5)' },
  { id: 'forage', glyph: '🔍', label: 'Forage', hint: '⌘5 · reputable-source feed (Ω4)' },
  { id: 'operator', glyph: '⚙', label: 'Operator', hint: '⌘6 · task queue (Ω9 ; absorbed Lazarus)' },
  { id: 'reasoner', glyph: 'Ψ', label: 'Reasoner', hint: '⌘7 · Tessera bridge (Ω10)' },
  { id: 'repl', glyph: '🐚', label: 'REPL', hint: '⌘8 · live process access' },
];

interface Props {
  active: ApocryphaView;
  onSelect: (v: ApocryphaView) => void;
}

export function NavRail({ active, onSelect }: Props) {
  return (
    <nav
      aria-label="Apocrypha views"
      style={{
        width: 64,
        display: 'flex',
        flexDirection: 'column',
        gap: '0.25rem',
        padding: '0.5rem 0.25rem',
        background: 'rgba(15, 15, 24, 0.6)',
        borderRight: '1px solid #1f1f2a',
        flexShrink: 0,
      }}
    >
      {VIEWS.map((v) => {
        const isActive = v.id === active;
        return (
          <button
            key={v.id}
            type="button"
            onClick={() => onSelect(v.id)}
            title={v.hint}
            aria-label={v.label}
            aria-current={isActive ? 'page' : undefined}
            style={{
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              gap: 2,
              padding: '0.45rem 0.25rem',
              border: '1px solid transparent',
              borderLeft: isActive ? '2px solid #ffaa55' : '2px solid transparent',
              borderRadius: 4,
              background: isActive ? 'rgba(255, 170, 85, 0.08)' : 'transparent',
              color: isActive ? '#ffaa55' : '#a0a0b0',
              cursor: 'pointer',
              fontFamily: 'inherit',
              fontSize: '0.6rem',
            }}
          >
            <span style={{ fontSize: '1.05rem' }} aria-hidden="true">
              {v.glyph}
            </span>
            <span style={{ letterSpacing: '0.05em' }}>{v.label}</span>
          </button>
        );
      })}
    </nav>
  );
}
