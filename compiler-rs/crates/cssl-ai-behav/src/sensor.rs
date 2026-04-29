//! Sensor — perception primitives for NPCs (sight-cone + hearing-radius).
//!
//! § THESIS
//!   NPCs need a way to query "is this target perceivable to me from
//!   here?". Stage-0 supplies two canonical sensor kinds :
//!     - **SightCone** : an observer-position + facing + half-angle (FOV)
//!                       + range. Target visible iff target-direction is
//!                       within the cone AND distance ≤ range.
//!     - **HearingRadius** : an observer-position + radius. Target heard
//!                       iff distance ≤ radius.
//!
//!   Stage-0 does NOT model occlusion / line-of-sight raycasting — that's
//!   the broadphase-physics layer (deferred dep on `cssl-physics`).
//!   The sensor here returns a "the geometry permits perception" answer ;
//!   the brain's higher-level decision can layer occlusion atop.
//!
//! § DETERMINISM
//!   - All checks are pure floating-point arithmetic (dot-product, distance).
//!   - Bit-identical inputs ⇒ bit-identical outputs.
//!   - No internal RNG ; no clock reads.
//!
//! § PRIME_DIRECTIVE-ALIGNMENT (‼ load-bearing)
//!   - **§1 PROHIBITIONS § surveillance** : the sensor only takes
//!     `NpcId` targets ; Companion-archetype positions are NOT accessible
//!     here (they live in the `CompanionView` projection per spec).
//!     Consequently no NPC can use this primitive to surveil a sovereign.
//!   - The sensor APIs return `Result` so the audit layer can record
//!     the perception query — silent perception ≠ allowed.

use thiserror::Error;

use crate::navmesh::Point2;

/// Identifier for an NPC. Distinct type from `TriId` so the type-system
/// helps prevent accidentally feeding a triangle-id to a sensor.
///
/// § STAGE-0 DESIGN
///   Stage-0 uses `u32` for NpcId. The brain layer maintains a registry
///   that maps NpcId → Ω-tensor entity-id. Sensors don't directly touch
///   the Ω-tensor — they take pre-resolved positions as input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NpcId(pub u32);

/// Errors the Sensor surfaces.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum SensorError {
    /// FOV half-angle is out of the valid range [0, π].
    #[error("AIBEHAV0070 — sight-cone fov_half_rad {0} out of range [0, π]")]
    InvalidFov(f64),
    /// Range is negative (must be ≥ 0).
    #[error("AIBEHAV0071 — sensor range {0} must be ≥ 0")]
    NegativeRange(f64),
    /// Facing vector has zero length (cannot determine direction).
    #[error("AIBEHAV0072 — sight-cone facing vector has zero length")]
    ZeroFacing,
}

impl SensorError {
    /// Stable diagnostic code prefix.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidFov(_) => "AIBEHAV0070",
            Self::NegativeRange(_) => "AIBEHAV0071",
            Self::ZeroFacing => "AIBEHAV0072",
        }
    }
}

/// The kind of perception this sensor models.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SensorKind {
    /// 2D sight-cone : FOV angle (full-angle in radians) + range.
    /// Stage-0 takes the **full** angle (not half-angle) from the API
    /// for caller-readability ; internally we store the half-angle
    /// because the dot-product check uses cos(half).
    SightCone {
        /// Half of the cone's apex angle, in radians. Range [0, π].
        /// 0 = laser-line ; π = full sphere (always-visible).
        fov_half_rad: f64,
        /// Maximum sight distance.
        range: f64,
    },
    /// Hearing-radius : circular distance check.
    HearingRadius {
        /// Maximum hearing distance.
        range: f64,
    },
}

/// A perception sensor attached to an NPC.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sensor {
    /// What kind of perception this is.
    pub kind: SensorKind,
}

impl Sensor {
    /// Construct a sight-cone sensor. Validates `fov_full_rad` in [0, 2π]
    /// and `range >= 0`.
    pub fn sight_cone(fov_full_rad: f64, range: f64) -> Result<Self, SensorError> {
        if !(0.0..=2.0 * std::f64::consts::PI).contains(&fov_full_rad) {
            return Err(SensorError::InvalidFov(fov_full_rad));
        }
        if range < 0.0 {
            return Err(SensorError::NegativeRange(range));
        }
        Ok(Self {
            kind: SensorKind::SightCone {
                fov_half_rad: fov_full_rad / 2.0,
                range,
            },
        })
    }

    /// Construct a hearing-radius sensor.
    pub fn hearing_radius(range: f64) -> Result<Self, SensorError> {
        if range < 0.0 {
            return Err(SensorError::NegativeRange(range));
        }
        Ok(Self {
            kind: SensorKind::HearingRadius { range },
        })
    }

    /// Test whether an NPC at `target_pos` is sensed by this sensor at
    /// `observer_pos` (with `observer_facing` being a unit-or-near-unit
    /// 2D vector for sight-cones).
    ///
    /// § PRIME_DIRECTIVE NOTE
    ///   This entry-point takes `_target` as `NpcId` to prevent
    ///   surveilling sovereigns ; Companion positions are not in
    ///   NpcId-space.
    pub fn sense_npc(
        &self,
        observer_pos: Point2,
        observer_facing: [f64; 2],
        _target: NpcId,
        target_pos: Point2,
    ) -> Result<bool, SensorError> {
        match self.kind {
            SensorKind::SightCone {
                fov_half_rad,
                range,
            } => {
                // Distance check first.
                let dx = target_pos.x - observer_pos.x;
                let dy = target_pos.y - observer_pos.y;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > range * range {
                    return Ok(false);
                }
                // Distance ≤ range. If distance is exactly 0 (same position),
                // call it sensed regardless of facing.
                if dist_sq == 0.0 {
                    return Ok(true);
                }

                // Facing must be non-zero.
                let fx = observer_facing[0];
                let fy = observer_facing[1];
                let f_len_sq = fx * fx + fy * fy;
                if f_len_sq == 0.0 {
                    return Err(SensorError::ZeroFacing);
                }

                // Compute cos-angle via dot-product.
                let dist = dist_sq.sqrt();
                let f_len = f_len_sq.sqrt();
                let dot = (fx * dx + fy * dy) / (f_len * dist);
                let cos_half = fov_half_rad.cos();
                Ok(dot >= cos_half)
            }
            SensorKind::HearingRadius { range } => {
                let dx = target_pos.x - observer_pos.x;
                let dy = target_pos.y - observer_pos.y;
                let dist_sq = dx * dx + dy * dy;
                Ok(dist_sq <= range * range)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn sight_cone_invalid_fov_high() {
        // 3π is way out of range
        let err = Sensor::sight_cone(3.0 * PI, 10.0).unwrap_err();
        assert!(matches!(err, SensorError::InvalidFov(_)));
        assert_eq!(err.code(), "AIBEHAV0070");
    }

    #[test]
    fn sight_cone_invalid_fov_negative() {
        let err = Sensor::sight_cone(-0.1, 10.0).unwrap_err();
        assert!(matches!(err, SensorError::InvalidFov(_)));
    }

    #[test]
    fn sight_cone_negative_range() {
        let err = Sensor::sight_cone(PI / 2.0, -1.0).unwrap_err();
        assert!(matches!(err, SensorError::NegativeRange(_)));
        assert_eq!(err.code(), "AIBEHAV0071");
    }

    #[test]
    fn hearing_radius_negative_rejected() {
        let err = Sensor::hearing_radius(-1.0).unwrap_err();
        assert!(matches!(err, SensorError::NegativeRange(_)));
    }

    #[test]
    fn sight_cone_target_in_front() {
        // FOV = 90° (π/2), range = 10 ; observer at origin facing +x ;
        // target at (5, 0) — directly in front, distance 5 → sensed.
        let s = Sensor::sight_cone(PI / 2.0, 10.0).unwrap();
        let observer = Point2::new(0.0, 0.0);
        let facing = [1.0, 0.0];
        let target = Point2::new(5.0, 0.0);
        assert!(s.sense_npc(observer, facing, NpcId(0), target).unwrap());
    }

    #[test]
    fn sight_cone_target_behind() {
        // FOV = 90° ; target behind observer → NOT sensed.
        let s = Sensor::sight_cone(PI / 2.0, 10.0).unwrap();
        let observer = Point2::new(0.0, 0.0);
        let facing = [1.0, 0.0];
        let target = Point2::new(-5.0, 0.0);
        assert!(!s.sense_npc(observer, facing, NpcId(0), target).unwrap());
    }

    #[test]
    fn sight_cone_target_at_edge_of_fov() {
        // FOV = 90° (full) → 45° half. Target at +y at 5 dist : direction
        // (0, 1), facing (1, 0), dot = 0, cos(45°) = 0.707 → NOT sensed.
        let s = Sensor::sight_cone(PI / 2.0, 10.0).unwrap();
        let observer = Point2::new(0.0, 0.0);
        let facing = [1.0, 0.0];
        let target = Point2::new(0.0, 5.0);
        assert!(!s.sense_npc(observer, facing, NpcId(0), target).unwrap());
    }

    #[test]
    fn sight_cone_target_within_45_degrees() {
        // Target at (5, 1) : direction roughly forward + slightly up.
        let s = Sensor::sight_cone(PI / 2.0, 10.0).unwrap();
        let observer = Point2::new(0.0, 0.0);
        let facing = [1.0, 0.0];
        let target = Point2::new(5.0, 1.0);
        assert!(s.sense_npc(observer, facing, NpcId(0), target).unwrap());
    }

    #[test]
    fn sight_cone_target_outside_range() {
        let s = Sensor::sight_cone(PI / 2.0, 10.0).unwrap();
        let observer = Point2::new(0.0, 0.0);
        let facing = [1.0, 0.0];
        let target = Point2::new(50.0, 0.0); // range exceeded
        assert!(!s.sense_npc(observer, facing, NpcId(0), target).unwrap());
    }

    #[test]
    fn sight_cone_zero_facing_errors() {
        let s = Sensor::sight_cone(PI / 2.0, 10.0).unwrap();
        let observer = Point2::new(0.0, 0.0);
        let facing = [0.0, 0.0]; // zero
        let target = Point2::new(5.0, 0.0);
        let err = s.sense_npc(observer, facing, NpcId(0), target).unwrap_err();
        assert!(matches!(err, SensorError::ZeroFacing));
        assert_eq!(err.code(), "AIBEHAV0072");
    }

    #[test]
    fn sight_cone_target_at_observer_position_sensed() {
        // Same-point — facing direction is undefined ; spec calls it sensed.
        let s = Sensor::sight_cone(PI / 2.0, 10.0).unwrap();
        let observer = Point2::new(1.0, 1.0);
        let facing = [1.0, 0.0];
        let target = Point2::new(1.0, 1.0);
        assert!(s.sense_npc(observer, facing, NpcId(0), target).unwrap());
    }

    #[test]
    fn sight_cone_full_2pi_always_sensed_within_range() {
        // FOV 2π = full circle ; target anywhere within range sensed.
        let s = Sensor::sight_cone(2.0 * PI, 10.0).unwrap();
        let observer = Point2::new(0.0, 0.0);
        let facing = [1.0, 0.0];
        // Behind observer.
        let target = Point2::new(-5.0, 0.0);
        assert!(s.sense_npc(observer, facing, NpcId(0), target).unwrap());
    }

    #[test]
    fn hearing_radius_within() {
        let s = Sensor::hearing_radius(5.0).unwrap();
        let observer = Point2::new(0.0, 0.0);
        let target = Point2::new(3.0, 4.0); // dist 5
        assert!(s.sense_npc(observer, [1.0, 0.0], NpcId(0), target).unwrap());
    }

    #[test]
    fn hearing_radius_outside() {
        let s = Sensor::hearing_radius(5.0).unwrap();
        let observer = Point2::new(0.0, 0.0);
        let target = Point2::new(6.0, 0.0);
        assert!(!s.sense_npc(observer, [1.0, 0.0], NpcId(0), target).unwrap());
    }

    #[test]
    fn hearing_radius_zero_dist() {
        let s = Sensor::hearing_radius(0.0).unwrap();
        let observer = Point2::new(0.0, 0.0);
        let target = Point2::new(0.0, 0.0);
        assert!(s.sense_npc(observer, [1.0, 0.0], NpcId(0), target).unwrap());
    }

    #[test]
    fn hearing_radius_facing_irrelevant() {
        // Hearing doesn't care about facing.
        let s = Sensor::hearing_radius(5.0).unwrap();
        let observer = Point2::new(0.0, 0.0);
        let target = Point2::new(3.0, 0.0);
        assert!(s.sense_npc(observer, [0.0, 0.0], NpcId(0), target).unwrap());
    }

    #[test]
    fn sense_determinism() {
        let s = Sensor::sight_cone(PI / 2.0, 10.0).unwrap();
        let observer = Point2::new(0.0, 0.0);
        let facing = [1.0, 0.0];
        let target = Point2::new(5.0, 1.0);
        let r1 = s.sense_npc(observer, facing, NpcId(0), target).unwrap();
        let r2 = s.sense_npc(observer, facing, NpcId(0), target).unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn npc_id_ord_stable() {
        assert!(NpcId(0) < NpcId(1));
    }
}
