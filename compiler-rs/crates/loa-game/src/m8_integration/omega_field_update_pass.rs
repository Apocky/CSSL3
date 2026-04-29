//! Stage 3 — omega_field_update.
//!
//! § ROLE   Ω-field tier-resolution + Σ-mask propagate per-frame
//! § OUTPUT updated Ω-field cells (T0..T3) with Σ-mask deltas
//! § DENSITY-BUDGET ≤ 2.5ms (90Hz) per DENSITY_BUDGET §V.3
//! § SPEC   `specs/30_SUBSTRATE_v2.csl` Ω-field-as-truth + Σ-mask-per-cell
//!          + RENDERING.csl §I.3
//!
//! § INSTRUMENTATION-NOTE
//! `execute()` body is a synthetic placeholder ; in real impl, this evolves
//! the Ω-field per Wave-Jε substrate-evolution (T11-D144 cssl-omega-field-cell).

use crate::m8_integration::{Pass, PassContext, StageId};

/// Stage-3 driver — Ω-field update + Σ-mask propagate.
#[derive(Debug, Default)]
pub struct OmegaFieldUpdatePass {
    /// Per-frame internal counter.
    pub frames_executed: u64,
    /// Synthetic field-state accumulator.
    pub last_state: u64,
}

impl OmegaFieldUpdatePass {
    /// Construct a default pass.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Pass for OmegaFieldUpdatePass {
    fn stage_id(&self) -> StageId {
        StageId::OmegaFieldUpdate
    }

    fn execute(&mut self, ctx: &PassContext) {
        // Synthetic deterministic workload.
        // In real impl : iterate Ω-field cells, propagate Σ-mask deltas,
        // resolve tier-promotions/demotions per substrate-v2 spec.
        let mut acc: u64 = ctx.frame_n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        for i in 0..ctx.workload {
            acc = acc.wrapping_add(u64::from(i)).rotate_right(7);
        }
        self.last_state = acc;
        self.frames_executed = self.frames_executed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omega_field_update_stage_id_correct() {
        let p = OmegaFieldUpdatePass::new();
        assert_eq!(p.stage_id(), StageId::OmegaFieldUpdate);
        assert_eq!(p.stage_id().index(), 3);
    }

    #[test]
    fn omega_field_update_namespace_canonical() {
        let p = OmegaFieldUpdatePass::new();
        assert_eq!(
            p.stage_id().metric_namespace(),
            "pipeline.stage_3_omega_field_update.frame_time_ms"
        );
    }

    #[test]
    fn omega_field_update_execute_increments_counter() {
        let mut p = OmegaFieldUpdatePass::new();
        let ctx = PassContext::default();
        p.execute(&ctx);
        p.execute(&ctx);
        p.execute(&ctx);
        assert_eq!(p.frames_executed, 3);
    }

    #[test]
    fn omega_field_update_execute_deterministic() {
        let mut p1 = OmegaFieldUpdatePass::new();
        let mut p2 = OmegaFieldUpdatePass::new();
        let ctx = PassContext {
            frame_n: 1234,
            workload: 200,
            ..Default::default()
        };
        p1.execute(&ctx);
        p2.execute(&ctx);
        assert_eq!(p1.last_state, p2.last_state);
    }
}
