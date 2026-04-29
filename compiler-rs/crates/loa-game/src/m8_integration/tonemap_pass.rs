//! Stage 10 — tonemap.
//!
//! § ROLE   HDR → display tonemap + fovea-tier compose per-frame
//! § OUTPUT tonemapped LDR-buffer ready for display-encode
//! § DENSITY-BUDGET ≤ 0.5ms (90Hz) per DENSITY_BUDGET §V.10
//! § SPEC   `specs/RENDERING.csl` §I.10 + tonemapping operator
//!
//! § INSTRUMENTATION-NOTE
//! `execute()` body is a synthetic placeholder ; in real impl, this performs
//! ACES / Reinhard / configurable tonemap + fovea-tier color-encode.

use crate::m8_integration::{Pass, PassContext, StageId};

/// Stage-10 driver — HDR tonemap + fovea-tier compose.
#[derive(Debug, Default)]
pub struct TonemapPass {
    /// Per-frame internal counter.
    pub frames_executed: u64,
    /// Synthetic display-luminance accumulator.
    pub last_luminance: f32,
}

impl TonemapPass {
    /// Construct a default pass.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Pass for TonemapPass {
    fn stage_id(&self) -> StageId {
        StageId::Tonemap
    }

    fn execute(&mut self, ctx: &PassContext) {
        // Synthetic deterministic workload : average-pixel-luminance.
        // In real impl : ACES / Reinhard tonemap on HDR-buffer, write LDR.
        let mut acc: f32 = 0.0;
        for i in 0..ctx.workload {
            acc += (i as f32) * 0.001;
        }
        let avg = if ctx.workload > 0 {
            acc / ctx.workload as f32
        } else {
            0.0
        };
        self.last_luminance = avg;
        self.frames_executed = self.frames_executed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tonemap_stage_id_correct() {
        let p = TonemapPass::new();
        assert_eq!(p.stage_id(), StageId::Tonemap);
        assert_eq!(p.stage_id().index(), 10);
    }

    #[test]
    fn tonemap_namespace_canonical() {
        let p = TonemapPass::new();
        assert_eq!(
            p.stage_id().metric_namespace(),
            "pipeline.stage_10_tonemap.frame_time_ms"
        );
    }

    #[test]
    fn tonemap_execute_increments_counter() {
        let mut p = TonemapPass::new();
        let ctx = PassContext::default();
        p.execute(&ctx);
        p.execute(&ctx);
        assert_eq!(p.frames_executed, 2);
    }

    #[test]
    fn tonemap_zero_workload_no_division_panic() {
        let mut p = TonemapPass::new();
        let ctx = PassContext {
            workload: 0,
            ..Default::default()
        };
        p.execute(&ctx);
        assert!((p.last_luminance - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn tonemap_execute_deterministic() {
        let mut p1 = TonemapPass::new();
        let mut p2 = TonemapPass::new();
        let ctx = PassContext {
            workload: 100,
            ..Default::default()
        };
        p1.execute(&ctx);
        p2.execute(&ctx);
        assert!((p1.last_luminance - p2.last_luminance).abs() < f32::EPSILON);
    }
}
