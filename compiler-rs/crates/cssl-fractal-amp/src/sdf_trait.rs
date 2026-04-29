//! § sdf_trait — Trait surface that ties this crate to cssl-render-v2 (D116)
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The integration boundary between Stage-7 (this crate) and Stage-5
//!   (cssl-render-v2 ; D116). The trait declares a minimal `SdfHitInfo`
//!   accessor surface that any RayHit type can implement, plus the
//!   [`SdfRaymarchAmplifier`] trait that exposes the amplifier's
//!   `amplify_at_hit` entry-point.
//!
//!   By naming the trait HERE rather than in render-v2, this crate stays
//!   buildable when D116 has not landed — callers simply implement the
//!   trait against their own RayHit equivalent. When D116 lands, its
//!   `RayHit` type implements `SdfHitInfo` and the amplifier slots in
//!   without any API churn.
//!
//! § DESIGN — minimal surface
//!   The trait surface is intentionally narrow. It exposes :
//!
//!     - `world_pos()`            — the hit point in world coordinates.
//!     - `view_dir()`             — unit vector from hit to camera origin.
//!     - `base_sdf_grad()`        — the SDF gradient at the hit point.
//!     - `pixel_projected_area()` — sub-pixel area of this fragment on
//!                                  the image plane (PGA-derived ; this
//!                                  is the cone-radius × cone-distance
//!                                  product the SDF raymarch already
//!                                  computes in its bisection-refine
//!                                  step).
//!     - `view_distance()`        — Euclidean distance from camera to
//!                                  hit. Conditioning factor for the
//!                                  detail-budget (far surfaces get
//!                                  cheaper amplification).
//!     - `sigma_privacy()`        — the Σ-mask classification. The
//!                                  amplifier consults this before any
//!                                  KAN-network evaluation per
//!                                  `00_EXOTICISM § V.3 (d) sovereignty`.
//!
//!   Anything richer (cell-handle, Φ-pattern-link, M-coord) is the
//!   business of higher stages and is NOT exposed through this trait.
//!   The amplifier is purely positional + view-conditioned.
//!
//! § REFERENCE IMPLEMENTATION — `MockSdfHit`
//!   The unit-test scaffolding uses a `MockSdfHit` struct that carries
//!   the trait's seven accessor values directly. Tests construct one by
//!   value, pass it to the amplifier, and inspect the resulting
//!   `AmplifiedFragment`. This is what lets the determinism / recursion
//!   / fovea-mask integration tests exercise the amplifier without any
//!   D116 dependency.

use crate::amplifier::AmplifierError;
use crate::budget::DetailBudget;
use crate::fragment::AmplifiedFragment;
use crate::sigma_mask::{SigmaMaskCheck, SigmaPrivacy};

/// § The minimal accessor surface that any RayHit type must expose for
///   the Stage-7 amplifier to consume it. D116 (cssl-render-v2)
///   implements this for its own `RayHit` ; tests use [`MockSdfHit`].
pub trait SdfHitInfo {
    /// § World-space hit position.
    fn world_pos(&self) -> [f32; 3];
    /// § Unit vector from hit toward camera origin.
    fn view_dir(&self) -> [f32; 3];
    /// § SDF gradient at the hit point (normalized).
    fn base_sdf_grad(&self) -> [f32; 3];
    /// § Sub-pixel area on the image plane in scene-units squared.
    ///   Computed by the raymarch's bisection-refine step.
    fn pixel_projected_area(&self) -> f32;
    /// § Euclidean distance from camera origin to hit, in scene units.
    fn view_distance(&self) -> f32;
    /// § Σ-privacy classification. Amplifier refuses to evaluate when
    ///   this returns `SigmaPrivacy::Private`.
    fn sigma_privacy(&self) -> SigmaPrivacy;
}

/// § The integration entry point that D116's raymarch calls from its
///   bisection-refine path. Any type that wraps a [`crate::FractalAmplifier`]
///   with a configured [`DetailBudget`] can implement this.
pub trait SdfRaymarchAmplifier {
    /// § Amplify the fragment at the given hit. Returns `AmplifiedFragment`
    ///   on success, `AmplifierError` on budget-exhaustion or
    ///   confidence-below-threshold.
    fn amplify_at_hit<H: SdfHitInfo>(
        &self,
        hit: &H,
        budget: &DetailBudget,
    ) -> Result<AmplifiedFragment, AmplifierError>;
}

/// § Reference implementation of [`SdfHitInfo`] for unit tests. Carries
///   all seven accessor values by direct struct fields. Tests construct
///   one with `MockSdfHit::new(...)` or via fluent builders and pass it
///   to the amplifier. The values match the input layout of the
///   `KAN_micro_displacement` network's input vector exactly :
///
///     `[pos.xyz | view.xy_proj | grad.norm_2D]`
///
///   per `07_KAN_RUNTIME_SHADING § IX § canonical-call-site-signature`.
#[derive(Debug, Clone, Copy)]
pub struct MockSdfHit {
    /// § World-space hit position (3 floats).
    pub world_pos: [f32; 3],
    /// § View direction unit vector (3 floats).
    pub view_dir: [f32; 3],
    /// § SDF gradient (3 floats, normalized).
    pub base_sdf_grad: [f32; 3],
    /// § Sub-pixel projected area.
    pub pixel_projected_area: f32,
    /// § View distance (scalar).
    pub view_distance: f32,
    /// § Σ-privacy classification.
    pub sigma_privacy: SigmaPrivacy,
}

impl Default for MockSdfHit {
    fn default() -> Self {
        Self {
            world_pos: [0.0, 0.0, 0.0],
            view_dir: [0.0, 0.0, 1.0],
            base_sdf_grad: [0.0, 1.0, 0.0],
            pixel_projected_area: 1.0e-4,
            view_distance: 1.0,
            sigma_privacy: SigmaPrivacy::Public,
        }
    }
}

impl MockSdfHit {
    /// § Construct with explicit world-pos and view-dir, default everything else.
    #[must_use]
    pub fn new(world_pos: [f32; 3], view_dir: [f32; 3]) -> Self {
        Self {
            world_pos,
            view_dir,
            ..Self::default()
        }
    }

    /// § Builder — set the SDF gradient.
    #[must_use]
    pub fn with_sdf_grad(mut self, g: [f32; 3]) -> Self {
        self.base_sdf_grad = g;
        self
    }

    /// § Builder — set the sub-pixel projected area.
    #[must_use]
    pub fn with_pixel_projected_area(mut self, a: f32) -> Self {
        self.pixel_projected_area = a;
        self
    }

    /// § Builder — set the view distance.
    #[must_use]
    pub fn with_view_distance(mut self, d: f32) -> Self {
        self.view_distance = d;
        self
    }

    /// § Builder — set the Σ-privacy classification.
    #[must_use]
    pub fn with_sigma_privacy(mut self, p: SigmaPrivacy) -> Self {
        self.sigma_privacy = p;
        self
    }
}

impl SdfHitInfo for MockSdfHit {
    fn world_pos(&self) -> [f32; 3] {
        self.world_pos
    }
    fn view_dir(&self) -> [f32; 3] {
        self.view_dir
    }
    fn base_sdf_grad(&self) -> [f32; 3] {
        self.base_sdf_grad
    }
    fn pixel_projected_area(&self) -> f32 {
        self.pixel_projected_area
    }
    fn view_distance(&self) -> f32 {
        self.view_distance
    }
    fn sigma_privacy(&self) -> SigmaPrivacy {
        self.sigma_privacy
    }
}

impl SigmaMaskCheck for MockSdfHit {
    fn classify_privacy(&self) -> SigmaPrivacy {
        self.sigma_privacy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § default constructs a viable hit.
    #[test]
    fn default_hit_is_viable() {
        let h = MockSdfHit::default();
        assert_eq!(h.world_pos(), [0.0, 0.0, 0.0]);
        assert!(h.view_distance() > 0.0);
        assert!(h.pixel_projected_area() > 0.0);
        assert!(h.sigma_privacy().is_public());
    }

    /// § new() preserves world-pos.
    #[test]
    fn new_preserves_world_pos() {
        let h = MockSdfHit::new([1.0, 2.0, 3.0], [0.0, 0.0, 1.0]);
        assert_eq!(h.world_pos(), [1.0, 2.0, 3.0]);
    }

    /// § with_sdf_grad sets the gradient.
    #[test]
    fn with_sdf_grad_sets_gradient() {
        let h = MockSdfHit::default().with_sdf_grad([1.0, 0.0, 0.0]);
        assert_eq!(h.base_sdf_grad(), [1.0, 0.0, 0.0]);
    }

    /// § with_pixel_projected_area sets the area.
    #[test]
    fn with_area_sets_area() {
        let h = MockSdfHit::default().with_pixel_projected_area(5e-4);
        assert!((h.pixel_projected_area() - 5e-4).abs() < 1e-9);
    }

    /// § with_view_distance sets distance.
    #[test]
    fn with_distance_sets_distance() {
        let h = MockSdfHit::default().with_view_distance(10.0);
        assert!((h.view_distance() - 10.0).abs() < 1e-6);
    }

    /// § with_sigma_privacy switches classification.
    #[test]
    fn with_sigma_private() {
        let h = MockSdfHit::default().with_sigma_privacy(SigmaPrivacy::Private);
        assert!(h.sigma_privacy().is_private());
        // § The SigmaMaskCheck blanket-impl agrees.
        assert!(h.classify_privacy().is_private());
    }

    /// § fluent-build composes correctly.
    #[test]
    fn builder_chain_composes() {
        let h = MockSdfHit::new([0.5, 0.5, 0.5], [0.0, 0.0, 1.0])
            .with_sdf_grad([0.0, 1.0, 0.0])
            .with_pixel_projected_area(2e-4)
            .with_view_distance(5.0)
            .with_sigma_privacy(SigmaPrivacy::Public);
        assert_eq!(h.world_pos(), [0.5, 0.5, 0.5]);
        assert_eq!(h.base_sdf_grad(), [0.0, 1.0, 0.0]);
        assert!((h.pixel_projected_area() - 2e-4).abs() < 1e-9);
        assert!((h.view_distance() - 5.0).abs() < 1e-6);
        assert!(h.sigma_privacy().is_public());
    }
}
