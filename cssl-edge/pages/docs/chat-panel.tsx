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
      description="How to use Labyrinth of Apocalypse's in-game chat panel — focus with /, submit with Enter, browse history with arrow keys, and dispatch typed intents to the live engine."
    >
      <h1 className="docs-h1">Chat Panel</h1>
      <p className="docs-blurb">§ The text-line interface to the Game-Master + intent dispatcher.</p>

      <h2 className="docs-h2">§ What it is</h2>
      <p className="docs-p">
        The chat panel is the bottom-of-screen text line where you type free-form requests and the engine
        executes them. It is wired into the same intent-router that the MCP <code className="docs-ic">intent.translate</code>{' '}
        and <code className="docs-ic">intent.recent</code> tools use, which means whatever you can say in chat,
        any external tool talking to the engine can say programmatically.
      </p>

      <Callout kind="note" title="Three back-end stages">
        Stage-0 (deterministic keyword classifier · ~30 phrase rules · zero-dep · shipping today) is what answers
        you. Stage-1 (KAN classifier) and Stage-2 (LLM-driven intent extraction) drop in by replacing one function
        without changing the chat UI. See <a href="/docs/intents" style={{ color: '#7dd3fc' }}>/docs/intents</a>.
      </Callout>

      <h2 className="docs-h2">§ Basic flow</h2>
      <ol className="docs-ol">
        <li>Press <span className="docs-kbd">/</span> · the chat-line gains focus and movement is suspended.</li>
        <li>Type your request in plain language.</li>
        <li>Press <span className="docs-kbd">Enter</span> · the classifier runs, the dispatcher fires, focus releases.</li>
        <li>Press <span className="docs-kbd">Esc</span> at any point to cancel without dispatching.</li>
      </ol>

      <CodeBlock lang="plain" caption="Example session">{`/                          ← focuses the panel
> spawn cube at 5 5 5      ← enter
✓ Intent::SpawnAt { kind: 0, pos: [5.0, 5.0, 5.0] }
  → render.spawn_stress · ok

/
> illuminant d65
✓ Intent::SetIlluminant { name: "D65" }
  → render.set_illuminant · ok

/
> snapshot
✓ Intent::Snapshot
  → render.snapshot_png · saved snapshots/snap_142_1714123412.png`}</CodeBlock>

      <h2 className="docs-h2">§ History</h2>
      <p className="docs-p">
        While the chat-line is focused, <span className="docs-kbd">↑</span> and <span className="docs-kbd">↓</span>{' '}
        cycle through your last 16 submissions. The history is the same recent-ring exposed by the{' '}
        <code className="docs-ic">intent.recent</code> MCP tool — capacity is the
        <code className="docs-ic"> RECENT_INTENT_CAP</code> constant in{' '}
        <code className="docs-ic">loa-host/src/intent_router.rs</code>.
      </p>

      <h2 className="docs-h2">§ Sample intents to try</h2>
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

      <h2 className="docs-h2">§ When the classifier doesn't understand</h2>
      <p className="docs-p">
        Stage-0 falls through to <code className="docs-ic">Intent::Unknown</code> when no rule matches. The chat
        echoes the normalized input back so you can see what the classifier saw. A typical fix is one of:
      </p>
      <ul className="docs-ul">
        <li>Use a verb the rule-table knows · see <a href="/docs/intents" style={{ color: '#7dd3fc' }}>/docs/intents</a> for the full list.</li>
        <li>Spell room or material names from the alias tables (e.g. <code className="docs-ic">color</code>, <code className="docs-ic">material</code>, <code className="docs-ic">brass</code>).</li>
        <li>Swap word order · the classifier accepts several phrasings per intent.</li>
      </ul>

      <Callout kind="warn" title="No off-machine relay">
        The chat panel does not call any external service. Every keystroke stays in-process. Stage-2 (LLM-driven
        intent extraction) will only activate if you grant a sovereign-cap to bridge to a model — the cap is
        revocable and is off by default.
      </Callout>

      <h2 className="docs-h2">§ Programmatic access</h2>
      <p className="docs-p">
        The chat panel is one of three entry points into the same router. The other two are MCP tools you can
        invoke from the host or from a connected agent:
      </p>
      <CodeBlock lang="cssl" caption="Equivalent MCP calls">{`// Tool · intent.translate
{ "text": "spawn cube at 5 5 5" }
// → returns the typed Intent JSON · without dispatching.

// Tool · intent.recent
// → returns the last 16 dispatches + per-kind counters.`}</CodeBlock>

      <PrevNextNav slug="chat-panel" />
    </DocsLayout>
  );
};

export default Page;
