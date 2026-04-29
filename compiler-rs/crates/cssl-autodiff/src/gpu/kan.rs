//! KAN-runtime GPU forward-pass adapter.
//!
//! § SPEC : `Omniverse/07_AESTHETIC/07_KAN_RUNTIME_SHADING § II + § III`
//!         (canonical KAN-shape variants + dispatch tiers).
//!
//! § PURPOSE
//!   Provide the integration-point that the KAN-runtime crate (T11-D115)
//!   uses to evaluate a differentiable forward-pass on the GPU. The forward
//!   pass returns a [`GpuJet`] so the KAN-output gradient can flow back
//!   through the spectral-BRDF / BRDF-params input via the standard
//!   reverse-pass tape.
//!
//! § INTEGRATION
//!   The KAN-runtime crate calls `KanGpuForward::eval` once per fragment :
//!     - input : 32-D MaterialCoord embedding (from
//!       `06_PROCEDURAL/01_MATERIALS_FROM_PATTERN`)
//!     - output : Jet<f32, 2> over the chosen variant's output dim
//!     - tape : optional ; when present, the forward pass records each
//!       per-spline op so the reverse-pass can compute ∂loss/∂(spline-coeffs).
//!
//! § DESIGN-NOTE
//!   The actual spline math is the KAN-runtime crate's responsibility ; this
//!   adapter only owns the tape recording + Jet construction. The
//!   `KanShape::canonical` constructor handles the variant table from
//!   spec § II.

use crate::gpu::jet_gpu::{GpuJet, GpuJetError};
use crate::gpu::tape::{GpuTape, GpuTapeError, OpRecordKind, RecordedOperand};
use crate::Jet;
use crate::JetField;

/// KAN-network variant per spec § II table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KanVariant {
    /// 32 → 16 (hyperspectral BRDF λ).
    SpectralBrdf,
    /// 32 → 4 (R / ρ / F0 / anisotropy).
    BrdfParams,
    /// 7 → 1 (sub-pixel fractal micro-displacement).
    MicroDisplacement,
    /// 32 → 1 (density / conductivity / etc.).
    MaterialProperty,
    /// 33 → 16 (thin-film angle × λ iridescence stack).
    IridescenceStack,
    /// 17 → 16 (absorption → emission λ fluorescence).
    Fluorescence,
}

impl KanVariant {
    /// All catalogued variants (test surface).
    pub const ALL: [Self; 6] = [
        Self::SpectralBrdf,
        Self::BrdfParams,
        Self::MicroDisplacement,
        Self::MaterialProperty,
        Self::IridescenceStack,
        Self::Fluorescence,
    ];

    /// Stable text name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SpectralBrdf => "spectral-brdf",
            Self::BrdfParams => "brdf-params",
            Self::MicroDisplacement => "micro-displacement",
            Self::MaterialProperty => "material-property",
            Self::IridescenceStack => "iridescence-stack",
            Self::Fluorescence => "fluorescence",
        }
    }

    /// Canonical shape per spec § II.
    #[must_use]
    pub const fn canonical(self) -> KanShape {
        match self {
            Self::SpectralBrdf => KanShape {
                input_dim: 32,
                output_dim: 16,
                hidden_layers: 2,
                splines_per_layer: 16,
                knot_count: 10,
            },
            Self::BrdfParams => KanShape {
                input_dim: 32,
                output_dim: 4,
                hidden_layers: 2,
                splines_per_layer: 16,
                knot_count: 10,
            },
            Self::MicroDisplacement => KanShape {
                input_dim: 7,
                output_dim: 1,
                hidden_layers: 3,
                splines_per_layer: 32,
                knot_count: 12,
            },
            Self::MaterialProperty => KanShape {
                input_dim: 32,
                output_dim: 1,
                hidden_layers: 2,
                splines_per_layer: 16,
                knot_count: 10,
            },
            Self::IridescenceStack => KanShape {
                input_dim: 33,
                output_dim: 16,
                hidden_layers: 3,
                splines_per_layer: 32,
                knot_count: 12,
            },
            Self::Fluorescence => KanShape {
                input_dim: 17,
                output_dim: 16,
                hidden_layers: 2,
                splines_per_layer: 16,
                knot_count: 10,
            },
        }
    }
}

/// KAN-network shape (per spec § II runtime canonical).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KanShape {
    pub input_dim: u16,
    pub output_dim: u16,
    pub hidden_layers: u8,
    pub splines_per_layer: u16,
    pub knot_count: u8,
}

impl KanShape {
    /// Total per-network coefficient count (rough estimate :
    /// `hidden_layers * splines_per_layer * input_dim * knot_count`).
    #[must_use]
    pub const fn coefficient_count(self) -> u32 {
        (self.hidden_layers as u32)
            * (self.splines_per_layer as u32)
            * (self.input_dim as u32)
            * (self.knot_count as u32)
    }
}

/// Per-layer kind discriminator. The forward pass routes to spline-eval for
/// hidden layers and edge-mixer for the final layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KanLayerKind {
    /// B-spline activation layer.
    Spline,
    /// Linear edge-mixer (final output projection).
    EdgeMix,
}

/// KAN forward-pass adapter. Threads the Jet through each layer + records
/// each evaluation op on the tape so the reverse-pass can compute gradients.
#[derive(Debug, Clone)]
pub struct KanGpuForward {
    pub variant: KanVariant,
    pub shape: KanShape,
}

impl KanGpuForward {
    /// Construct a forward-pass adapter for the given variant.
    #[must_use]
    pub const fn new(variant: KanVariant) -> Self {
        Self {
            variant,
            shape: variant.canonical(),
        }
    }

    /// Evaluate the network on a single input scalar (degenerate but useful
    /// for unit testing the tape recording). Real KAN-runtime evaluates a
    /// 32-D vector via the matrix-engine path.
    ///
    /// The forward pass is a stand-in : it computes
    /// `output = (a * a + sin(a)) * weight`, recording each op on the tape.
    /// This shape exercises FAdd, FMul, Sin, and a final FMul against a
    /// frozen weight (the spline-coefficient placeholder).
    ///
    /// Returns the forward-pass Jet packed for the GPU register-file.
    pub fn eval<T: JetField>(
        &self,
        input: GpuJet<T, 2>,
        weight: T,
        tape: &mut GpuTape,
    ) -> Result<GpuJet<T, 2>, KanGpuError> {
        // 1. record `a_in` as a tape input slot.
        let a_val = input.primal().to_f64();
        let a_slot = tape.record(
            OpRecordKind::Load,
            vec![RecordedOperand::input(a_val)],
            a_val,
        )?;

        // 2. record `square = a * a` (FMul).
        let square_val = a_val * a_val;
        let square_slot = tape.record(
            OpRecordKind::FMul,
            vec![
                RecordedOperand::from_slot(a_slot, a_val),
                RecordedOperand::from_slot(a_slot, a_val),
            ],
            square_val,
        )?;

        // 3. record `sa = sin(a)` (Sin).
        let sa_val = a_val.sin();
        let sa_slot = tape.record(
            OpRecordKind::Sin,
            vec![RecordedOperand::from_slot(a_slot, a_val)],
            sa_val,
        )?;

        // 4. record `sum = square + sa` (FAdd).
        let sum_val = square_val + sa_val;
        let sum_slot = tape.record(
            OpRecordKind::FAdd,
            vec![
                RecordedOperand::from_slot(square_slot, square_val),
                RecordedOperand::from_slot(sa_slot, sa_val),
            ],
            sum_val,
        )?;

        // 5. record `output = sum * weight` (FMul).
        let w_val = weight.to_f64();
        let out_val = sum_val * w_val;
        let _out_slot = tape.record(
            OpRecordKind::FMul,
            vec![
                RecordedOperand::from_slot(sum_slot, sum_val),
                RecordedOperand::input(w_val), // weight is frozen ; no upstream grad accumulated
            ],
            out_val,
        )?;

        // 6. compute the Jet output via direct algebra (mirrors the recorded
        //    ops on the Jet algebra side ; lets the test check primal +
        //    derivative match the analytic forward).
        let a_jet = *input.inner();
        let square_jet = a_jet * a_jet;
        let sin_jet = a_jet.sin();
        let sum_jet = square_jet + sin_jet;
        let w_jet: Jet<T, 2> = Jet::lift(weight);
        let out_jet = sum_jet * w_jet;

        Ok(GpuJet::pack(out_jet)?)
    }
}

/// Errors the KAN forward-pass adapter can surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KanGpuError {
    /// Tape recording failed.
    TapeError(GpuTapeError),
    /// GPU-jet packing failed.
    JetError(GpuJetError),
}

impl core::fmt::Display for KanGpuError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TapeError(e) => write!(f, "KAN tape error : {e}"),
            Self::JetError(e) => write!(f, "KAN jet error : {e}"),
        }
    }
}

impl std::error::Error for KanGpuError {}

impl From<GpuTapeError> for KanGpuError {
    fn from(e: GpuTapeError) -> Self {
        Self::TapeError(e)
    }
}

impl From<GpuJetError> for KanGpuError {
    fn from(e: GpuJetError) -> Self {
        Self::JetError(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::storage::TapeStorageMode;

    #[test]
    fn variant_names_unique() {
        use std::collections::HashSet;
        let names: HashSet<_> = KanVariant::ALL.iter().map(|v| v.name()).collect();
        assert_eq!(names.len(), KanVariant::ALL.len());
    }

    #[test]
    fn spectral_brdf_shape_is_32_to_16() {
        let s = KanVariant::SpectralBrdf.canonical();
        assert_eq!(s.input_dim, 32);
        assert_eq!(s.output_dim, 16);
    }

    #[test]
    fn brdf_params_shape_is_32_to_4() {
        let s = KanVariant::BrdfParams.canonical();
        assert_eq!(s.input_dim, 32);
        assert_eq!(s.output_dim, 4);
    }

    #[test]
    fn coefficient_count_nonzero() {
        for v in KanVariant::ALL {
            assert!(v.canonical().coefficient_count() > 0, "{}", v.name());
        }
    }

    #[test]
    fn forward_pass_records_five_ops() {
        let kan = KanGpuForward::new(KanVariant::BrdfParams);
        let mut tape = GpuTape::new(TapeStorageMode::WorkgroupShared);
        let input: GpuJet<f32, 2> = GpuJet::pack(Jet::promote(0.5)).unwrap();
        let _output = kan.eval(input, 2.0_f32, &mut tape).unwrap();
        // Load + FMul + Sin + FAdd + FMul = 5 records.
        assert_eq!(tape.len(), 5);
    }

    #[test]
    fn forward_pass_primal_matches_analytic() {
        let kan = KanGpuForward::new(KanVariant::BrdfParams);
        let mut tape = GpuTape::new(TapeStorageMode::WorkgroupShared);
        let a = 0.4_f32;
        let w = 1.5_f32;
        let input: GpuJet<f32, 2> = GpuJet::pack(Jet::promote(a)).unwrap();
        let output = kan.eval(input, w, &mut tape).unwrap();
        let expected = (a * a + a.sin()) * w;
        assert!((output.primal() - expected).abs() < 1e-5);
    }

    #[test]
    fn forward_pass_first_derivative_matches_analytic() {
        // y = (a² + sin(a)) * w  ⇒  dy/da = (2a + cos(a)) * w
        let kan = KanGpuForward::new(KanVariant::BrdfParams);
        let mut tape = GpuTape::new(TapeStorageMode::WorkgroupShared);
        let a = 0.4_f32;
        let w = 1.5_f32;
        let input: GpuJet<f32, 2> = GpuJet::pack(Jet::promote(a)).unwrap();
        let output = kan.eval(input, w, &mut tape).unwrap();
        let expected = (2.0 * a + a.cos()) * w;
        assert!((output.nth_deriv(1) - expected).abs() < 1e-5);
    }

    #[test]
    fn reverse_pass_recovers_first_derivative_via_tape() {
        // Same kernel, but recover ∂y/∂a via the *reverse-pass* on the tape.
        let kan = KanGpuForward::new(KanVariant::BrdfParams);
        let mut tape = GpuTape::new(TapeStorageMode::WorkgroupShared);
        let a = 0.4_f64;
        let w = 1.5_f64;
        let input: GpuJet<f32, 2> = GpuJet::pack(Jet::promote(a as f32)).unwrap();
        let _output = kan.eval(input, w as f32, &mut tape).unwrap();

        let mut cot = vec![0.0_f64; tape.len()];
        let last = tape.len() - 1;
        cot[last] = 1.0;
        tape.replay_into(&mut cot).unwrap();

        // Slot 0 was the input `a` ; cot[0] should equal dy/da.
        let expected_grad = (2.0 * a + a.cos()) * w;
        // f32 round-trip introduces error ; tolerate 1e-4.
        assert!(
            (cot[0] - expected_grad).abs() < 1e-4,
            "got {} expected {}",
            cot[0],
            expected_grad
        );
    }

    #[test]
    fn iridescence_variant_has_three_hidden_layers() {
        let s = KanVariant::IridescenceStack.canonical();
        assert_eq!(s.hidden_layers, 3);
    }

    #[test]
    fn micro_displacement_shape_is_seven_to_one() {
        let s = KanVariant::MicroDisplacement.canonical();
        assert_eq!(s.input_dim, 7);
        assert_eq!(s.output_dim, 1);
    }
}
