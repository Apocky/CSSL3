//! § MirrorSurface — SDF + KanMaterial mirror-surface detector
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per spec § Stage-9.compute step-1 :
//!     ```text
//!     1. detect mirror/portal surfaces :
//!        - M-facet axis-7 (mirrorness) > threshold
//!        - SDF surface-normal stable + curvature-low
//!        - portal-throat-marker (Axiom-1 throat-flag)
//!     ```
//!
//!   This module implements the SDF + KanMaterial probe that determines
//!   whether a surface-hit is a mirror (and therefore subject to recursive
//!   bouncing). It is intentionally agnostic to the actual SDF storage
//!   format — it consumes a `MirrorRaymarchProbe::probe(...) -> ProbeResult`
//!   that yields the surface position, the SDF gradient (from which the
//!   tangent-plane is built), and the M-facet handle (which dereferences
//!   into a `KanMaterial` whose `mirrorness` channel decides reflectivity).
//!
//! § INTEGRATION
//!   Stage-5 SDF raymarch (T11-D116) provides the underlying ray-march that
//!   fills `Ω.next.SDF`. This module's `MirrorSurface::is_mirror` is called
//!   AT a known surface hit ; it does not run its own raymarch. The detector
//!   examines the M-facet → `KanMaterial` resolution to extract the
//!   mirrorness scalar and decides via `MirrorDetectionThreshold::accept`.

use cssl_substrate_kan::{KanMaterial, KanMaterialKind};
use cssl_substrate_projections::vec::Vec3;

/// § Which channel of `KanMaterial` carries the mirrorness scalar.
///
///   The spec § V.6.c says "Ω.M-facet axis-13/14 (roughness/metallic)
///   drives mirror-quality ⊗ KAN-derived" and the eye-of-creature spec
///   says "M-facet axis-7 = mirrorness-of-cornea". Different surfaces use
///   different channels, so we expose the choice as an enum that the
///   caller selects per-surface-class.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MirrornessChannel {
    /// § Axis-7 of the embedding : creature-cornea mirrorness. The cornea
    ///   is `KanMaterialKind::CreatureMorphology` and the mirrorness lives
    ///   in `embedding[7]`.
    CorneaAxis7,
    /// § Axes-13/14 of the embedding : roughness/metallic for mirror-quality
    ///   surfaces (planar mirrors, water, polished metal). The mirrorness
    ///   is derived as `metallic * (1 - roughness)`.
    RoughnessMetallic13_14,
    /// § A direct caller-supplied scalar, for testing and for surfaces that
    ///   compute their mirrorness via a custom path. The variant payload IS
    ///   the mirrorness in `[0, 1]`.
    Direct(f32),
}

impl MirrornessChannel {
    /// § Extract the mirrorness scalar from the given KanMaterial.
    ///
    ///   Returns `0.0` for material kinds that do not carry mirrorness in
    ///   the requested channel. Always returns a value in `[0, 1]`.
    #[must_use]
    pub fn extract(self, material: &KanMaterial) -> f32 {
        match self {
            Self::CorneaAxis7 => {
                // § Cornea mirrorness lives in embedding axis-7 of a
                //   CreatureMorphology material. For other kinds, the
                //   axis carries different semantics, so we return 0.
                if matches!(material.kind, KanMaterialKind::CreatureMorphology) {
                    material.embedding[7].clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }
            Self::RoughnessMetallic13_14 => {
                // § Per spec, roughness lives at axis-13, metallic at axis-14.
                //   Mirrorness = metallic * (1 - roughness), bounded to [0,1].
                let roughness = material.embedding[13].clamp(0.0, 1.0);
                let metallic = material.embedding[14].clamp(0.0, 1.0);
                let m = metallic * (1.0 - roughness);
                m.clamp(0.0, 1.0)
            }
            Self::Direct(m) => m.clamp(0.0, 1.0),
        }
    }
}

/// § Acceptance threshold for the mirror-detection step.
///
///   Per spec § Stage-9.compute step-1, the detector does THREE checks :
///     1. M-facet mirrorness > `mirrorness_min`
///     2. SDF gradient magnitude > `gradient_min` (well-defined normal)
///     3. SDF curvature (estimated via finite-diff of normals) <
///        `curvature_max` — surface is locally planar enough to support
///        a stable mirror-tangent-plane. Curvature is supplied separately
///        by the probe.
#[derive(Debug, Clone, Copy)]
pub struct MirrorDetectionThreshold {
    /// § Minimum mirrorness for the surface to qualify as a mirror.
    ///   Default = `0.05` per spec § V.6.c (very low-mirrorness surfaces
    ///   like rough water still bounce, just heavily attenuated by KAN).
    pub mirrorness_min: f32,
    /// § Minimum SDF gradient magnitude for the tangent-plane to be valid.
    ///   Default = `1e-3` ; below this the gradient is degenerate.
    pub gradient_min: f32,
    /// § Maximum curvature (1/radius) for the surface to be locally
    ///   planar enough. Default = `2.0` (i.e. radius >= 0.5) ; tighter
    ///   curvature is treated as "lens-like" not "mirror-like".
    pub curvature_max: f32,
}

impl Default for MirrorDetectionThreshold {
    fn default() -> Self {
        Self::SUBSTRATE
    }
}

impl MirrorDetectionThreshold {
    /// § Substrate-canonical thresholds. These are the values the
    ///   render-graph orchestrator uses ; tests and benchmarks may
    ///   override individual fields.
    pub const SUBSTRATE: Self = Self {
        mirrorness_min: 0.05,
        gradient_min: 1e-3,
        curvature_max: 2.0,
    };

    /// § Strict thresholds for "true mirror" detection — water surfaces
    ///   below this threshold do NOT recurse, only true mirrors do.
    ///   Used in unit tests + by the orchestrator's "mirror-only" preset.
    pub const STRICT: Self = Self {
        mirrorness_min: 0.6,
        gradient_min: 1e-2,
        curvature_max: 0.5,
    };

    /// § Decide whether the given (mirrorness, gradient_magnitude,
    ///   curvature) tuple qualifies as a mirror surface.
    #[must_use]
    pub fn accept(self, mirrorness: f32, gradient_magnitude: f32, curvature: f32) -> bool {
        mirrorness > self.mirrorness_min
            && gradient_magnitude > self.gradient_min
            && curvature < self.curvature_max
    }
}

/// § Detected mirror surface ready for recursive ray-cast.
///
///   The structure carries everything the recursion needs to compute the
///   reflected camera + spawn a sub-frame ray :
///     - `position` : the world-space hit-point on the mirror surface
///     - `normal` : the unit-normalized SDF gradient (mirror-tangent-plane normal)
///     - `mirrorness` : the `[0,1]` reflectivity scalar
///     - `roughness` : `1 - mirrorness` for the KAN-confidence input
///     - `region_id` : the region that owns the mirror (for anti-surveillance check)
#[derive(Debug, Clone, Copy)]
pub struct MirrorSurface {
    /// § World-space hit position on the mirror.
    pub position: Vec3,
    /// § Unit normal to the mirror tangent-plane.
    pub normal: Vec3,
    /// § Mirrorness in [0, 1].
    pub mirrorness: f32,
    /// § Roughness in [0, 1] (= 1 - mirrorness for clean mirrors,
    ///   independently sampled for water-like surfaces).
    pub roughness: f32,
    /// § Region that owns the mirror — used for surveillance gate.
    pub region_id: super::region::RegionId,
    /// § Curvature of the SDF at the hit (1/radius). Stored so the cost-
    ///   model can budget more aggressively on highly-curved surfaces.
    pub curvature: f32,
}

impl MirrorSurface {
    /// § Build a `MirrorSurface` from a probe-result + a `KanMaterial`-
    ///   derived mirrorness. Returns `None` if the surface fails the
    ///   threshold check.
    #[must_use]
    pub fn try_from_probe(
        position: Vec3,
        gradient: Vec3,
        curvature: f32,
        material: &KanMaterial,
        channel: MirrornessChannel,
        region_id: super::region::RegionId,
        threshold: MirrorDetectionThreshold,
    ) -> Option<Self> {
        let gradient_magnitude = gradient.length();
        let mirrorness = channel.extract(material);
        if !threshold.accept(mirrorness, gradient_magnitude, curvature) {
            return None;
        }
        // § Normalize the gradient to produce the mirror-tangent-plane
        //   normal. We've already checked gradient_magnitude > epsilon.
        let normal = gradient.normalize();
        if normal == Vec3::ZERO {
            // § Defensive : in case Vec3::normalize fell back to ZERO due
            //   to FP-drift below epsilon. Preserve totality.
            return None;
        }
        let roughness = (1.0 - mirrorness).clamp(0.0, 1.0);
        Some(Self {
            position,
            normal,
            mirrorness,
            roughness,
            region_id,
            curvature,
        })
    }

    /// § Predicate : true iff this is a "true mirror" (mirrorness ≥ 0.6).
    ///   Useful for the cost-model's "deep recursion only on true mirrors"
    ///   preset.
    #[must_use]
    pub fn is_true_mirror(&self) -> bool {
        self.mirrorness >= 0.6
    }

    /// § Compute the reflected ray direction given an incoming view ray.
    ///   This is the PGA Plane sandwich-product specialized for unit normal :
    ///
    ///   ```text
    ///     r = d - 2 * dot(d, n) * n
    ///   ```
    ///
    ///   (which agrees with PGA `r = (-n) * d * n` collapsed to vector form).
    #[must_use]
    pub fn reflect_direction(&self, view_dir: Vec3) -> Vec3 {
        let two_d_dot_n = 2.0 * view_dir.dot(self.normal);
        Vec3::new(
            view_dir.x - two_d_dot_n * self.normal.x,
            view_dir.y - two_d_dot_n * self.normal.y,
            view_dir.z - two_d_dot_n * self.normal.z,
        )
    }

    /// § Compute the reflected camera position. The camera position is
    ///   reflected through the mirror-tangent-plane (pass through the
    ///   plane and flip on the other side).
    ///
    ///   ```text
    ///     p' = p - 2 * dot(p - hit, n) * n
    ///   ```
    #[must_use]
    pub fn reflect_position(&self, world_position: Vec3) -> Vec3 {
        let delta = world_position - self.position;
        let two_d_dot_n = 2.0 * delta.dot(self.normal);
        Vec3::new(
            world_position.x - two_d_dot_n * self.normal.x,
            world_position.y - two_d_dot_n * self.normal.y,
            world_position.z - two_d_dot_n * self.normal.z,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_substrate_kan::EMBEDDING_DIM;

    fn make_material_with_axis(kind: KanMaterialKind, axis: usize, value: f32) -> KanMaterial {
        let mut emb = [0.0_f32; EMBEDDING_DIM];
        if axis < EMBEDDING_DIM {
            emb[axis] = value;
        }
        match kind {
            KanMaterialKind::CreatureMorphology => KanMaterial::creature_morphology(emb),
            KanMaterialKind::SingleBandBrdf => KanMaterial::single_band_brdf(emb),
            KanMaterialKind::SpectralBrdf { .. } => KanMaterial::spectral_brdf::<8>(emb),
            KanMaterialKind::PhysicsImpedance => KanMaterial::physics_impedance(emb),
        }
    }

    /// § CorneaAxis7 reads embedding[7] for CreatureMorphology materials.
    #[test]
    fn cornea_axis_7_extracts_correctly() {
        let m = make_material_with_axis(KanMaterialKind::CreatureMorphology, 7, 0.85);
        assert!((MirrornessChannel::CorneaAxis7.extract(&m) - 0.85).abs() < 1e-5);
    }

    /// § CorneaAxis7 returns 0 for non-creature materials.
    #[test]
    fn cornea_axis_7_zero_for_non_creature() {
        let m = make_material_with_axis(KanMaterialKind::SingleBandBrdf, 7, 0.85);
        assert_eq!(MirrornessChannel::CorneaAxis7.extract(&m), 0.0);
    }

    /// § RoughnessMetallic13_14 computes metallic * (1 - roughness).
    #[test]
    fn roughness_metallic_combine() {
        let mut emb = [0.0_f32; EMBEDDING_DIM];
        emb[13] = 0.2; // roughness
        emb[14] = 0.9; // metallic
        let m = KanMaterial::single_band_brdf(emb);
        let mn = MirrornessChannel::RoughnessMetallic13_14.extract(&m);
        // expected = 0.9 * (1 - 0.2) = 0.72
        assert!((mn - 0.72).abs() < 1e-5);
    }

    /// § Direct passthrough is clamped to [0, 1].
    #[test]
    fn direct_clamp() {
        let m = KanMaterial::single_band_brdf([0.0; EMBEDDING_DIM]);
        assert!((MirrornessChannel::Direct(0.5).extract(&m) - 0.5).abs() < 1e-6);
        assert_eq!(MirrornessChannel::Direct(1.5).extract(&m), 1.0);
        assert_eq!(MirrornessChannel::Direct(-0.5).extract(&m), 0.0);
    }

    /// § The SUBSTRATE threshold accepts a true mirror.
    #[test]
    fn substrate_accepts_true_mirror() {
        let t = MirrorDetectionThreshold::SUBSTRATE;
        assert!(t.accept(0.9, 1.0, 0.1));
    }

    /// § The SUBSTRATE threshold rejects below mirrorness_min.
    #[test]
    fn substrate_rejects_low_mirrorness() {
        let t = MirrorDetectionThreshold::SUBSTRATE;
        assert!(!t.accept(0.01, 1.0, 0.1));
    }

    /// § The SUBSTRATE threshold rejects degenerate gradient.
    #[test]
    fn substrate_rejects_degenerate_gradient() {
        let t = MirrorDetectionThreshold::SUBSTRATE;
        assert!(!t.accept(0.9, 1e-6, 0.1));
    }

    /// § The SUBSTRATE threshold rejects high curvature (lens-like).
    #[test]
    fn substrate_rejects_high_curvature() {
        let t = MirrorDetectionThreshold::SUBSTRATE;
        assert!(!t.accept(0.9, 1.0, 5.0));
    }

    /// § STRICT only accepts mirrorness >= 0.6.
    #[test]
    fn strict_rejects_water() {
        let t = MirrorDetectionThreshold::STRICT;
        assert!(!t.accept(0.3, 1.0, 0.1));
        assert!(t.accept(0.7, 1.0, 0.1));
    }

    /// § Reflection direction satisfies Snell's law (angle of incidence = angle of reflection)
    ///   for a planar mirror with normal +Y, ray going in +Y direction reflects to -Y.
    #[test]
    fn reflect_direction_planar_mirror() {
        let m = MirrorSurface {
            position: Vec3::ZERO,
            normal: Vec3::Y,
            mirrorness: 1.0,
            roughness: 0.0,
            region_id: super::super::region::RegionId(0),
            curvature: 0.0,
        };
        // Incoming ray heading +Y (into the floor mirror's normal) should
        // reflect to -Y (away from the floor).
        let r = m.reflect_direction(Vec3::Y);
        assert!((r.x).abs() < 1e-6);
        assert!((r.y - (-1.0)).abs() < 1e-6);
        assert!((r.z).abs() < 1e-6);
    }

    /// § Reflection across a 45-deg plane swaps direction components.
    #[test]
    fn reflect_direction_45_deg() {
        // § normal = (1,1,0)/sqrt(2)
        let n = Vec3::new(1.0, 1.0, 0.0).normalize();
        let m = MirrorSurface {
            position: Vec3::ZERO,
            normal: n,
            mirrorness: 1.0,
            roughness: 0.0,
            region_id: super::super::region::RegionId(0),
            curvature: 0.0,
        };
        // § Ray going in +X direction reflects through (1,1,0)/sqrt(2)
        //   plane → comes out in -Y direction.
        let r = m.reflect_direction(Vec3::X);
        assert!((r.x).abs() < 1e-5);
        assert!((r.y - (-1.0)).abs() < 1e-5);
        assert!((r.z).abs() < 1e-5);
    }

    /// § reflect_position : a point above a +Y mirror at origin maps to its
    ///   negative-Y mirror image.
    #[test]
    fn reflect_position_planar_mirror() {
        let m = MirrorSurface {
            position: Vec3::ZERO,
            normal: Vec3::Y,
            mirrorness: 1.0,
            roughness: 0.0,
            region_id: super::super::region::RegionId(0),
            curvature: 0.0,
        };
        let p = Vec3::new(0.0, 3.0, 0.0);
        let p_ref = m.reflect_position(p);
        assert!((p_ref.x).abs() < 1e-6);
        assert!((p_ref.y - (-3.0)).abs() < 1e-6);
        assert!((p_ref.z).abs() < 1e-6);
    }

    /// § Building a MirrorSurface from a probe with sub-threshold mirrorness
    ///   returns None.
    #[test]
    fn try_from_probe_rejects_subthreshold() {
        let m = make_material_with_axis(KanMaterialKind::CreatureMorphology, 7, 0.0);
        let r = MirrorSurface::try_from_probe(
            Vec3::ZERO,
            Vec3::Y,
            0.1,
            &m,
            MirrornessChannel::CorneaAxis7,
            super::super::region::RegionId(0),
            MirrorDetectionThreshold::SUBSTRATE,
        );
        assert!(r.is_none());
    }

    /// § Building a MirrorSurface from a probe with degenerate gradient
    ///   returns None.
    #[test]
    fn try_from_probe_rejects_degenerate_gradient() {
        let m = make_material_with_axis(KanMaterialKind::CreatureMorphology, 7, 0.9);
        let r = MirrorSurface::try_from_probe(
            Vec3::ZERO,
            Vec3::new(1e-9, 0.0, 0.0),
            0.1,
            &m,
            MirrornessChannel::CorneaAxis7,
            super::super::region::RegionId(0),
            MirrorDetectionThreshold::SUBSTRATE,
        );
        assert!(r.is_none());
    }

    /// § Building a MirrorSurface from a passing probe returns Some with
    ///   correct fields.
    #[test]
    fn try_from_probe_accepts_clean_mirror() {
        let m = make_material_with_axis(KanMaterialKind::CreatureMorphology, 7, 0.9);
        let r = MirrorSurface::try_from_probe(
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::Y,
            0.05,
            &m,
            MirrornessChannel::CorneaAxis7,
            super::super::region::RegionId(7),
            MirrorDetectionThreshold::SUBSTRATE,
        )
        .unwrap();
        assert!((r.mirrorness - 0.9).abs() < 1e-5);
        assert!((r.roughness - 0.1).abs() < 1e-5);
        assert_eq!(r.region_id.0, 7);
        assert_eq!(r.normal, Vec3::Y);
        assert!(r.is_true_mirror());
    }

    /// § Roughness defaults to 1 - mirrorness ; a very rough mirror has
    ///   high roughness and low mirrorness.
    #[test]
    fn roughness_complements_mirrorness() {
        let m = make_material_with_axis(KanMaterialKind::CreatureMorphology, 7, 0.3);
        let r = MirrorSurface::try_from_probe(
            Vec3::ZERO,
            Vec3::Y,
            0.1,
            &m,
            MirrornessChannel::CorneaAxis7,
            super::super::region::RegionId(0),
            MirrorDetectionThreshold::SUBSTRATE,
        )
        .unwrap();
        assert!((r.mirrorness - 0.3).abs() < 1e-5);
        assert!((r.roughness - 0.7).abs() < 1e-5);
        assert!(!r.is_true_mirror());
    }
}
