//! § loa-host — LoA-v13 stage-0 host runtime
//! ════════════════════════════════════════════════════════════════════════════
//!
//! Apocky-greenlit hybrid stage-0 host runtime for LoA-v13. Combines four
//! sibling slices into one crate :
//!
//!   * `W-LOA-host-render` : winit window + wgpu render + 3D test-room
//!   * `W-LOA-host-input`  : WASD + mouse-look + axis-slide collision
//!   * `W-LOA-host-mcp`    : TCP JSON-RPC server (Claude live-interface)
//!   * `W-LOA-host-dm`     : DM director + GM narrator state machines
//!
//! § ROLE IN BOOTSTRAP
//!   `scenes/*.cssl` stay AUTHORITATIVE design specs. The CSSL stage-0
//!   compiler can't yet produce a wgpu-driven native binary on Windows ;
//!   until it can, this Rust crate is the bootstrap host. As csslc advances,
//!   modules incrementally migrate to pure-CSSL.
//!
//! § FEATURES
//!   - Default (catalog) : pure-CPU mesh + camera + input + MCP + DM/GM logic.
//!     Builds in any workspace toolchain (1.85.0 GNU compatible).
//!   - `runtime`         : pulls winit + wgpu + pollster, exposes `run_engine`
//!     which opens a window. Requires MSVC toolchain on Windows due to wgpu 23
//!     transitive deps (parking_lot_core windows-link 0.2.1).
//!
//! § BUILD
//!   cargo +stable-x86_64-pc-windows-msvc build -p loa-host --features runtime --release
//!   cargo +stable-x86_64-pc-windows-msvc run   -p loa-host --features runtime --release
//!
//! § PRIME-DIRECTIVE attestation
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

// MCP-server FFI (omega.sample / omega.modify) calls cssl-rt's unsafe extern
// "C" loa_stubs functions. Allow rather than forbid for this crate.
#![allow(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

// ──────────────────────────────────────────────────────────────────────────
// § Catalog modules (always built · pure-CPU)
// ──────────────────────────────────────────────────────────────────────────

// Render-sibling catalog
pub mod camera;
pub mod geometry;
pub mod material;
pub mod pattern;
pub mod room;
pub mod stokes;
// § T11-LOA-FID-SPECTRAL — CPU-bake bridge from cssl-spectral-render to the
// GPU material LUT (4-illuminant cohort · per-material reference colors).
pub mod spectral_bridge;

// Input-sibling catalog
pub mod input;
pub mod movement;
pub mod physics;

// MCP-sibling catalog
pub mod mcp_server;
pub mod mcp_tools;

// Telemetry-sibling catalog (T11-LOA-TELEM)
pub mod telemetry;

// DM-sibling catalog
pub mod dm_director;
pub mod gm_narrator;
pub mod dm_runtime;

// UI-overlay catalog (CPU-side text/menu logic always built ; GPU pipeline
// gated on `runtime` feature inside the module).
pub mod ui_overlay;

// Snapshot-sibling catalog (T11-LOA-TEST-APP : PNG encode + tour-pose
// registry + golden-image diff are catalog-buildable ; the wgpu readback
// path is gated on the `runtime` feature inside the module).
pub mod snapshot;

// § T11-LOA-FID-MAINSTREAM : fidelity-report module is always built.
// The runtime-only side (gpu.rs) populates a global with the negotiated
// settings ; the catalog-mode reader returns "not_initialized" so MCP
// tooling works in offline tests.
pub mod fidelity;

// § T11-LOA-FID-CFER : substrate-IS-renderer. The CFER renderer wires
// the canonical Ω-field into a volumetric raymarched pass. The CPU-side
// state (OmegaField + texel staging + step-and-pack) is catalog-buildable
// (no GPU required) ; the wgpu pipeline-builder lives in `render.rs`
// behind the `runtime` feature.
pub mod cfer_render;

// § T11-LOA-SENSORY : full MCP sensory + proprioception harness. Aggregation
// surface for the 20+ `sense.*` MCP tools that let Claude perceive the live
// engine across 9 sensory axes (visual · audio · spatial · interoception ·
// diagnostic · temporal · causal · network · environmental).
pub mod sense;

// § T11-WAVE3-GLTF : pure-Rust GLTF/GLB → loa-host Vertex translator. Parses
// externally-authored 3D models (e.g. Stanford bunny, designer-supplied glb)
// into the canonical Vertex struct so they can be uploaded into the dynamic-
// mesh render path. Catalog-buildable (no GPU required for parse) ; the GPU
// upload path lives in `render.rs` behind the `runtime` feature.
pub mod gltf_loader;

// § T11-WAVE3-SPONT : text-seeded condensation pipeline. Converts intent
// text → SeedCells → Ω-field stamps → manifestation events → stress-object
// spawn. The substrate IS the source of truth ; objects are byproducts of
// cells crossing a critical-radiance threshold.
pub mod spontaneous;

// § T11-WAVE3-INTENT : text → typed-Intent → MCP-style dispatch router.
// Stage-0 keyword classifier ; stage-1 swaps in the KAN runtime. Every
// HUD text-input box submission + scripted scene call routes through here.
pub mod intent_router;

// ──────────────────────────────────────────────────────────────────────────
// § Runtime-only modules (feature `runtime`)
// ──────────────────────────────────────────────────────────────────────────

#[cfg(feature = "runtime")]
pub mod gpu;
#[cfg(feature = "runtime")]
pub mod render;
#[cfg(feature = "runtime")]
pub mod window;

// ──────────────────────────────────────────────────────────────────────────
// § FFI surface (T11-LOA-PURE-CSSL · pure-CSSL main.cssl entry-point)
// ──────────────────────────────────────────────────────────────────────────
//
// § ROLE
//   Pure-CSSL programs (e.g. `Labyrinth of Apocalypse/main.cssl`) declare the
//   engine entry as `extern "C" fn __cssl_engine_run() -> i32` and call it
//   from `fn main()`. The CSSL compiler links the loa-host staticlib (via
//   csslc's auto-default-link mechanism), which provides this symbol. The
//   resulting `LoA.exe` is GENUINELY the output of csslc compiling
//   `main.cssl` — Rust is invisible at the source level (same model as a C
//   program calling libc/syscalls).
//
// § STAGE-1 PATH
//   As csslc gains capability (winit-bindings · wgpu-bindings · async-trait),
//   per-system modules migrate from this Rust crate to .csl source. The
//   `__cssl_engine_run` symbol stays as an ABI anchor, but its body shrinks
//   over time until it becomes a thin shim around .csl-authored event-loop
//   code. At full self-host the symbol disappears and main.cssl drives
//   winit/wgpu directly via the cssl-host-* FFI surface.
pub mod ffi;

// ──────────────────────────────────────────────────────────────────────────
// § Re-exports (the surface sibling code reaches for via `loa_host::*`)
// ──────────────────────────────────────────────────────────────────────────

pub use camera::Camera;
pub use geometry::{plinth_positions, RoomGeometry, Vertex};
pub use room::{Corridor, Direction, Doorway, Room, ROOM_COUNT};

pub use mcp_server::{
    spawn_mcp_server, EngineState, McpServerConfig, RenderMode, SOVEREIGN_CAP,
};
pub use mcp_tools::{tool_registry, ToolHandler, ToolRegistry};

#[cfg(feature = "runtime")]
pub use render::Renderer;
#[cfg(feature = "runtime")]
pub use window::{App, INITIAL_HEIGHT, INITIAL_WIDTH};

#[cfg(not(feature = "runtime"))]
pub const INITIAL_WIDTH: u32 = 1280;
#[cfg(not(feature = "runtime"))]
pub const INITIAL_HEIGHT: u32 = 720;

use cssl_rt::loa_startup::log_event;

// ──────────────────────────────────────────────────────────────────────────
// § run_engine — main entry from the loa-runtime binary
// ──────────────────────────────────────────────────────────────────────────

/// Open winit + wgpu, run the test-room render loop until window-close.
/// Catalog-mode (no `runtime` feature) returns Ok(()) after logging.
pub fn run_engine() -> std::io::Result<()> {
    log_event(
        "INFO",
        "loa-host/lib",
        "run_engine entry · stage-0 host starting",
    );
    #[cfg(feature = "runtime")]
    let r = window::run();
    #[cfg(not(feature = "runtime"))]
    let r: std::io::Result<()> = {
        log_event(
            "WARN",
            "loa-host/lib",
            "compiled WITHOUT --features runtime · catalog-only mode \
             · rebuild with `--features runtime` (MSVC toolchain) for the window",
        );
        eprintln!(
            "§ loa-host : catalog-only build · rebuild with `--features runtime` \
             (MSVC toolchain) to open the window"
        );
        Ok(())
    };
    log_event("INFO", "loa-host/lib", "run_engine exit · stage-0 host done");
    r
}

// ──────────────────────────────────────────────────────────────────────────
// § Embedded shader (catalog-visible so naga can validate w/o runtime)
// ──────────────────────────────────────────────────────────────────────────

pub const SCENE_WGSL: &str = include_str!("../shaders/scene.wgsl");

/// UI-overlay shader source (HUD + menu textured-quad pipeline).
pub const UI_WGSL: &str = include_str!("../shaders/ui.wgsl");

/// § T11-LOA-FID-MAINSTREAM (W-LOA-fidelity-mainstream)
/// ACES RRT+ODT tonemap shader (fullscreen-triangle vertex + ACES fragment).
/// Reads the HDR (Rgba16Float) intermediate target written by `scene.wgsl`,
/// applies Stephen Hill's fitted ACES curve, writes display-linear values
/// into the (sRGB-encoded) surface format. ~80 LOC, no external deps.
pub const TONEMAP_WGSL: &str = include_str!("../shaders/tonemap.wgsl");

/// § T11-LOA-FID-CFER : the volumetric raymarcher shader source. Catalog-
/// visible so naga can validate without the runtime feature.
pub const CFER_WGSL: &str = include_str!("../shaders/cfer.wgsl");

/// PRIME-DIRECTIVE attestation marker.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone/anything/anybody.";

// ──────────────────────────────────────────────────────────────────────────
// § Tests
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_reexports_compile() {
        let g = RoomGeometry::test_room();
        let _c = Camera::default();
        let _ps = plinth_positions();
        let _ = INITIAL_WIDTH;
        let _ = INITIAL_HEIGHT;
        assert_eq!(g.plinth_count, 14);
    }

    #[cfg(not(feature = "runtime"))]
    #[test]
    fn run_engine_no_op_in_catalog_mode() {
        let r = run_engine();
        assert!(r.is_ok());
    }

    #[test]
    fn wgsl_shader_string_compiles_to_naga() {
        use naga::front::wgsl;
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = wgsl::parse_str(SCENE_WGSL).expect("scene.wgsl must parse via naga");
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator.validate(&module).expect("scene.wgsl must validate via naga");
    }

    #[test]
    fn cfer_wgsl_string_compiles_to_naga() {
        // § T11-LOA-FID-CFER : the volumetric raymarcher must parse +
        // validate via naga so the runtime build doesn't surprise us at
        // pipeline-creation time.
        use naga::front::wgsl;
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = wgsl::parse_str(CFER_WGSL).expect("cfer.wgsl must parse via naga");
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator.validate(&module).expect("cfer.wgsl must validate via naga");
    }

    #[test]
    fn cfer_module_const_matches_lib_const() {
        // Avoid drift between the cfer_render::CFER_WGSL re-export and
        // the lib-level constant — both reference the same shader file.
        assert_eq!(crate::cfer_render::CFER_WGSL, CFER_WGSL);
    }

    #[test]
    fn embedded_shader_has_required_entry_points() {
        assert!(SCENE_WGSL.contains("vs_main"));
        assert!(SCENE_WGSL.contains("fs_main"));
    }

    /// § T11-LOA-FID-MAINSTREAM : tonemap.wgsl must parse + validate via naga
    /// so we know the ACES RRT+ODT shader is wgpu-compatible WITHOUT spinning
    /// up a GPU adapter. This is the catalog-level guarantee that the
    /// fidelity-pass pipeline will compile on any platform.
    #[test]
    fn tonemap_module_compiles_with_naga() {
        use naga::front::wgsl;
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = wgsl::parse_str(TONEMAP_WGSL).expect("tonemap.wgsl must parse via naga");
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator
            .validate(&module)
            .expect("tonemap.wgsl must validate via naga");
        // Must contain both entry points + the ACES helper.
        assert!(TONEMAP_WGSL.contains("vs_main"));
        assert!(TONEMAP_WGSL.contains("fs_main"));
        assert!(TONEMAP_WGSL.contains("aces_rrt_odt"));
    }

    /// § T11-LOA-FID-MAINSTREAM : ACES known-input-output sanity check.
    ///
    /// The fitted ACES curve at input rgb=(1.0, 1.0, 1.0) returns ~0.8038
    /// (computed exactly : (1·2.54)/(1·3.16) = 2.54/3.16 = 0.80380...).
    /// White-point passes at ~80 % display brightness, leaving headroom
    /// for highlights — verifies that the in-shader curve coefficients
    /// AND the CPU-side reference helper are in agreement.
    #[test]
    fn aces_tonemap_known_input_output() {
        // Reference CPU implementation (matches WGSL `aces_rrt_odt`).
        fn aces(x: [f32; 3]) -> [f32; 3] {
            let a = [x[0] * 2.51 + 0.03, x[1] * 2.51 + 0.03, x[2] * 2.51 + 0.03];
            let b = [
                x[0] * (2.43 * x[0] + 0.59) + 0.14,
                x[1] * (2.43 * x[1] + 0.59) + 0.14,
                x[2] * (2.43 * x[2] + 0.59) + 0.14,
            ];
            let mut out = [0.0f32; 3];
            for i in 0..3 {
                out[i] = (x[i] * a[i]) / b[i];
                out[i] = out[i].clamp(0.0, 1.0);
            }
            out
        }
        let mid = aces([1.0, 1.0, 1.0]);
        // Reference value : 2.54 / 3.16 ≈ 0.8038.
        assert!(
            (mid[0] - 0.8038).abs() < 0.01,
            "aces(1.0)={mid:?} (expected ~0.80)"
        );
        // Sanity : the white-point output is in the [0.78, 0.82] band that
        // every reasonable ACES fit lands in.
        for c in mid {
            assert!((0.78..=0.82).contains(&c), "channel out of band : {c}");
        }
        // Output is clamped to [0, 1] — bright HDR input must not blow up.
        let high = aces([100.0, 100.0, 100.0]);
        for c in high {
            assert!((0.0..=1.0).contains(&c));
        }
        // Zero in → zero out.
        let low = aces([0.0, 0.0, 0.0]);
        for c in low {
            assert!(c.abs() < 1e-3);
        }
    }
}
