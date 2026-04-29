//! Stage 4 — wave_solver.
//!
//! § ROLE   ψ-evolution (Schrödinger-like) + spectral propagate per-frame
//! § OUTPUT ψ-buffer + per-band spectral coefficients
//! § DENSITY-BUDGET ≤ 2.0ms (90Hz) per DENSITY_BUDGET §V.4
//! § SPEC   `specs/RENDERING.csl` §I.4 + WAVE-SOLVER dispatch (T11-D114)
//!
//! § INSTRUMENTATION-NOTE
//! `execute()` body is a synthetic placeholder ; in real impl, this evolves
//! ψ via split-step Fourier / Crank-Nicolson and computes spectral bands.

use crate::m8_integration::{Pass, PassContext, StageId};

/// Stage-4 driver — wave-solver (ψ-evolution).
#[derive(Debug, Default)]
pub struct WaveSolverPass {
    /// Per-frame internal counter.
    pub frames_executed: u64,
    /// Synthetic spectral coefficient.
    pub last_psi_norm: f64,
}

impl WaveSolverPass {
    /// Construct a default pass.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Pass for WaveSolverPass {
    fn stage_id(&self) -> StageId {
        StageId::WaveSolver
    }

    fn execute(&mut self, ctx: &PassContext) {
        // Synthetic deterministic workload.
        // In real impl : split-step Fourier propagate ; compute |ψ|^2 norm.
        let mut acc: f64 = (ctx.frame_n as f64).sin();
        for i in 0..ctx.workload {
            acc = (acc + f64::from(i) * 0.001).cos();
        }
        self.last_psi_norm = acc;
        self.frames_executed = self.frames_executed.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_solver_stage_id_correct() {
        let p = WaveSolverPass::new();
        assert_eq!(p.stage_id(), StageId::WaveSolver);
        assert_eq!(p.stage_id().index(), 4);
    }

    #[test]
    fn wave_solver_namespace_canonical() {
        let p = WaveSolverPass::new();
        assert_eq!(
            p.stage_id().metric_namespace(),
            "pipeline.stage_4_wave_solver.frame_time_ms"
        );
    }

    #[test]
    fn wave_solver_execute_increments_counter() {
        let mut p = WaveSolverPass::new();
        let ctx = PassContext::default();
        p.execute(&ctx);
        p.execute(&ctx);
        assert_eq!(p.frames_executed, 2);
    }

    #[test]
    fn wave_solver_execute_psi_finite() {
        let mut p = WaveSolverPass::new();
        let ctx = PassContext {
            frame_n: 100,
            workload: 16,
            ..Default::default()
        };
        p.execute(&ctx);
        assert!(p.last_psi_norm.is_finite());
        assert!(p.last_psi_norm.abs() <= 1.0);
    }

    #[test]
    fn wave_solver_execute_deterministic() {
        let mut p1 = WaveSolverPass::new();
        let mut p2 = WaveSolverPass::new();
        let ctx = PassContext {
            frame_n: 999,
            workload: 32,
            ..Default::default()
        };
        p1.execute(&ctx);
        p2.execute(&ctx);
        assert!((p1.last_psi_norm - p2.last_psi_norm).abs() < f64::EPSILON);
    }
}
