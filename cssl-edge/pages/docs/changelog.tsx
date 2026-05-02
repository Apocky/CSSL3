// apocky.com/docs/changelog

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

interface Release {
  version: string;
  date: string;
  highlights: string[];
  status: 'shipped' | 'in-progress' | 'planned';
}

const RELEASES: Release[] = [
  {
    version: 'v0.1.0 · alpha',
    date: '2026-04 · current',
    status: 'shipped',
    highlights: [
      'LoA.exe single-binary build · ~8.9 MB · zero external runtime deps',
      'Test-room with 4 colored quadrants, 4 calibration walls, navigable in first-person',
      'Intent router stage-0 · ~30 phrase rules · 12 typed Intent variants',
      'Chat panel · / focus · Enter dispatch · Esc cancel · 16-entry history ring',
      'F-row render modes (F1–F8) · F9 burst · F12 single capture · F11 fullscreen',
      'Cap-based sovereignty model · default-deny · ~/.loa-secrets/caps.toml',
      'Substrate Wave-7 host crates landed · cssl-substrate-omega-field keystone',
      'Mycelial-network spec + cssl-host-mycelium primitive · Mode-A airgap verified',
      'apocky.com portfolio hub · /download · /docs · /devblog · /press · /transparency',
    ],
  },
  {
    version: 'v0.0.x · pre-alpha · internal builds',
    date: '2025-Q4 → 2026-Q1',
    status: 'shipped',
    highlights: [
      'Substrate primitives bootstrap · ω-field, Σ-mask, KAN, HDC',
      'csslc stage-0 · lex/parse/HIR/MIR/cranelift-object pipeline',
      'auto-default-link for cssl-rt + loa-host staticlibs',
      '11+ host-side substrate crates · 27+ wave-15 host crates',
      'MCP tool surface · ~110+ tools across substrate + host',
      'spec/grand-vision/* · 25+ CSL3 architecture documents',
    ],
  },
  {
    version: 'v0.2.0 · multi-module',
    date: 'next major slice',
    status: 'in-progress',
    highlights: [
      'csslc multi-module compile · POD-4-D3 (◐)',
      '10 sibling modules ingested at compile time · POD-4-D4 (◐)',
      'Per-system staticlib auto-link · POD-4-D5..D8 (◐)',
      'main.cssl hot-loop scaffold activated · all 10 systems tick',
      'Stage-1 KAN intent classifier wired · stage-0 fallback retained',
    ],
  },
  {
    version: 'v0.3.0 · mycelium-online',
    date: 'planned',
    status: 'planned',
    highlights: [
      'Home pocket-dimension UI · 7 archetypes selectable',
      'Mycelium privacy modes A–E · cap-gated',
      'Drop-in invites · friend-list bootstrap rendezvous',
      'Federated KAN bias-learning · attested-anonymous',
      'cap.akashic writer mode · long-term federated memory',
    ],
  },
  {
    version: 'v0.4.0+ · full game',
    date: 'planned · grand-vision spec/13',
    status: 'planned',
    highlights: [
      'Combat / inventory / crafting / alchemy / magic systems live in-game',
      'Procgen city + procgen dungeon scenes navigable',
      '4096+ NPC sustained 60fps with 4-tier LOD',
      'Coherence-Engine 13 axes (DEPRECATED-Infinite-Labyrinth design carried forward)',
      'Nexus-Bazaar 5-tier marketplace · cosmetic-only · gift-economy',
      'Σ-Chain Coherence-Proof consensus · NO PoW · NO PoS · NO gas',
    ],
  },
];

const Page: NextPage = () => {
  const colorFor = (s: Release['status']) => s === 'shipped' ? '#34d399' : s === 'in-progress' ? '#fbbf24' : '#9aa0a6';
  const labelFor = (s: Release['status']) => s === 'shipped' ? '✓ shipped' : s === 'in-progress' ? '◐ in progress' : '○ planned';
  return (
    <DocsLayout
      activeSlug="changelog"
      title="Changelog · Apocky Docs"
      description="What landed when, what is shipping next, and what is on the long-term roadmap. The truth-in-doc rule applies — every line is real or clearly marked as planned."
    >
      <h1 className="docs-h1">Changelog</h1>
      <p className="docs-blurb">§ What landed · what is in flight · what is planned.</p>

      <Callout kind="note" title="Truth-in-doc">
        Every line below is either shipped (✓), in active development (◐), or honestly labeled as planned (○).
        We do not list speculative features or marketing-driven roadmap items. If something is on this page,
        it has either landed or is being worked on with a defined slice.
      </Callout>

      {RELEASES.map((r) => (
        <section key={r.version} style={{ marginTop: '2rem' }}>
          <h2 className="docs-h2" style={{ marginBottom: '0.2rem' }}>{r.version}</h2>
          <div style={{ fontSize: '0.78rem', color: '#7a7a8c', marginBottom: '0.6rem' }}>
            {r.date} · <span style={{ color: colorFor(r.status) }}>{labelFor(r.status)}</span>
          </div>
          <ul className="docs-ul">
            {r.highlights.map((h, i) => <li key={i}>{h}</li>)}
          </ul>
        </section>
      ))}

      <h2 className="docs-h2">§ Where to follow along</h2>
      <ul className="docs-ul">
        <li><a href="https://github.com/Apocky" style={{ color: '#7dd3fc' }}>github.com/Apocky</a> — release tags, source, issues</li>
        <li><a href="/devblog" style={{ color: '#7dd3fc' }}>/devblog</a> — long-form context for major slices</li>
        <li><a href="/press" style={{ color: '#7dd3fc' }}>/press</a> — high-level project state for outsiders</li>
      </ul>

      <PrevNextNav slug="changelog" />
    </DocsLayout>
  );
};

export default Page;
