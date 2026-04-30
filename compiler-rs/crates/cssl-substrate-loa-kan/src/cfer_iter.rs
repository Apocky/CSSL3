//! § cfer_iter — CFER iteration driver (per-cell `kan_step`).
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § PROVENANCE
//!   Substantive W-S-CORE-3 code (`KanBand` + `KanBandTable` + canonical
//!   5-rule update-set + `kan_step` driver + `drive_iteration_with_evidence`
//!   + 56 tests across kan_band/update_rules/cfer_iter modules) landed via
//!   commit f7b5be8 (mis-labeled T11-D303 W-S-CORE-4 due to concurrent-
//!   fanout commit-collision : the W-S-CORE-4 adjoint-method commit's
//!   workspace-add picked up these 3 files alongside its own crate). This
//!   attribution-note records the canonical task-tag T11-D302 (W-S-CORE-3)
//!   for telemetry / audit-trail discovery — same pattern as 1ebd24d's
//!   recovery for T11-D305 W-S-CORE-6.
//!
//! § ROLE
//!   The substrate-side per-cell CFER iteration step. Per
//!   specs/36_CFER_RENDERER.csl § ALGORITHM, each iteration computes
//!
//!       L_c^{(k+1)} = KAN_c( L_c^{(k)}, {L_n^{(k)} : n ∈ neighbors(c)},
//!                            material_c )
//!
//!   This module wires the canonical rule-set ([`CanonicalRuleSet`]) to a
//!   per-cell driver `kan_step` and exposes the supporting iteration-loop
//!   helpers (convergence-test, evidence-glyph update, dirty-set discipline).
//!
//! § DRIVER-CONTRACT
//!   - `kan_step(cell, neighbors, material) -> new_state` is PURE +
//!     deterministic — required for the adjoint backward-pass.
//!   - Output rank = input rank — adaptive-rank changes are deferred to a
//!     separate adapt-rank pass to keep `kan_step` simple.
//!   - All produced coefs are clamped within [`COEF_BOUND`].
//!
//! § CONVERGENCE
//!   The CFER iteration loop is driven by the L1 norm of (new - old) :
//!   when ‖ΔL‖ < EPSILON the cell is marked ✓ (trusted) and skipped on
//!   subsequent iterations ; when above the confidence threshold the cell
//!   is marked ◐ (uncertain) for re-iteration.

use crate::kan_band::{KanBand, COEF_BOUND};
use crate::update_rules::{
    CanonicalRuleSet, MaterialContext, Neighbor, UpdateRuleError,
};

/// § Default canonical EPSILON for cell-level convergence (specs/36
///   uses 1e-3 globally ; per-cell uses a tighter local threshold).
pub const KAN_STEP_EPSILON: f32 = 1e-3;

/// § Default canonical CONVERGENCE_THRESHOLD for the iteration-loop
///   sum-of-deltas (specs/36 § ALGORITHM).
pub const KAN_LOOP_THRESHOLD: f32 = 1e-3;

/// § Default canonical MAX_ITER per CFER iteration (specs/36 § ALGORITHM).
pub const KAN_LOOP_MAX_ITER: u32 = 64;

/// § Default canonical confidence threshold for ◐-flagging cells.
pub const KAN_CONFIDENCE_THRESHOLD: f32 = 1e-2;

/// § The evidence-glyph state of a cell post-iteration. Drives adaptive-
///   sampling per specs/36 § Adaptive-sampling-via-evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum EvidenceGlyph {
    /// § ◐ uncertain : prioritize KAN-update-iteration · re-converge.
    #[default]
    Uncertain = 0,
    /// § ✓ trusted : skip · use cached state.
    Trusted = 1,
    /// § ✗ rejected : skip + null-light.
    Rejected = 2,
    /// § ○ default : standard cadence.
    Default = 3,
}

impl EvidenceGlyph {
    /// § Stable canonical name for telemetry + audit.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Uncertain => "uncertain",
            Self::Trusted => "trusted",
            Self::Rejected => "rejected",
            Self::Default => "default",
        }
    }

    /// § Decide the new evidence based on the per-cell delta-L1.
    #[must_use]
    pub fn from_delta(delta_l1: f32, confidence_threshold: f32) -> EvidenceGlyph {
        if delta_l1 < KAN_STEP_EPSILON {
            Self::Trusted
        } else if delta_l1 > confidence_threshold {
            Self::Uncertain
        } else {
            Self::Default
        }
    }

    /// § Should the cell be re-iterated next pass?
    #[must_use]
    pub fn needs_iteration(self) -> bool {
        matches!(self, Self::Uncertain | Self::Default)
    }
}

/// § Cfer-step error : adapter over UpdateRuleError + step-specific gates.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum CferStepError {
    /// § The underlying update-rule failed.
    #[error("cfer_step: rule error : {0}")]
    Rule(#[from] UpdateRuleError),
    /// § The cell's basis-kind is incompatible with at least one neighbor.
    #[error("cfer_step: basis mismatch")]
    BasisMismatch,
    /// § Iteration count exceeded MAX_ITER without converging.
    #[error("cfer_step: did not converge after {iters} iterations (final delta {delta})")]
    DidNotConverge { iters: u32, delta: f32 },
}

/// § Per-cell CFER step : applies the canonical 5-rule set + accumulates
///   the delta into the cell's KanBand.
///
///   Returns the L1 norm of (new - old) for the iteration-loop's
///   convergence test.
///
/// # Errors
///   Propagates rule errors via [`CferStepError::Rule`].
pub fn kan_step(
    cell: &mut KanBand,
    neighbors: &[Neighbor<'_>],
    material: &MaterialContext,
) -> Result<f32, CferStepError> {
    let set = CanonicalRuleSet::new();
    let n = cell.rank();
    let mut delta = vec![0.0_f32; n];
    set.apply_all(cell, neighbors, material, &mut delta)?;
    // Apply the delta into the cell's coefs, recording L1 norm.
    let mut l1 = 0.0_f32;
    for i in 0..n {
        let next = (cell.coefs[i] + delta[i]).clamp(-COEF_BOUND, COEF_BOUND);
        l1 += (next - cell.coefs[i]).abs();
        cell.coefs[i] = next;
    }
    Ok(l1)
}

/// § Run kan_step in a fixed-point iteration loop until convergence or
///   MAX_ITER. Returns the iteration-count + final delta + evidence-glyph.
///
/// # Errors
///   Returns [`CferStepError::DidNotConverge`] if max_iter exhausted before
///   the convergence threshold.
pub fn kan_iterate_to_convergence(
    cell: &mut KanBand,
    neighbors: &[Neighbor<'_>],
    material: &MaterialContext,
    max_iter: u32,
    threshold: f32,
) -> Result<(u32, f32, EvidenceGlyph), CferStepError> {
    let mut last_delta = f32::INFINITY;
    for k in 0..max_iter {
        let d = kan_step(cell, neighbors, material)?;
        last_delta = d;
        if d < threshold {
            return Ok((
                k + 1,
                d,
                EvidenceGlyph::from_delta(d, KAN_CONFIDENCE_THRESHOLD),
            ));
        }
    }
    Err(CferStepError::DidNotConverge {
        iters: max_iter,
        delta: last_delta,
    })
}

/// § Single-iteration convergence-test : returns true iff the L1 norm of
///   (new - old) is below threshold. Useful for sub-loop-driven tests
///   without committing changes.
#[must_use]
pub fn is_converged(delta_l1: f32, threshold: f32) -> bool {
    delta_l1 < threshold
}

/// § Apply a rule-set to many cells in sequence (serial fallback before
///   the GPU-parallel kernel lands). Each cell gets one kan_step ; the
///   sum-L1 across all cells drives the loop-level convergence.
///
/// # Errors
///   Propagates the first cell's update error.
pub fn parallel_step_serial<'a>(
    cells: &mut [&mut KanBand],
    neighbor_set: &[&'a [Neighbor<'a>]],
    materials: &[MaterialContext],
) -> Result<f32, CferStepError> {
    if cells.len() != neighbor_set.len() || cells.len() != materials.len() {
        // No specific error variant ; map to a Rule-rank-mismatch path
        // for caller-clarity.
        return Err(CferStepError::Rule(UpdateRuleError::RankMismatch {
            cell: cells.len(),
            neighbor: neighbor_set.len(),
        }));
    }
    let mut total_l1 = 0.0_f32;
    for i in 0..cells.len() {
        let d = kan_step(cells[i], neighbor_set[i], &materials[i])?;
        total_l1 += d;
    }
    Ok(total_l1)
}

/// § Drive a multi-cell CFER iteration loop with adaptive evidence-glyph
///   tagging per cell. Each iteration runs kan_step over all uncertain
///   cells ; trusted cells are skipped. Returns the per-cell final
///   evidence-glyphs + per-iteration total-deltas.
///
/// # Errors
///   Propagates the first failed kan_step.
pub fn drive_iteration_with_evidence<'a>(
    cells: &mut [&mut KanBand],
    neighbor_set: &[&'a [Neighbor<'a>]],
    materials: &[MaterialContext],
    max_iter: u32,
    threshold: f32,
) -> Result<DriveReport, CferStepError> {
    let n_cells = cells.len();
    if neighbor_set.len() != n_cells || materials.len() != n_cells {
        return Err(CferStepError::Rule(UpdateRuleError::RankMismatch {
            cell: n_cells,
            neighbor: neighbor_set.len(),
        }));
    }
    let mut evidences = vec![EvidenceGlyph::default(); n_cells];
    let mut history: Vec<f32> = Vec::with_capacity(max_iter as usize);
    let mut iters = 0_u32;
    for k in 0..max_iter {
        iters = k + 1;
        let mut total = 0.0_f32;
        for i in 0..n_cells {
            if !evidences[i].needs_iteration() {
                continue;
            }
            let d = kan_step(cells[i], neighbor_set[i], &materials[i])?;
            total += d;
            evidences[i] = EvidenceGlyph::from_delta(d, KAN_CONFIDENCE_THRESHOLD);
        }
        history.push(total);
        if total < threshold {
            break;
        }
    }
    let final_delta = *history.last().unwrap_or(&f32::INFINITY);
    Ok(DriveReport {
        iters,
        final_delta,
        evidences,
        history,
        converged: final_delta < threshold,
    })
}

/// § Output of a multi-cell CFER drive : iteration history + per-cell
///   evidence-glyphs + convergence flag.
#[derive(Debug, Clone)]
pub struct DriveReport {
    /// § Number of iterations actually run.
    pub iters: u32,
    /// § Final per-iteration sum-L1 delta.
    pub final_delta: f32,
    /// § Per-cell evidence-glyphs at termination.
    pub evidences: Vec<EvidenceGlyph>,
    /// § Per-iteration total-delta history (length = iters).
    pub history: Vec<f32>,
    /// § True iff final_delta < threshold.
    pub converged: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kan_band::{BasisKind, KanBand};

    fn band(rank: usize, val: f32) -> KanBand {
        let coefs: Vec<f32> = (0..rank).map(|_| val).collect();
        KanBand::from_slice(&coefs, BasisKind::GaussianMix).unwrap()
    }

    // ── EvidenceGlyph ───────────────────────────────────────────────

    #[test]
    fn evidence_glyph_default_uncertain() {
        assert_eq!(EvidenceGlyph::default(), EvidenceGlyph::Uncertain);
    }

    #[test]
    fn evidence_glyph_canonical_names_unique() {
        let names = [
            EvidenceGlyph::Uncertain.canonical_name(),
            EvidenceGlyph::Trusted.canonical_name(),
            EvidenceGlyph::Rejected.canonical_name(),
            EvidenceGlyph::Default.canonical_name(),
        ];
        let mut s = names.to_vec();
        s.sort_unstable();
        let pre = s.len();
        s.dedup();
        assert_eq!(s.len(), pre);
    }

    #[test]
    fn evidence_glyph_low_delta_yields_trusted() {
        let g = EvidenceGlyph::from_delta(1e-6, KAN_CONFIDENCE_THRESHOLD);
        assert_eq!(g, EvidenceGlyph::Trusted);
    }

    #[test]
    fn evidence_glyph_high_delta_yields_uncertain() {
        let g = EvidenceGlyph::from_delta(0.5, KAN_CONFIDENCE_THRESHOLD);
        assert_eq!(g, EvidenceGlyph::Uncertain);
    }

    #[test]
    fn evidence_glyph_mid_delta_yields_default() {
        // delta in (KAN_STEP_EPSILON, KAN_CONFIDENCE_THRESHOLD).
        let g = EvidenceGlyph::from_delta(5e-3, KAN_CONFIDENCE_THRESHOLD);
        assert_eq!(g, EvidenceGlyph::Default);
    }

    #[test]
    fn evidence_glyph_needs_iteration_per_kind() {
        assert!(EvidenceGlyph::Uncertain.needs_iteration());
        assert!(EvidenceGlyph::Default.needs_iteration());
        assert!(!EvidenceGlyph::Trusted.needs_iteration());
        assert!(!EvidenceGlyph::Rejected.needs_iteration());
    }

    // ── kan_step ───────────────────────────────────────────────────

    #[test]
    fn kan_step_black_body_drives_to_zero() {
        // black-body : full absorption ; cell at 1.0 ; one step pulls coefs
        // to ~0 (delta ≈ -1.0 per coef).
        let mut cell = band(3, 1.0);
        let m = MaterialContext::black_body();
        let neighbors: [Neighbor<'_>; 0] = [];
        let l1 = kan_step(&mut cell, &neighbors, &m).unwrap();
        // L1 drop = 3 (all three coefs went 1.0 → 0.0).
        assert!(l1 > 0.0);
        for &v in cell.coefs.iter() {
            assert!(v.abs() < 0.5, "coef {} too large after absorption", v);
        }
    }

    #[test]
    fn kan_step_emissive_grows_first_coef() {
        // empty cell + emissive material ⇒ first coef grows.
        let mut cell = band(3, 0.0);
        let m = MaterialContext::emissive(0.5);
        let neighbors: [Neighbor<'_>; 0] = [];
        kan_step(&mut cell, &neighbors, &m).unwrap();
        assert!(cell.coefs[0] > 0.0);
    }

    #[test]
    fn kan_step_zero_state_zero_material_zero_delta() {
        // zero cell + black-body but with absorption=0 + emission=0
        // (modified) ⇒ delta = 0.
        let mut cell = band(3, 0.0);
        let mut m = MaterialContext::black_body();
        m.absorption = 0.0;
        let neighbors: [Neighbor<'_>; 0] = [];
        let l1 = kan_step(&mut cell, &neighbors, &m).unwrap();
        assert!(l1 < 1e-6);
    }

    #[test]
    fn kan_step_clamps_to_coef_bound() {
        // Construct a high-emission material to push above bound.
        let mut cell = band(2, COEF_BOUND - 0.5);
        let m = MaterialContext::emissive(100.0); // would push past bound.
        let neighbors: [Neighbor<'_>; 0] = [];
        kan_step(&mut cell, &[], &m).unwrap();
        for &v in cell.coefs.iter() {
            assert!(v.abs() <= COEF_BOUND);
        }
        let _ = neighbors;
    }

    // ── kan_iterate_to_convergence ──────────────────────────────────

    #[test]
    fn kan_iterate_converges_under_absorption() {
        // black-body : fully-absorbing ⇒ cell drives toward zero.
        let mut cell = band(3, 1.0);
        let m = MaterialContext::black_body();
        let r = kan_iterate_to_convergence(&mut cell, &[], &m, 16, 1e-2);
        assert!(r.is_ok());
        let (iters, _, glyph) = r.unwrap();
        assert!(iters >= 1);
        assert_eq!(glyph, EvidenceGlyph::Trusted);
        // Final state close to zero.
        for &v in cell.coefs.iter() {
            assert!(v.abs() < 0.5);
        }
    }

    #[test]
    fn kan_iterate_returns_did_not_converge_on_max_iter_breach() {
        // Construct an oscillating system : alternating diffusion + emission.
        // Use max_iter=1, threshold tiny ⇒ cannot converge.
        let mut cell = band(3, 1.0);
        let m = MaterialContext::lambertian();
        let r = kan_iterate_to_convergence(&mut cell, &[], &m, 1, 1e-9);
        // With one iter and tiny threshold, almost surely not converged.
        // Test asserts the error variant (or success if accidentally converged).
        match r {
            Err(CferStepError::DidNotConverge { iters, .. }) => {
                assert_eq!(iters, 1);
            }
            Ok((_, d, _)) => {
                // If it accidentally converged on first step, ensure the
                // delta really was below threshold.
                assert!(d < 1e-9);
            }
            other => panic!("unexpected result {:?}", other),
        }
    }

    // ── is_converged + parallel_step_serial ─────────────────────────

    #[test]
    fn is_converged_below_threshold() {
        assert!(is_converged(1e-5, 1e-3));
        assert!(!is_converged(1e-1, 1e-3));
    }

    #[test]
    fn parallel_step_serial_runs_all_cells() {
        let mut c1 = band(3, 1.0);
        let mut c2 = band(3, 0.5);
        let mut cells: Vec<&mut KanBand> = vec![&mut c1, &mut c2];
        let empty: [Neighbor<'_>; 0] = [];
        let neighbor_set: Vec<&[Neighbor<'_>]> = vec![&empty, &empty];
        let materials = vec![
            MaterialContext::black_body(),
            MaterialContext::black_body(),
        ];
        let r = parallel_step_serial(&mut cells, &neighbor_set, &materials);
        assert!(r.is_ok());
        let total = r.unwrap();
        assert!(total > 0.0);
    }

    #[test]
    fn parallel_step_serial_length_mismatch_errors() {
        let mut c1 = band(3, 1.0);
        let mut cells: Vec<&mut KanBand> = vec![&mut c1];
        let empty: [Neighbor<'_>; 0] = [];
        let neighbor_set: Vec<&[Neighbor<'_>]> = vec![&empty, &empty];
        let materials = vec![MaterialContext::default()];
        let r = parallel_step_serial(&mut cells, &neighbor_set, &materials);
        assert!(r.is_err());
    }

    // ── drive_iteration_with_evidence ───────────────────────────────

    #[test]
    fn drive_iteration_converges_for_black_body_swarm() {
        let mut c1 = band(3, 1.0);
        let mut c2 = band(3, 0.5);
        let mut c3 = band(3, 0.25);
        let mut cells: Vec<&mut KanBand> = vec![&mut c1, &mut c2, &mut c3];
        let empty: [Neighbor<'_>; 0] = [];
        let neighbor_set: Vec<&[Neighbor<'_>]> = vec![&empty, &empty, &empty];
        let materials = vec![
            MaterialContext::black_body(),
            MaterialContext::black_body(),
            MaterialContext::black_body(),
        ];
        let r =
            drive_iteration_with_evidence(&mut cells, &neighbor_set, &materials, 32, 1e-2);
        assert!(r.is_ok());
        let report = r.unwrap();
        assert!(report.converged, "expected convergence got {:?}", report);
        // Once converged, remaining glyphs are Trusted (or skipped).
        for g in &report.evidences {
            assert!(matches!(*g, EvidenceGlyph::Trusted | EvidenceGlyph::Default));
        }
    }

    #[test]
    fn drive_iteration_history_grows_with_iterations() {
        let mut c1 = band(3, 1.0);
        let mut cells: Vec<&mut KanBand> = vec![&mut c1];
        let empty: [Neighbor<'_>; 0] = [];
        let neighbor_set: Vec<&[Neighbor<'_>]> = vec![&empty];
        let materials = vec![MaterialContext::black_body()];
        let r =
            drive_iteration_with_evidence(&mut cells, &neighbor_set, &materials, 8, 1e-9);
        assert!(r.is_ok());
        let report = r.unwrap();
        assert!(report.history.len() <= 8);
        assert!(!report.history.is_empty());
        // Strictly decreasing delta as the cell trends to zero.
        if report.history.len() >= 2 {
            assert!(report.history[0] >= report.history[1]);
        }
    }

    #[test]
    fn drive_iteration_length_mismatch_errors() {
        let mut c1 = band(3, 1.0);
        let mut cells: Vec<&mut KanBand> = vec![&mut c1];
        let empty: [Neighbor<'_>; 0] = [];
        let neighbor_set: Vec<&[Neighbor<'_>]> = vec![&empty, &empty];
        let materials = vec![MaterialContext::default()];
        let r =
            drive_iteration_with_evidence(&mut cells, &neighbor_set, &materials, 8, 1e-3);
        assert!(r.is_err());
    }

    // ── invariants ─────────────────────────────────────────────────

    #[test]
    fn kan_step_preserves_rank() {
        let mut cell = band(5, 0.7);
        let m = MaterialContext::lambertian();
        let nb = band(5, 0.3);
        let neighbors = [Neighbor { band: &nb, weight: 1.0 }];
        kan_step(&mut cell, &neighbors, &m).unwrap();
        assert_eq!(cell.rank(), 5);
    }

    #[test]
    fn kan_step_with_neighbors_pulls_toward_neighbor() {
        // empty cell + bright neighbor + lambertian (transport_kernel=0.6)
        // → cell coefs increase toward neighbor's value.
        let mut cell = band(3, 0.0);
        let nb = band(3, 1.0);
        let neighbors = [Neighbor { band: &nb, weight: 1.0 }];
        let mut m = MaterialContext::lambertian();
        // Disable absorption + scattering + emission to isolate transport.
        m.absorption = 0.0;
        m.scattering = 0.0;
        m.emission = 0.0;
        m.diffusion = 0.0;
        kan_step(&mut cell, &neighbors, &m).unwrap();
        for &v in cell.coefs.iter() {
            assert!(v > 0.0, "cell coef should be positive after transport");
        }
    }
}
