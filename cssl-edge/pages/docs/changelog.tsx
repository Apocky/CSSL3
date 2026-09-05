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
      'Internal permission-related source work; complete player controls are not yet verified',
      'Experimental foundation libraries, including coordinate-based state work',
      'Mycelium design documents and source modules; not a finished public multiplayer feature',
      'apocky.com project hub, download page, documentation, and development writing',
    ],
  },
  {
    version: 'v0.0.x · pre-alpha · internal builds',
    date: '2025-Q4 → 2026-Q1',
    status: 'shipped',
    highlights: [
      'Early source work for coordinate state, permission masks, compact learning, and high-dimensional computing',
      'Early csslc compiler pipeline',
      'Automatic linking for the core runtime and game host libraries',
      'Multiple experimental host libraries',
      'Internal Model Context Protocol (MCP) development interfaces',
      'CSLv3 architecture documents',
    ],
  },
  {
    version: 'v0.2.0 · multi-module',
    date: 'next major slice',
    status: 'in-progress',
    highlights: [
      'csslc multi-module compile · POD-4-D3 — in progress',
      '10 sibling modules ingested at compile time · POD-4-D4 — in progress',
      'Per-system staticlib auto-link · POD-4-D5..D8 — in progress',
      'main.cssl hot-loop scaffold activated · all 10 systems tick',
      'Experimental compact request classifier with fixed-rule fallback',
    ],
  },
  {
    version: 'v0.3.0 · planned network research',
    date: 'planned',
    status: 'planned',
    highlights: [
      'Home pocket-dimension UI · 7 archetypes selectable',
      'Clearly explained connection and privacy choices',
      'Invitations and friend connections',
      'Optional shared-learning research, subject to privacy and security review',
      'Optional retained-history research, subject to explicit consent and withdrawal design',
    ],
  },
  {
    version: 'v0.4.0+ · proposed game work',
    date: 'planned · grand-vision spec/13',
    status: 'planned',
    highlights: [
      'Combat / inventory / crafting / alchemy / magic systems live in-game',
      'Procgen city + procgen dungeon scenes navigable',
      'Large non-player-character scenes, subject to measured performance testing',
      'Additional game-system research described in the technical plans',
      'Optional sharing and exchange features, subject to a separate product decision',
      'Experimental coordination research described in the technical plans',
    ],
  },
];

const Page: NextPage = () => {
  const colorFor = (s: Release['status']) => s === 'shipped' ? '#34d399' : s === 'in-progress' ? '#fbbf24' : '#9aa0a6';
  const labelFor = (s: Release['status']) => s === 'shipped' ? 'Available or recorded' : s === 'in-progress' ? 'In progress' : 'Planned';
  return (
    <DocsLayout
      activeSlug="changelog"
      title="Changelog · Apocky Docs"
      description="Release notes that separate available work, work in progress, and plans."
    >
      <h1 className="docs-h1">Changelog</h1>
      <p className="docs-blurb">What is available, what is being developed, and what remains a plan.</p>

      <Callout kind="warn" title="How to read these notes">
        A source file or internal module is not the same as a finished public feature. “Available or recorded”
        means the item appears in the named build or its release record; it does not certify every performance,
        privacy, or security claim. Planned items may change or never ship.
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

      <h2 className="docs-h2">Where to follow along</h2>
      <ul className="docs-ul">
        <li><a href="https://github.com/Apocky" style={{ color: '#7dd3fc' }}>github.com/Apocky</a> — release tags, source, issues</li>
        <li><a href="/now" style={{ color: '#7dd3fc' }}>New & worth exploring</a> — current writing, tools, and work in progress</li>
        <li><a href="/showcase" style={{ color: '#7dd3fc' }}>Project showcase</a> — an introduction to the work</li>
      </ul>

      <PrevNextNav slug="changelog" />
    </DocsLayout>
  );
};

export default Page;
