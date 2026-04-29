//! § kan_amplifier — D119 sub-pixel-detail amplifier integration trait.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   T11-D119 (FractalAmplifierPass / Stage-7) is a separate slice that
//!   computes sub-pixel-detail-amplification via a KAN-network. Stage-5
//!   provides the [`KanAmplifierHook`] trait + a [`MockAmplifier`] passthrough
//!   so the Stage-5 driver compiles + tests independently of D119 landing.
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § III Stage-7` —
//!     KAN-detail-amplifier per-pixel ; sub-pixel-fractal-tessellation.
//!   - `Omniverse/14_NOVEL_RENDERING § Sub-Pixel-Fractal-Tessellation` —
//!     fractal-self-similar amplifier emerging-from-physics ¬ stamped-from-
//!     library.
//!
//! § INTEGRATION CONTRACT
//!   Stage-5 emits a [`KanAmplifierInput`] for each surface-hit pixel ; the
//!   amplifier returns a [`KanAmplifierOutput`] with sub-pixel-detail
//!   coefficients. Stage-5 stores the coefficients in the GBuffer's
//!   `material_handle`-adjacent slot (or a separate mip-level texture in the
//!   GPU-side path) ; downstream stages read it.

use thiserror::Error;

/// Input to the KAN amplifier : surface-context for one pixel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KanAmplifierInput {
    /// World-space hit-position.
    pub world_pos: [f32; 3],
    /// Surface normal at hit.
    pub normal: [f32; 3],
    /// View-direction (origin → hit, normalized).
    pub view_dir: [f32; 3],
    /// Surface curvature (∇·n approximation ; 0 = flat).
    pub curvature: f32,
    /// Material-coordinate axis-vector (M-facet 15-dim KAN-input hint).
    pub material_coord: [f32; 4],
    /// Detail-budget allocated by Stage-2 for this pixel.
    pub detail_budget: f32,
}

impl KanAmplifierInput {
    /// New input.
    #[must_use]
    pub fn new(
        world_pos: [f32; 3],
        normal: [f32; 3],
        view_dir: [f32; 3],
        curvature: f32,
        material_coord: [f32; 4],
        detail_budget: f32,
    ) -> Self {
        KanAmplifierInput {
            world_pos,
            normal,
            view_dir,
            curvature,
            material_coord,
            detail_budget,
        }
    }
}

/// Output of the KAN amplifier : sub-pixel detail coefficients.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KanAmplifierOutput {
    /// Sub-pixel-detail-coefficient (≥ 0). 0 = no detail added.
    pub detail_coefficient: f32,
    /// Octave-count of the fractal-self-similar pattern (1 = pure flat).
    pub octave_count: u8,
    /// Self-similarity ratio (0..1 ; smaller = more compressed octaves).
    pub similarity_ratio: f32,
}

impl KanAmplifierOutput {
    /// Passthrough : no detail.
    #[must_use]
    pub fn passthrough() -> Self {
        KanAmplifierOutput {
            detail_coefficient: 0.0,
            octave_count: 1,
            similarity_ratio: 1.0,
        }
    }

    /// New output with explicit fields.
    #[must_use]
    pub fn new(detail_coefficient: f32, octave_count: u8, similarity_ratio: f32) -> Self {
        KanAmplifierOutput {
            detail_coefficient,
            octave_count: octave_count.max(1),
            similarity_ratio: similarity_ratio.clamp(0.0, 1.0),
        }
    }

    /// Whether the output represents a non-trivial amplification.
    #[must_use]
    pub fn has_detail(&self) -> bool {
        self.detail_coefficient > 1e-6 && self.octave_count > 1
    }
}

/// Errors from the amplifier hook.
#[derive(Debug, Error, PartialEq)]
pub enum KanAmplifierError {
    /// Detail-budget exceeded for this pixel ; amplifier was throttled.
    #[error("detail-budget {requested} > {ceiling} ; throttled")]
    BudgetExceeded { requested: f32, ceiling: f32 },
    /// Amplifier KAN-net unavailable (D119 not landed and no fallback supplied).
    #[error("amplifier unavailable : {reason}")]
    Unavailable { reason: String },
}

/// Trait the Stage-7 amplifier (T11-D119) implements. Stage-5 invokes this
/// per-pixel after surface-hit ; the returned coefficients are stored in the
/// gbuffer companion texture (or an indirect-detail-texture in the spectral
/// pipeline path).
pub trait KanAmplifierHook {
    /// Amplify detail at one pixel. Returns the detail-coefficient + octave
    /// configuration ; or [`KanAmplifierError`] if the budget was exceeded.
    fn amplify(&self, input: &KanAmplifierInput) -> Result<KanAmplifierOutput, KanAmplifierError>;

    /// Per-frame budget the amplifier owns. Stage-5 reads this for telemetry.
    fn frame_budget(&self) -> f32 {
        1.0
    }
}

/// Mock amplifier that returns passthrough (no detail). Used as the default
/// implementation pre-D119 ; tests verify the integration-contract rather than
/// the amplifier-quality.
#[derive(Debug, Clone, Copy, Default)]
pub struct MockAmplifier {
    /// If non-zero, the mock returns a constant detail coefficient instead of
    /// passthrough. Useful for regression-testing the integration plumbing.
    pub fixed_detail: f32,
    /// Fixed octave-count for the mock output.
    pub fixed_octaves: u8,
}

impl MockAmplifier {
    /// Construct a passthrough mock.
    #[must_use]
    pub fn passthrough() -> Self {
        MockAmplifier::default()
    }

    /// Construct a fixed-output mock (for plumbing tests).
    #[must_use]
    pub fn with_fixed(detail: f32, octaves: u8) -> Self {
        MockAmplifier {
            fixed_detail: detail,
            fixed_octaves: octaves,
        }
    }
}

impl KanAmplifierHook for MockAmplifier {
    fn amplify(
        &self,
        _input: &KanAmplifierInput,
    ) -> Result<KanAmplifierOutput, KanAmplifierError> {
        if self.fixed_detail > 0.0 {
            Ok(KanAmplifierOutput::new(
                self.fixed_detail,
                self.fixed_octaves.max(1),
                0.5,
            ))
        } else {
            Ok(KanAmplifierOutput::passthrough())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_input() -> KanAmplifierInput {
        KanAmplifierInput::new(
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, -1.0],
            0.1,
            [0.5; 4],
            1.0,
        )
    }

    #[test]
    fn passthrough_output_has_no_detail() {
        let o = KanAmplifierOutput::passthrough();
        assert!(!o.has_detail());
    }

    #[test]
    fn output_clamps_octave_to_min_1() {
        let o = KanAmplifierOutput::new(0.5, 0, 0.7);
        assert_eq!(o.octave_count, 1);
    }

    #[test]
    fn output_clamps_similarity_to_0_1() {
        let o1 = KanAmplifierOutput::new(0.5, 4, -0.3);
        let o2 = KanAmplifierOutput::new(0.5, 4, 1.7);
        assert!((o1.similarity_ratio - 0.0).abs() < 1e-6);
        assert!((o2.similarity_ratio - 1.0).abs() < 1e-6);
    }

    #[test]
    fn input_construction_round_trips() {
        let i = dummy_input();
        assert_eq!(i.world_pos, [0.0, 0.0, 0.0]);
        assert_eq!(i.normal, [0.0, 1.0, 0.0]);
        assert!((i.detail_budget - 1.0).abs() < 1e-6);
    }

    #[test]
    fn mock_passthrough_returns_zero_detail() {
        let m = MockAmplifier::passthrough();
        let out = m.amplify(&dummy_input()).unwrap();
        assert!(!out.has_detail());
    }

    #[test]
    fn mock_fixed_returns_fixed_detail() {
        let m = MockAmplifier::with_fixed(0.5, 4);
        let out = m.amplify(&dummy_input()).unwrap();
        assert!(out.has_detail());
        assert!((out.detail_coefficient - 0.5).abs() < 1e-6);
        assert_eq!(out.octave_count, 4);
    }

    #[test]
    fn mock_frame_budget_default() {
        let m = MockAmplifier::passthrough();
        assert!((m.frame_budget() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn budget_exceeded_error_carries_values() {
        let e = KanAmplifierError::BudgetExceeded {
            requested: 2.0,
            ceiling: 1.0,
        };
        let s = format!("{e}");
        assert!(s.contains("2"));
        assert!(s.contains("1"));
    }

    #[test]
    fn output_has_detail_requires_both_coefficient_and_octaves() {
        // Octaves > 1 but coefficient = 0 → no detail.
        let o = KanAmplifierOutput::new(0.0, 5, 0.5);
        assert!(!o.has_detail());
        // Coefficient > 0 but octaves = 1 → no detail (single octave is flat).
        let o = KanAmplifierOutput::new(0.5, 1, 0.5);
        assert!(!o.has_detail());
        // Both > threshold → detail present.
        let o = KanAmplifierOutput::new(0.5, 4, 0.5);
        assert!(o.has_detail());
    }
}
