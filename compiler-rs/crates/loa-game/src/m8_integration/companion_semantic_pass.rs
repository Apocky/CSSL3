//! Stage 8 — companion_semantic.
//!
//! § ROLE   companion-perspective layer per-character per-frame
//! § OUTPUT per-companion perspective-aware visual deltas
//! § DENSITY-BUDGET ≤ 0.7ms (90Hz) per DENSITY_BUDGET §V.8
//! § SPEC   `specs/RENDERING.csl` §I.8 + COMPANION-PERSPECTIVE
//!          dispatch (T11-D121)
//!
//! § INSTRUMENTATION-NOTE
//! `execute()` body is a synthetic placeholder ; in real impl, this applies
//! companion-perspective transforms (per-character semantic relationship
//! to scene state) for narrative-density rendering.

use crate::m8_integration::{Pass, PassContext, StageId};

/// Stage-8 driver — companion-perspective semantic layer.
#[derive(Debug, Default)]
pub struct CompanionSemanticPass {
    /// Per-frame internal counter.
    pub frames_executed: u64,
    /// Synthetic per-companion delta accumulator.
    pub last_delta: i64,
}

impl CompanionSemanticPass {
    /// Construct a default pass.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Pass for CompanionSemanticPass {
    fn stage_id(&self) -> StageId {
        StageId::CompanionSemantic
    }

    fn execute(&mut self, ctx: &PassContext) {
        // Synthetic deterministic workload.
        // In real impl : iterate companions, compute perspective-deltas.
        let mut acc: i64 = i64::from(ctx.frame_n as i32);
        for i in 0..ctx.workload {
            acc = acc.wrapping_add(i64::from(i as i32) - 50);
        }
        self.last_delta = acc;
        self.frames_executed = self.frames_executed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn companion_semantic_stage_id_correct() {
        let p = CompanionSemanticPass::new();
        assert_eq!(p.stage_id(), StageId::CompanionSemantic);
        assert_eq!(p.stage_id().index(), 8);
    }

    #[test]
    fn companion_semantic_namespace_canonical() {
        let p = CompanionSemanticPass::new();
        assert_eq!(
            p.stage_id().metric_namespace(),
            "pipeline.stage_8_companion_semantic.frame_time_ms"
        );
    }

    #[test]
    fn companion_semantic_execute_increments_counter() {
        let mut p = CompanionSemanticPass::new();
        let ctx = PassContext::default();
        p.execute(&ctx);
        p.execute(&ctx);
        p.execute(&ctx);
        assert_eq!(p.frames_executed, 3);
    }

    #[test]
    fn companion_semantic_execute_deterministic() {
        let mut p1 = CompanionSemanticPass::new();
        let mut p2 = CompanionSemanticPass::new();
        let ctx = PassContext {
            frame_n: 17,
            workload: 100,
            ..Default::default()
        };
        p1.execute(&ctx);
        p2.execute(&ctx);
        assert_eq!(p1.last_delta, p2.last_delta);
    }
}
