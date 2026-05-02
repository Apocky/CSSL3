// apocky.com/docs/intents

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import CodeBlock from '@/components/CodeBlock';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

interface IntentRow {
  variant: string;
  tool: string;
  examples: string[];
  notes: string;
}

const INTENTS: IntentRow[] = [
  {
    variant: 'Snapshot',
    tool: 'render.snapshot_png',
    examples: ['snapshot', 'snap', 'capture', 'screenshot'],
    notes: 'Writes a single PNG into ./snapshots/ next to LoA.exe.',
  },
  {
    variant: 'Burst { count }',
    tool: 'render.start_burst',
    examples: ['burst', 'burst 30', 'burst of 100', 'burst 10 frames'],
    notes: 'count clamped to 1..1000 · default 10 if no number given.',
  },
  {
    variant: 'Tour { tour_id }',
    tool: 'render.tour',
    examples: ['tour', 'tour walls', 'tour floor', 'tour plinths', 'tour ceiling', 'tour default'],
    notes: 'Unknown tour ids fall back to "default".',
  },
  {
    variant: 'SetCferIntensity { intensity }',
    tool: 'render.cfer_intensity',
    examples: ['intensity 0.5', 'intensity cfer 0.5', 'fog 0.3', 'atmosphere 0.7'],
    notes: 'Clamped to [0.0, 1.0] · default 0.10.',
  },
  {
    variant: 'SetIlluminant { name }',
    tool: 'render.set_illuminant',
    examples: ['illuminant d65', 'set illuminant d50', 'illuminant a', 'illuminant f11'],
    notes: 'Canonicalized to D65 / D50 / A / F11.',
  },
  {
    variant: 'Teleport { room_id }',
    tool: 'room.teleport',
    examples: ['teleport color', 'teleport to color room', 'go to material', 'goto pattern'],
    notes: 'Rooms 0..4 · test/material/pattern/scale/color · alias-tolerant.',
  },
  {
    variant: 'SpawnAt { kind, pos[3] }',
    tool: 'render.spawn_stress',
    examples: ['spawn cube at 5 5 5', 'drop sphere at 0 0 0', 'place pyramid at 1.5 0 -3', 'put torus at 5,5,5'],
    notes: '14 stress shapes · cube/sphere/pyramid/cylinder/cone/tet/oct/torus/capsule/wedge/plinth/ramp/icosa/dodeca.',
  },
  {
    variant: 'SetWallPattern { wall_id, pattern_id }',
    tool: 'render.set_wall_pattern',
    examples: ['set wall north pattern qr', 'wall n to qr', 'wall east checker'],
    notes: 'Walls N=0 / E=1 / S=2 / W=3 · 22 pattern names available.',
  },
  {
    variant: 'SetFloorPattern { quadrant, pattern_id }',
    tool: 'render.set_floor_pattern',
    examples: ['set floor ne pattern checker', 'floor sw checker', 'set floor northeast huewheel'],
    notes: 'Quadrants NE=0 / NW=1 / SW=2 / SE=3.',
  },
  {
    variant: 'SetMaterial { quad_id, material_id }',
    tool: 'render.set_material',
    examples: ['material on plinth 3 brass', 'set material 3 to brass', 'material 0 marble'],
    notes: '16 material aliases · default/wood/metal/brass/copper/gold/marble/glass/rubber/ceramic/fabric/leather/iridescent/obsidian/lacquer/carrara.',
  },
  {
    variant: 'SpontaneousSeed { text }',
    tool: 'world.spontaneous_seed',
    examples: ['spontaneous a sphere', 'seed orb', 'imagine a forest'],
    notes: '◐ Logged + classified · world-handler is a stub today; returns a "pending" status.',
  },
  {
    variant: 'Unknown { reason }',
    tool: '(no dispatch)',
    examples: ['(any unmatched text)'],
    notes: 'Reason carries the normalized input · increments INTENTS_UNKNOWN counter.',
  },
];

const PATTERN_NAMES = [
  'solid', 'grid (1m / 100mm)', 'checkerboard', 'macbeth', 'snellen', 'qr', 'ean13', 'grayscale',
  'huewheel', 'perlin', 'rings', 'spokes', 'zoneplate', 'frequency-sweep', 'radial-gradient',
  'mandelbulb', 'raymarch-sphere', 'raymarch-torus', 'gyroid', 'julia', 'menger',
];

const Page: NextPage = () => {
  return (
    <DocsLayout
      activeSlug="intents"
      title="Intent Vocabulary · Apocky Docs"
      description="The full vocabulary the chat panel and MCP intent.translate tool understand. 12 typed Intent variants, ~30 stage-0 keyword rules, every example you can paste."
    >
      <h1 className="docs-h1">Intent Vocabulary</h1>
      <p className="docs-blurb">§ 12 typed Intent variants · ~30 stage-0 phrase rules · examples you can paste verbatim.</p>

      <p className="docs-p">
        The intent router is a deterministic keyword + phrase classifier (no regex, no allocator beyond the input
        string) that maps free-form text to one of 12 typed variants of the <code className="docs-ic">Intent</code>{' '}
        enum. Each variant routes to exactly one MCP tool. The same vocabulary applies to the in-game chat panel,
        the MCP <code className="docs-ic">intent.translate</code> tool, and any scripted scene that calls{' '}
        <code className="docs-ic">classify()</code> directly.
      </p>

      <Callout kind="note" title="Source of truth">
        The classifier lives in <code className="docs-ic">compiler-rs/crates/loa-host/src/intent_router.rs</code>.
        The <code className="docs-ic">scenes/intent_translation.cssl</code> mirror is the eventual pure-CSSL
        re-implementation; both files are kept 1-to-1.
      </Callout>

      <h2 className="docs-h2">§ All 12 variants</h2>
      <table className="docs-table">
        <thead>
          <tr>
            <th>Variant</th>
            <th>MCP tool</th>
            <th>Example phrases</th>
          </tr>
        </thead>
        <tbody>
          {INTENTS.map((i) => (
            <tr key={i.variant}>
              <td><code className="docs-ic">{i.variant}</code></td>
              <td><code className="docs-ic">{i.tool}</code></td>
              <td>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: '0.3rem' }}>
                  {i.examples.map((ex, ei) => (
                    <code key={ei} className="docs-ic">{ex}</code>
                  ))}
                </div>
                <div style={{ fontSize: '0.78rem', color: '#7a7a8c', marginTop: '0.4rem' }}>{i.notes}</div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      <h2 className="docs-h2">§ Pattern-name vocabulary</h2>
      <p className="docs-p">
        <code className="docs-ic">SetWallPattern</code> and <code className="docs-ic">SetFloorPattern</code> accept these
        ~22 alias-tolerant pattern names. Type either the friendly name or the numeric pattern id.
      </p>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: '0.35rem', margin: '0.8rem 0 1.4rem' }}>
        {PATTERN_NAMES.map((p) => <code key={p} className="docs-ic">{p}</code>)}
      </div>

      <h2 className="docs-h2">§ Material-name vocabulary</h2>
      <CodeBlock lang="plain" caption="Aliases · alphabetical order is alphabetical-tolerant">{`default | plastic
wood | oak
metal | steel | iron
brass | gold-brass
copper
gold
marble | stone
glass
rubber
ceramic | porcelain
fabric | cloth | velvet
leather
iridescent | soap-bubble
obsidian | black-glass
vermillion-lacquer | lacquer | red-lacquer
white-marble | carrara`}</CodeBlock>

      <h2 className="docs-h2">§ Phrasing notes</h2>
      <ul className="docs-ul">
        <li>The classifier lowercases input and splits on whitespace + commas.</li>
        <li>Most rules tolerate filler words (<code className="docs-ic">to</code>, <code className="docs-ic">the</code>, <code className="docs-ic">on</code>, <code className="docs-ic">a</code>).</li>
        <li>Numbers parse as f32 / u32 lenient · trailing commas allowed.</li>
        <li>"set X" and "X" forms are both accepted for wall / floor / material / illuminant.</li>
        <li>"go to" and "goto" both teleport.</li>
      </ul>

      <h2 className="docs-h2">§ Forward-looking</h2>
      <Callout kind="coming-soon" title="Stage-1 KAN + Stage-2 LLM">
        ◐ Stage-1 (KAN classifier already shipped at <code className="docs-ic">cssl-kan-runtime</code>) drops in by
        replacing <code className="docs-ic">classify()</code>; the call sites stay identical. Stage-2 (LLM-driven
        intent extraction via MCP <code className="docs-ic">gm.parse_intent</code>) round-trips for the long-tail of
        phrasings the rule-base + KAN both miss. Both are sovereign-cap-gated.
      </Callout>

      <PrevNextNav slug="intents" />
    </DocsLayout>
  );
};

export default Page;
