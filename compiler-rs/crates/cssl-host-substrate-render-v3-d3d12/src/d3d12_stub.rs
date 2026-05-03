//! § d3d12_stub — non-Windows / non-runtime mock D3D12SubstrateRenderer.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! The L8 host is **Windows-only by design** (D3D12 + DXGI are Windows-
//! native). On Linux + macOS CI runners — and on Windows builds where the
//! `runtime` feature is off — the renderer-type still needs to be exposed
//! at the crate root so downstream callers can depend on the type-name
//! without flipping features at every use-site.
//!
//! This stub :
//!   - Holds the [`DxilArtifact`] + the resolved [`TearingPolicy`] +
//!     the current per-frame ring index.
//!   - Returns `Ok(stub-renderer)` from `try_new` — never errors. This
//!     mirrors the v3-vulkan `try_headless_ash_renderer` skip-path that
//!     returns `None` on GPU-less CI ; the L8 mock returns a working
//!     stub so the full layered-construction surface is exercised by
//!     `cargo test --workspace` on every platform.
//!   - Returns `Err(PresentError::UnsupportedWindowHandle)` from every
//!     swapchain-bound entry-point. Callers on non-Windows that try to
//!     hand us a window-handle get a clean error.
//!   - Advances the per-frame ring on every `dispatch_with_present` call
//!     (modulo [`FRAMES_IN_FLIGHT`]) so the ring-index sanity test runs
//!     on non-Windows CI.
//!
//! The stub is **#![forbid(unsafe_code)]** at the crate root (the
//! `runtime` feature is off here) ; no FFI, no D3D12, no DXGI. Pure
//! Rust state-machine.

use super::{
    Crystal, DxilArtifact, ObserverCoord, PresentError, TearingPolicy, FRAMES_IN_FLIGHT,
};

/// § Mock D3D12 substrate-renderer — non-Windows / non-runtime.
///
/// Field-for-field shape-match against the real
/// [`crate::d3d12_runtime::D3D12SubstrateRenderer`] for the public surface
/// (`frames_in_flight`, `tearing_policy`, `current_frame`,
/// `dispatch_with_present`). All D3D12 / DXGI handles are **omitted** —
/// the stub does not link to `windows-rs`. Any caller that tries to grab
/// a real handle would have to flip the `runtime` feature, at which
/// point the real module replaces this one.
#[derive(Debug)]
pub struct D3D12SubstrateRenderer {
    /// The DXIL artifact ; carried for introspection + the post-FOUNDATION
    /// PSO-build slice.
    artifact: DxilArtifact,
    /// Resolved tearing-policy from `LOA_DXIL_PRESENT_TEAR` env-override.
    tearing_policy: TearingPolicy,
    /// Render-target dimensions (width, height) supplied at construction.
    /// The stub does not own a swapchain ; this is recorded for parity
    /// with the runtime module + for downstream replay.
    extent: (u32, u32),
    /// Current per-frame ring index. Starts at 0 ; advances modulo
    /// [`FRAMES_IN_FLIGHT`] on every `dispatch_with_present` call.
    current_frame: usize,
    /// Total frames presented — exposed for tests + telemetry.
    frame_counter: u64,
}

impl D3D12SubstrateRenderer {
    /// § Headless construction — always succeeds on non-Windows / non-
    /// runtime. The real module's `try_new` may fail with
    /// [`PresentError::DeviceCreate`] on driver-less Windows hosts ;
    /// the stub never does.
    ///
    /// `extent = (width, height)` is the render-target size. The stub
    /// records it but does not allocate any GPU memory — it's a pure
    /// state-machine.
    pub fn try_new(
        artifact: DxilArtifact,
        extent: (u32, u32),
    ) -> Result<Self, PresentError> {
        let tearing_policy =
            TearingPolicy::from_env_bool(crate::tearing_enabled_from_env());
        Ok(Self {
            artifact,
            tearing_policy,
            extent,
            current_frame: 0,
            frame_counter: 0,
        })
    }

    /// § Swapchain construction — never available on the stub backend
    /// (the L8 host is Windows-only by design ; cross-platform present
    /// lives in L7 / `cssl-host-substrate-render-v3` on `ash`).
    ///
    /// Returns [`PresentError::UnsupportedWindowHandle`] unconditionally.
    /// Callers that hit this on non-Windows should fall back to L7.
    #[cfg(feature = "present")]
    pub fn try_new_with_swapchain<W: raw_window_handle::HasWindowHandle>(
        _window: &W,
        _dxil_bytes: &[u8],
        _extent: (u32, u32),
    ) -> Result<Self, PresentError> {
        Err(PresentError::UnsupportedWindowHandle)
    }

    /// Per-frame ring depth — always 3. Const-fn for compile-time use.
    #[must_use]
    pub const fn frames_in_flight(&self) -> usize {
        FRAMES_IN_FLIGHT
    }

    /// Resolved tearing-policy.
    #[must_use]
    pub const fn tearing_policy(&self) -> TearingPolicy {
        self.tearing_policy
    }

    /// Render-target extent `(width, height)` recorded at construction.
    #[must_use]
    pub const fn extent(&self) -> (u32, u32) {
        self.extent
    }

    /// Current per-frame ring index `[0, FRAMES_IN_FLIGHT)`.
    #[must_use]
    pub const fn current_frame(&self) -> usize {
        self.current_frame
    }

    /// Total frames presented since construction.
    #[must_use]
    pub const fn frame_counter(&self) -> u64 {
        self.frame_counter
    }

    /// Borrow the underlying DXIL artifact.
    #[must_use]
    pub fn artifact(&self) -> &DxilArtifact {
        &self.artifact
    }

    /// § Per-frame dispatch — mock-mode. Advances the per-frame ring
    /// index + the frame-counter. Does **not** touch any GPU surface
    /// (none exists on the stub). Returns `Ok(())` always.
    pub fn dispatch_with_present(
        &mut self,
        _observer: ObserverCoord,
        _crystals: &[Crystal],
    ) -> Result<(), PresentError> {
        self.current_frame = (self.current_frame + 1) % FRAMES_IN_FLIGHT;
        self.frame_counter = self.frame_counter.saturating_add(1);
        Ok(())
    }

    /// § Resize — mock no-op that updates the recorded extent. Real
    /// module rebuilds the swapchain via `IDXGISwapChain3::ResizeBuffers`.
    pub fn resize(&mut self, extent: (u32, u32)) -> Result<(), PresentError> {
        self.extent = extent;
        Ok(())
    }
}
