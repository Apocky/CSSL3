//! Stage 11 — motion_vec.
//!
//! § ROLE   frame-N→N+1 motion-vector buffer per-frame (AppSW-feed)
//! § OUTPUT 2D motion-vectors per pixel (or per fovea-tile)
//! § DENSITY-BUDGET ≤ 0.4ms (90Hz) per DENSITY_BUDGET §V.11
//! § SPEC   `specs/RENDERING.csl` §I.11 + motion-vec for AppSW reprojection
//!
//! § INSTRUMENTATION-NOTE
//! `execute()` body is a synthetic placeholder ; in real impl, this writes
//! per-pixel motion-vectors used by Application-SpaceWarp (AppSW) for
//! frame-rate-doubling reprojection.

use crate::m8_integration::{Pass, PassContext, StageId};

/// Stage-11 driver — motion-vector buffer (AppSW-feed).
#[derive(Debug, Default)]
pub struct MotionVecPass {
    /// Per-frame internal counter.
    pub frames_executed: u64,
    /// Synthetic per-pixel-magnitude accumulator.
    pub last_max_magnitude: f32,
}

impl MotionVecPass {
    /// Construct a default pass.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Pass for MotionVecPass {
    fn stage_id(&self) -> StageId {
        StageId::MotionVec
    }

    fn execute(&mut self, ctx: &PassContext) {
        // Synthetic deterministic workload : track max-magnitude.
        // In real impl : write motion-vec G-buffer for AppSW.
        let mut max_mag: f32 = 0.0;
        for i in 0..ctx.workload {
            let m = ((i as f32).sin().abs()) * (ctx.frame_n as f32 * 0.01).cos().abs();
            if m > max_mag {
                max_mag = m;
            }
        }
        self.last_max_magnitude = max_mag;
        self.frames_executed = self.frames_executed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_vec_stage_id_correct() {
        let p = MotionVecPass::new();
        assert_eq!(p.stage_id(), StageId::MotionVec);
        assert_eq!(p.stage_id().index(), 11);
    }

    #[test]
    fn motion_vec_namespace_canonical() {
        let p = MotionVecPass::new();
        assert_eq!(
            p.stage_id().metric_namespace(),
            "pipeline.stage_11_motion_vec.frame_time_ms"
        );
    }

    #[test]
    fn motion_vec_execute_increments_counter() {
        let mut p = MotionVecPass::new();
        let ctx = PassContext::default();
        p.execute(&ctx);
        assert_eq!(p.frames_executed, 1);
    }

    #[test]
    fn motion_vec_max_mag_bounded_by_one() {
        let mut p = MotionVecPass::new();
        let ctx = PassContext {
            frame_n: 10,
            workload: 100,
            ..Default::default()
        };
        p.execute(&ctx);
        assert!(p.last_max_magnitude <= 1.0);
        assert!(p.last_max_magnitude >= 0.0);
    }

    #[test]
    fn motion_vec_execute_deterministic() {
        let mut p1 = MotionVecPass::new();
        let mut p2 = MotionVecPass::new();
        let ctx = PassContext {
            frame_n: 3,
            workload: 50,
            ..Default::default()
        };
        p1.execute(&ctx);
        p2.execute(&ctx);
        assert!((p1.last_max_magnitude - p2.last_max_magnitude).abs() < f32::EPSILON);
    }
}
