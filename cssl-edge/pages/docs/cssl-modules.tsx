// apocky.com/docs/cssl-modules

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import CodeBlock from '@/components/CodeBlock';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

const Page: NextPage = () => {
  return (
    <DocsLayout
      activeSlug="cssl-modules"
      title="CSSL Modules · Apocky Docs"
      description="How CSSL divides a program into named source files, followed by current limitations and plans."
    >
      <h1 className="docs-h1">CSSL Modules</h1>
      <p className="docs-blurb">How a CSSL program names and organizes its source files.</p>

      <p className="docs-p">
        A <strong>module</strong> is a named part of a program, usually stored in its own file. Modules help keep
        a larger program organized. CSSL can name modules today, but compiling several CSSL modules together is
        still unfinished.
      </p>

      <h2 className="docs-h2">Declaring a module</h2>
      <p className="docs-p">
        Every CSSL source file begins with one <code className="docs-ic">module</code> declaration. The path is
        reverse-DNS, mirroring the directory layout under the project root.
      </p>

      <CodeBlock lang="cssl" caption="Reverse-DNS module paths">{`module com.apocky.loa.main                       // main.cssl
module com.apocky.loa.systems.combat              // systems/combat.csl
module com.apocky.loa.systems.crafting            // systems/crafting.csl
module com.apocky.loa.scenes.city_central_hub     // scenes/city_central_hub.csl
module com.apocky.loa.scenes.dungeon_template     // scenes/dungeon_template.csl`}</CodeBlock>

      <Callout kind="note" title="Why reverse-DNS">
        Reverse-DNS naming begins with a reversed internet domain, such as <code className="docs-ic">com.apocky</code>.
        It helps prevent two projects from accidentally using the same module name. For example,
        <code className="docs-ic">com.apocky.loa.*</code> versus <code className="docs-ic">com.apocky.cssl.*</code> versus
        <code className="docs-ic"> com.apocky.sigma.*</code>.
      </Callout>

      <h2 className="docs-h2">Today · single-module compile — available now</h2>
      <p className="docs-p">
        <code className="docs-ic">csslc build</code> accepts one positional <code className="docs-ic">&lt;input&gt;</code>{' '}
        and produces one binary. The auto-default-link mechanism finds <code className="docs-ic">cssl-rt</code> and
        <code className="docs-ic"> loa-host</code> in <code className="docs-ic">compiler-rs/target/release/</code> and links them
        in. This is the path that produces the shipping <code className="docs-ic">LoA.exe</code> today.
      </p>

      <CodeBlock lang="bash" caption="The build command shipping today">{`csslc build "Labyrinth of Apocalypse/main.cssl" \\
  --output target/release/LoA.exe \\
  --release

# csslc auto-discovers cssl-rt + loa-host staticlibs and prepends
# them to the linker invocation. The result is a single ~8.9 MB
# self-contained Windows .exe.`}</CodeBlock>

      <h2 className="docs-h2">Sibling modules · how POD-3 staged the contracts</h2>
      <p className="docs-p">
        While csslc is single-module-only, the LoA project already authors all 10 sibling modules in CSSL. They
        are tracked in git, they declare their <code className="docs-ic">extern "C"</code> contracts to the host-side
        staticlibs, and they are forward-looking specs that future wave-phases compile in place.
      </p>

      <table className="docs-table">
        <thead>
          <tr>
            <th>Module</th>
            <th>Path</th>
            <th>Host staticlib</th>
          </tr>
        </thead>
        <tbody>
          <tr><td><code className="docs-ic">.systems.combat</code></td><td><code className="docs-ic">systems/combat.csl</code></td><td><code className="docs-ic">cssl-host-combat-sim</code></td></tr>
          <tr><td><code className="docs-ic">.systems.inventory</code></td><td><code className="docs-ic">systems/inventory.csl</code></td><td><code className="docs-ic">cssl-host-inventory</code></td></tr>
          <tr><td><code className="docs-ic">.systems.crafting</code></td><td><code className="docs-ic">systems/crafting.csl</code></td><td><code className="docs-ic">cssl-host-craft-graph</code></td></tr>
          <tr><td><code className="docs-ic">.systems.alchemy</code></td><td><code className="docs-ic">systems/alchemy.csl</code></td><td><code className="docs-ic">cssl-host-alchemy</code></td></tr>
          <tr><td><code className="docs-ic">.systems.magic</code></td><td><code className="docs-ic">systems/magic.csl</code></td><td><code className="docs-ic">cssl-host-magic</code></td></tr>
          <tr><td><code className="docs-ic">.systems.run</code></td><td><code className="docs-ic">systems/run.csl</code></td><td><code className="docs-ic">cssl-host-roguelike-run</code></td></tr>
          <tr><td><code className="docs-ic">.systems.npc</code></td><td><code className="docs-ic">systems/npc.csl</code></td><td><code className="docs-ic">cssl-host-npc-bt</code></td></tr>
          <tr><td><code className="docs-ic">.systems.multiplayer</code></td><td><code className="docs-ic">systems/multiplayer.csl</code></td><td><code className="docs-ic">cssl-host-mycelium</code></td></tr>
          <tr><td><code className="docs-ic">.scenes.city_central_hub</code></td><td><code className="docs-ic">scenes/city_central_hub.csl</code></td><td><code className="docs-ic">cssl-host-procgen-city</code></td></tr>
          <tr><td><code className="docs-ic">.scenes.dungeon_template</code></td><td><code className="docs-ic">scenes/dungeon_template.csl</code></td><td><code className="docs-ic">cssl-host-procgen-dungeon</code></td></tr>
        </tbody>
      </table>

      <h2 className="docs-h2">Multi-module compile roadmap — in progress</h2>
      <p className="docs-p">
        The <code className="docs-ic">main.cssl</code> file documents the gap explicitly. Three POD-4 slices unlock
        the full multi-module pipeline:
      </p>

      <Callout kind="coming-soon" title="POD-4-D3 · multi-module compile">
        Planned command: <code className="docs-ic">csslc build main.cssl --module-path systems/combat.csl --module-path systems/...</code>
        OR auto-discover sibling .csl files from main.cssl's manifest. This unlocks invoking the per-system tick
        functions from <code className="docs-ic">main</code>.
      </Callout>

      <Callout kind="coming-soon" title="POD-4-D4 · temporary host functions">
        Planned: add temporary <code className="docs-ic">__cssl_&lt;sys&gt;_tick_*</code> functions so the per-system
        extern decls resolve while the per-system staticlibs are still in development.
      </Callout>

      <Callout kind="coming-soon" title="POD-4-D5..D8 · per-system staticlibs auto-link">
        Planned: extend the compiler’s automatic linking so it can discover the additional compiled libraries, mirroring
        how <code className="docs-ic">cssl-rt</code> and <code className="docs-ic">loa-host</code> are discovered today.
      </Callout>

      <h2 className="docs-h2">The eventual hot loop</h2>
      <p className="docs-p">
        Once the three slices land, <code className="docs-ic">main.cssl</code> evolves from the thin shim shipping
        today into a per-frame driver that calls each sibling's tick:
      </p>

      <CodeBlock lang="cssl" caption="Forward-looking · main.cssl after POD-4-D3..D8">{`fn __cssl_loa_frame_tick(dt: f32) -> i32 {
    tick_run_systems(dt) ;          // run.csl       — meta-loop · seed · run-state
    tick_combat_systems(dt) ;       // combat.csl    — combat-sim per-frame step
    tick_npc_systems(dt) ;          // npc.csl       — BT + GOAP planner LOD
    tick_inventory_systems(dt) ;    // inventory.csl — bag + equip + transmute
    tick_crafting_systems(dt) ;     // crafting.csl  — DAG recipe + deconstruct
    tick_alchemy_systems(dt) ;      // alchemy.csl   — brew + reagent-quality
    tick_magic_systems(dt) ;        // magic.csl     — spell + glyph + cast-graph
    tick_multiplayer_systems(dt) ;  // multiplayer.csl — peer-mesh + sync · CRDT
    tick_scene_dungeon(dt) ;        // dungeon_template.csl — floor procgen
    tick_scene_city_hub(dt) ;       // city_central_hub.csl — hub orchestration
    0
}`}</CodeBlock>

      <PrevNextNav slug="cssl-modules" />
    </DocsLayout>
  );
};

export default Page;
