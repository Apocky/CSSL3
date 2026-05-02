// apocky.com/docs/cssl-ffi

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import CodeBlock from '@/components/CodeBlock';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

const Page: NextPage = () => {
  return (
    <DocsLayout
      activeSlug="cssl-ffi"
      title="CSSL FFI · Apocky Docs"
      description="The CSSL foreign-function-interface conventions — extern C, pointer + length pairs, u32 status codes, and the auto-default-link mechanism."
    >
      <h1 className="docs-h1">CSSL FFI</h1>
      <p className="docs-blurb">§ extern "C" declarations · pointer + length pairs · u32 status codes · auto-default-link.</p>

      <h2 className="docs-h2">§ The contract</h2>
      <p className="docs-p">
        CSSL FFI is intentionally narrow: declare a function signature with{' '}
        <code className="docs-ic">extern "C" fn name(args) -&gt; T ;</code> and the auto-default-link mechanism in
        <code className="docs-ic"> csslc</code> resolves the symbol against either <code className="docs-ic">cssl-rt</code>{' '}
        (allocator + panic + exit) or one of the host-side staticlibs (<code className="docs-ic">loa-host</code>{' '}
        and the per-system <code className="docs-ic">cssl-host-*</code> crates).
      </p>

      <CodeBlock lang="cssl" caption="Minimal extern declaration">{`extern "C" fn __cssl_engine_run() -> i32 ;

fn main() -> i32 {
    let exit_code: i32 = __cssl_engine_run() ;
    exit_code
}`}</CodeBlock>

      <h2 className="docs-h2">§ Pointer + length convention</h2>
      <p className="docs-p">
        CSSL stage-0 does not yet pass <code className="docs-ic">Vec&lt;T&gt;</code> across the FFI boundary. When a host
        symbol needs to expose a buffer, the convention is two adjacent arguments — one raw pointer and one
        length — both passed as primitive integers. This matches the C ABI exactly and works on every target
        Cranelift emits today.
      </p>

      <CodeBlock lang="cssl" caption="Reading a host-side buffer">{`// Host signature in Rust:
//   #[no_mangle]
//   pub extern "C" fn __cssl_audit_read(out_ptr: *mut u8, out_len: u32) -> u32

extern "C" fn __cssl_audit_read(out_ptr: u64, out_len: u32) -> u32 ;

fn read_audit(buf_ptr: u64, buf_cap: u32) -> u32 {
    let written: u32 = __cssl_audit_read(buf_ptr, buf_cap) ;
    written            // 0 on no-data · ≤ buf_cap on success
}`}</CodeBlock>

      <Callout kind="note" title="Pointers as u64">
        Stage-0 CSSL types pointers as <code className="docs-ic">u64</code> at the FFI boundary on 64-bit targets.
        This is verbose but unambiguous. A typed <code className="docs-ic">*mut T</code> / <code className="docs-ic">*const T</code>{' '}
        surface lands once the type system gains aliasing rules — see the language overview.
      </Callout>

      <h2 className="docs-h2">§ u32 status-code pattern</h2>
      <p className="docs-p">
        Almost every host symbol returns a <code className="docs-ic">u32</code> status. Zero is success; non-zero is
        a structured error code that the caller can route to a typed error. This avoids exceptions, avoids
        <code className="docs-ic"> Result</code>-shaped FFI marshalling, and keeps the audit-trail emission cheap (one
        u32 to log per call).
      </p>

      <table className="docs-table">
        <thead>
          <tr>
            <th>Code</th>
            <th>Convention</th>
          </tr>
        </thead>
        <tbody>
          <tr><td><code className="docs-ic">0</code></td><td>OK · no error · result valid</td></tr>
          <tr><td><code className="docs-ic">1..127</code></td><td>Domain-specific error · stable per host crate</td></tr>
          <tr><td><code className="docs-ic">128..255</code></td><td>Sovereign-cap error · cap missing or revoked</td></tr>
          <tr><td><code className="docs-ic">256..</code></td><td>Internal error · log-and-attest · should not surface to user</td></tr>
        </tbody>
      </table>

      <h2 className="docs-h2">§ Real example · scene FFI surface</h2>
      <p className="docs-p">
        From <code className="docs-ic">scenes/city_central_hub.csl</code>: a complete scene surface with eleven extern
        symbols, all returning <code className="docs-ic">u32</code> status, all dispatched against <code className="docs-ic">loa-host</code>{' '}
        wired-fn glue.
      </p>

      <CodeBlock lang="cssl" caption="Eleven extern symbols · one scene">{`extern "C" fn scene_open(player_id: u64, world_seed: u128, city_id: u32) -> u32 ;
extern "C" fn scene_close(handle: u32) -> u32 ;
extern "C" fn scene_procgen_grid(handle: u32, biome_affinity: u32) -> u32 ;
extern "C" fn scene_procgen_buildings(handle: u32, density_tier: u32) -> u32 ;
extern "C" fn scene_procgen_interiors(handle: u32, enterable_pct: u32) -> u32 ;
extern "C" fn scene_procgen_npc_population(handle: u32, target_count: u32) -> u32 ;
extern "C" fn scene_kan_classify_template(handle: u32, hist_hash: u64, zone: u32, bias: u64) -> u32 ;
extern "C" fn scene_query_npc_count(handle: u32) -> u32 ;
extern "C" fn scene_apply_lod(handle: u32, player_x: i32, player_y: i32, player_z: i32) -> u32 ;
extern "C" fn scene_audit_emit(handle: u32, event_kind: u32, payload_hash: u64) -> u32 ;
extern "C" fn scene_tick(handle: u32, dt_micros: u32) -> u32 ;`}</CodeBlock>

      <h2 className="docs-h2">§ The auto-default-link mechanism</h2>
      <p className="docs-p">
        When <code className="docs-ic">csslc</code> emits an object file, the linker invocation automatically prepends
        the canonical staticlibs found in <code className="docs-ic">compiler-rs/target/release/</code>:
      </p>
      <ul className="docs-ul">
        <li><code className="docs-ic">libcssl_rt.a</code> · provides <code className="docs-ic">__cssl_alloc</code>, <code className="docs-ic">__cssl_panic</code>, <code className="docs-ic">__cssl_exit</code>, etc.</li>
        <li><code className="docs-ic">libloa_host.a</code> · provides <code className="docs-ic">__cssl_engine_run</code>, scene fns, render fns, MCP server</li>
      </ul>

      <Callout kind="coming-soon" title="Per-system staticlib auto-link">
        ○ POD-4-D5..D8 extends auto-default-link to also discover <code className="docs-ic">cssl-host-combat-sim</code>,
        <code className="docs-ic"> cssl-host-craft-graph</code>, <code className="docs-ic">cssl-host-procgen-city</code>, etc., so the
        sibling-module FFI surfaces resolve without manual <code className="docs-ic">--link</code> flags.
      </Callout>

      <h2 className="docs-h2">§ Symbol-name conventions</h2>
      <ul className="docs-ul">
        <li><code className="docs-ic">__cssl_*</code> — runtime + engine symbols (cssl-rt, loa-host)</li>
        <li><code className="docs-ic">scene_*</code> — scene-orchestration symbols</li>
        <li><code className="docs-ic">render.*</code> — MCP-tool route names (dotted, with router lookup)</li>
        <li>system tick fns are <code className="docs-ic">__cssl_&lt;system&gt;_tick_&lt;phase&gt;</code></li>
      </ul>

      <PrevNextNav slug="cssl-ffi" />
    </DocsLayout>
  );
};

export default Page;
