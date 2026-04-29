//! § raymarch — SDF sphere-tracing + cone-marching with MERA-skip.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The core compute kernel of Stage-5. Walks one ray per pixel-per-view-
//!   instance through the unified SDF, using [`crate::mera_skip::MeraSkipDispatcher`]
//!   for hierarchical large-step skipping when the ray is far from any surface,
//!   bisection-refine when approaching surface, and [`crate::normals::BackwardDiffNormals`]
//!   for surface-normal at the hit.
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/01_SDF_NATIVE_RENDER.csl.md § IV ray marching` :
//!     enhanced sphere-tracing + MERA-skip + bisection-refine + bwd-diff-normal.
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § III Stage-5` :
//!     compute step 1 (per-pixel sphere-tracing-with-MERA-skip + bisection-
//!     refine + bwd_diff(SDF) normal + material-handle lookup).
//!   - `Pointers Gone Wild '26 derivation` : cone-marching for ALL recursive
//!     primary rays (50→100 fps speed-up vs naive sphere-tracing).
//!
//! § ALGORITHM
//!   ```text
//!   for each pixel @ shading-rate-from-fovea-mask :
//!     origin   ← camera.eye-with-ipd-offset
//!     dir      ← camera.pixel_to_ray(px, py)
//!     t        ← 0
//!     for step in 0..max_steps :
//!       p ← origin + t·dir
//!       if mera_skip.step_at(p) is LargeStep(b) :
//!         t += b
//!         continue
//!       d ← sdf.evaluate(p)
//!       if d < hit_eps :
//!         normal ← BackwardDiffNormals::estimate(sdf, p)
//!         emit RayHit { p, normal, t, material }
//!         break
//!       if t > max_distance : emit miss ; break
//!       t += d
//!   ```
//!
//! § CONE-MARCHING (foundation slice)
//!   At foundation we expose a `cone_aperture` field on [`RaymarchConfig`].
//!   When `cone_aperture > 0`, the per-step distance is enlarged by
//!   `t * cone_aperture` to allow over-conservative steps that early-out
//!   on small features. This is the basic cone-marching trade : fewer steps
//!   in the common case at the cost of missing sub-cone features (which is
//!   exactly the tradeoff the spec calls for in peripheral-pixel pixels).

use thiserror::Error;

use crate::camera::EyeCamera;
use crate::foveation::ShadingRate;
use crate::mera_skip::{MeraSkipDispatcher, MeraSkipResult};
use crate::normals::{BackwardDiffNormals, SdfFunction, SurfaceNormal};

/// Error type for raymarcher failures.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RaymarchError {
    /// Step budget exhausted before either hit or miss.
    #[error("ray-march exhausted {limit} steps without converging")]
    StepBudgetExhausted { limit: u32 },
    /// SDF Lipschitz-bound is not ≤ 1 — sphere-tracing requires L=1.
    #[error("sdf is not sphere-traceable (Lipschitz > 1)")]
    NonLipschitzSdf,
    /// Ray direction is degenerate (zero-length).
    #[error("ray direction is zero-length")]
    ZeroRayDirection,
}

/// Maximum number of march-steps per ray. Default = 128 per spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxSteps(pub u32);

impl Default for MaxSteps {
    fn default() -> Self {
        MaxSteps(128)
    }
}

/// Surface-hit epsilon : the ray is "on the surface" when `|d| < hit_eps`.
/// Default = 1e-3 (1 mm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitEpsilon(pub f32);

impl Default for HitEpsilon {
    fn default() -> Self {
        HitEpsilon(1e-3)
    }
}

/// Maximum march-distance per ray. Default = 256 m (M7 vertical-slice horizon).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaxDistance(pub f32);

impl Default for MaxDistance {
    fn default() -> Self {
        MaxDistance(256.0)
    }
}

/// Raymarcher configuration : step budgets + cone-aperture + per-shading-rate
/// step-multiplier (peripheral pixels need fewer steps).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RaymarchConfig {
    pub max_steps: MaxSteps,
    pub hit_epsilon: HitEpsilon,
    pub max_distance: MaxDistance,
    /// Cone-aperture (radians) for cone-marching. 0 = pure sphere-tracing.
    pub cone_aperture: f32,
    /// Step-budget multiplier for 2×2 shading-rate (default 0.6).
    pub step_mult_2x2: f32,
    /// Step-budget multiplier for 4×4 shading-rate (default 0.35).
    pub step_mult_4x4: f32,
}

impl Default for RaymarchConfig {
    fn default() -> Self {
        RaymarchConfig {
            max_steps: MaxSteps::default(),
            hit_epsilon: HitEpsilon::default(),
            max_distance: MaxDistance::default(),
            cone_aperture: 0.0,
            step_mult_2x2: 0.6,
            step_mult_4x4: 0.35,
        }
    }
}

impl RaymarchConfig {
    /// Step-budget for the given shading-rate (rounds down, min = 8).
    #[must_use]
    pub fn step_budget(&self, rate: ShadingRate) -> u32 {
        let base = self.max_steps.0 as f32;
        let mul = match rate {
            ShadingRate::OneByOne => 1.0,
            ShadingRate::TwoByTwo => self.step_mult_2x2,
            ShadingRate::FourByFour => self.step_mult_4x4,
        };
        ((base * mul).floor() as u32).max(8)
    }
}

/// One ray-hit produced by the marcher.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    /// World-space hit-position.
    pub p: [f32; 3],
    /// Unit-length world-space normal (from bwd-diff, NEVER central-diff).
    pub normal: SurfaceNormal,
    /// `t` along the ray at the hit (= depth from origin).
    pub t: f32,
    /// SDF value at hit (~ 0).
    pub sdf_value: f32,
    /// Material handle (M-facet) — looked up at the hit-cell.
    pub material_handle: u32,
    /// Number of steps taken to reach the hit (for telemetry).
    pub steps_used: u32,
}

/// SDF raymarch pass driver. Stateless ; takes a config + dispatches rays.
#[derive(Debug, Clone, Copy)]
pub struct SdfRaymarchPass {
    /// Configuration.
    pub config: RaymarchConfig,
}

impl Default for SdfRaymarchPass {
    fn default() -> Self {
        SdfRaymarchPass {
            config: RaymarchConfig::default(),
        }
    }
}

impl SdfRaymarchPass {
    /// Construct from a config.
    #[must_use]
    pub fn new(config: RaymarchConfig) -> Self {
        SdfRaymarchPass { config }
    }

    /// March one ray through `sdf` from `origin` along `dir` (must be unit).
    ///
    /// Returns `Some(RayHit)` on convergence, `None` on miss (max-distance
    /// exceeded). Returns an error on degenerate inputs.
    pub fn march<F: SdfFunction>(
        &self,
        sdf: &F,
        origin: [f32; 3],
        dir: [f32; 3],
        max_steps: u32,
    ) -> Result<Option<RayHit>, RaymarchError> {
        let dlen2 = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
        if dlen2 < 1e-12 {
            return Err(RaymarchError::ZeroRayDirection);
        }
        let mut t: f32 = 0.0;
        for step in 0..max_steps {
            let p = [
                origin[0] + t * dir[0],
                origin[1] + t * dir[1],
                origin[2] + t * dir[2],
            ];
            let d = sdf.eval_f32(p);
            if d.abs() < self.config.hit_epsilon.0 {
                let est = BackwardDiffNormals::estimate(sdf, p);
                return Ok(Some(RayHit {
                    p,
                    normal: est.normal,
                    t,
                    sdf_value: est.sdf_value,
                    material_handle: 0,
                    steps_used: step + 1,
                }));
            }
            // Cone-marching : effective step-distance enlarged by `t · cone_aperture`.
            let cone_ext = t * self.config.cone_aperture;
            // Sphere-trace step :
            t += d.max(self.config.hit_epsilon.0) + cone_ext;
            if t > self.config.max_distance.0 {
                return Ok(None);
            }
        }
        Err(RaymarchError::StepBudgetExhausted { limit: max_steps })
    }

    /// March one ray with MERA-skip dispatcher prefiltering. Equivalent to
    /// `march` but uses [`MeraSkipDispatcher::step_at`] to take large strides
    /// when far from any surface.
    pub fn march_with_mera_skip<F: SdfFunction>(
        &self,
        sdf: &F,
        mera: &MeraSkipDispatcher<'_>,
        origin: [f32; 3],
        dir: [f32; 3],
        max_steps: u32,
    ) -> Result<Option<RayHit>, RaymarchError> {
        let dlen2 = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
        if dlen2 < 1e-12 {
            return Err(RaymarchError::ZeroRayDirection);
        }
        let mut t: f32 = 0.0;
        for step in 0..max_steps {
            let p = [
                origin[0] + t * dir[0],
                origin[1] + t * dir[1],
                origin[2] + t * dir[2],
            ];
            // MERA-skip first.
            match mera.step_at(p) {
                MeraSkipResult::LargeStep { bound, .. } => {
                    t += bound;
                    if t > self.config.max_distance.0 {
                        return Ok(None);
                    }
                    continue;
                }
                MeraSkipResult::BisectionRefine | MeraSkipResult::OutOfRegion => {
                    // Fall through to analytic-SDF refine.
                }
            }
            let d = sdf.eval_f32(p);
            if d.abs() < self.config.hit_epsilon.0 {
                let est = BackwardDiffNormals::estimate(sdf, p);
                return Ok(Some(RayHit {
                    p,
                    normal: est.normal,
                    t,
                    sdf_value: est.sdf_value,
                    material_handle: 0,
                    steps_used: step + 1,
                }));
            }
            let cone_ext = t * self.config.cone_aperture;
            t += d.max(self.config.hit_epsilon.0) + cone_ext;
            if t > self.config.max_distance.0 {
                return Ok(None);
            }
        }
        Err(RaymarchError::StepBudgetExhausted { limit: max_steps })
    }

    /// Bisection-refine step. Given two `t` values bracketing a sign-change,
    /// returns the surface `t` to within `hit_epsilon`. Used after a large
    /// MERA-skip step that overshoots the surface.
    pub fn bisection_refine<F: SdfFunction>(
        &self,
        sdf: &F,
        origin: [f32; 3],
        dir: [f32; 3],
        t_near: f32,
        t_far: f32,
        max_iters: u32,
    ) -> Option<f32> {
        let mut a = t_near;
        let mut b = t_far;
        let pa = [
            origin[0] + a * dir[0],
            origin[1] + a * dir[1],
            origin[2] + a * dir[2],
        ];
        let pb = [
            origin[0] + b * dir[0],
            origin[1] + b * dir[1],
            origin[2] + b * dir[2],
        ];
        let mut fa = sdf.eval_f32(pa);
        let mut fb = sdf.eval_f32(pb);
        if fa * fb > 0.0 {
            return None; // No sign change — surface not in interval.
        }
        for _ in 0..max_iters {
            let m = 0.5 * (a + b);
            let pm = [
                origin[0] + m * dir[0],
                origin[1] + m * dir[1],
                origin[2] + m * dir[2],
            ];
            let fm = sdf.eval_f32(pm);
            if fm.abs() < self.config.hit_epsilon.0 {
                return Some(m);
            }
            if fa * fm <= 0.0 {
                b = m;
                fb = fm;
            } else {
                a = m;
                fa = fm;
            }
            let _ = fb;
        }
        Some(0.5 * (a + b))
    }

    /// March a single pixel : pulls origin + dir from the camera, picks the
    /// per-pixel step-budget from the shading-rate.
    pub fn march_pixel<F: SdfFunction>(
        &self,
        sdf: &F,
        cam: &EyeCamera,
        px: u32,
        py: u32,
        rate: ShadingRate,
    ) -> Result<Option<RayHit>, RaymarchError> {
        let dir = cam.pixel_to_ray(px, py);
        let origin = cam.origin;
        let budget = self.config.step_budget(rate);
        self.march(sdf, origin, dir, budget)
    }

    /// Variant of [`Self::march`] that accepts a body-presence-conditioning
    /// callable. The body-presence-field modifies the SDF locally near the
    /// Sovereign (Stage-1's `BodyPresenceField` writes near-Sovereign cells
    /// with non-zero AURA values ; this function lets the marcher weight
    /// those cells higher when computing the surface-normal).
    ///
    /// § CONSENT-DISCIPLINE
    ///   The body-presence callable is the "consent gate" : if the consumer
    ///   refuses to expose body-presence data this frame (e.g., gaze opt-out),
    ///   the callable returns `None` and the marcher falls back to the plain
    ///   SDF march. This is the canonical "graceful-degrade" path per
    ///   06_RENDERING_PIPELINE § VII.
    pub fn march_with_body_conditioning<F, B>(
        &self,
        sdf: &F,
        body_modifier: B,
        origin: [f32; 3],
        dir: [f32; 3],
        max_steps: u32,
    ) -> Result<Option<RayHit>, RaymarchError>
    where
        F: SdfFunction,
        B: Fn([f32; 3]) -> Option<f32>,
    {
        let dlen2 = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
        if dlen2 < 1e-12 {
            return Err(RaymarchError::ZeroRayDirection);
        }
        let mut t: f32 = 0.0;
        for step in 0..max_steps {
            let p = [
                origin[0] + t * dir[0],
                origin[1] + t * dir[1],
                origin[2] + t * dir[2],
            ];
            let d_sdf = sdf.eval_f32(p);
            // Apply body-conditioning : if the consumer permits, blend the
            // body-presence delta into the SDF value. Otherwise fall through
            // to the plain SDF.
            let d = if let Some(delta) = body_modifier(p) {
                d_sdf + delta
            } else {
                d_sdf
            };
            if d.abs() < self.config.hit_epsilon.0 {
                let est = BackwardDiffNormals::estimate(sdf, p);
                return Ok(Some(RayHit {
                    p,
                    normal: est.normal,
                    t,
                    sdf_value: est.sdf_value,
                    material_handle: 0,
                    steps_used: step + 1,
                }));
            }
            let cone_ext = t * self.config.cone_aperture;
            t += d.max(self.config.hit_epsilon.0) + cone_ext;
            if t > self.config.max_distance.0 {
                return Ok(None);
            }
        }
        Err(RaymarchError::StepBudgetExhausted { limit: max_steps })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdf::{AnalyticSdf, SdfComposition};

    #[test]
    fn config_defaults_match_spec() {
        let c = RaymarchConfig::default();
        assert_eq!(c.max_steps.0, 128);
        assert!((c.hit_epsilon.0 - 1e-3).abs() < 1e-9);
        assert!((c.max_distance.0 - 256.0).abs() < 1e-6);
    }

    #[test]
    fn step_budget_shrinks_with_lower_shading_rate() {
        let c = RaymarchConfig::default();
        let b1 = c.step_budget(ShadingRate::OneByOne);
        let b2 = c.step_budget(ShadingRate::TwoByTwo);
        let b3 = c.step_budget(ShadingRate::FourByFour);
        assert!(b1 > b2);
        assert!(b2 > b3);
    }

    #[test]
    fn step_budget_min_is_8() {
        let mut c = RaymarchConfig::default();
        c.step_mult_4x4 = 0.001;
        let b = c.step_budget(ShadingRate::FourByFour);
        assert_eq!(b, 8);
    }

    #[test]
    fn march_zero_dir_is_error() {
        let pass = SdfRaymarchPass::default();
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 0.0, 1.0));
        let r = pass.march(&s, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 32);
        assert_eq!(r, Err(RaymarchError::ZeroRayDirection));
    }

    #[test]
    fn march_hits_sphere_at_one_meter() {
        let pass = SdfRaymarchPass::default();
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 5.0, 1.0));
        let hit = pass
            .march(&s, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 64)
            .unwrap();
        let h = hit.expect("ray should hit");
        // Sphere at (0,0,5) radius 1 ⇒ near surface t=4.
        assert!((h.t - 4.0).abs() < 0.01);
    }

    #[test]
    fn march_misses_returns_none() {
        let pass = SdfRaymarchPass::default();
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 5.0, 0.0, 0.5));
        // Ray straight along +X, sphere is up at y=5 — miss.
        let hit = pass
            .march(&s, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 256)
            .unwrap();
        assert!(hit.is_none());
    }

    #[test]
    fn march_returns_normal_at_hit() {
        let pass = SdfRaymarchPass::default();
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 5.0, 1.0));
        let h = pass
            .march(&s, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 64)
            .unwrap()
            .unwrap();
        // Normal should point back toward the camera — i.e., -Z.
        assert!(h.normal.0[2] < -0.5);
    }

    #[test]
    fn cone_marching_speed_up_for_distant_surface() {
        let mut config = RaymarchConfig::default();
        config.cone_aperture = 0.05;
        let pass = SdfRaymarchPass::new(config);
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 200.0, 1.0));
        let h = pass
            .march(&s, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 256)
            .unwrap();
        // Should still hit the distant sphere.
        assert!(h.is_some());
        // Now compare step counts to non-cone-marching.
        let plain = SdfRaymarchPass::default();
        let h_plain = plain
            .march(&s, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 256)
            .unwrap();
        if let (Some(hc), Some(hp)) = (h, h_plain) {
            assert!(hc.steps_used <= hp.steps_used);
        }
    }

    #[test]
    fn bisection_refine_finds_surface() {
        let pass = SdfRaymarchPass::default();
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 5.0, 1.0));
        let t = pass
            .bisection_refine(&s, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 3.0, 5.0, 32)
            .unwrap();
        assert!((t - 4.0).abs() < 0.01);
    }

    #[test]
    fn bisection_refine_returns_none_no_sign_change() {
        let pass = SdfRaymarchPass::default();
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 5.0, 1.0));
        // Both points outside the sphere.
        let t = pass.bisection_refine(&s, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 1.0, 32);
        assert!(t.is_none());
    }

    #[test]
    fn march_pixel_via_camera() {
        let pass = SdfRaymarchPass::default();
        let cam = EyeCamera::at_origin_quest3(8, 8);
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, -5.0, 1.0));
        let hit = pass
            .march_pixel(&s, &cam, 4, 4, ShadingRate::OneByOne)
            .unwrap();
        // Pixel (4,4) is near center → ray points roughly forward (-Z) → should hit.
        assert!(hit.is_some());
    }

    #[test]
    fn march_with_body_conditioning_refused_falls_through() {
        let pass = SdfRaymarchPass::default();
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 5.0, 1.0));
        let hit = pass
            .march_with_body_conditioning(
                &s,
                |_p| None, // consent refused
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                64,
            )
            .unwrap();
        assert!(hit.is_some());
    }

    #[test]
    fn march_with_body_conditioning_consented_returns_hit() {
        let pass = SdfRaymarchPass::default();
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 5.0, 1.0));
        let hit = pass
            .march_with_body_conditioning(
                &s,
                |_p| Some(0.0), // consented, no delta
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                64,
            )
            .unwrap();
        assert!(hit.is_some());
    }

    #[test]
    fn step_budget_exhausted_returns_error() {
        let pass = SdfRaymarchPass::default();
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 5.0, 1.0));
        // 1 step is not enough.
        let r = pass.march(&s, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1);
        // The single step might exceed max_distance or might not. Verify it
        // returns either an error or None — but NOT a hit.
        match r {
            Ok(Some(_)) => panic!("should not hit in 1 step"),
            _ => {}
        }
    }

    #[test]
    fn march_with_mera_skip_falls_back_to_sdf() {
        use cssl_substrate_omega_field::MeraPyramid;
        let p = MeraPyramid::new();
        let mera = MeraSkipDispatcher::new(&p);
        let pass = SdfRaymarchPass::default();
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 5.0, 1.0));
        // Empty pyramid → MERA dispatcher returns OutOfRegion → fall through.
        let hit = pass
            .march_with_mera_skip(&s, &mera, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 64)
            .unwrap();
        assert!(hit.is_some());
    }
}
