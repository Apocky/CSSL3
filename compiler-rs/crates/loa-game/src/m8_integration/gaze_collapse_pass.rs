//! Stage 2 — gaze_collapse.
//!
//! § ROLE   saccade-driven Ω-collapse bias application per-frame
//! § OUTPUT collapse-bias-tensor for Ω-field stage
//! § DENSITY-BUDGET ≤ 0.4ms (90Hz) per DENSITY_BUDGET §V.4
//! § SPEC   `specs/RENDERING.csl` §I.2 + GAZE-COLLAPSE dispatch (T11-D120)
//! § PRIVACY ¬gaze-direction logged (DIAGNOSTIC_INFRA_PLAN § 3.3.7)
//!
//! § INSTRUMENTATION-NOTE
//! `execute()` body is a synthetic placeholder ; in real impl, this reads
//! saccade-prediction from gaze-tracker and writes collapse-bias. The
//! Timer-wrap measures wall-clock-ms only and never accesses or logs any
//! gaze-direction data — this maintains gaze-privacy invariants.

use crate::m8_integration::{Pass, PassContext, StageId};

/// Stage-2 driver — gaze-driven Ω-collapse bias.
#[derive(Debug, Default)]
pub struct GazeCollapsePass {
    /// Per-frame internal counter (for test-introspection).
    pub frames_executed: u64,
    /// Synthetic collapse-bias accumulator.
    pub last_bias: u32,
}

impl GazeCollapsePass {
    /// Construct a default pass.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Pass for GazeCollapsePass {
    fn stage_id(&self) -> StageId {
        StageId::GazeCollapse
    }

    fn execute(&mut self, ctx: &PassContext) {
        // Synthetic deterministic workload.
        // In real impl : read saccade-prediction (aggregate-only, ¬direction
        // logged), compute collapse-bias-tensor, write to Ω-field input.
        let mut acc: u32 = (ctx.frame_n as u32) ^ 0xCAFE_F00D;
        for i in 0..ctx.workload {
            acc = acc.wrapping_add(i).rotate_left(3);
        }
        self.last_bias = acc;
        self.frames_executed = self.frames_executed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaze_collapse_stage_id_correct() {
        let p = GazeCollapsePass::new();
        assert_eq!(p.stage_id(), StageId::GazeCollapse);
        assert_eq!(p.stage_id().index(), 2);
    }

    #[test]
    fn gaze_collapse_name_snake() {
        let p = GazeCollapsePass::new();
        assert_eq!(p.name(), "gaze_collapse");
    }

    #[test]
    fn gaze_collapse_namespace_canonical() {
        let p = GazeCollapsePass::new();
        assert_eq!(
            p.stage_id().metric_namespace(),
            "pipeline.stage_2_gaze_collapse.frame_time_ms"
        );
    }

    #[test]
    fn gaze_collapse_execute_increments_counter() {
        let mut p = GazeCollapsePass::new();
        let ctx = PassContext::default();
        p.execute(&ctx);
        p.execute(&ctx);
        assert_eq!(p.frames_executed, 2);
    }

    #[test]
    fn gaze_collapse_execute_deterministic() {
        let mut p1 = GazeCollapsePass::new();
        let mut p2 = GazeCollapsePass::new();
        let ctx = PassContext {
            frame_n: 7,
            workload: 50,
            ..Default::default()
        };
        p1.execute(&ctx);
        p2.execute(&ctx);
        assert_eq!(p1.last_bias, p2.last_bias);
    }
}
