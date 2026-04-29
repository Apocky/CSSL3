//! Stage 5 — sdf_raymarch.
//!
//! § ROLE   sphere-trace + foveated pixel-march per-frame
//! § OUTPUT depth + hit-position + surface-normal G-buffer
//! § DENSITY-BUDGET ≤ 3.5ms (90Hz) per DENSITY_BUDGET §V.5
//! § SPEC   `specs/RENDERING.csl` §I.5 + SDF-RAYMARCH dispatch (T11-D116)
//!
//! § INSTRUMENTATION-NOTE
//! `execute()` body is a synthetic placeholder ; in real impl, this drives
//! the GPU sphere-trace shader with foveated march-budget per pixel.

use crate::m8_integration::{Pass, PassContext, StageId};

/// Stage-5 driver — SDF raymarch (sphere-trace).
#[derive(Debug, Default)]
pub struct SdfRaymarchPass {
    /// Per-frame internal counter.
    pub frames_executed: u64,
    /// Synthetic march-step accumulator.
    pub last_steps: u64,
}

impl SdfRaymarchPass {
    /// Construct a default pass.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Pass for SdfRaymarchPass {
    fn stage_id(&self) -> StageId {
        StageId::SdfRaymarch
    }

    fn execute(&mut self, ctx: &PassContext) {
        // Synthetic deterministic workload : counts march-steps.
        // In real impl : dispatch sphere-trace shader, write G-buffer.
        let mut acc: u64 = 0;
        for i in 0..ctx.workload {
            acc = acc.wrapping_add(u64::from(i ^ ctx.frame_n as u32));
        }
        self.last_steps = acc;
        self.frames_executed = self.frames_executed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdf_raymarch_stage_id_correct() {
        let p = SdfRaymarchPass::new();
        assert_eq!(p.stage_id(), StageId::SdfRaymarch);
        assert_eq!(p.stage_id().index(), 5);
    }

    #[test]
    fn sdf_raymarch_namespace_canonical() {
        let p = SdfRaymarchPass::new();
        assert_eq!(
            p.stage_id().metric_namespace(),
            "pipeline.stage_5_sdf_raymarch.frame_time_ms"
        );
    }

    #[test]
    fn sdf_raymarch_execute_increments_counter() {
        let mut p = SdfRaymarchPass::new();
        let ctx = PassContext::default();
        p.execute(&ctx);
        assert_eq!(p.frames_executed, 1);
    }

    #[test]
    fn sdf_raymarch_execute_deterministic() {
        let mut p1 = SdfRaymarchPass::new();
        let mut p2 = SdfRaymarchPass::new();
        let ctx = PassContext {
            frame_n: 555,
            workload: 64,
            ..Default::default()
        };
        p1.execute(&ctx);
        p2.execute(&ctx);
        assert_eq!(p1.last_steps, p2.last_steps);
    }
}
