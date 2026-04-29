//! § compose_xr_layers — Stage 12 : XR composition + flat-screen fallback.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage 12 of the pipeline. Composes the per-eye color + depth + motion
//!   into XR composition layers when the OpenXR runtime is present, OR
//!   into a single flat-screen mono frame when running headless.
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody."

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::motion_vec_pass::MotionVecBuffer;
use super::tonemap_pass::ToneMapOutputs;

/// Final composed frame — what would be submitted to xrEndFrame OR drawn
/// to the host window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComposedFrame {
    /// Frame this snapshot covers.
    pub frame_idx: u64,
    /// Final RGB output (linear, pre-OETF).
    pub rgb: [f32; 3],
    /// Whether stereo composition was used (vs flat-screen mono).
    pub stereo: bool,
    /// Number of composition layers in the final stack.
    pub layer_count: u8,
}

impl ComposedFrame {
    /// Determinism-hash hook — feeds into the FramePipelineDigest.
    pub fn hash_for_determinism<H: Hasher>(&self, h: &mut H) {
        self.frame_idx.hash(h);
        for v in self.rgb {
            v.to_bits().hash(h);
        }
        self.stereo.hash(h);
        self.layer_count.hash(h);
    }

    /// Convenience hash function (for in-pipeline determinism comparison).
    #[must_use]
    pub fn determinism_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.hash_for_determinism(&mut h);
        h.finish()
    }
}

/// Composition path report — which path was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComposeXrReport {
    /// Frame this report covers.
    pub frame_idx: u64,
    /// Whether the XR runtime path was used.
    pub xr_path_used: bool,
    /// Whether the flat-screen fallback was used.
    pub flat_path_used: bool,
    /// Number of composition layers submitted.
    pub layer_count: u8,
}

/// Stage 12 driver.
#[derive(Debug, Clone, Copy)]
pub struct ComposeXrLayers {
    xr_enabled: bool,
}

impl ComposeXrLayers {
    /// Construct with the given XR mode.
    #[must_use]
    pub fn new(xr_enabled: bool) -> Self {
        Self { xr_enabled }
    }

    /// Whether the XR path is currently active.
    #[must_use]
    pub fn xr_enabled(&self) -> bool {
        self.xr_enabled
    }

    /// Run Stage 12. Returns the composed frame + a path-decomposition report.
    pub fn run(
        &self,
        tonemap: &ToneMapOutputs,
        motion_vec: &MotionVecBuffer,
        frame_idx: u64,
    ) -> (ComposedFrame, ComposeXrReport) {
        if self.xr_enabled {
            self.compose_xr(tonemap, motion_vec, frame_idx)
        } else {
            self.compose_flat(tonemap, motion_vec, frame_idx)
        }
    }

    fn compose_xr(
        &self,
        tonemap: &ToneMapOutputs,
        motion_vec: &MotionVecBuffer,
        frame_idx: u64,
    ) -> (ComposedFrame, ComposeXrReport) {
        let _ = motion_vec; // motion-vec layer would be appended in real impl
                            // Two layers : main color + AppSW motion-vec (when available).
        let layer_count = 2_u8;
        let composed = ComposedFrame {
            frame_idx,
            rgb: [
                tonemap.linear_rgb.r,
                tonemap.linear_rgb.g,
                tonemap.linear_rgb.b,
            ],
            stereo: true,
            layer_count,
        };
        let report = ComposeXrReport {
            frame_idx,
            xr_path_used: true,
            flat_path_used: false,
            layer_count,
        };
        (composed, report)
    }

    fn compose_flat(
        &self,
        tonemap: &ToneMapOutputs,
        motion_vec: &MotionVecBuffer,
        frame_idx: u64,
    ) -> (ComposedFrame, ComposeXrReport) {
        let _ = motion_vec; // unused on flat path
                            // Flat path emits one mono layer.
        let layer_count = 1_u8;
        let composed = ComposedFrame {
            frame_idx,
            rgb: [
                tonemap.linear_rgb.r,
                tonemap.linear_rgb.g,
                tonemap.linear_rgb.b,
            ],
            stereo: false,
            layer_count,
        };
        let report = ComposeXrReport {
            frame_idx,
            xr_path_used: false,
            flat_path_used: true,
            layer_count,
        };
        (composed, report)
    }
}

/// Helper for the flat-screen compose path (used by tests + unit checks).
#[must_use]
pub fn flat_screen_compose(rgb: [f32; 3], frame_idx: u64) -> ComposedFrame {
    ComposedFrame {
        frame_idx,
        rgb,
        stereo: false,
        layer_count: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_spectral_render::{Cie1931Xyz, DisplayPrimaries, SrgbColor};

    fn tonemap() -> ToneMapOutputs {
        ToneMapOutputs {
            frame_idx: 0,
            xyz: Cie1931Xyz::new(0.5, 0.5, 0.5),
            linear_rgb: SrgbColor::new(0.5, 0.5, 0.5),
            primaries: DisplayPrimaries::Srgb,
        }
    }

    fn motion_vec() -> MotionVecBuffer {
        MotionVecBuffer {
            frame_idx: 0,
            width: 8,
            height: 8,
            mean_motion_mag: 0.01,
            max_motion_mag: 0.02,
            appsw_recommended: true,
        }
    }

    #[test]
    fn compose_constructs() {
        let _ = ComposeXrLayers::new(true);
        let _ = ComposeXrLayers::new(false);
    }

    #[test]
    fn compose_xr_path_emits_stereo() {
        let c = ComposeXrLayers::new(true);
        let (frame, report) = c.run(&tonemap(), &motion_vec(), 0);
        assert!(report.xr_path_used);
        assert!(frame.stereo);
    }

    #[test]
    fn compose_flat_path_emits_mono() {
        let c = ComposeXrLayers::new(false);
        let (frame, report) = c.run(&tonemap(), &motion_vec(), 0);
        assert!(report.flat_path_used);
        assert!(!frame.stereo);
    }

    #[test]
    fn compose_replay_bit_equal() {
        let c1 = ComposeXrLayers::new(false);
        let c2 = ComposeXrLayers::new(false);
        let (a, _) = c1.run(&tonemap(), &motion_vec(), 7);
        let (b, _) = c2.run(&tonemap(), &motion_vec(), 7);
        assert_eq!(a, b);
    }
}
