// apocky.com/docs/chat-panel

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import CodeBlock from '@/components/CodeBlock';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

const Page: NextPage = () => {
  return (
    <DocsLayout
      activeSlug="chat-panel"
      title="Chat Panel · Apocky Docs"
      description="How to type a request in Labyrinth of Apocalypse and see what the current test build understood."
    >
      <h1 className="docs-h1">Chat Panel</h1>
      <p className="docs-blurb">Type a request and see what the current game build understood.</p>

      <h2 className="docs-h2">What it is</h2>
      <p className="docs-p">
        The panel is a text line at the bottom of the game window. You type a request, and the current game
        compares it with a small list of recognized phrases. The technical name for a recognized request is an
        <strong> intent</strong>. This is a command interface for the game, not a conversation with Apocrypha.
      </p>

      <Callout kind="note" title="What works now">
        The current method uses fixed keyword and phrase rules. Source documents discuss two possible later
        methods: a compact mathematical classifier and an outside language model. Those are plans, not part of
        the basic instructions on this page. See <a href="/docs/intents" style={{ color: '#7dd3fc' }}>
          How the game reads requests
        </a>.
      </Callout>

      <h2 className="docs-h2">Basic flow</h2>
      <ol className="docs-ol">
        <li>Press <span className="docs-kbd">/</span>. The text line becomes active and movement is suspended.</li>
        <li>Type your request in plain language.</li>
        <li>Press <span className="docs-kbd">Enter</span>. The game checks the request, tries the matching action, and returns control to movement.</li>
        <li>Press <span className="docs-kbd">Esc</span> at any point to cancel without running an action.</li>
      </ol>

      <CodeBlock lang="plain" caption="Example session">{`/                          ← focuses the panel
> spawn cube at 5 5 5      ← enter
Recognized: SpawnAt { kind: 0, pos: [5.0, 5.0, 5.0] }
Action: render.spawn_stress · completed

/
> illuminant d65
Recognized: SetIlluminant { name: "D65" }
Action: render.set_illuminant · completed

/
> snapshot
Recognized: Snapshot
Action: render.snapshot_png · saved snapshots/snap_142_1714123412.png`}</CodeBlock>

      <h2 className="docs-h2">History</h2>
      <p className="docs-p">
        While the chat-line is focused, <span className="docs-kbd">↑</span> and <span className="docs-kbd">↓</span>{' '}
        cycle through your last 16 submissions. The game keeps that fixed-length recent list for the{' '}
        <code className="docs-ic">intent.recent</code> MCP tool — capacity is the
        <code className="docs-ic"> RECENT_INTENT_CAP</code> constant in{' '}
        <code className="docs-ic">loa-host/src/intent_router.rs</code>.
      </p>

      <h2 className="docs-h2">Sample intents to try</h2>
      <CodeBlock lang="plain" caption="Calibration + camera">{`snapshot
burst 30
tour walls
intensity fog 0.5
illuminant d50`}</CodeBlock>

      <CodeBlock lang="plain" caption="World manipulation">{`spawn cube at 5 5 5
drop sphere at 0 0 0
place pyramid at 1.5 0 -3
teleport to color room
go to material`}</CodeBlock>

      <CodeBlock lang="plain" caption="Material + pattern setup">{`set wall north pattern qr
floor sw checker
material on plinth 3 brass
set illuminant d65`}</CodeBlock>

      <h2 className="docs-h2">When the classifier doesn't understand</h2>
      <p className="docs-p">
        The current rule list returns <code className="docs-ic">Intent::Unknown</code> when no rule matches. The text line
        echoes the normalized input back so you can see what the classifier saw. A typical fix is one of:
      </p>
      <ul className="docs-ul">
        <li>Use a verb the rule-table knows · see <a href="/docs/intents" style={{ color: '#7dd3fc' }}>/docs/intents</a> for the full list.</li>
        <li>Spell room or material names from the alias tables (e.g. <code className="docs-ic">color</code>, <code className="docs-ic">material</code>, <code className="docs-ic">brass</code>).</li>
        <li>Swap word order · the classifier accepts several phrasings per intent.</li>
      </ul>

      <Callout kind="warn" title="Current release boundary">
        The current panel is documented as a local rule-based game control. A future connection to an outside
        language model would be a separate network feature and would require a clear, specific permission
        before use. It is not implied by typing into the current panel.
      </Callout>

      <h2 className="docs-h2">Programmatic access</h2>
      <p className="docs-p">
        The chat panel is one way to use the same request handler. Advanced testers can also use
        <strong> Model Context Protocol (MCP)</strong>, a technical format that lets another local program call
        named developer functions. Do not expose the local developer port to another computer or the internet.
      </p>
      <CodeBlock lang="cssl" caption="Equivalent MCP calls">{`// Tool · intent.translate
{ "text": "spawn cube at 5 5 5" }
// → returns the typed Intent JSON · without running it.

// Tool · intent.recent
// → returns the last 16 dispatches + per-kind counters.`}</CodeBlock>

      <PrevNextNav slug="chat-panel" />
    </DocsLayout>
  );
};

export default Page;
