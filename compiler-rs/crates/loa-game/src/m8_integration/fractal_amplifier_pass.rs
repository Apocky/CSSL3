//! Stage 7 — fractal_amplifier.
//!
//! § ROLE   RC-fractal detail injection per-frame
//! § OUTPUT amplified detail-tensor injected into G-buffer
//! § DENSITY-BUDGET ≤ 1.0ms (90Hz) per DENSITY_BUDGET §V.7
//! § SPEC   `specs/RENDERING.csl` §I.7 + FRACTAL-AMPLIFIER dispatch (T11-D119)
//!
//! § INSTRUMENTATION-NOTE
//! `execute()` body is a synthetic placeholder ; in real impl, this performs
//! RC-fractal scale-coherent detail amplification on the G-buffer.

use crate::m8_integration::{Pass, PassContext, StageId};

/// Stage-7 driver — RC-fractal detail amplifier.
#[derive(Debug, Default)]
pub struct FractalAmplifierPass {
    /// Per-frame internal counter.
    pub frames_executed: u64,
    /// Synthetic recursion-depth witnessed.
    pub last_depth: u8,
}

impl FractalAmplifierPass {
    /// Construct a default pass.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Pass for FractalAmplifierPass {
    fn stage_id(&self) -> StageId {
        StageId::FractalAmplifier
    }

    fn execute(&mut self, ctx: &PassContext) {
        // Synthetic deterministic workload : count fractal-iter depth.
        // In real impl : RC-fractal multi-scale detail injection.
        let mut depth: u8 = 0;
        let mut acc: u32 = ctx.workload;
        while acc > 1 && depth < 12 {
            acc /= 2;
            depth = depth.saturating_add(1);
        }
        self.last_depth = depth;
        self.frames_executed = self.frames_executed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractal_amplifier_stage_id_correct() {
        let p = FractalAmplifierPass::new();
        assert_eq!(p.stage_id(), StageId::FractalAmplifier);
        assert_eq!(p.stage_id().index(), 7);
    }

    #[test]
    fn fractal_amplifier_namespace_canonical() {
        let p = FractalAmplifierPass::new();
        assert_eq!(
            p.stage_id().metric_namespace(),
            "pipeline.stage_7_fractal_amplifier.frame_time_ms"
        );
    }

    #[test]
    fn fractal_amplifier_execute_increments_counter() {
        let mut p = FractalAmplifierPass::new();
        let ctx = PassContext::default();
        p.execute(&ctx);
        assert_eq!(p.frames_executed, 1);
    }

    #[test]
    fn fractal_amplifier_depth_log_2_workload() {
        let mut p = FractalAmplifierPass::new();
        let ctx = PassContext {
            workload: 64, // log2(64) = 6
            ..Default::default()
        };
        p.execute(&ctx);
        assert_eq!(p.last_depth, 6);
    }

    #[test]
    fn fractal_amplifier_execute_deterministic() {
        let mut p1 = FractalAmplifierPass::new();
        let mut p2 = FractalAmplifierPass::new();
        let ctx = PassContext {
            workload: 256,
            ..Default::default()
        };
        p1.execute(&ctx);
        p2.execute(&ctx);
        assert_eq!(p1.last_depth, p2.last_depth);
    }
}
