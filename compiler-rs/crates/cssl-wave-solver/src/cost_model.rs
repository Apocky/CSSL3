//! § GPU cost-model for the Wave-Unity solver — verifies §IX targets.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § PURPOSE (Wave-Unity §IX)
//!   The §IX hardware-feasibility table claims :
//!     * ~30 GF/frame at 1 M cells × 4 LIGHT-bands + AUDIO @ 60 Hz.
//!     * Quest-3 / RTX-3060-tier feasibility-confirmed.
//!     * 36 % of an M7 GPU at realistic occupancy.
//!
//!   This module computes the FLOP budget directly from the kernel
//!   parameters (cell count, substep count, band class) so the cost
//!   model can be verified at audit time + extended to the 8-band
//!   config without re-deriving by hand.
//!
//! § FLOP BREAKDOWN (per cell per substep)
//!   - LBM stream + collide @ D3Q19 :  ~200 FLOP (audio band).
//!   - IMEX implicit step :              ~20 FLOP (light envelope).
//!   - Cross-band coupling :              ~6 FLOP per active pair.
//!   - Robin BC application :            ~10 FLOP per boundary cell.
//!
//!   Boundaries are typically ≤ 5 % of cells ⇒ BC contribution is ~5 %.
//!
//! § PUBLIC SURFACE
//!   [`estimate_gpu_cost`] returns a [`GpuCostEstimate`] ; the
//!   [`GpuCostEstimate::within_target`] predicate enforces ≤ 30 GF/frame
//!   at the 5-band default config.

use crate::band::{BandClass, DEFAULT_BANDS};

/// § FLOP per cell per substep for an LBM stream + collide @ D3Q19.
pub const FLOP_LBM_PER_CELL_PER_SUBSTEP: u64 = 200;

/// § FLOP per cell per substep for one IMEX implicit step.
pub const FLOP_IMEX_PER_CELL_PER_SUBSTEP: u64 = 20;

/// § FLOP per coupling write (multiply + add).
pub const FLOP_COUPLING_PER_WRITE: u64 = 6;

/// § FLOP per boundary cell update.
pub const FLOP_BC_PER_CELL: u64 = 10;

/// § Default coupling-pair count in the 5-band config (LIGHT↔AUDIO + LIGHT
///   thermal couplings = ~10 pairs).
pub const DEFAULT_COUPLING_PAIRS: u64 = 10;

/// § GF target per Wave-Unity §IX. ≤ 30 GF/frame.
pub const GF_TARGET_PER_FRAME: f64 = 30.0;

/// § Estimate of the GPU cost for one wave-solver frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpuCostEstimate {
    /// § Total LBM FLOPs across all fast-direct bands.
    pub lbm_flops: u64,
    /// § Total IMEX FLOPs across all fast-envelope + slow-envelope bands.
    pub imex_flops: u64,
    /// § Cross-band coupling FLOPs.
    pub coupling_flops: u64,
    /// § Boundary-condition FLOPs.
    pub bc_flops: u64,
    /// § Total FLOPs for the frame.
    pub total_flops: u64,
    /// § Total in GF (10⁹ FLOP).
    pub total_gf: f64,
    /// § Active-region cell count used for the estimate.
    pub cells_active: u64,
    /// § Substep count used for the estimate.
    pub substeps: u32,
    /// § Boundary-cell fraction (0..=1) used in the BC-cost estimate.
    pub boundary_fraction: f32,
}

impl GpuCostEstimate {
    /// § True iff the estimate fits within `gf_budget` GF/frame.
    #[must_use]
    pub fn within_target(&self, gf_budget: f64) -> bool {
        self.total_gf <= gf_budget
    }
}

/// § Compute a GPU cost estimate.
///
/// `cells_active` — the number of cells in the active region.
/// `substeps` — the number of substeps per frame (≤ MAX_SUBSTEPS).
/// `boundary_fraction` — the fraction of cells that touch the SDF
/// boundary (typical: 0.05 = 5 %).
#[must_use]
pub fn estimate_gpu_cost(
    cells_active: u64,
    substeps: u32,
    boundary_fraction: f32,
) -> GpuCostEstimate {
    let n_substeps = u64::from(substeps);
    // Count fast-direct (LBM) vs fast-envelope (IMEX) bands.
    let mut n_lbm_bands = 0_u64;
    let mut n_imex_bands = 0_u64;
    for b in DEFAULT_BANDS.iter() {
        match b.class() {
            BandClass::FastDirect => n_lbm_bands += 1,
            BandClass::FastEnvelope | BandClass::SlowEnvelope => n_imex_bands += 1,
        }
    }
    let lbm_flops = n_lbm_bands * cells_active * FLOP_LBM_PER_CELL_PER_SUBSTEP * n_substeps;
    let imex_flops = n_imex_bands * cells_active * FLOP_IMEX_PER_CELL_PER_SUBSTEP * n_substeps;
    let coupling_flops =
        DEFAULT_COUPLING_PAIRS * cells_active * FLOP_COUPLING_PER_WRITE * n_substeps;
    let bc_cells = (cells_active as f64 * boundary_fraction as f64) as u64;
    let bc_flops = bc_cells * FLOP_BC_PER_CELL * n_substeps * (DEFAULT_BANDS.len() as u64);
    let total_flops = lbm_flops + imex_flops + coupling_flops + bc_flops;
    let total_gf = total_flops as f64 / 1.0e9;
    GpuCostEstimate {
        lbm_flops,
        imex_flops,
        coupling_flops,
        bc_flops,
        total_flops,
        total_gf,
        cells_active,
        substeps,
        boundary_fraction,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_field_zero_cost() {
        let e = estimate_gpu_cost(0, 1, 0.0);
        assert_eq!(e.total_flops, 0);
        assert_eq!(e.total_gf, 0.0);
    }

    #[test]
    fn baseline_cost_under_30_gf() {
        // Spec target : 1 M cells × 4 LIGHT bands + AUDIO at 16 substeps.
        // The default 5-band table has 4 fast-envelope (light) + 1 fast-direct (audio).
        let e = estimate_gpu_cost(1_000_000, 16, 0.05);
        // The 30 GF target is the §IX header — let's verify our model.
        // LBM (1 audio band) : 1 * 1M * 200 * 16 = 3.2 GF.
        // IMEX (4 light bands) : 4 * 1M * 20 * 16 = 1.28 GF.
        // Coupling (10 pairs) : 10 * 1M * 6 * 16 = 0.96 GF.
        // BC : 50000 cells * 10 FLOP * 16 substeps * 5 bands = 0.04 GF.
        // Total ≈ 5.5 GF — well under the 30 GF target.
        assert!(e.total_gf < 30.0);
        assert!(e.total_gf > 0.0);
    }

    #[test]
    fn cost_scales_with_cells() {
        let e1 = estimate_gpu_cost(100_000, 8, 0.05);
        let e2 = estimate_gpu_cost(1_000_000, 8, 0.05);
        assert!(e2.total_flops > e1.total_flops);
        assert!((e2.total_flops as f64) > (e1.total_flops as f64) * 9.5);
    }

    #[test]
    fn cost_scales_with_substeps() {
        let e1 = estimate_gpu_cost(100_000, 4, 0.05);
        let e2 = estimate_gpu_cost(100_000, 16, 0.05);
        assert!(e2.total_flops > e1.total_flops);
    }

    #[test]
    fn within_target_predicate() {
        let e = estimate_gpu_cost(1_000_000, 16, 0.05);
        assert!(e.within_target(GF_TARGET_PER_FRAME));
        assert!(!e.within_target(0.001));
    }

    #[test]
    fn cost_high_substeps_high_cells_at_target() {
        // Stress test : 8 M cells × 16 substeps. Should still be within
        // a generous budget but probably above 30 GF.
        let e = estimate_gpu_cost(8_000_000, 16, 0.10);
        // It will be ~44 GF — that's the high-end stress case.
        // We only assert it's positive + computable here.
        assert!(e.total_gf > 0.0);
    }

    #[test]
    fn cost_constants_match_spec() {
        assert_eq!(FLOP_LBM_PER_CELL_PER_SUBSTEP, 200);
        assert_eq!(FLOP_IMEX_PER_CELL_PER_SUBSTEP, 20);
        assert_eq!(GF_TARGET_PER_FRAME, 30.0);
    }
}
