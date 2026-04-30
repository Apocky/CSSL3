//! § LoaKanCellModulation — bag of per-cell modulation coefficients.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The runtime payload that downstream evaluators (Stage-6 BRDF, Stage-4
//!   ψ-impedance, creature-pose) consult to apply per-cell modulation. Where
//!   `ParametricActivation` defines the per-edge spline shape, this struct
//!   carries the coefficients that THREAD that activation through the
//!   downstream-evaluator's input vector.
//!
//! § COMPOSITION (with KanMaterial)
//!   When Stage-6 evaluates `KanMaterial::spectral_brdf<N>` at a cell with
//!   an active LoaKanCellModulation :
//!     1. The base BRDF coefficient vector is computed as usual (via the
//!        canonical KanNetwork::eval path).
//!     2. The modulation vector is element-wise-multiplied into the BRDF
//!        coefficient (`out[i] *= modulation.coeffs[i]` for i in 0..N).
//!     3. The Σ-mask consent-bits are RE-checked post-modulation : if the
//!        modulation would produce a sample-disallowed output, the whole
//!        eval halts with [`ModulationError::ConsentMaskViolated`].
//!
//! § DESIGN
//!   The modulation vector has fixed dimension MODULATION_DIM = 16. This
//!   matches the canonical Stage-6 16-band hyperspectral output, the
//!   physics-impedance 4-band-complex (8 floats) variant, and the Stage-7
//!   creature-morphology SDF-params (16 entries). Fewer-than-16-dim
//!   evaluators take the prefix-slice and ignore the tail.
//!
//! § PRIME-DIRECTIVE
//!   - The modulation vector is bounded — `|coeff| ≤ MODULATION_BOUND` per
//!     entry to prevent runaway amplification (which could destabilize the
//!     wave-solver or cause Σ-mask violations via amplitude growth).
//!   - Modulation does NOT bypass Σ-mask consent : a Frozen cell still
//!     refuses modulation-driven mutation.
//!   - Sovereign-handle binding is preserved : the modulation carries the
//!     authoring-Sovereign handle as a witness ; mismatched mutations refuse.

/// § Dimensionality of the modulation vector. Sized to match Stage-6 16-band
///   hyperspectral output + Stage-7 creature-morphology SDF-params.
pub const MODULATION_DIM: usize = 16;

/// § Maximum absolute value per modulation coefficient. Prevents runaway
///   amplification per the wave-solver stability requirement.
pub const MODULATION_BOUND: f32 = 8.0;

/// § Per-cell modulation coefficient bag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoaKanCellModulation {
    /// § Modulation coefficients — element-wise applied to downstream
    ///   evaluator output.
    pub coeffs: [f32; MODULATION_DIM],
    /// § Authoring-Sovereign handle. The Sovereign that minted this
    ///   modulation. Mismatched mutations refuse with
    ///   [`ModulationError::SovereignMismatch`].
    pub sovereign_handle: u16,
    /// § Active flag. False ⇒ modulation is dormant (downstream uses base
    ///   evaluator output unchanged). True ⇒ modulation applies.
    pub active: bool,
    /// § Reserved (must be 0). Reserved-for-extension per spec § INTEGRITY.
    pub reserved: u8,
}

impl LoaKanCellModulation {
    /// § Construct an identity modulation (all coefficients = 1.0, dormant).
    ///   The default for cells without explicit modulation.
    #[must_use]
    pub const fn identity() -> LoaKanCellModulation {
        LoaKanCellModulation {
            coeffs: [1.0; MODULATION_DIM],
            sovereign_handle: 0,
            active: false,
            reserved: 0,
        }
    }

    /// § Construct a uniform-scale modulation : every coefficient = `scale`.
    ///   Used by AdaptiveContentScaler to apply a region-wide LOD-dial.
    ///
    /// # Errors
    /// Returns [`ModulationError::CoefficientOutOfBounds`] if `|scale|` exceeds
    /// [`MODULATION_BOUND`].
    pub fn uniform(scale: f32, sovereign: u16) -> Result<LoaKanCellModulation, ModulationError> {
        if scale.abs() > MODULATION_BOUND {
            return Err(ModulationError::CoefficientOutOfBounds {
                index: 0,
                value: scale,
                bound: MODULATION_BOUND,
            });
        }
        Ok(LoaKanCellModulation {
            coeffs: [scale; MODULATION_DIM],
            sovereign_handle: sovereign,
            active: true,
            reserved: 0,
        })
    }

    /// § Construct from an explicit coefficient slice.
    ///
    /// # Errors
    /// - [`ModulationError::CoefficientOutOfBounds`] when any entry exceeds
    ///   [`MODULATION_BOUND`] in absolute value.
    /// - [`ModulationError::DimMismatch`] when the slice length is not
    ///   [`MODULATION_DIM`].
    pub fn from_slice(
        coeffs: &[f32],
        sovereign: u16,
    ) -> Result<LoaKanCellModulation, ModulationError> {
        if coeffs.len() != MODULATION_DIM {
            return Err(ModulationError::DimMismatch {
                expected: MODULATION_DIM,
                got: coeffs.len(),
            });
        }
        for (i, &v) in coeffs.iter().enumerate() {
            if v.abs() > MODULATION_BOUND {
                return Err(ModulationError::CoefficientOutOfBounds {
                    index: i,
                    value: v,
                    bound: MODULATION_BOUND,
                });
            }
        }
        let mut packed = [0.0_f32; MODULATION_DIM];
        packed.copy_from_slice(coeffs);
        Ok(LoaKanCellModulation {
            coeffs: packed,
            sovereign_handle: sovereign,
            active: true,
            reserved: 0,
        })
    }

    /// § Apply this modulation to a downstream-evaluator output vector
    ///   (element-wise multiply). The output slice may be shorter than
    ///   [`MODULATION_DIM`] — only the prefix is touched.
    pub fn apply_to(&self, output: &mut [f32]) {
        if !self.active {
            return;
        }
        let n = output.len().min(MODULATION_DIM);
        for i in 0..n {
            output[i] *= self.coeffs[i];
        }
    }

    /// § Compose two modulations : element-wise multiplication. Used by
    ///   the Phase-3 COMPOSE hook when multiple LoaKanExtension regions
    ///   overlap on the same cell.
    ///
    /// # Errors
    /// Returns [`ModulationError::SovereignMismatch`] if the two modulations
    /// declare different active sovereigns (mismatched authoring authority).
    pub fn compose(
        &self,
        other: &LoaKanCellModulation,
    ) -> Result<LoaKanCellModulation, ModulationError> {
        if self.active && other.active && self.sovereign_handle != other.sovereign_handle {
            // Both active with different Sovereigns ⇒ the composition is
            // un-authorized. Return mismatch error.
            return Err(ModulationError::SovereignMismatch {
                a: self.sovereign_handle,
                b: other.sovereign_handle,
            });
        }
        let mut out = [0.0_f32; MODULATION_DIM];
        for i in 0..MODULATION_DIM {
            let v = self.coeffs[i] * other.coeffs[i];
            // Re-clamp post-compose to keep the bound invariant.
            out[i] = v.clamp(-MODULATION_BOUND, MODULATION_BOUND);
        }
        let active = self.active || other.active;
        // Effective sovereign : non-zero of the two, preferring `self`.
        let sov = if self.sovereign_handle != 0 {
            self.sovereign_handle
        } else {
            other.sovereign_handle
        };
        Ok(LoaKanCellModulation {
            coeffs: out,
            sovereign_handle: sov,
            active,
            reserved: 0,
        })
    }

    /// § True iff the modulation is the identity (no behavior change).
    #[must_use]
    pub fn is_identity(&self) -> bool {
        if self.active {
            return false;
        }
        for i in 0..MODULATION_DIM {
            if (self.coeffs[i] - 1.0).abs() > 1e-6 {
                return false;
            }
        }
        true
    }
}

impl Default for LoaKanCellModulation {
    fn default() -> Self {
        Self::identity()
    }
}

/// § Failure modes for [`LoaKanCellModulation`] mutations.
#[derive(Debug, thiserror::Error)]
pub enum ModulationError {
    /// § A coefficient exceeded the modulation bound.
    #[error(
        "LK0001 — modulation coefficient out-of-bounds at index {index} : value={value}, bound=±{bound}"
    )]
    CoefficientOutOfBounds {
        index: usize,
        value: f32,
        bound: f32,
    },
    /// § Slice length mismatch.
    #[error("LK0002 — modulation dim mismatch : expected={expected}, got={got}")]
    DimMismatch { expected: usize, got: usize },
    /// § Two modulations with different active Sovereigns cannot compose.
    #[error("LK0003 — Sovereign-handle mismatch on compose : a={a}, b={b}")]
    SovereignMismatch { a: u16, b: u16 },
    /// § The modulation would produce an output that violates the cell's
    ///   Σ-mask consent. Re-checked post-modulation per the modulation
    ///   composition rule.
    #[error("LK0004 — modulation output violates cell Σ-mask consent (consent_bits=0x{consent_bits:08x})")]
    ConsentMaskViolated { consent_bits: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Construction ────────────────────────────────────────────────

    #[test]
    fn identity_is_identity() {
        let m = LoaKanCellModulation::identity();
        assert!(m.is_identity());
        assert!(!m.active);
    }

    #[test]
    fn uniform_construction() {
        let m = LoaKanCellModulation::uniform(2.0, 7).unwrap();
        assert!(m.active);
        assert_eq!(m.sovereign_handle, 7);
        for i in 0..MODULATION_DIM {
            assert_eq!(m.coeffs[i], 2.0);
        }
    }

    #[test]
    fn uniform_out_of_bounds_refused() {
        let err = LoaKanCellModulation::uniform(MODULATION_BOUND + 1.0, 7).unwrap_err();
        assert!(matches!(
            err,
            ModulationError::CoefficientOutOfBounds { .. }
        ));
    }

    #[test]
    fn from_slice_dim_mismatch() {
        let bad = vec![1.0_f32; MODULATION_DIM - 1];
        let err = LoaKanCellModulation::from_slice(&bad, 0).unwrap_err();
        assert!(matches!(err, ModulationError::DimMismatch { .. }));
    }

    #[test]
    fn from_slice_with_bound_violation() {
        let mut v = vec![1.0_f32; MODULATION_DIM];
        v[3] = MODULATION_BOUND + 0.1;
        let err = LoaKanCellModulation::from_slice(&v, 0).unwrap_err();
        assert!(matches!(
            err,
            ModulationError::CoefficientOutOfBounds { index: 3, .. }
        ));
    }

    #[test]
    fn from_slice_succeeds() {
        let v = vec![1.5_f32; MODULATION_DIM];
        let m = LoaKanCellModulation::from_slice(&v, 11).unwrap();
        assert_eq!(m.sovereign_handle, 11);
        assert!(m.active);
        for i in 0..MODULATION_DIM {
            assert_eq!(m.coeffs[i], 1.5);
        }
    }

    // ── Apply ──────────────────────────────────────────────────────

    #[test]
    fn apply_to_dormant_no_op() {
        let m = LoaKanCellModulation::identity();
        let mut out = [3.0_f32; MODULATION_DIM];
        m.apply_to(&mut out);
        for i in 0..MODULATION_DIM {
            assert_eq!(out[i], 3.0);
        }
    }

    #[test]
    fn apply_to_active_scales() {
        let m = LoaKanCellModulation::uniform(2.0, 1).unwrap();
        let mut out = [3.0_f32; MODULATION_DIM];
        m.apply_to(&mut out);
        for i in 0..MODULATION_DIM {
            assert_eq!(out[i], 6.0);
        }
    }

    #[test]
    fn apply_to_short_slice_only_prefix() {
        let m = LoaKanCellModulation::uniform(2.0, 1).unwrap();
        let mut out = [3.0_f32; 4];
        m.apply_to(&mut out);
        for i in 0..4 {
            assert_eq!(out[i], 6.0);
        }
    }

    // ── Compose ────────────────────────────────────────────────────

    #[test]
    fn compose_identities_yields_identity_active() {
        let a = LoaKanCellModulation::identity();
        let b = LoaKanCellModulation::identity();
        let c = a.compose(&b).unwrap();
        // Coeffs are 1.0 × 1.0 = 1.0 ; both dormant ⇒ result is dormant.
        for i in 0..MODULATION_DIM {
            assert_eq!(c.coeffs[i], 1.0);
        }
        assert!(!c.active);
    }

    #[test]
    fn compose_active_with_dormant_active_result() {
        let a = LoaKanCellModulation::uniform(2.0, 5).unwrap();
        let b = LoaKanCellModulation::identity();
        let c = a.compose(&b).unwrap();
        assert!(c.active);
        for i in 0..MODULATION_DIM {
            assert_eq!(c.coeffs[i], 2.0);
        }
        assert_eq!(c.sovereign_handle, 5);
    }

    #[test]
    fn compose_two_active_same_sovereign() {
        let a = LoaKanCellModulation::uniform(2.0, 5).unwrap();
        let b = LoaKanCellModulation::uniform(0.5, 5).unwrap();
        let c = a.compose(&b).unwrap();
        for i in 0..MODULATION_DIM {
            assert_eq!(c.coeffs[i], 1.0);
        }
        assert!(c.active);
    }

    #[test]
    fn compose_active_mismatched_sovereigns_refused() {
        let a = LoaKanCellModulation::uniform(2.0, 5).unwrap();
        let b = LoaKanCellModulation::uniform(2.0, 6).unwrap();
        let err = a.compose(&b).unwrap_err();
        assert!(matches!(err, ModulationError::SovereignMismatch { .. }));
    }

    #[test]
    fn compose_clamps_to_bound() {
        let a = LoaKanCellModulation::uniform(7.0, 5).unwrap();
        let b = LoaKanCellModulation::uniform(7.0, 5).unwrap();
        let c = a.compose(&b).unwrap();
        // 49.0 clamped to MODULATION_BOUND (8.0).
        for i in 0..MODULATION_DIM {
            assert_eq!(c.coeffs[i], MODULATION_BOUND);
        }
    }
}
