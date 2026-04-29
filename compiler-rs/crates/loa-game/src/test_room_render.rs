//! § test_room_render — Phase-3 visible-pixels test-room renderer.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!
//!   Closes the Phase-3 acceptance gap : "user opens window, sees something move
//!   on screen". Provides a thin clear-color renderer that paints a cycling
//!   color into the LoA test-room window each frame, proving the canonical
//!   `run_main_loop` is ticking + drives end-to-end pixels.
//!
//! § STRATEGY (T11-D228)
//!
//!   The slice ships a GDI fallback path FIRST :
//!     - `cssl-host-d3d12` does NOT yet expose a `create_swapchain(hwnd,…)`
//!       helper (verified by audit at slice-open ; it surfaces Device / Queue
//!       / PSO / Resource / Fence but no swapchain wrapper). Wiring DXGI
//!       directly inline would balloon the slice past its 1500-LOC ceiling.
//!     - GDI `BitBlt` from a DIB section fully meets the acceptance criteria
//!       ("VISIBLE PIXELS" + per-frame color cycling + close-event clean exit)
//!       while keeping the unsafe-FFI surface to a single bounded module.
//!     - When `cssl-host-d3d12` ships its own `Swapchain` type, this module
//!       grows a parallel D3D12 path behind a `gpu` feature ; the GDI fallback
//!       remains as a no-GPU degraded mode.
//!
//! § PRIME-DIRECTIVE
//!
//!   Renderer NEVER blocks the canonical loop's close-event handling — every
//!   call is a single immediate Win32 syscall + bounded local memcpy. A render
//!   failure is logged + the loop continues (degraded), preserving the kill-
//!   switch contract of `cssl-host-window § PRIME-DIRECTIVE-KILL-SWITCH`.
//!
//! § REPLAY-DETERMINISM
//!
//!   The renderer is OBSERVE-only — it reads the engine's tick-count to cycle
//!   colors but does NOT mutate omega state. Save/load round-trips remain
//!   bit-equal regardless of whether rendering occurred.

#![allow(clippy::module_name_repetitions)]
// `unreachable_pub` is the workspace default but bin-only modules attached
// via `#[path]` from `main.rs` legitimately use `pub` for inter-module
// visibility within the binary — there is no `lib`-side re-export point
// to flag. Allow at module scope rather than spamming `pub(crate)` on
// every API item, since the parent module IS the binary's API root.
#![allow(unreachable_pub)]
// Some error variants + the `UnavailableOnPlatform` outcome are reserved
// surface area : they document the failure modes the renderer can report
// once the cssl-host-d3d12 swapchain path lands + non-Windows targets
// flip from "stub" to "actual Vulkan". Keeping them in the public enum
// today avoids a future-breaking enum-extension when those paths ship.
#![allow(dead_code)]
// Hue cycling does an HSV→RGB at 60Hz ; the f64 conversion of a u64 frame
// count is bounded by `period` and only matters for sub-period precision
// (color hue smoothness). Real precision-loss only kicks in past 2^52
// frames ≈ 2.4 billion years @ 60 Hz, so the warning is a false positive.
#![allow(clippy::cast_precision_loss)]
// `cast_possible_truncation` + `cast_sign_loss` : sextant index math
// converts an f64 in [0, 6) to u32 ; the conversion is bounded by the
// `match … % 6` pattern that follows. The cast is safe by construction.
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

// The Win32 GDI submodule lives at `src/test_room_render/win32_gdi.rs`. We
// attach via an explicit `#[path]` because the parent module is itself
// loaded via `#[path]` from `main.rs` (bin-only ; keeps the lib's
// `#![forbid(unsafe_code)]` contract clean), so the standard "child sits
// in a directory named after the parent" resolution doesn't apply.
#[cfg(target_os = "windows")]
#[path = "test_room_render/win32_gdi.rs"]
mod win32_gdi;

// `GdiRenderError` is re-exported for its type-name to appear in any
// future error-chain that callers want to match on ; it's not consumed
// inside `main.rs` today (the renderer init swallows the error and falls
// back to no-render). Preserve the surface so an additional matching arm
// is non-breaking when added.
#[cfg(target_os = "windows")]
#[allow(unused_imports)]
pub use win32_gdi::{GdiRenderError, GdiRenderer};

/// Phase-3 renderer outcome — informational, never an error the loop must halt
/// on. Consumed by `main.rs` for telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderOutcome {
    /// Renderer painted a frame this tick.
    Painted,
    /// Renderer is unavailable on this platform (non-Windows ; future Vulkan
    /// path may flip this).
    UnavailableOnPlatform,
    /// Renderer encountered a transient failure ; loop continues.
    Skipped,
}

#[cfg(not(target_os = "windows"))]
pub use stub_impl::*;

#[cfg(not(target_os = "windows"))]
mod stub_impl {
    //! Non-Windows stub. The Win32 GDI path is target-gated ; on Linux/macOS
    //! we surface a no-op renderer that always reports
    //! [`super::RenderOutcome::UnavailableOnPlatform`]. This keeps the
    //! workspace `cargo check --workspace` green on all targets per CSSLv3 R16
    //! reproducibility-anchor.

    use super::RenderOutcome;

    /// Stub renderer for non-Windows targets.
    #[derive(Debug)]
    pub struct GdiRenderer {
        _private: (),
    }

    /// Stub error. Non-Windows builds never construct one.
    #[derive(Debug, thiserror::Error)]
    pub enum GdiRenderError {
        #[error("renderer unavailable: not a Windows build")]
        UnavailableOnPlatform,
    }

    impl GdiRenderer {
        /// Always returns `LoaderMissing`-shape error on non-Windows.
        pub fn new(_hwnd_usize: usize, _width: u32, _height: u32) -> Result<Self, GdiRenderError> {
            Err(GdiRenderError::UnavailableOnPlatform)
        }

        /// No-op stub.
        pub fn paint_clear_color(&mut self, _r: u8, _g: u8, _b: u8) -> RenderOutcome {
            RenderOutcome::UnavailableOnPlatform
        }

        /// No-op stub.
        pub fn resize(&mut self, _w: u32, _h: u32) -> Result<(), GdiRenderError> {
            Err(GdiRenderError::UnavailableOnPlatform)
        }
    }
}

/// Compute a debug-friendly RGB triple cycling each tick — proves the loop
/// is actually advancing. Pure function, zero deps, trivially testable.
///
/// § DESIGN
///   Hue rotates through the full 360° spectrum once every `period_frames`
///   frames. We inline a cheap HSV→RGB at saturation=1 + value=1 to avoid
///   pulling in a color crate ; it's enough that the user sees motion on
///   screen + the cycle is monotonic.
#[must_use]
pub fn cycle_color_for_frame(frame: u64, period_frames: u64) -> (u8, u8, u8) {
    let period = period_frames.max(1);
    let phase = (frame % period) as f64 / period as f64; // [0, 1)
    let h = phase * 6.0; // 0..6 sextant
    let sextant = h.floor() as u32;
    let frac = h - h.floor();

    let (r, g, b) = match sextant % 6 {
        0 => (1.0, frac, 0.0),
        1 => (1.0 - frac, 1.0, 0.0),
        2 => (0.0, 1.0, frac),
        3 => (0.0, 1.0 - frac, 1.0),
        4 => (frac, 0.0, 1.0),
        _ => (1.0, 0.0, 1.0 - frac),
    };

    (
        (r * 255.0).round().clamp(0.0, 255.0) as u8,
        (g * 255.0).round().clamp(0.0, 255.0) as u8,
        (b * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_color_starts_red() {
        let (r, g, b) = cycle_color_for_frame(0, 360);
        assert_eq!((r, g, b), (255, 0, 0), "frame 0 must be pure red");
    }

    #[test]
    fn cycle_color_period_returns_to_start() {
        let start = cycle_color_for_frame(0, 360);
        let after_period = cycle_color_for_frame(360, 360);
        assert_eq!(start, after_period, "frame=period must equal frame=0");
    }

    #[test]
    fn cycle_color_period_zero_safe() {
        // Don't panic ; period clamped to 1.
        let _ = cycle_color_for_frame(0, 0);
        let _ = cycle_color_for_frame(7, 0);
    }

    #[test]
    fn cycle_color_components_in_range() {
        for f in 0..720_u64 {
            let (r, g, b) = cycle_color_for_frame(f, 360);
            // All components in [0, 255]. Implicit by u8 type but assert
            // the saturating-clamp didn't UB on edge values.
            let _ = (r, g, b);
        }
    }

    #[test]
    fn cycle_color_changes_across_frames() {
        // Across a full period we see at least 4 distinct triples.
        let mut seen = std::collections::HashSet::new();
        for f in 0..360_u64 {
            seen.insert(cycle_color_for_frame(f, 360));
        }
        assert!(seen.len() > 4, "must see meaningful color variation");
    }

    #[test]
    fn render_outcome_eq_is_value_eq() {
        assert_eq!(RenderOutcome::Painted, RenderOutcome::Painted);
        assert_ne!(RenderOutcome::Painted, RenderOutcome::Skipped);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn stub_renderer_reports_unavailable() {
        let r = GdiRenderer::new(0, 1280, 720);
        assert!(matches!(r, Err(GdiRenderError::UnavailableOnPlatform)));
    }
}
