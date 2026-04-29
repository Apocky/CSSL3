//! § motion_vec_pass — Stage 11 : AppSW motion-vector + depth output.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage 11 of the pipeline. Produces the motion-vector + depth buffer
//!   that the OpenXR compositor uses for App-SpaceWarp reprojection. For
//!   M8 vertical-slice the motion-vec is a small deterministic mock —
//!   real motion-vec generation requires per-fragment differencing across
//!   adjacent frames.
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody."

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::sdf_raymarch_pass::SdfRaymarchOutputs;

/// Mock motion-vector buffer — 2-channel f16 motion-vec + 1-channel f16 depth.
/// For M8 we summarize via aggregate stats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionVecBuffer {
    /// Frame this snapshot covers.
    pub frame_idx: u64,
    /// Per-eye width.
    pub width: u32,
    /// Per-eye height.
    pub height: u32,
    /// Mean motion-vec magnitude across the frame.
    pub mean_motion_mag: f32,
    /// Max motion-vec magnitude across the frame.
    pub max_motion_mag: f32,
    /// Whether AppSW reprojection is recommended based on motion-vec validity.
    pub appsw_recommended: bool,
}

impl MotionVecBuffer {
    /// Hash for determinism comparison.
    #[must_use]
    pub fn determinism_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.frame_idx.hash(&mut h);
        self.width.hash(&mut h);
        self.height.hash(&mut h);
        self.mean_motion_mag.to_bits().hash(&mut h);
        self.max_motion_mag.to_bits().hash(&mut h);
        self.appsw_recommended.hash(&mut h);
        h.finish()
    }
}

/// Stage 11 driver.
#[derive(Debug, Clone, Copy)]
pub struct AppSwPassDriver {
    width: u32,
    height: u32,
}

impl AppSwPassDriver {
    /// Construct.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Run Stage 11.
    pub fn run(&self, raymarch: &SdfRaymarchOutputs, frame_idx: u64) -> MotionVecBuffer {
        // Synthesize motion-vec stats from the raymarch GBuffer summary.
        // In production this is a per-pixel current-vs-prev difference.
        let mean_motion_mag = (raymarch.mean_hit_t.max(0.0) * 0.001).min(0.5);
        let max_motion_mag = mean_motion_mag * 2.0;
        // AppSW is recommended when motion is small enough for stable reproj.
        let appsw_recommended = max_motion_mag < 0.25;

        MotionVecBuffer {
            frame_idx,
            width: self.width,
            height: self.height,
            mean_motion_mag,
            max_motion_mag,
            appsw_recommended,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raymarch() -> SdfRaymarchOutputs {
        SdfRaymarchOutputs {
            frame_idx: 0,
            hit_count: 4,
            total_steps: 16,
            mean_hit_t: 1.5,
            fovea_dist_left: [0.5, 0.3, 0.2],
            fovea_dist_right: [0.5, 0.3, 0.2],
            width: 8,
            height: 8,
        }
    }

    #[test]
    fn motion_vec_runs() {
        let d = AppSwPassDriver::new(8, 8);
        let o = d.run(&raymarch(), 0);
        assert_eq!(o.frame_idx, 0);
    }

    #[test]
    fn motion_vec_replay_bit_equal() {
        let d1 = AppSwPassDriver::new(8, 8);
        let d2 = AppSwPassDriver::new(8, 8);
        let a = d1.run(&raymarch(), 7);
        let b = d2.run(&raymarch(), 7);
        assert_eq!(a.determinism_hash(), b.determinism_hash());
    }

    #[test]
    fn motion_vec_recommends_appsw_on_low_motion() {
        let d = AppSwPassDriver::new(8, 8);
        let o = d.run(&raymarch(), 0);
        assert!(o.appsw_recommended);
    }
}
