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

// Input-sibling catalog
pub mod input;
pub mod movement;
pub mod physics;

// MCP-sibling catalog
pub mod mcp_server;
pub mod mcp_tools;

// DM-sibling catalog
pub mod dm_director;
pub mod gm_narrator;
pub mod dm_runtime;

// UI-overlay catalog (CPU-side text/menu logic always built ; GPU pipeline
// gated on `runtime` feature inside the module).
pub mod ui_overlay;

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
    fn embedded_shader_has_required_entry_points() {
        assert!(SCENE_WGSL.contains("vs_main"));
        assert!(SCENE_WGSL.contains("fs_main"));
    }
}
