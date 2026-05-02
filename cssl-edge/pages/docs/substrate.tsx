// apocky.com/docs/substrate

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

const Page: NextPage = () => {
  return (
    <DocsLayout
      activeSlug="substrate"
      title="Substrate Primitives · Apocky Docs"
      description="The four primitives every Apocky system shares — ω-field, Σ-mask, KAN, HDC — explained for end users with concrete examples from the LoA engine."
    >
      <h1 className="docs-h1">Substrate Primitives</h1>
      <p className="docs-blurb">§ ω-field · Σ-mask · KAN · HDC. The trunk every Apocky branch grows from.</p>

      <p className="docs-p">
        Every Apocky project — Labyrinth of Apocalypse, the Σ-Chain, the Akashic Records, the Mycelial Network —
        shares one runtime foundation. It is made of four primitives. This page explains them in plain
        language; the long-form architectural argument lives in the grand-vision specs (the index page links
        them).
      </p>

      <h2 className="docs-h2">§ ω-field · the addressable manifold</h2>
      <p className="docs-p">
        The <strong>ω-field</strong> is a typed manifold of values addressed by coordinates rather than by
        pointers. Think of it as a programmable physics: cells have locations, the relationships between cells
        are first-class, and the topology can warp continuously.
      </p>
      <p className="docs-p">
        The ω-field is <em>the</em> truth — the master state. Everything else (visible mesh, network sync,
        analytics surface) is a projection or annotation of cells in the ω-field. This collapses what would
        normally be three or four independent state stores into one.
      </p>
      <p className="docs-p">
        In the LoA engine, the ω-field stamps every procgen cell at scene-genesis time. When a city in
        NeverhomeRise spawns 4096+ NPCs, each one has an ω-field address that survives across replays from the
        same seed.
      </p>

      <h2 className="docs-h2">§ Σ-mask · consent as data</h2>
      <p className="docs-p">
        Every cell in the ω-field carries a <strong>Σ-mask</strong>: a bitmask describing which observers may
        see, read, or write that cell. Σ-masks compose multiplicatively. They are revocable. They are
        sovereignty made structural.
      </p>
      <p className="docs-p">
        There is no "private mode" toggle. There is no opt-in checkbox bolted onto analytics. The default mask
        is <strong>deny-all</strong>, and the cell-owner alone can loosen it. When a cell with a deny-all mask
        is asked to leave the machine, the answer is <code className="docs-ic">EOPNOTSUPP</code> from the cap layer
        before the data ever reaches a serializer.
      </p>

      <Callout kind="success" title="What this means for you">
        Nothing in LoA leaves your machine without an explicit sovereign-cap that you grant. There is no
        analytics panel, no telemetry phone-home, no first-launch consent dialog because there is nothing to
        consent to. See <a href="/docs/sovereignty" style={{ color: '#7dd3fc' }}>/docs/sovereignty</a>.
      </Callout>

      <h2 className="docs-h2">§ KAN · the small-but-real learning substrate</h2>
      <p className="docs-p">
        <strong>KAN</strong> stands for Kolmogorov-Arnold Networks. Functions over the ω-field are parameterized
        as compositions of univariate splines on the edges of a network. This sounds abstract; it has three
        concrete consequences:
      </p>
      <ul className="docs-ul">
        <li>Cheap to evaluate. Fits in <em>kilobytes</em>, not gigabytes. Why LoA ships at 8.9 MB instead of needing a model dump.</li>
        <li>Cheap to update online. The engine adapts in-loop without retraining cycles.</li>
        <li>Per-edge interpretability. Every spline edge is a function you can graph and reason about.</li>
      </ul>
      <p className="docs-p">
        KAN-classifiers are wired at five "swap points" in the procgen pipeline (SP-PG-1 through SP-PG-5):
        floor-template pick, biome-grammar tune, creature-spawn mix, loot-affix bias, city-NPC routine skew.
        Each swap point has a <em>stage-0 always-fallback</em> table-lookup, so KAN never takes over the world —
        it nudges the table.
      </p>

      <h2 className="docs-h2">§ HDC · symbolic communication</h2>
      <p className="docs-p">
        <strong>HDC</strong> stands for Hyperdimensional Computing. Cells signal to one another with
        high-dimensional binary vectors and the operations <em>bind</em>, <em>bundle</em>, and <em>unbind</em>.
        Picture chemical messengers in mycelium: a cell emits, the topology decides who listens, downstream
        cells decode by un-binding the relevant key.
      </p>
      <p className="docs-p">
        HDC is how the substrate <strong>talks to itself</strong> without serializing through a central
        message-bus. It is also the layer the Mycelial-Network primitives use to carry inter-Home signals,
        which is how multiplayer in LoA is structured — see{' '}
        <a href="/docs/mycelium" style={{ color: '#7dd3fc' }}>/docs/mycelium</a>.
      </p>

      <h2 className="docs-h2">§ Why these four together</h2>
      <p className="docs-p">
        Because together they are sufficient to express physics simulation (ω-field + Σ-mask scoping force
        domains), runtime learning (KAN updating in-loop), distributed messaging (HDC over network edges),
        cryptographic attestation (Σ-mask as access proof), creative procgen (KAN-driven sampling over the
        ω-field), and live hotfixing (online KAN updates over deployed instances).
      </p>
      <p className="docs-p">
        One trunk. Many branches. The substrate evolution memory note records that as of T11, eleven host-side
        substrate crates have shipped with the keystone <code className="docs-ic">cssl-substrate-omega-field</code>{' '}
        crate. The substrate is real today, not a future promise.
      </p>

      <h2 className="docs-h2">§ Where to read more</h2>
      <ul className="docs-ul">
        <li><a href="/devblog/what-is-the-substrate" style={{ color: '#7dd3fc' }}>devblog · "What is the Substrate?"</a></li>
        <li><a href="/docs/sovereignty" style={{ color: '#7dd3fc' }}>How the Σ-mask shows up at runtime</a></li>
        <li><a href="/docs/mycelium" style={{ color: '#7dd3fc' }}>How HDC powers the multiplayer mesh</a></li>
        <li>Spec · <code className="docs-ic">specs/grand-vision/15_UNIFIED_SUBSTRATE.csl</code></li>
        <li>Spec · <code className="docs-ic">specs/30_SUBSTRATE_v2.csl</code></li>
      </ul>

      <PrevNextNav slug="substrate" />
    </DocsLayout>
  );
};

export default Page;
