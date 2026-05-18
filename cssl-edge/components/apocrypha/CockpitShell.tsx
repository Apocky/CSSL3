// Apocrypha CockpitShell — 4-zone layout root (status bar + nav rail + main + organ rack)
// Phase-2 first deliverable per Apocrypha/specs/12_APOCKY_COM_INTEGRATION.csl §PHASE-2.
// Subsumes Phase-0 placeholder. Lazarus/Tessera views land in operator/reasoner tabs (D020).

import { useState } from 'react';
import { useApocryphaStatus } from '../../lib/apocrypha/useApocryphaStatus';
import { NavRail, type ApocryphaView } from './NavRail';
import { OrganRack } from './OrganRack';
import { StatusBar } from './StatusBar';

interface ViewPaneProps {
  view: ApocryphaView;
}

function PlaceholderPane({ view }: ViewPaneProps) {
  const COPY: Record<ApocryphaView, { title: string; sub: string; phase: string }> = {
    chat: {
      title: 'Chat',
      sub: 'Talk with Apocrypha. Tier-0 Born-sample (always), Tier-A Mamba (local XPU), Tier-B DeepSeek (escalation).',
      phase: 'Phase-2 stub · Phase-3 wires /ws/chat with msgpack tick stream',
    },
    memory: {
      title: 'Memory',
      sub: 'Episodic + semantic + provenance browser. 60K+ episodes via TanStack Virtual.',
      phase: 'Phase-3 wires UMAP + density-bin scatter (Agent-B L7)',
    },
    dream: {
      title: 'Dream',
      sub: 'AIF rollouts during idle. EFE-ranked policies + hypothesis recombine. Imagined ≠ real (I-13).',
      phase: 'Phase-3 wires aurora-veil ambient overlay (Agent-D L4)',
    },
    evolve: {
      title: 'Evolve (Ω5)',
      sub: 'Self-modifier cockpit. 5-stage pipeline + Win32 sandbox + held-out probe A/B.',
      phase: 'Phase-3 wires DNA-strand ribbon + floating-control-bar (Agent-D L6)',
    },
    forage: {
      title: 'Forage (Ω4)',
      sub: 'Reputable-source streaming. Wikipedia + arXiv + PubMed + RFC. Provenance-tagged at ingest.',
      phase: 'Phase-3 wires NetCap toggles + recent-ingestion feed',
    },
    operator: {
      title: 'Operator (Ω9 ← Lazarus)',
      sub: 'Task queue + runner fleet + approval gates. Multi-project (Apocrypha-self + LoA + ...).',
      phase: 'Phase-5 absorbs Lazarus APIs + adds operator UI per spec 12',
    },
    reasoner: {
      title: 'Reasoner (Ω10 ← Tessera)',
      sub: 'Omnimindv2 CFE bridge. Tessera reasons in Rust ; surfaced here as Apocrypha sub-mind.',
      phase: 'Phase-5 P5.11 wires tessera-bridge as Ω10 organ-slot (D020)',
    },
    repl: {
      title: 'REPL',
      sub: 'Live process access. IPython namespace with all organs loaded. Admin-auth only.',
      phase: 'Phase-4 wires CodeMirror 6 + WS /ws/repl bidirectional',
    },
  };
  const c = COPY[view];
  return (
    <section
      style={{
        flex: 1,
        padding: '1rem 1.25rem',
        overflowY: 'auto',
        color: '#cdd6e4',
        background: 'rgba(10, 10, 16, 0.4)',
      }}
    >
      <h1
        style={{
          fontSize: '1.5rem',
          margin: 0,
          backgroundImage: 'linear-gradient(135deg, #ffaa55 0%, #c084fc 100%)',
          WebkitBackgroundClip: 'text',
          WebkitTextFillColor: 'transparent',
        }}
      >
        § {c.title}
      </h1>
      <p style={{ color: '#a0a0b0', maxWidth: 720, fontSize: '0.92rem', marginTop: '0.4rem' }}>
        {c.sub}
      </p>
      <div
        style={{
          marginTop: '1.2rem',
          padding: '0.6rem 0.8rem',
          border: '1px solid #2a2a3a',
          borderRadius: 6,
          color: '#7a7a8c',
          fontSize: '0.78rem',
          maxWidth: 720,
          fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
        }}
      >
        ◐ {c.phase}
      </div>
    </section>
  );
}

export function CockpitShell() {
  const [view, setView] = useState<ApocryphaView>('chat');
  const status = useApocryphaStatus(10_000);

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        height: 'calc(100dvh - 32px)',
        minHeight: 600,
        background: 'radial-gradient(ellipse at top, #15151f 0%, #0a0a0f 50%, #050507 100%)',
        fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
      }}
    >
      <StatusBar status={status.data} loading={status.loading} error={status.error} />
      <div style={{ display: 'flex', flex: 1, minHeight: 0 }}>
        <NavRail active={view} onSelect={setView} />
        <PlaceholderPane view={view} />
        <OrganRack status={status.data} />
      </div>
    </div>
  );
}
