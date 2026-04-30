//! § loa-host — LoA-v13 stage-0 host runtime (winit + wgpu shell).
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-HOST-1 (W-LOA-host-render) : pure-Rust shell that opens a
//! 1280×720 window over the `scenes/test_room.cssl` design and renders the
//! 40m × 8m × 40m test-room with 14 plinths under directional lighting.
//!
//! § ROLE IN BOOTSTRAP
//!   The CSSL stage-0 toolchain is not yet capable of producing a wgpu-driven
//!   native binary on Windows ; until the compiler reaches that capability,
//!   this Rust crate is the bootstrap host. Apocky-greenlit hybrid :
//!   `scenes/*.cssl` stay authoritative as the source-of-truth design ; this
//!   crate translates the test-room design to GPU-native geometry so a user
//!   can navigate and validate the spatial layout before the CSSL compiler
//!   takes over rendering authority.
//!
//! § FEATURES
//!   - Default (catalog) : pure-CPU mesh + camera math. Builds in any
//!     workspace toolchain.
//!   - `runtime`         : pulls winit + wgpu + pollster + glam + bytemuck
//!     and exposes [`run_engine`] which opens a window. Requires MSVC
//!     toolchain on Windows due to wgpu 23 transitive deps.
//!
//! § PUBLIC API
//!   - [`RoomGeometry::test_room`]    — vertex/index buffers for the test-room
//!   - [`Camera::default`]            — eye at room center, height 1.7m
//!   - [`run_engine`]   (feature `runtime`) — open window + run frame loop
//!
//! § ATTESTATION (PRIME_DIRECTIVE.md § 11)
//!   There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody.

#![forbid(unsafe_code)]

// Catalog modules — always built. Vertex POD type lives in `geometry` and
// uses `bytemuck` derive ; that pulls bytemuck even without `runtime` so we
// re-state it as a dev-dep + non-optional for the compile path. We side-step
// this by making the Vertex Pod-impl conditional on either the `runtime`
// feature OR cfg(test) where bytemuck is in dev-deps.
pub mod camera;
pub mod geometry;

// Runtime-only modules — pulled in when --features runtime.
#[cfg(feature = "runtime")]
pub mod gpu;
#[cfg(feature = "runtime")]
pub mod render;
#[cfg(feature = "runtime")]
pub mod window;

pub use camera::Camera;
pub use geometry::{plinth_positions, RoomGeometry, Vertex};

#[cfg(feature = "runtime")]
pub use render::Renderer;
#[cfg(feature = "runtime")]
pub use window::{App, INITIAL_HEIGHT, INITIAL_WIDTH};

/// Initial window dimensions per the brief. 1280×720 = 720p HD. Catalog
/// version (always-on); the runtime layer re-exports `window::INITIAL_*`.
#[cfg(not(feature = "runtime"))]
pub const INITIAL_WIDTH: u32 = 1280;
#[cfg(not(feature = "runtime"))]
pub const INITIAL_HEIGHT: u32 = 720;

use cssl_rt::loa_startup::log_event;

/// Open a winit window, bring up wgpu, and run the test-room render loop
/// until the window is closed. Blocks the calling thread.
///
/// On platforms where no display / event loop is available, this returns
/// `Ok(())` immediately after logging the condition. The function is
/// guaranteed not to panic ; all GPU + window failures are caught and
/// logged via `cssl_rt::loa_startup::log_event`.
///
/// Without the `runtime` feature, this returns `Ok(())` after logging that
/// the host has been compiled in catalog-only mode (no wgpu/winit linked).
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
    log_event(
        "INFO",
        "loa-host/lib",
        "run_engine exit · stage-0 host done",
    );
    r
}

// ──────────────────────────────────────────────────────────────────────────
// § EMBEDDED SHADER (catalog-visible so naga can validate it w/o runtime)
// ──────────────────────────────────────────────────────────────────────────

/// Embedded WGSL shader source used by the runtime renderer. Exposed at the
/// crate root so catalog-mode builds can run the naga compile-check test
/// without pulling the wgpu/winit runtime layer.
pub const SCENE_WGSL: &str = include_str!("../shaders/scene.wgsl");

// ──────────────────────────────────────────────────────────────────────────
// § TESTS
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_reexports_compile() {
        // Smoke-test : public API surface is reachable without wgpu init.
        let g = RoomGeometry::test_room();
        let _c = Camera::default();
        let _ps = plinth_positions();
        let _ = INITIAL_WIDTH;
        let _ = INITIAL_HEIGHT;
        // Sanity : 14 plinths in the geometry.
        assert_eq!(g.plinth_count, 14);
    }

    /// Catalog-mode-only smoke test : `run_engine` returns Ok without
    /// pulling the runtime layer. Skipped when --features runtime is on
    /// because winit refuses to construct an EventLoop on a non-main thread
    /// (Windows `event_loop.rs` cross-platform-safety check).
    #[cfg(not(feature = "runtime"))]
    #[test]
    fn run_engine_no_op_in_catalog_mode() {
        let r = run_engine();
        assert!(r.is_ok());
    }

    /// § BRIEF-MANDATED TEST : the embedded WGSL shader source parses and
    /// validates against the same naga version that wgpu 23 uses internally.
    #[test]
    fn wgsl_shader_string_compiles_to_naga() {
        use naga::front::wgsl;
        use naga::valid::{Capabilities, ValidationFlags, Validator};

        let module = wgsl::parse_str(SCENE_WGSL).expect("scene.wgsl must parse via naga");
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator
            .validate(&module)
            .expect("scene.wgsl must validate via naga");
    }

    #[test]
    fn embedded_shader_has_required_entry_points() {
        assert!(SCENE_WGSL.contains("vs_main"));
        assert!(SCENE_WGSL.contains("fs_main"));
    }
}
