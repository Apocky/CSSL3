// § hit_detection.rs — SDF-vs-SDF hit-detection (math-only stage-0)
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § HIT-DETECTION ; weapon-SDF ⊓ target-SDF dist-min ≤ ε
// § I> stage-0 : sphere-vs-sphere math (sphere = degenerate-SDF for an actor-AABB-proxy)
// § I> stage-1 : swap apply_distance_field with `cssl_render_v2::sdf_eval`
//      ⊘ NOT YET WIRED — stage-0 sufficient for combat-sim deterministic-replay tests
// § I> sample-count fixed (4 default per GDD) ; ε frozen
// § I> ¬ panic ; saturating arithmetic
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// ε for full-hit per GDD § EPSILON.
pub const EPSILON_HIT: f32 = 0.02;
/// ε for glance-hit (×0.5 dmg) per GDD.
pub const EPSILON_GLANCE: f32 = 0.05;
/// ε for max ; beyond is no-hit.
pub const EPSILON_MAX: f32 = 0.08;

/// 3-vec — point in metres ; FFI-Copy ; bit-equal across hosts.
pub type Vec3 = [f32; 3];

/// One sample along a weapon's swing-arc.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HitSample {
    /// Sample position in world-metres.
    pub pos: Vec3,
    /// Sample radius (weapon-thickness ; e.g. 0.04m blade).
    pub radius: f32,
}

/// Stage-0 target representation : sphere ; stage-1 will accept full SDF.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TargetSphere {
    pub center: Vec3,
    pub radius: f32,
}

/// Interpolate `count` samples between two blade-tip positions.
/// `count` clamped ≥ 2 ; output always has `count` entries.
#[must_use]
pub fn weapon_path_samples(start: Vec3, end: Vec3, count: usize, blade_radius: f32) -> Vec<HitSample> {
    let count = count.max(2);
    let denom = (count - 1) as f32;
    let safe_radius = if blade_radius.is_finite() {
        blade_radius.max(0.0)
    } else {
        0.0
    };
    (0..count)
        .map(|i| {
            let t = (i as f32) / denom;
            HitSample {
                pos: [
                    start[0] + (end[0] - start[0]) * t,
                    start[1] + (end[1] - start[1]) * t,
                    start[2] + (end[2] - start[2]) * t,
                ],
                radius: safe_radius,
            }
        })
        .collect()
}

/// Euclidean distance between two points (NaN-safe ; saturating ≥ 0).
#[must_use]
pub fn distance(a: Vec3, b: Vec3) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    let sq = dx * dx + dy * dy + dz * dz;
    if sq.is_finite() {
        sq.max(0.0).sqrt()
    } else {
        f32::INFINITY
    }
}

/// Stage-0 SDF distance-min : minimum over all samples of
///   max(0, distance(sample, target.center) - sample.radius - target.radius).
///
/// Returns f32::INFINITY if `samples` is empty (no-hit).
#[must_use]
pub fn sdf_distance_min(samples: &[HitSample], target: TargetSphere) -> f32 {
    if samples.is_empty() {
        return f32::INFINITY;
    }
    let mut best = f32::INFINITY;
    for s in samples {
        let d_centre = distance(s.pos, target.center);
        let surf = (d_centre - s.radius - target.radius).max(0.0);
        if surf < best {
            best = surf;
        }
    }
    best
}

/// Returns true iff the swing produced a hit (distance ≤ EPSILON_HIT).
#[must_use]
pub fn is_hit(samples: &[HitSample], target: TargetSphere) -> bool {
    sdf_distance_min(samples, target) <= EPSILON_HIT
}

/// Returns true iff the swing produced a glance-hit (EPSILON_HIT < d ≤ EPSILON_GLANCE).
#[must_use]
pub fn is_glance(samples: &[HitSample], target: TargetSphere) -> bool {
    let d = sdf_distance_min(samples, target);
    d > EPSILON_HIT && d <= EPSILON_GLANCE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_count_correct() {
        let s = weapon_path_samples([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 4, 0.04);
        assert_eq!(s.len(), 4);
        // First sample at start
        assert!((s[0].pos[0] - 0.0).abs() < 1e-3);
        // Last sample at end
        assert!((s[3].pos[0] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn empty_samples_no_hit() {
        let target = TargetSphere {
            center: [0.0, 0.0, 0.0],
            radius: 0.5,
        };
        assert!(!is_hit(&[], target));
    }

    #[test]
    fn far_target_no_hit() {
        let s = weapon_path_samples([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 4, 0.04);
        let target = TargetSphere {
            center: [10.0, 0.0, 0.0],
            radius: 0.5,
        };
        assert!(!is_hit(&s, target));
    }

    #[test]
    fn close_target_full_hit() {
        let s = weapon_path_samples([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 4, 0.04);
        let target = TargetSphere {
            center: [0.5, 0.0, 0.0],
            radius: 0.5,
        };
        assert!(is_hit(&s, target));
    }
}
