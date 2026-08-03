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
      title="Getting started with LoA · Apocky Documentation"
      description="Download and open the current Labyrinth of Apocalypse Windows test build."
    >
      <h1 className="docs-h1">Getting Started</h1>
      <p className="docs-blurb">Download, open, move around, and try the current test-room.</p>

      <h2 className="docs-h2">What you are installing</h2>
      <p className="docs-p">
        <strong>Labyrinth of Apocalypse</strong> (LoA) is an early Windows game and engine test. The current
        download contains the program and a small room you can explore. It is not the complete game.
      </p>

      <Callout kind="note" title="What installation means here">
        There is no traditional installer. Put the downloaded files in a folder you control and open
        <code className="docs-ic"> LoA.exe</code>. The release is intended to run without administrator access.
        See <a href="/docs/sovereignty" style={{ color: '#7dd3fc' }}>Permissions and data sharing</a> for the
        difference between current behavior and future plans.
      </Callout>

      <h2 className="docs-h2">Step 1 · Download</h2>
      <p className="docs-p">
        Visit <a href="/download" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>apocky.com/download</a> and
        get the latest test build. If the download is a ZIP archive, extract it before opening
        <code className="docs-ic"> LoA.exe</code>. The download page explains how to compare its SHA-256
        fingerprint, a number used to check that the file arrived unchanged.
      </p>

      <h2 className="docs-h2">Step 2 · Launch</h2>
      <p className="docs-p">
        Drop <code className="docs-ic">LoA.exe</code> anywhere on your filesystem (a fresh folder is recommended,
        because the engine creates local files while it runs). It creates a <code className="docs-ic">logs/</code>{' '}
        directory next to the program and may create screenshots, cached files, or experimental state elsewhere
        on your computer. Double-click to launch. The window opens in borderless-fullscreen mode at your primary
        monitor&apos;s native resolution.
      </p>

      <CodeBlock lang="bash" caption="Optional · launch from a terminal to see startup output">{`# PowerShell or cmd
.\\LoA.exe

# Or open the program from another folder
"C:\\Games\\LoA\\LoA.exe"`}</CodeBlock>

      <h2 className="docs-h2">Step 3 · The test-room</h2>
      <p className="docs-p">
        On boot you enter a <strong>test room</strong>: a small space used to check movement, materials, lighting,
        and display modes. It intentionally does not contain the full game. Some recognized text requests can
        alter the room; many planned game systems are not connected to this build.
      </p>

      <ul className="docs-ul">
        <li>Move with <span className="docs-kbd">W</span> <span className="docs-kbd">A</span> <span className="docs-kbd">S</span> <span className="docs-kbd">D</span></li>
        <li>Look with the mouse</li>
        <li>Crouch with <span className="docs-kbd">Ctrl</span>, sprint with <span className="docs-kbd">Shift</span></li>
        <li>Press <span className="docs-kbd">Tab</span> or <span className="docs-kbd">Esc</span> to pause</li>
      </ul>

      <h2 className="docs-h2">Step 4 · Open the chat panel</h2>
      <p className="docs-p">
        Press <span className="docs-kbd">/</span> to focus the chat panel. Type a request and press{' '}
        <span className="docs-kbd">Enter</span>. The game compares your words with the requests it currently
        recognizes and then performs the matching action. The technical name for a recognized request is an
        <strong> intent</strong>.
      </p>

      <CodeBlock lang="plain" caption="Sample first messages">{`/   ← focuses the chat panel
spawn cube at 5 5 5
illuminant d65
intensity fog 0.3
teleport to color room
snapshot`}</CodeBlock>

      <p className="docs-p">
        Each line is one request. The panel shows what it understood, and the room updates when the action
        succeeds. See <a href="/docs/intents" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>
          How the game reads requests
        </a> for the full list.
      </p>

      <h2 className="docs-h2">Step 5 · Quit cleanly</h2>
      <p className="docs-p">
        Press <span className="docs-kbd">Esc</span> to bring up the menu, then close the window. The engine
        writes several local diagnostic files in the <code className="docs-ic">logs/</code> folder next to the
        binary. “Diagnostic” means information used to understand performance and failures. You may delete the
        logs after closing the program. Other local experimental state can affect later runs, so back it up
        before removing it if you want to preserve that state.
      </p>

      <Callout kind="warn" title="Alpha caveats">
        This is alpha software. The test-room is the empty container — combat, NPCs, full procgen worlds, and
        crafting appear in source or plans but are not available as a complete first-time experience.
        See <a href="/docs/changelog" style={{ color: '#7dd3fc' }}>the release notes</a> for current status.
      </Callout>

      <h2 className="docs-h2">Where to next</h2>
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
