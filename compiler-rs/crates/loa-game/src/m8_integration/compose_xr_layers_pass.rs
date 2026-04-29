//! Stage 12 — compose_xr_layers.
//!
//! § ROLE   quad/cyl/cube layer compose per-eye per-frame
//! § OUTPUT final composited framebuffer per-eye, ready for present
//! § DENSITY-BUDGET ≤ 0.6ms (90Hz) per DENSITY_BUDGET §V.12
//! § SPEC   `specs/RENDERING.csl` §I.12 + OpenXR layer-compose
//!          + render.cmd_buf_per_eye_pair = 1 invariant
//!
//! § INSTRUMENTATION-NOTE
//! `execute()` body is a synthetic placeholder ; in real impl, this submits
//! the final layer-compose dispatch (quad/cyl/cube) per eye-pair.

use crate::m8_integration::{Pass, PassContext, StageId};

/// Stage-12 driver — XR layer compose (per-eye-pair).
#[derive(Debug, Default)]
pub struct ComposeXrLayersPass {
    /// Per-frame internal counter.
    pub frames_executed: u64,
    /// Synthetic layer-count submitted (cmd-buf mass).
    pub last_layers_submitted: u32,
}

impl ComposeXrLayersPass {
    /// Construct a default pass.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Pass for ComposeXrLayersPass {
    fn stage_id(&self) -> StageId {
        StageId::ComposeXrLayers
    }

    fn execute(&mut self, ctx: &PassContext) {
        // Synthetic deterministic workload : count layer-types submitted.
        // In real impl : submit XR layer-compose draws per eye-pair.
        // Constraint : render.cmd_buf_per_eye_pair = 1 invariant (frozen-set).
        let layers = ctx.workload.min(8); // cap at 8 layer-types
        self.last_layers_submitted = layers;
        self.frames_executed = self.frames_executed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_xr_layers_stage_id_correct() {
        let p = ComposeXrLayersPass::new();
        assert_eq!(p.stage_id(), StageId::ComposeXrLayers);
        assert_eq!(p.stage_id().index(), 12);
    }

    #[test]
    fn compose_xr_layers_namespace_canonical() {
        let p = ComposeXrLayersPass::new();
        assert_eq!(
            p.stage_id().metric_namespace(),
            "pipeline.stage_12_compose_xr_layers.frame_time_ms"
        );
    }

    #[test]
    fn compose_xr_layers_execute_increments_counter() {
        let mut p = ComposeXrLayersPass::new();
        let ctx = PassContext::default();
        p.execute(&ctx);
        p.execute(&ctx);
        assert_eq!(p.frames_executed, 2);
    }

    #[test]
    fn compose_xr_layers_caps_at_8_layers() {
        let mut p = ComposeXrLayersPass::new();
        let ctx = PassContext {
            workload: 1000,
            ..Default::default()
        };
        p.execute(&ctx);
        assert_eq!(p.last_layers_submitted, 8);
    }

    #[test]
    fn compose_xr_layers_execute_deterministic() {
        let mut p1 = ComposeXrLayersPass::new();
        let mut p2 = ComposeXrLayersPass::new();
        let ctx = PassContext {
            workload: 5,
            ..Default::default()
        };
        p1.execute(&ctx);
        p2.execute(&ctx);
        assert_eq!(p1.last_layers_submitted, p2.last_layers_submitted);
    }
}
