//! § MirrorRaymarchProbe — Stage-5-replay shim for recursive bounces
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage-9 needs to RE-RAYMARCH from the reflected camera at each
//!   recursion bounce. The actual SDF-raymarch implementation lives in
//!   the sibling slice T11-D116 / W4-02 (`cssl-render-v2`'s Stage-5).
//!   This slice (T11-D122) defines the **trait interface**
//!   [`MirrorRaymarchProbe`] that the orchestrator-slice T11-D125 wires
//!   to the Stage-5 walker.
//!
//!   Pulling the dependency through a trait keeps this slice's compile-
//!   surface tight (no transitive Stage-5 deps) and lets us provide a
//!   trivial test-double [`ConstantProbe`] for unit tests.
//!
//! § CONTRACT
//!   Given (start_origin, direction), the probe returns either :
//!     - `Ok(ProbeResult)` with the hit information (position, normal,
//!       material handle, region, atmosphere along the ray segment)
//!     - `Err(())` if the ray misses (escapes the world bounds)
//!
//!   The probe is responsible for honoring `Σ-mask` consent at each cell
//!   it visits ; this is done through the omega-field's standard
//!   `OmegaField::sample` API rather than re-implemented here.

use cssl_substrate_kan::KanMaterial;
use cssl_substrate_projections::vec::Vec3;

/// § The probe-trait : a thin abstraction over Stage-5 SDF raymarch.
pub trait MirrorRaymarchProbe {
    /// § Cast a ray from `origin` in direction `direction`. Returns
    ///   `ProbeResult::Hit` with surface info or `ProbeResult::Miss`.
    fn probe(&self, origin: Vec3, direction: Vec3) -> ProbeResult;
}

/// § Hit-information returned by a probe. Mirrors what the Stage-5
///   walker carries back per-hit ; the only fields Stage-9 reads are
///   `position` + `gradient` + `material` + `region_id` + `atmosphere`.
pub enum ProbeResult {
    /// § A surface was hit at the given parameters.
    Hit {
        /// § World-space hit position.
        position: Vec3,
        /// § SDF gradient at the hit (NOT necessarily unit-length — the
        ///   mirror-detector normalizes if needed).
        gradient: Vec3,
        /// § Curvature of the SDF at the hit (1/radius).
        curvature: f32,
        /// § Material at the hit (resolved from the FieldCell's M-facet).
        material: KanMaterial,
        /// § Region that owns the hit cell — used by the anti-surveillance
        ///   gate.
        region_id: super::region::RegionId,
        /// § Atmospheric extinction along the ray segment, [0, 1]. The
        ///   probe accumulates absorption along the ray ; this scalar is
        ///   the integrated extinction at hit-time.
        atmosphere: f32,
    },
    /// § The ray escaped the world bounds without hitting any surface.
    Miss,
}

impl ProbeResult {
    /// § Predicate : true iff this is a Hit variant.
    #[must_use]
    pub fn is_hit(&self) -> bool {
        matches!(self, Self::Hit { .. })
    }
}

/// § A test-only probe that always returns the same hit (or always
///   misses). Used in unit tests where we want to drive the Stage-9 logic
///   without needing a full Stage-5 walker.
#[derive(Debug, Clone)]
pub struct ConstantProbe {
    /// § The fixed hit returned. `None` means "always miss".
    pub fixed_hit: Option<FixedHit>,
}

/// § Plain-old-data variant of `ProbeResult::Hit` that's `Clone`.
#[derive(Debug, Clone)]
pub struct FixedHit {
    /// § World-space hit position.
    pub position: Vec3,
    /// § SDF gradient at the hit.
    pub gradient: Vec3,
    /// § Curvature at the hit.
    pub curvature: f32,
    /// § Material — `KanMaterial` is `Clone` so we can copy on each
    ///   probe call.
    pub material: KanMaterial,
    /// § Region that owns the hit cell.
    pub region_id: super::region::RegionId,
    /// § Atmospheric extinction.
    pub atmosphere: f32,
}

impl ConstantProbe {
    /// § Construct a probe that always returns the given fixed hit.
    #[must_use]
    pub fn always_hit(hit: FixedHit) -> Self {
        Self {
            fixed_hit: Some(hit),
        }
    }

    /// § Construct a probe that always misses.
    #[must_use]
    pub fn always_miss() -> Self {
        Self { fixed_hit: None }
    }
}

impl MirrorRaymarchProbe for ConstantProbe {
    fn probe(&self, _origin: Vec3, _direction: Vec3) -> ProbeResult {
        match &self.fixed_hit {
            Some(h) => ProbeResult::Hit {
                position: h.position,
                gradient: h.gradient,
                curvature: h.curvature,
                material: h.material.clone(),
                region_id: h.region_id,
                atmosphere: h.atmosphere,
            },
            None => ProbeResult::Miss,
        }
    }
}

/// § A test-probe that returns hits at a sequence of positions, advancing
///   one entry per `probe()` call. Useful for testing recursion-depth
///   behaviour where each bounce should hit a different mirror.
#[derive(Debug)]
pub struct ScriptedProbe {
    /// § The sequence of hits to return. After the sequence is exhausted,
    ///   subsequent probes return Miss.
    hits: Vec<FixedHit>,
    /// § Cursor into `hits`. Wrapped in `RefCell` because the
    ///   `MirrorRaymarchProbe::probe` method takes `&self`.
    cursor: core::cell::Cell<usize>,
}

impl ScriptedProbe {
    /// § Construct from a sequence of hits.
    #[must_use]
    pub fn new(hits: Vec<FixedHit>) -> Self {
        Self {
            hits,
            cursor: core::cell::Cell::new(0),
        }
    }
}

impl MirrorRaymarchProbe for ScriptedProbe {
    fn probe(&self, _origin: Vec3, _direction: Vec3) -> ProbeResult {
        let i = self.cursor.get();
        if i < self.hits.len() {
            self.cursor.set(i + 1);
            let h = &self.hits[i];
            ProbeResult::Hit {
                position: h.position,
                gradient: h.gradient,
                curvature: h.curvature,
                material: h.material.clone(),
                region_id: h.region_id,
                atmosphere: h.atmosphere,
            }
        } else {
            ProbeResult::Miss
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_substrate_kan::EMBEDDING_DIM;

    fn fixture_hit(mirrorness_at_axis_7: f32) -> FixedHit {
        let mut emb = [0.0_f32; EMBEDDING_DIM];
        emb[7] = mirrorness_at_axis_7;
        FixedHit {
            position: Vec3::ZERO,
            gradient: Vec3::Y,
            curvature: 0.1,
            material: KanMaterial::creature_morphology(emb),
            region_id: super::super::region::RegionId(0),
            atmosphere: 0.0,
        }
    }

    /// § ConstantProbe::always_hit returns the fixed hit on every call.
    #[test]
    fn constant_probe_always_hit() {
        let h = fixture_hit(0.9);
        let p = ConstantProbe::always_hit(h);
        let r1 = p.probe(Vec3::ZERO, Vec3::X);
        let r2 = p.probe(Vec3::Y, Vec3::Z);
        assert!(r1.is_hit());
        assert!(r2.is_hit());
    }

    /// § ConstantProbe::always_miss returns Miss every call.
    #[test]
    fn constant_probe_always_miss() {
        let p = ConstantProbe::always_miss();
        let r = p.probe(Vec3::ZERO, Vec3::X);
        assert!(!r.is_hit());
    }

    /// § ScriptedProbe returns the configured sequence of hits.
    #[test]
    fn scripted_probe_yields_in_order() {
        let h1 = fixture_hit(0.8);
        let h2 = fixture_hit(0.6);
        let p = ScriptedProbe::new(vec![h1, h2]);
        assert!(p.probe(Vec3::ZERO, Vec3::X).is_hit());
        assert!(p.probe(Vec3::ZERO, Vec3::X).is_hit());
        // exhausted
        assert!(!p.probe(Vec3::ZERO, Vec3::X).is_hit());
    }

    /// § ProbeResult::is_hit discriminates correctly.
    #[test]
    fn probe_result_is_hit_predicate() {
        let h = fixture_hit(0.7);
        let p = ConstantProbe::always_hit(h);
        assert!(p.probe(Vec3::ZERO, Vec3::X).is_hit());
        let m = ConstantProbe::always_miss();
        assert!(!m.probe(Vec3::ZERO, Vec3::X).is_hit());
    }
}
