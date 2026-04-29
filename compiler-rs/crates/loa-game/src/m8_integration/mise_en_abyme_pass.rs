//! Stage 9 — mise_en_abyme.
//!
//! § ROLE   recursion-depth-witnessed amplification per-frame
//! § OUTPUT recursion-depth-aware visual amplification
//! § DENSITY-BUDGET ≤ 0.6ms (90Hz) per DENSITY_BUDGET §V.9
//! § SPEC   `specs/RENDERING.csl` §I.9 + MISE-EN-ABYME dispatch (T11-D122)
//!
//! § INSTRUMENTATION-NOTE
//! `execute()` body is a synthetic placeholder ; in real impl, this applies
//! recursion-depth-witnessed amplification (the "drama" of nested-frames)
//! per RENDERING.csl §I.9. Recursion-depth is tracked in `render.recursion_depth_witnessed`
//! Histogram at stage=9.

use crate::m8_integration::{Pass, PassContext, StageId};

/// Stage-9 driver — mise-en-abyme recursion-depth amplification.
#[derive(Debug, Default)]
pub struct MiseEnAbymePass {
    /// Per-frame internal counter.
    pub frames_executed: u64,
    /// Synthetic recursion-depth witnessed (Histogram-feed in real impl).
    pub last_recursion_depth: u8,
}

impl MiseEnAbymePass {
    /// Construct a default pass.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Pass for MiseEnAbymePass {
    fn stage_id(&self) -> StageId {
        StageId::MiseEnAbyme
    }

    fn execute(&mut self, ctx: &PassContext) {
        // Synthetic deterministic workload : compute recursion-depth.
        // In real impl : recursion-depth-aware amplification per spec §I.9.
        let mut depth: u8 = 1;
        let mut load = ctx.workload;
        while load > 4 && depth < 8 {
            load = (load * 3) / 4; // shrink ¬-pow2
            depth = depth.saturating_add(1);
        }
        self.last_recursion_depth = depth;
        self.frames_executed = self.frames_executed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mise_en_abyme_stage_id_correct() {
        let p = MiseEnAbymePass::new();
        assert_eq!(p.stage_id(), StageId::MiseEnAbyme);
        assert_eq!(p.stage_id().index(), 9);
    }

    #[test]
    fn mise_en_abyme_namespace_canonical() {
        let p = MiseEnAbymePass::new();
        assert_eq!(
            p.stage_id().metric_namespace(),
            "pipeline.stage_9_mise_en_abyme.frame_time_ms"
        );
    }

    #[test]
    fn mise_en_abyme_execute_increments_counter() {
        let mut p = MiseEnAbymePass::new();
        let ctx = PassContext::default();
        p.execute(&ctx);
        assert_eq!(p.frames_executed, 1);
    }

    #[test]
    fn mise_en_abyme_recursion_depth_bounded() {
        let mut p = MiseEnAbymePass::new();
        let ctx = PassContext {
            workload: 1_000_000,
            ..Default::default()
        };
        p.execute(&ctx);
        assert!(p.last_recursion_depth <= 8);
        assert!(p.last_recursion_depth >= 1);
    }

    #[test]
    fn mise_en_abyme_execute_deterministic() {
        let mut p1 = MiseEnAbymePass::new();
        let mut p2 = MiseEnAbymePass::new();
        let ctx = PassContext {
            workload: 100,
            ..Default::default()
        };
        p1.execute(&ctx);
        p2.execute(&ctx);
        assert_eq!(p1.last_recursion_depth, p2.last_recursion_depth);
    }
}
