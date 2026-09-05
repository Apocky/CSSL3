// apocky.com/docs/keyboard-shortcuts

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

interface Row { keys: string[]; action: string; notes?: string; status?: 'available' | 'unfinished' | 'planned' }

const movement: Row[] = [
  { keys: ['W'], action: 'Move forward', status: 'available' },
  { keys: ['A'], action: 'Strafe left', status: 'available' },
  { keys: ['S'], action: 'Move backward', status: 'available' },
  { keys: ['D'], action: 'Strafe right', status: 'available' },
  { keys: ['Space'], action: 'Jump', status: 'available' },
  { keys: ['Shift'], action: 'Sprint while held', status: 'available' },
  { keys: ['Ctrl'], action: 'Crouch while held', status: 'available' },
  { keys: ['Mouse'], action: 'Look around', notes: 'Cursor is captured while the window is focused', status: 'available' },
];

const renderModes: Row[] = [
  { keys: ['F1'], action: 'Default display', status: 'available' },
  { keys: ['F2'], action: 'Wireframe view', status: 'available' },
  { keys: ['F3'], action: 'Surface-direction view', status: 'available' },
  { keys: ['F4'], action: 'Texture-coordinate grid', status: 'available' },
  { keys: ['F5'], action: 'Base color only, without shading', status: 'available' },
  { keys: ['F6'], action: 'Material-number heat map', status: 'available' },
  { keys: ['F7'], action: 'Separate light contributions', status: 'unfinished' },
  { keys: ['F8'], action: 'Light-spectrum debugging view', status: 'unfinished' },
];

const captureMode: Row[] = [
  { keys: ['F9'], action: 'Capture 10 frames', notes: 'Saves PNG image files into ./snapshots/', status: 'available' },
  { keys: ['F11'], action: 'Turn borderless full-screen on or off', status: 'available' },
  { keys: ['F12'], action: 'Take one screenshot', notes: 'Saves a PNG image into ./snapshots/', status: 'available' },
];

const ui: Row[] = [
  { keys: ['/'], action: 'Focus the request panel', notes: 'Type a request', status: 'available' },
  { keys: ['Enter'], action: 'Submit the request', status: 'available' },
  { keys: ['Esc'], action: 'Cancel text entry or open the pause menu', status: 'available' },
  { keys: ['Tab'], action: 'Pause and open the menu', status: 'available' },
  { keys: ['↑', '↓'], action: 'Browse earlier requests while the panel is focused', status: 'available' },
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
            <td><span style={{ color: r.status === 'available' ? '#34d399' : r.status === 'unfinished' ? '#fbbf24' : '#9aa0a6' }}>
              {r.status === 'available' ? 'Available' : r.status === 'unfinished' ? 'Unfinished' : 'Planned'}
            </span></td>
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
      <p className="docs-blurb">Movement · render modes · capture · UI focus.</p>

      <p className="docs-p">
        These are the default controls. <span style={{ color: '#34d399' }}>Available</span> means the control
        is expected to work in the current test build. <span style={{ color: '#fbbf24' }}>Unfinished</span>{' '}
        means the control exists but its result may still change.
      </p>

      <Section title="Movement" rows={movement} />
      <Section title="Render modes (F-row)" rows={renderModes} />
      <Section title="Capture + window" rows={captureMode} />
      <Section title="UI + chat" rows={ui} />

      <h2 className="docs-h2">Notes</h2>
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
        A file for changing the controls is planned. For the alpha, the bindings above are fixed in
        <code className="docs-ic"> compiler-rs/crates/loa-host/src/input.rs</code>. The
        <code className="docs-ic"> /docs/changelog</code> page will note when rebinding ships.
      </Callout>

      <PrevNextNav slug="keyboard-shortcuts" />
    </DocsLayout>
  );
};

export default Page;
