//! Stage 1 — embodiment.
//!
//! § ROLE   body-tracking + IK retargeting per-frame
//! § OUTPUT bone-pose + companion-rig deltas
//! § DENSITY-BUDGET ≤ 1.5ms (90Hz) per DENSITY_BUDGET §V
//! § SPEC   `specs/RENDERING.csl` §I.1 + ANIM dispatch (T11-D125a)
//!
//! § INSTRUMENTATION-NOTE
//! `execute()` body is a synthetic placeholder ; in real impl, this drives
//! body-tracker + IK solver. Placeholder uses ctx.workload to scale work
//! deterministically so tests can verify Timer-wrap is observe-only.

use crate::m8_integration::{Pass, PassContext, StageId};

/// Stage-1 driver — embodiment / IK retargeting.
#[derive(Debug, Default)]
pub struct EmbodimentPass {
    /// Per-frame internal counter (for test-introspection).
    pub frames_executed: u64,
    /// Last accumulator value (synthetic-work output).
    pub last_acc: u64,
}

impl EmbodimentPass {
    /// Construct a default pass.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Pass for EmbodimentPass {
    fn stage_id(&self) -> StageId {
        StageId::Embodiment
    }

    fn execute(&mut self, ctx: &PassContext) {
        // Synthetic deterministic workload (replay-determinism gate).
        // In real impl : resolve body-tracker frame ; run IK ; write pose-buf.
        let mut acc: u64 = ctx.frame_n & 0xFF;
        for i in 0..ctx.workload {
            acc = acc.wrapping_add(u64::from(i)).wrapping_mul(31);
        }
        self.last_acc = acc;
        self.frames_executed = self.frames_executed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embodiment_stage_id_correct() {
        let p = EmbodimentPass::new();
        assert_eq!(p.stage_id(), StageId::Embodiment);
        assert_eq!(p.stage_id().index(), 1);
    }

    #[test]
    fn embodiment_name_snake() {
        let p = EmbodimentPass::new();
        assert_eq!(p.name(), "embodiment");
    }

    #[test]
    fn embodiment_namespace_canonical() {
        let p = EmbodimentPass::new();
        assert_eq!(
            p.stage_id().metric_namespace(),
            "pipeline.stage_1_embodiment.frame_time_ms"
        );
    }

    #[test]
    fn embodiment_execute_increments_counter() {
        let mut p = EmbodimentPass::new();
        let ctx = PassContext::default();
        p.execute(&ctx);
        assert_eq!(p.frames_executed, 1);
        p.execute(&ctx);
        assert_eq!(p.frames_executed, 2);
    }

    #[test]
    fn embodiment_execute_deterministic_same_input() {
        // Replay-determinism : same input → same `last_acc`.
        let mut p1 = EmbodimentPass::new();
        let mut p2 = EmbodimentPass::new();
        let ctx = PassContext {
            frame_n: 42,
            workload: 100,
            ..Default::default()
        };
        p1.execute(&ctx);
        p2.execute(&ctx);
        assert_eq!(p1.last_acc, p2.last_acc);
    }
}
