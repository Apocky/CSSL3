// apocky.com/docs/cssl-language

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import CodeBlock from '@/components/CodeBlock';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

const Page: NextPage = () => {
  return (
    <DocsLayout
      activeSlug="cssl-language"
      title="CSSL Language Overview · Apocky Docs"
      description="An overview of CSSL — the Conscious Substrate System Language Apocky is building. Why proprietary, what it compiles to, sample programs, and where to read the spec."
    >
      <h1 className="docs-h1">CSSL Language Overview</h1>
      <p className="docs-blurb">§ Conscious Substrate System Language · the source the engine compiles from.</p>

      <h2 className="docs-h2">§ What is CSSL</h2>
      <p className="docs-p">
        CSSL (Conscious Substrate System Language) is the proprietary source language Apocky uses to author
        substrate-native systems. It compiles via <code className="docs-ic">csslc</code> — a Rust-hosted stage-0
        compiler — through a standard pipeline (lex → parse → HIR → MIR → cranelift-object → linker) into native
        binaries that depend only on the Apocky-authored runtime (<code className="docs-ic">cssl-rt</code>) and
        host staticlibs (<code className="docs-ic">loa-host</code>, etc.).
      </p>

      <Callout kind="note" title="One language, three roles">
        CSSL is the source for (1) the LoA engine and game-logic, (2) the substrate primitives that surface as
        compiler intrinsics, and (3) the .csl specs that document the project. Every project Apocky ships moves
        toward CSSL as the canonical authoring surface.
      </Callout>

      <h2 className="docs-h2">§ Why a new language</h2>
      <p className="docs-p">
        Languages encode values. Rust encodes safety. Go encodes simplicity. C++ encodes performance. CSSL
        encodes <strong>consent</strong> — the Σ-mask is not a library, it is a compile-time-checked feature of
        every reference. Read the long-form argument at{' '}
        <a href="/devblog/why-cssl" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>/devblog/why-cssl</a>.
      </p>

      <h2 className="docs-h2">§ Sample · the smallest LoA program</h2>
      <p className="docs-p">
        This is <code className="docs-ic">Labyrinth of Apocalypse/main.cssl</code>, the actual root module that
        <code className="docs-ic"> csslc</code> compiles into <code className="docs-ic">LoA.exe</code>. Twelve lines of
        executable code. Every other tick of the engine is delegated to the <code className="docs-ic">loa-host</code>{' '}
        staticlib, auto-linked by csslc's default-link mechanism.
      </p>

      <CodeBlock lang="cssl" caption="Labyrinth of Apocalypse/main.cssl (excerpt)">{`module com.apocky.loa.main

// § FFI declaration · engine entry-point
extern "C" fn __cssl_engine_run() -> i32 ;

// § main · the pure-CSSL entry-point
fn main() -> i32 {
    let exit_code: i32 = __cssl_engine_run() ;
    exit_code
}`}</CodeBlock>

      <h2 className="docs-h2">§ Sample · scene with FFI</h2>
      <p className="docs-p">
        A more representative file: a runtime-procgen city scene. Each <code className="docs-ic">extern "C" fn</code>{' '}
        is a host-side staticlib symbol auto-linked at compile time.
      </p>

      <CodeBlock lang="cssl" caption="scenes/city_central_hub.csl (excerpt)">{`module com.apocky.loa.scenes.city_central_hub

extern "C" fn scene_open(player_id: u64, world_seed: u128, city_id: u32) -> u32 ;
extern "C" fn scene_procgen_grid(handle: u32, biome_affinity: u32) -> u32 ;
extern "C" fn scene_procgen_npc_population(handle: u32, target_count: u32) -> u32 ;

fn on_scene_enter(player_id: u64, world_seed: u128) -> u32 {
    let city: u32 = 1 ;                                        // NeverhomeRise
    let h: u32 = scene_open(player_id, world_seed, city) ;
    let _g: u32 = scene_procgen_grid(h, city) ;
    let _n: u32 = scene_procgen_npc_population(h, 4096) ;       // ≥ 4096 NPCs @ 60fps
    h
}`}</CodeBlock>

      <h2 className="docs-h2">§ Available today (✓)</h2>
      <ul className="docs-ul">
        <li><code className="docs-ic">module &lt;path&gt;</code> declarations · single-module compile</li>
        <li><code className="docs-ic">fn name(args) -&gt; T</code> with i8/i16/i32/i64/i128/u8/u16/u32/u64/u128/f32/f64/bool</li>
        <li><code className="docs-ic">extern "C" fn</code> for FFI to the loa-host + cssl-rt staticlibs</li>
        <li><code className="docs-ic">struct</code>, <code className="docs-ic">let mut</code>, control flow, expressions</li>
        <li>Cranelift object-file emission · auto-default-link for <code className="docs-ic">cssl-rt</code> + <code className="docs-ic">loa-host</code></li>
        <li>Glyph-tolerant comments and docs (CSL3 dialect)</li>
      </ul>

      <h2 className="docs-h2">§ In progress (◐) · Coming soon (○)</h2>
      <Callout kind="coming-soon" title="csslc multi-module compile">
        ◐ The 10 sibling modules under <code className="docs-ic">Labyrinth of Apocalypse/systems/</code> and{' '}
        <code className="docs-ic">scenes/</code> are tracked in git but not yet ingested at compile time. The
        <code className="docs-ic"> csslc build</code> subcommand currently accepts one positional input. POD-4-D3
        unlocks multi-module compile via <code className="docs-ic">--module-path</code> or auto-discovery from
        a manifest. See <a href="/docs/cssl-modules" style={{ color: '#7dd3fc' }}>/docs/cssl-modules</a>.
      </Callout>

      <Callout kind="coming-soon" title="Σ-mask in the type system">
        ○ First-class Σ-masks at every reference site — the consent invariant compiler-checked at every call —
        is the next major language slice after multi-module. Until then, sovereignty is enforced by the cap
        runtime (see <a href="/docs/sovereignty" style={{ color: '#7dd3fc' }}>/docs/sovereignty</a>).
      </Callout>

      <Callout kind="coming-soon" title="Iterate-everywhere syntax">
        ○ Loops · recursion · fold/scan/map across ω-field axes unified into one syntactic form, with the compiler
        choosing GPU-shader vs CPU-SIMD vs streaming-KAN-edge as the lowering target.
      </Callout>

      <h2 className="docs-h2">§ Where to read more</h2>
      <ul className="docs-ul">
        <li><a href="/docs/cssl-modules" style={{ color: '#7dd3fc' }}>Module system</a></li>
        <li><a href="/docs/cssl-ffi" style={{ color: '#7dd3fc' }}>FFI conventions</a></li>
        <li><a href="/docs/substrate" style={{ color: '#7dd3fc' }}>Substrate primitives the language wraps</a></li>
        <li><a href="/devblog/why-cssl" style={{ color: '#7dd3fc' }}>Why CSSL · long-form devblog post</a></li>
      </ul>

      <PrevNextNav slug="cssl-language" />
    </DocsLayout>
  );
};

export default Page;
