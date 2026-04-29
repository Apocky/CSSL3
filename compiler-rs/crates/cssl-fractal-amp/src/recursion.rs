//! § recursion — RecursiveDetailLOD (fractal-tessellation depth controller)
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The fractal-tessellation depth controller per
//!   `06_RENDERING_PIPELINE § Stage-7 § fractal-property` :
//!
//!     ‼ amplifier ⊗ self-similar-pattern @ multiple-scales
//!     ‼ amplifier ⊗ KAN-derived ⊗ N! authored-texture-detail
//!     consequence : detail emerges-from-physics ⊗ ¬ stamped-from-library
//!
//!   The recursion driver descends through a stack of fractal-detail
//!   levels, each level halving the spatial-step and feeding the
//!   previous level's micro-displacement back into the next-level's
//!   input vector. This is the fractal-self-similarity property : the
//!   same KAN-network produces sub-pixel detail at each scale,
//!   giving the spec's "infinite-zoom-detail @ NO-asset-cost" claim
//!   from `00_EXOTICISM § V.3 (a)`.
//!
//!   Termination is triggered by any of :
//!     - depth ≥ budget.max_recursion_depth
//!     - kan_confidence < budget.kan_confidence_floor
//!     - sigma_privacy classifies as Private
//!     - cumulative cost crossing budget threshold
//!
//! § DETERMINISM
//!   The recursion is purely a function of `(input, budget,
//!   amplifier_weights)`. The level-stack is reset at each call. No
//!   thread-local state. No frame-counter. The `tests::recursion_is_
//!   deterministic` test pins this contract.

use crate::amplifier::{AmplifierError, FractalAmplifier};
use crate::budget::DetailBudget;
use crate::fragment::AmplifiedFragment;
use crate::sdf_trait::SdfHitInfo;
use crate::sigma_mask::SigmaPrivacy;
use smallvec::SmallVec;

/// § Maximum recursion depth across all fovea-tiers. The full-tier max
///   is 5 levels (matching the spec's "fovea evaluates 3-5 levels deep"
///   literal). The mid-tier max is 2. The peripheral-tier max is 0.
pub const MAX_RECURSION_DEPTH: usize = 5;

/// § A single fractal-detail level in the recursion stack. Records the
///   spatial-step, the accumulated micro-displacement so far, the
///   accumulated micro-roughness, the accumulated micro-color, and
///   the current per-level confidence. Each level's KAN-eval feeds its
///   output into the next level's `accumulated_*` fields.
#[derive(Debug, Clone, Copy, Default)]
pub struct DetailLevel {
    /// § Recursion depth (0 = first level, max = MAX_RECURSION_DEPTH-1).
    pub depth: u8,
    /// § Spatial-step at this level. Halves with each recursion step :
    ///   `step[d] = step[0] × 0.5^d`. Used to perturb the input
    ///   `world_pos` for the level-d KAN evaluation.
    pub spatial_step: f32,
    /// § The amplified fragment at this level (before composition with
    ///   prior levels).
    pub fragment: AmplifiedFragment,
    /// § Cumulative attenuation factor applied to this level's
    ///   contribution. `attenuation[d] = (typical-falloff)^d`. The
    ///   typical-falloff is `0.7` per the analogous mise-en-abyme
    ///   recursion in `06_RENDERING_PIPELINE § Stage-9`.
    pub attenuation: f32,
}

/// § Errors that the recursion driver can return.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RecursionError {
    /// § The requested recursion depth exceeds MAX_RECURSION_DEPTH.
    #[error("recursion depth {0} exceeds MAX_RECURSION_DEPTH ({1})")]
    DepthExceedsMax(usize, usize),
    /// § Underlying amplifier error.
    #[error("amplifier error during recursion : {0}")]
    Amplifier(#[from] AmplifierError),
}

/// § Per-level attenuation factor matching the mise-en-abyme falloff
///   from `06_RENDERING_PIPELINE § Stage-9 § typical-falloff`. Each
///   recursion level multiplies the prior level's contribution by this
///   factor so deeper levels contribute less to the final fragment.
pub const LEVEL_ATTENUATION_FACTOR: f32 = 0.7;

/// § The recursion driver. Holds a stack of `DetailLevel`s allocated on
///   the stack via `SmallVec<DetailLevel, MAX_RECURSION_DEPTH>` to avoid
///   heap traffic on the per-fragment hot path.
pub struct RecursiveDetailLOD {
    /// § The amplifier that this driver invokes at each level.
    pub amplifier: FractalAmplifier,
    /// § Initial spatial-step at depth 0. Defaults to the
    ///   `EPSILON_DISP` constant from `fragment.rs`. Future levels
    ///   halve this.
    pub initial_spatial_step: f32,
}

impl RecursiveDetailLOD {
    /// § Construct with a given amplifier and initial spatial-step.
    #[must_use]
    pub fn new(amplifier: FractalAmplifier, initial_spatial_step: f32) -> Self {
        Self {
            amplifier,
            initial_spatial_step,
        }
    }

    /// § Construct with default amplifier (untrained) and the canonical
    ///   `EPSILON_DISP / 2` initial spatial-step.
    #[must_use]
    pub fn new_default() -> Self {
        Self::new(
            FractalAmplifier::new_untrained(),
            crate::fragment::EPSILON_DISP * 0.5,
        )
    }

    /// § Recurse into the fragment. Walks the level stack from depth 0
    ///   to `budget.max_recursion_depth`, accumulating amplified
    ///   contributions with the LEVEL_ATTENUATION_FACTOR multiplicative
    ///   falloff. Returns the composed fragment at the top.
    ///
    ///   Termination conditions :
    ///     - depth ≥ budget.max_recursion_depth
    ///     - kan_confidence < budget.kan_confidence_floor
    ///     - sigma_privacy = Private at any level
    ///
    ///   The recursion is DETERMINISTIC : same input ⇒ same output, no
    ///   RNG, no time, no mutable cache.
    pub fn recurse<H: SdfHitInfo>(
        &self,
        hit: &H,
        budget: &DetailBudget,
    ) -> Result<AmplifiedFragment, RecursionError> {
        // § Σ-private + peripheral skip → ZERO at depth 0.
        if hit.sigma_privacy().is_private() {
            return Ok(AmplifiedFragment::ZERO.with_privacy(SigmaPrivacy::Private));
        }
        if !budget.should_amplify() {
            return Ok(AmplifiedFragment::ZERO);
        }

        let mut composed = AmplifiedFragment::ZERO;
        let mut levels: SmallVec<[DetailLevel; MAX_RECURSION_DEPTH]> = SmallVec::new();
        let max_depth = budget.max_recursion_depth as usize;

        // § Walk depth 0..max_depth.
        for d in 0..max_depth {
            let depth_u8 = d as u8;
            let spatial_step = self.initial_spatial_step * 0.5_f32.powi(d as i32);
            let attenuation = LEVEL_ATTENUATION_FACTOR.powi(d as i32);

            // § At each level, feed the prior level's micro-displacement
            //   back into the input as a perturbation of world-pos
            //   along the gradient. This is the fractal-self-similarity
            //   step : the KAN sees a slightly-shifted position at
            //   each level, so the same network produces fresh detail
            //   at each scale.
            let pos = hit.world_pos();
            let grad = hit.base_sdf_grad();
            let shifted_pos = if d == 0 {
                pos
            } else {
                let prior_disp = composed.micro_displacement;
                [
                    pos[0] + grad[0] * prior_disp * spatial_step,
                    pos[1] + grad[1] * prior_disp * spatial_step,
                    pos[2] + grad[2] * prior_disp * spatial_step,
                ]
            };

            let level_fragment = self.amplifier.amplify(
                shifted_pos,
                hit.view_dir(),
                grad,
                budget,
                hit.sigma_privacy(),
            )?;

            // § Confidence-floor termination : if the per-level
            //   confidence is below the floor, stop recursing.
            if level_fragment.kan_confidence < budget.kan_confidence_floor {
                break;
            }

            let attenuated = level_fragment.attenuate(attenuation);
            // § Compose attenuated level into the running fragment.
            //   Displacement / roughness sum ; color sums channel-wise.
            composed.micro_displacement += attenuated.micro_displacement;
            composed.micro_roughness += attenuated.micro_roughness;
            composed.micro_color = composed.micro_color.add(attenuated.micro_color);
            // § Confidence is the MIN across levels (the recursion is
            //   only as confident as its weakest level).
            if d == 0 {
                composed.kan_confidence = level_fragment.kan_confidence;
            } else {
                composed.kan_confidence =
                    composed.kan_confidence.min(level_fragment.kan_confidence);
            }
            composed.sigma_privacy = level_fragment.sigma_privacy;

            levels.push(DetailLevel {
                depth: depth_u8,
                spatial_step,
                fragment: level_fragment,
                attenuation,
            });
        }

        // § Final saturation pass : even though each level was already
        //   saturated, the channel-sum could exceed the EPSILON_DISP /
        //   EPSILON_ROUGHNESS bands. Re-saturate via AmplifiedFragment::new.
        let final_frag = AmplifiedFragment::new(
            composed.micro_displacement,
            composed.micro_roughness,
            composed.micro_color,
            composed.kan_confidence,
            composed.sigma_privacy,
        );
        Ok(final_frag)
    }

    /// § Recurse against a position triple (escape hatch for callers
    ///   that don't have a SdfHitInfo to hand).
    pub fn recurse_pos(
        &self,
        world_pos: [f32; 3],
        view_dir: [f32; 3],
        base_sdf_grad: [f32; 3],
        budget: &DetailBudget,
        sigma_privacy: SigmaPrivacy,
    ) -> Result<AmplifiedFragment, RecursionError> {
        use crate::sdf_trait::MockSdfHit;
        let hit = MockSdfHit {
            world_pos,
            view_dir,
            base_sdf_grad,
            pixel_projected_area: 1.0e-4,
            view_distance: 1.0,
            sigma_privacy,
        };
        self.recurse(&hit, budget)
    }
}

impl Default for RecursiveDetailLOD {
    fn default() -> Self {
        Self::new_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::FoveaTier;
    use crate::sdf_trait::MockSdfHit;

    /// § LEVEL_ATTENUATION_FACTOR is in (0, 1).
    #[test]
    fn attenuation_factor_in_unit_interval() {
        assert!(LEVEL_ATTENUATION_FACTOR > 0.0);
        assert!(LEVEL_ATTENUATION_FACTOR < 1.0);
    }

    /// § DetailLevel default has depth 0.
    #[test]
    fn default_level_depth_zero() {
        let l = DetailLevel::default();
        assert_eq!(l.depth, 0);
    }

    /// § new_default constructor produces a well-formed driver.
    #[test]
    fn new_default_well_formed() {
        let r = RecursiveDetailLOD::new_default();
        assert!(r.initial_spatial_step > 0.0);
    }

    /// § Σ-private hit → ZERO output regardless of budget.
    #[test]
    fn sigma_private_yields_zero() {
        let r = RecursiveDetailLOD::new_default();
        let budget = DetailBudget::default();
        let hit = MockSdfHit::default().with_sigma_privacy(SigmaPrivacy::Private);
        let out = r.recurse(&hit, &budget).unwrap();
        assert!(out.is_zero());
        assert_eq!(out.sigma_privacy, SigmaPrivacy::Private);
    }

    /// § Peripheral-tier yields ZERO.
    #[test]
    fn peripheral_yields_zero() {
        let r = RecursiveDetailLOD::new_default();
        let budget = DetailBudget::from_fovea_tier(FoveaTier::Peripheral);
        let hit = MockSdfHit::default();
        let out = r.recurse(&hit, &budget).unwrap();
        assert!(out.is_zero());
    }

    /// § Full-tier with a viable hit produces non-ZERO output.
    #[test]
    fn full_tier_produces_non_zero() {
        let r = RecursiveDetailLOD::new(
            FractalAmplifier::new_untrained().with_confidence_floor(0.05),
            1.0e-3,
        );
        let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
        let hit = MockSdfHit::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
        let out = r.recurse(&hit, &budget).unwrap();
        assert!(!out.is_zero());
    }

    /// § Full-tier exercises 5-level recursion (the spec's "3-5 levels"
    /// max).
    #[test]
    fn full_tier_five_levels() {
        let r = RecursiveDetailLOD::new(
            FractalAmplifier::new_untrained().with_confidence_floor(0.05),
            1.0e-3,
        );
        let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
        assert_eq!(budget.max_recursion_depth, 5);
        let hit = MockSdfHit::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
        let out = r.recurse(&hit, &budget).unwrap();
        // § The output should be saturated within the EPSILON bands.
        use crate::fragment::{EPSILON_DISP, EPSILON_ROUGHNESS};
        assert!(out.micro_displacement.abs() <= EPSILON_DISP + 1e-7);
        assert!(out.micro_roughness.abs() <= EPSILON_ROUGHNESS + 1e-7);
    }

    /// § Determinism : same hit ⇒ same output across two calls.
    #[test]
    fn recursion_is_deterministic() {
        let r = RecursiveDetailLOD::new(
            FractalAmplifier::new_untrained().with_confidence_floor(0.05),
            1.0e-3,
        );
        let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
        let hit = MockSdfHit::new([1.5, 2.5, 3.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
        let a = r.recurse(&hit, &budget).unwrap();
        let b = r.recurse(&hit, &budget).unwrap();
        assert_eq!(a, b);
    }

    /// § Confidence-floor termination : a high floor truncates the recursion.
    #[test]
    fn high_floor_truncates_recursion() {
        let r = RecursiveDetailLOD::new(FractalAmplifier::new_untrained(), 1.0e-3);
        // § Set the floor to 0.99 so most levels get truncated.
        let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.99).unwrap();
        let hit = MockSdfHit::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.0, 0.5, 0.0]);
        let out = r.recurse(&hit, &budget).unwrap();
        // § With high floor, most levels are below threshold so output is ZERO.
        assert!(out.is_zero());
    }

    /// § Mid-tier exercises only 2 levels.
    #[test]
    fn mid_tier_two_levels() {
        let r = RecursiveDetailLOD::new_default();
        let budget = DetailBudget::from_fovea_tier(FoveaTier::Mid);
        assert_eq!(budget.max_recursion_depth, 2);
        let hit = MockSdfHit::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
        // § Just check it doesn't panic and produces a result.
        let _out = r.recurse(&hit, &budget).unwrap();
    }

    /// § recurse_pos escape hatch composes correctly.
    #[test]
    fn recurse_pos_escape_hatch() {
        let r = RecursiveDetailLOD::new_default();
        let budget = DetailBudget::default();
        let _ = r
            .recurse_pos(
                [0.5, 0.5, 0.5],
                [0.0, 0.0, 1.0],
                [0.6, 0.8, 0.0],
                &budget,
                SigmaPrivacy::Public,
            )
            .unwrap();
    }

    /// § Different positions ⇒ different outputs (the recursion actually
    /// reads the input).
    #[test]
    fn recursion_reads_input() {
        let r = RecursiveDetailLOD::new(
            FractalAmplifier::new_untrained().with_confidence_floor(0.05),
            1.0e-3,
        );
        let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
        let hit_a =
            MockSdfHit::new([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
        let hit_b =
            MockSdfHit::new([1.0, 1.0, 1.0], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
        let a = r.recurse(&hit_a, &budget).unwrap();
        let b = r.recurse(&hit_b, &budget).unwrap();
        assert_ne!(a, b);
    }

    /// § Output respects EPSILON_DISP saturation even after composition.
    #[test]
    fn composed_output_respects_epsilon() {
        use crate::fragment::EPSILON_DISP;
        let r = RecursiveDetailLOD::new(
            FractalAmplifier::new_untrained().with_confidence_floor(0.05),
            1.0e-3,
        );
        let budget = DetailBudget::new(FoveaTier::Full, 1.0, 0.05).unwrap();
        // § Try many input positions and confirm none break saturation.
        for &x in &[0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0] {
            let hit = MockSdfHit::new([x, x, x], [0.0, 0.0, 1.0]).with_sdf_grad([0.6, 0.8, 0.0]);
            let out = r.recurse(&hit, &budget).unwrap();
            assert!(out.micro_displacement.abs() <= EPSILON_DISP + 1e-7);
        }
    }

    /// § DetailLevel records depth + spatial-step.
    #[test]
    fn detail_level_records_metadata() {
        let l = DetailLevel {
            depth: 3,
            spatial_step: 1.25e-4,
            fragment: AmplifiedFragment::ZERO,
            attenuation: 0.343,
        };
        assert_eq!(l.depth, 3);
        assert!((l.spatial_step - 1.25e-4).abs() < 1e-9);
    }
}
