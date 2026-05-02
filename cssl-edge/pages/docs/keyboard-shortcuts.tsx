// apocky.com/docs/keyboard-shortcuts

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

interface Row { keys: string[]; action: string; notes?: string; status?: '✓' | '◐' | '○' }

const movement: Row[] = [
  { keys: ['W'], action: 'Move forward', status: '✓' },
  { keys: ['A'], action: 'Strafe left', status: '✓' },
  { keys: ['S'], action: 'Move backward', status: '✓' },
  { keys: ['D'], action: 'Strafe right', status: '✓' },
  { keys: ['Space'], action: 'Jump', status: '✓' },
  { keys: ['Shift'], action: 'Sprint while held', status: '✓' },
  { keys: ['Ctrl'], action: 'Crouch while held', status: '✓' },
  { keys: ['Mouse'], action: 'Look around', notes: 'Cursor is captured while the window is focused', status: '✓' },
];

const renderModes: Row[] = [
  { keys: ['F1'], action: 'Default render mode', status: '✓' },
  { keys: ['F2'], action: 'Wireframe', status: '✓' },
  { keys: ['F3'], action: 'Normals visualization', status: '✓' },
  { keys: ['F4'], action: 'UV grid', status: '✓' },
  { keys: ['F5'], action: 'Albedo only (no shading)', status: '✓' },
  { keys: ['F6'], action: 'Material id heat-map', status: '✓' },
  { keys: ['F7'], action: 'Light contributions split', status: '◐' },
  { keys: ['F8'], action: 'Spectral debug overlay', status: '◐' },
];

const captureMode: Row[] = [
  { keys: ['F9'], action: 'Burst capture · 10 frames', notes: 'Saves PNGs into ./snapshots/', status: '✓' },
  { keys: ['F11'], action: 'Toggle borderless fullscreen', status: '✓' },
  { keys: ['F12'], action: 'Single screenshot', notes: 'Saves PNG into ./snapshots/', status: '✓' },
];

const ui: Row[] = [
  { keys: ['/'], action: 'Focus the chat panel', notes: 'Type a free-form request', status: '✓' },
  { keys: ['Enter'], action: 'Submit chat / dispatch intent', status: '✓' },
  { keys: ['Esc'], action: 'Cancel chat focus · pause menu', status: '✓' },
  { keys: ['Tab'], action: 'Pause · open menu', status: '✓' },
  { keys: ['↑', '↓'], action: 'Scroll chat history while focused', status: '✓' },
];

const Section = ({ title, rows }: { title: string; rows: Row[] }) => (
  <section style={{ marginTop: '1.6rem' }}>
    <h3 className="docs-h3">{title}</h3>
    <table className="docs-table">
      <thead>
        <tr>
          <th style={{ width: '12rem' }}>Keys</th>
          <th>Action</th>
          <th style={{ width: '5rem' }}>Status</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r, i) => (
          <tr key={i}>
            <td>{r.keys.map((k, ki) => <span key={ki} className="docs-kbd">{k}</span>)}</td>
            <td>
              <div style={{ color: '#e6e6f0' }}>{r.action}</div>
              {r.notes !== undefined ? <div style={{ fontSize: '0.78rem', color: '#7a7a8c', marginTop: '0.2rem' }}>{r.notes}</div> : null}
            </td>
            <td><span style={{ color: r.status === '✓' ? '#34d399' : r.status === '◐' ? '#fbbf24' : '#9aa0a6' }}>{r.status ?? '○'}</span></td>
          </tr>
        ))}
      </tbody>
    </table>
  </section>
);

const Page: NextPage = () => {
  return (
    <DocsLayout
      activeSlug="keyboard-shortcuts"
      title="Keyboard Shortcuts · Apocky Docs"
      description="The complete keyboard reference for Labyrinth of Apocalypse — movement, render modes, screenshots, burst capture, fullscreen, pause, chat focus."
    >
      <h1 className="docs-h1">Keyboard Shortcuts</h1>
      <p className="docs-blurb">§ Movement · render modes · capture · UI focus.</p>

      <p className="docs-p">
        Every binding below is the engine default and is wired into the loa-host event loop. Keys marked{' '}
        <span style={{ color: '#34d399' }}>✓</span> work in the current alpha build. Keys marked{' '}
        <span style={{ color: '#fbbf24' }}>◐</span> are wired but the visual output is still being polished.
      </p>

      <Section title="Movement" rows={movement} />
      <Section title="Render modes (F-row)" rows={renderModes} />
      <Section title="Capture + window" rows={captureMode} />
      <Section title="UI + chat" rows={ui} />

      <h2 className="docs-h2">§ Notes</h2>
      <Callout kind="note" title="Snapshots directory">
        F9 (burst) and F12 (single) write PNGs into a <code className="docs-ic">snapshots/</code> directory next to{' '}
        <code className="docs-ic">LoA.exe</code>. The engine creates the directory on first capture if it does not exist.
        File names are <code className="docs-ic">snap_&lt;frame&gt;_&lt;ts_ms&gt;.png</code>.
      </Callout>

      <Callout kind="note" title="Chat-panel ergonomics">
        While the chat is focused, movement keys are intercepted as text input. Press{' '}
        <span className="docs-kbd">Esc</span> to release focus, or <span className="docs-kbd">Enter</span> to submit
        and auto-release. The chat history is bounded to 16 most-recent dispatches (the same RECENT_INTENT_CAP
        constant the MCP <code className="docs-ic">intent.recent</code> tool reads).
      </Callout>

      <Callout kind="coming-soon" title="Custom rebinding">
        ○ A keybinding-config file is on the roadmap. For the alpha, the bindings above are hard-coded in
        <code className="docs-ic"> compiler-rs/crates/loa-host/src/input.rs</code>. The
        <code className="docs-ic"> /docs/changelog</code> page will note when rebinding ships.
      </Callout>

      <PrevNextNav slug="keyboard-shortcuts" />
    </DocsLayout>
  );
};

export default Page;
