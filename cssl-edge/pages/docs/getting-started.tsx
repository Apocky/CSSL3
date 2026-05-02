// apocky.com/docs/getting-started

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import CodeBlock from '@/components/CodeBlock';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

const Page: NextPage = () => {
  return (
    <DocsLayout
      activeSlug="getting-started"
      title="Getting Started · Apocky Docs"
      description="Install Labyrinth of Apocalypse, launch the engine, and have your first chat with the GM. Single-binary install · no external dependencies."
    >
      <h1 className="docs-h1">Getting Started</h1>
      <p className="docs-blurb">§ Install · launch · first chat with the GM. ≤ 5 minutes from download to playing.</p>

      <h2 className="docs-h2">§ What you are installing</h2>
      <p className="docs-p">
        <strong>Labyrinth of Apocalypse</strong> (LoA) is a single-binary Windows executable, around 8.9 MB, that
        ships the entire substrate-native engine, the CSSL runtime, and the test-room you can navigate today.
        There is no installer, no service, no auto-updater, no telemetry phone-home.
        You download <code className="docs-ic">LoA.exe</code>, you double-click it, you play.
      </p>

      <Callout kind="note" title="Sovereign-first install">
        Nothing leaves your machine without an explicit sovereign-cap. The executable does not write outside its
        own working directory, does not register itself in the registry, and does not require admin elevation.
      </Callout>

      <h2 className="docs-h2">§ Step 1 · Download</h2>
      <p className="docs-p">
        Visit <a href="/download" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>apocky.com/download</a> and
        grab the latest alpha build. The download is a single <code className="docs-ic">LoA.exe</code> file.
        You may verify the SHA-256 hash listed alongside the download.
      </p>

      <h2 className="docs-h2">§ Step 2 · Launch</h2>
      <p className="docs-p">
        Drop <code className="docs-ic">LoA.exe</code> anywhere on your filesystem (a fresh folder is recommended,
        because the engine will create a <code className="docs-ic">logs/</code> sibling directory next to the binary
        on first run). Double-click to launch. The window opens in borderless-fullscreen mode at your primary
        monitor's native resolution.
      </p>

      <CodeBlock lang="bash" caption="Optional · launch from a terminal to see startup output">{`# PowerShell or cmd
.\\LoA.exe

# Or from anywhere — the .exe is fully self-contained
"C:\\Games\\LoA\\LoA.exe"`}</CodeBlock>

      <h2 className="docs-h2">§ Step 3 · The test-room</h2>
      <p className="docs-p">
        On boot you spawn into the <strong>test-room</strong> — a 6×6×6 m container with four colored quadrants on
        the floor, four walls patterned with calibration targets, and a ceiling that responds to the active
        illuminant. This is the engine's empty stage; it intentionally has no game content. The runtime
        procgen pipelines fill it in once you ask them to.
      </p>

      <ul className="docs-ul">
        <li>Move with <span className="docs-kbd">W</span> <span className="docs-kbd">A</span> <span className="docs-kbd">S</span> <span className="docs-kbd">D</span></li>
        <li>Look with the mouse</li>
        <li>Crouch with <span className="docs-kbd">Ctrl</span>, sprint with <span className="docs-kbd">Shift</span></li>
        <li>Press <span className="docs-kbd">Tab</span> or <span className="docs-kbd">Esc</span> to pause</li>
      </ul>

      <h2 className="docs-h2">§ Step 4 · Open the chat panel</h2>
      <p className="docs-p">
        Press <span className="docs-kbd">/</span> to focus the chat panel. Type a request and press{' '}
        <span className="docs-kbd">Enter</span>. The router classifies your text into a typed intent, dispatches it
        against the live engine, and you see the result immediately.
      </p>

      <CodeBlock lang="plain" caption="Sample first messages">{`/   ← focuses the chat panel
spawn cube at 5 5 5
illuminant d65
intensity fog 0.3
teleport to color room
snapshot`}</CodeBlock>

      <p className="docs-p">
        Each line is a single intent. The engine confirms the dispatch in the chat scroll-back, and the world
        updates in front of you. The full intent vocabulary is documented at{' '}
        <a href="/docs/intents" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>/docs/intents</a>.
      </p>

      <h2 className="docs-h2">§ Step 5 · Quit cleanly</h2>
      <p className="docs-p">
        Press <span className="docs-kbd">Esc</span> to bring up the menu, then close the window. The engine
        flushes its log to <code className="docs-ic">logs/loa_runtime.log</code> next to the binary. That's the
        only file the engine writes by default; you can delete it with no ill effect.
      </p>

      <Callout kind="warn" title="Alpha caveats">
        This is alpha software. The test-room is the empty container — combat, NPCs, full procgen worlds, and
        crafting are wired at the code level but not yet exposed to first-time users without flags.
        See <a href="/docs/changelog" style={{ color: '#7dd3fc' }}>/docs/changelog</a> for what is shippable.
      </Callout>

      <h2 className="docs-h2">§ Where to next</h2>
      <ul className="docs-ul">
        <li><a href="/docs/keyboard-shortcuts" style={{ color: '#7dd3fc' }}>Full keyboard reference</a></li>
        <li><a href="/docs/chat-panel" style={{ color: '#7dd3fc' }}>How the chat panel works</a></li>
        <li><a href="/docs/intents" style={{ color: '#7dd3fc' }}>Every supported intent verb</a></li>
        <li><a href="/docs/troubleshooting" style={{ color: '#7dd3fc' }}>If something does not work</a></li>
      </ul>

      <PrevNextNav slug="getting-started" />
    </DocsLayout>
  );
};

export default Page;
