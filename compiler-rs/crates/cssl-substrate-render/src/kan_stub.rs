//! § kan_stub — stub KAN-update-rule ABI for W-S-CORE-3 (cssl-substrate-loa-kan).
//!
//! ## Purpose
//! Documents the per-cell KAN-update-rule contract the CFER iterator depends
//! on, BEFORE the real cell-aware KAN-update-rule from `cssl-substrate-loa-kan`
//! lands. The stub is a deterministic linear-blend (energy-conserving) update
//! that's good enough for the convergence-tests in this slice.
//!
//! ## Migration
//! When the real W-S-CORE-3 update-rule lands :
//!   1. Cargo.toml : add `cssl-substrate-loa-kan = { path = ... }` ;
//!      drop the local `kan_stub` re-exports.
//!   2. Replace `crate::kan_stub::CellKan` with the real per-cell KAN trait.
//!   3. The integration tests in `tests/cfer_*.rs` are written against the
//!      [`CellKan::update`] trait surface — same trait, real impl.
//!
//! ## Math
//! Per spec § 36 § Discretization :
//!   `L_c^{(k+1)} = KAN_c( L_c^{(k)}, {L_n^{(k)} : n ∈ neighbors(c)}, material_c )`
//!
//! The stub implements a Cone-tracing-style linear blend :
//!   `L_new = α · emission + β · reflectivity · Σ_n (w_n · L_n) + γ · L_self`
//! with `α + β + γ = 1` to enforce energy-conservation.

use crate::light_stub::{LightState, LIGHT_STATE_COEFS};
use thiserror::Error;

/// Error class for KAN-update failures.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum KanUpdateError {
    /// Material parameters violate the energy-conservation gate.
    #[error("material energy-conservation violated: α+β+γ = {0} > 1.0 + tol")]
    EnergyConservation(f32),
    /// Neighbor count exceeds the per-cell update window (max 26 for 3³-1).
    #[error("neighbor-count {0} exceeds max 26 (3x3x3 stencil)")]
    NeighborOverflow(usize),
}

/// Material-bag for a cell : `(emission, reflectivity, self-term)`.
///
/// Per spec § 36 § Field-evolution PDE the per-cell material drives :
///   - `S_c` (source term) ↦ [`MaterialBag::emission`] (light emitted by cell).
///   - reflectivity ↦ [`MaterialBag::reflectivity`] (fraction of neighbor-light kept).
///   - self-term ↦ [`MaterialBag::self_retention`] (fraction of own-state kept).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialBag {
    /// Per-coef emission (zero for non-emissive cells).
    pub emission: [f32; LIGHT_STATE_COEFS],
    /// Reflectivity in [0,1] : how much neighbor-light gets in.
    pub reflectivity: f32,
    /// Self-retention in [0,1] : how much own-state survives the step.
    pub self_retention: f32,
}

impl MaterialBag {
    /// Vacuum (no emission, full transmittance).
    pub const VACUUM: Self = Self {
        emission: [0.0; LIGHT_STATE_COEFS],
        reflectivity: 1.0,
        self_retention: 0.0,
    };

    /// Black-body absorber (no emission, no reflection, no self).
    pub const ABSORBER: Self = Self {
        emission: [0.0; LIGHT_STATE_COEFS],
        reflectivity: 0.0,
        self_retention: 0.0,
    };

    /// Constant white-emitter (uniform unit-emission).
    pub const fn emitter(intensity: f32) -> Self {
        Self {
            emission: [intensity; LIGHT_STATE_COEFS],
            reflectivity: 0.0,
            self_retention: 0.0,
        }
    }

    /// Diffuse mid-grey (50% reflectivity, 50% retention).
    pub const DIFFUSE_MID: Self = Self {
        emission: [0.0; LIGHT_STATE_COEFS],
        reflectivity: 0.5,
        self_retention: 0.5,
    };

    /// Verify energy-conservation : α + β + γ ≤ 1 + tol.
    /// (α implicitly = 1 here ; emission is additive ; β = reflectivity ;
    /// γ = self_retention. Pure-physical materials satisfy β + γ ≤ 1.)
    pub fn check_energy(self) -> Result<(), KanUpdateError> {
        let total = self.reflectivity + self.self_retention;
        if total > 1.0 + 1e-3 {
            Err(KanUpdateError::EnergyConservation(total))
        } else {
            Ok(())
        }
    }
}

impl Default for MaterialBag {
    fn default() -> Self {
        Self::VACUUM
    }
}

/// Trait : per-cell KAN-update-rule.
///
/// Per spec § 36 § Discretization the trait is the canonical update :
///   `KAN_c( L^{(k)}, {L_n^{(k)}}, material_c ) → L^{(k+1)}`.
///
/// The forward iterator in `cfer.rs` invokes [`CellKan::update`] in a parallel
/// loop ; the real W-S-CORE-3 impl will replace this trait with the
/// learned-spline-edge KAN per cell.
pub trait CellKan {
    /// Compute the next-step light-state given the current state, the
    /// neighbor-states, and the material parameters.
    fn update(
        &self,
        current: LightState,
        neighbors: &[LightState],
        material: MaterialBag,
    ) -> Result<LightState, KanUpdateError>;
}

/// Default linear-blend KAN-update-rule (stub).
///
/// `L_new[i] = self_retention · L_self[i] + reflectivity · mean(L_n[i]) + emission[i]`
#[derive(Debug, Clone, Copy, Default)]
pub struct LinearBlendKan;

impl CellKan for LinearBlendKan {
    fn update(
        &self,
        current: LightState,
        neighbors: &[LightState],
        material: MaterialBag,
    ) -> Result<LightState, KanUpdateError> {
        material.check_energy()?;
        if neighbors.len() > 26 {
            return Err(KanUpdateError::NeighborOverflow(neighbors.len()));
        }

        let n = neighbors.len().max(1) as f32;
        let mut out = [0.0_f32; LIGHT_STATE_COEFS];

        // self-retention term
        for i in 0..LIGHT_STATE_COEFS {
            out[i] = material.self_retention * current.coefs[i];
        }

        // neighbor-blend term (uniform weight over neighbors)
        if !neighbors.is_empty() {
            let inv_n = material.reflectivity / n;
            for nb in neighbors {
                for i in 0..LIGHT_STATE_COEFS {
                    out[i] += inv_n * nb.coefs[i];
                }
            }
        }

        // emission term
        for i in 0..LIGHT_STATE_COEFS {
            out[i] += material.emission[i];
        }

        Ok(LightState {
            coefs: out,
            converged: false,
        })
    }
}

/// Free-fn entry-point : default linear-blend update.
///
/// Equivalent to `LinearBlendKan.update(...)`. Convenient for the cfer.rs
/// driver which doesn't always carry a per-cell impl handle.
pub fn kan_update(
    current: LightState,
    neighbors: &[LightState],
    material: MaterialBag,
) -> Result<LightState, KanUpdateError> {
    LinearBlendKan.update(current, neighbors, material)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vacuum_no_emission_no_reflect_holds_self_at_zero() {
        let s = LightState::zero();
        let n = vec![];
        let out = kan_update(s, &n, MaterialBag::ABSORBER).unwrap();
        assert_eq!(out.norm_diff_l1(s), 0.0);
    }

    #[test]
    fn emitter_produces_emission_in_one_step() {
        let s = LightState::zero();
        let n = vec![];
        let m = MaterialBag::emitter(1.0);
        let out = kan_update(s, &n, m).unwrap();
        for i in 0..LIGHT_STATE_COEFS {
            assert_eq!(out.coefs[i], 1.0);
        }
    }

    #[test]
    fn reflectivity_blends_neighbors_uniformly() {
        let s = LightState::zero();
        let n1 = LightState::from_coefs([2.0; LIGHT_STATE_COEFS]);
        let n2 = LightState::from_coefs([4.0; LIGHT_STATE_COEFS]);
        let m = MaterialBag {
            emission: [0.0; LIGHT_STATE_COEFS],
            reflectivity: 1.0,
            self_retention: 0.0,
        };
        let out = kan_update(s, &[n1, n2], m).unwrap();
        for i in 0..LIGHT_STATE_COEFS {
            assert_eq!(out.coefs[i], 3.0); // mean(2,4) = 3
        }
    }

    #[test]
    fn energy_conservation_rejects_invalid_material() {
        let bad = MaterialBag {
            emission: [0.0; LIGHT_STATE_COEFS],
            reflectivity: 0.8,
            self_retention: 0.8,
        };
        let err = bad.check_energy();
        assert!(err.is_err());
    }

    #[test]
    fn neighbor_overflow_errors() {
        let s = LightState::zero();
        let nbrs = vec![LightState::zero(); 27];
        let out = kan_update(s, &nbrs, MaterialBag::VACUUM);
        assert!(matches!(out, Err(KanUpdateError::NeighborOverflow(27))));
    }

    #[test]
    fn linear_blend_kan_equals_free_fn() {
        let s = LightState::from_coefs([1.0; LIGHT_STATE_COEFS]);
        let n = vec![LightState::from_coefs([2.0; LIGHT_STATE_COEFS])];
        let m = MaterialBag::DIFFUSE_MID;
        let a = LinearBlendKan.update(s, &n, m).unwrap();
        let b = kan_update(s, &n, m).unwrap();
        assert_eq!(a.norm_diff_l1(b), 0.0);
    }
}
