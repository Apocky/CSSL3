//! Stage 6 — kan_brdf.
//!
//! § ROLE   KAN spectral-BRDF eval per-fragment per-frame
//! § OUTPUT spectral-radiance per-fragment + per-band coefficients
//! § DENSITY-BUDGET ≤ 1.8ms (90Hz) per DENSITY_BUDGET §V.6
//! § SPEC   `specs/RENDERING.csl` §I.6 + SPECTRAL-KAN-BRDF dispatch (T11-D118)
//!          + 33_F1_F6_LANGUAGE_FEATURES.csl KAN-substrate-runtime
//!
//! § INSTRUMENTATION-NOTE
//! `execute()` body is a synthetic placeholder ; in real impl, this evaluates
//! KAN-extension BRDF over per-band wavelet coefficients.

use crate::m8_integration::{Pass, PassContext, StageId};

/// Stage-6 driver — KAN spectral BRDF.
#[derive(Debug, Default)]
pub struct KanBrdfPass {
    /// Per-frame internal counter.
    pub frames_executed: u64,
    /// Synthetic BRDF eval count.
    pub last_evals: u64,
}

impl KanBrdfPass {
    /// Construct a default pass.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Pass for KanBrdfPass {
    fn stage_id(&self) -> StageId {
        StageId::KanBrdf
    }

    fn execute(&mut self, ctx: &PassContext) {
        // Synthetic deterministic workload : count BRDF-evals.
        // In real impl : KAN-extension BRDF lookup per-band, per-fragment.
        let mut acc: u64 = 0;
        for i in 0..ctx.workload {
            acc = acc
                .wrapping_add(u64::from(i))
                .wrapping_mul((ctx.frame_n & 0x3F).max(1));
        }
        self.last_evals = acc;
        self.frames_executed = self.frames_executed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kan_brdf_stage_id_correct() {
        let p = KanBrdfPass::new();
        assert_eq!(p.stage_id(), StageId::KanBrdf);
        assert_eq!(p.stage_id().index(), 6);
    }

    #[test]
    fn kan_brdf_namespace_canonical() {
        let p = KanBrdfPass::new();
        assert_eq!(
            p.stage_id().metric_namespace(),
            "pipeline.stage_6_kan_brdf.frame_time_ms"
        );
    }

    #[test]
    fn kan_brdf_execute_increments_counter() {
        let mut p = KanBrdfPass::new();
        let ctx = PassContext::default();
        p.execute(&ctx);
        p.execute(&ctx);
        assert_eq!(p.frames_executed, 2);
    }

    #[test]
    fn kan_brdf_execute_deterministic() {
        let mut p1 = KanBrdfPass::new();
        let mut p2 = KanBrdfPass::new();
        let ctx = PassContext {
            frame_n: 88,
            workload: 25,
            ..Default::default()
        };
        p1.execute(&ctx);
        p2.execute(&ctx);
        assert_eq!(p1.last_evals, p2.last_evals);
    }
}
