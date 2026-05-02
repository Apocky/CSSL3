// § hitscan.rs — single-frame raycast hit-detection
// ════════════════════════════════════════════════════════════════════
// § I> Single-frame raycast cast from camera ; hit-feedback within ≤ 1ms
//      budget (in practice : O(N) sphere sweep over targets ; N ≤ ~256
//      yields << 1µs ; 1ms budget is generous).
// § I> Multi-target pierce supported : configurable max-pierce per-tier.
//      Damage-falloff curve applied per-target distance.
// § I> NaN-safe ; no unsafe.
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

use crate::damage::{
    armor_modifier, compute_damage, damage_falloff, ArmorClass, DamageRoll, DamageType, HitZone,
};

pub type Vec3 = [f32; 3];

/// One target candidate for hitscan (sphere-proxy ; future SDF when wired).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HitscanTarget {
    pub id: u64,
    pub center: Vec3,
    pub radius: f32,
    pub armor: ArmorClass,
    /// True if this sphere represents a head-zone proxy (mechanic, not cosmetic).
    pub is_head: bool,
    /// True if this sphere represents a weak-point proxy.
    pub is_weak: bool,
}

/// Ray description : origin + unit direction.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ray {
    pub origin: Vec3,
    pub dir: Vec3,
}

/// Per-target hitscan result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HitscanHit {
    pub target_id: u64,
    pub distance: f32,
    pub damage: DamageRoll,
}

/// Hitscan parameters per-weapon-build.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HitscanParams {
    /// Maximum number of targets a single shot can pierce (sniper-slug = 4 ; pistol = 1).
    pub max_pierce: u32,
    /// Distance at which falloff begins.
    pub falloff_min_range_m: f32,
    /// Distance at which falloff hits floor.
    pub falloff_max_range_m: f32,
    /// Floor multiplier at max-range.
    pub falloff_floor_mult: f32,
    /// Per-shot raw damage (already factoring kind+tier ; cosmetic-skin agnostic).
    pub per_shot_damage: f32,
    /// Damage-type used for armor matrix.
    pub damage_type: DamageType,
}

impl HitscanParams {
    /// Stage-0 sane default (pistol-Common reference).
    pub const PISTOL_COMMON: Self = Self {
        max_pierce: 1,
        falloff_min_range_m: 12.0,
        falloff_max_range_m: 60.0,
        falloff_floor_mult: 0.4,
        per_shot_damage: 18.0,
        damage_type: DamageType::Kinetic,
    };
}

/// Solve ray-sphere intersection ; returns Some(t) for the entry-distance
/// along the ray (t ≥ 0). None when ray misses or sphere is behind.
#[must_use]
pub fn ray_sphere_t(ray: Ray, center: Vec3, radius: f32) -> Option<f32> {
    let ox = ray.origin[0] - center[0];
    let oy = ray.origin[1] - center[1];
    let oz = ray.origin[2] - center[2];
    let dx = ray.dir[0];
    let dy = ray.dir[1];
    let dz = ray.dir[2];
    let a = dx * dx + dy * dy + dz * dz;
    if a <= 0.0 || !a.is_finite() {
        return None;
    }
    let b = 2.0 * (ox * dx + oy * dy + oz * dz);
    let c = ox * ox + oy * oy + oz * oz - radius * radius;
    let disc = b * b - 4.0 * a * c;
    if !disc.is_finite() || disc < 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    let t0 = (-b - sq) / (2.0 * a);
    let t1 = (-b + sq) / (2.0 * a);
    if t0 >= 0.0 {
        return Some(t0);
    }
    if t1 >= 0.0 {
        return Some(t1);
    }
    None
}

/// Cast a single hitscan ray against `targets`, applying pierce-budget +
/// damage-falloff. Returns hits in ascending-distance order (deterministic).
///
/// `out_hits` is caller-provided to avoid heap allocs in hot-path. Returns
/// the count of hits written.
#[allow(clippy::needless_range_loop)]
pub fn cast_hitscan(
    ray: Ray,
    targets: &[HitscanTarget],
    params: HitscanParams,
    out_hits: &mut [HitscanHit],
) -> usize {
    // Stage-0 : O(N) ray-sphere ; collect into a small fixed buffer for sort.
    let mut buf: [(f32, usize); 64] = [(f32::INFINITY, usize::MAX); 64];
    let mut count: usize = 0;
    for (i, t) in targets.iter().enumerate() {
        if count >= buf.len() {
            break;
        }
        if let Some(dist) = ray_sphere_t(ray, t.center, t.radius) {
            buf[count] = (dist, i);
            count += 1;
        }
    }
    // Insertion-sort (small N ; cache-friendly ; deterministic).
    for i in 1..count {
        let mut j = i;
        while j > 0 && buf[j - 1].0 > buf[j].0 {
            buf.swap(j - 1, j);
            j -= 1;
        }
    }

    let max_pierce = params.max_pierce as usize;
    let mut pierced: usize = 0;
    let mut written: usize = 0;
    for k in 0..count {
        if pierced >= max_pierce {
            break;
        }
        if written >= out_hits.len() {
            break;
        }
        let (dist, idx) = buf[k];
        let target = targets[idx];
        let zone = if target.is_weak {
            HitZone::WeakPoint
        } else if target.is_head {
            HitZone::Head
        } else {
            HitZone::Body
        };
        let falloff = damage_falloff(
            dist,
            params.falloff_min_range_m,
            params.falloff_max_range_m,
            params.falloff_floor_mult,
        );
        let base_after_falloff = params.per_shot_damage * falloff;
        let roll = compute_damage(base_after_falloff, zone, params.damage_type, target.armor, false);
        out_hits[written] = HitscanHit {
            target_id: target.id,
            distance: dist,
            damage: roll,
        };
        // Sanity : also exercise armor_modifier path (tested elsewhere).
        let _ = armor_modifier(params.damage_type, target.armor);
        written += 1;
        pierced += 1;
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: u64, x: f32) -> HitscanTarget {
        HitscanTarget {
            id,
            center: [x, 0.0, 0.0],
            radius: 0.5,
            armor: ArmorClass::Unarmored,
            is_head: false,
            is_weak: false,
        }
    }

    #[test]
    fn hits_nearest_first() {
        let ray = Ray { origin: [0.0, 0.0, 0.0], dir: [1.0, 0.0, 0.0] };
        let ts = [t(1, 30.0), t(2, 10.0), t(3, 20.0)];
        let mut out = [HitscanHit {
            target_id: 0, distance: 0.0,
            damage: compute_damage(0.0, HitZone::Body, DamageType::Kinetic, ArmorClass::Unarmored, false),
        }; 8];
        let mut p = HitscanParams::PISTOL_COMMON;
        p.max_pierce = 8;
        let n = cast_hitscan(ray, &ts, p, &mut out);
        assert_eq!(n, 3);
        assert_eq!(out[0].target_id, 2);
        assert_eq!(out[1].target_id, 3);
        assert_eq!(out[2].target_id, 1);
    }

    #[test]
    fn pierce_cap_respected() {
        let ray = Ray { origin: [0.0, 0.0, 0.0], dir: [1.0, 0.0, 0.0] };
        let ts: Vec<HitscanTarget> = (0..6).map(|i| t(i as u64, (i as f32 + 1.0) * 5.0)).collect();
        let mut out = [HitscanHit {
            target_id: 0, distance: 0.0,
            damage: compute_damage(0.0, HitZone::Body, DamageType::Kinetic, ArmorClass::Unarmored, false),
        }; 8];
        let mut p = HitscanParams::PISTOL_COMMON;
        p.max_pierce = 2;
        let n = cast_hitscan(ray, &ts, p, &mut out);
        assert_eq!(n, 2);
    }

    #[test]
    fn falloff_applied_for_far_targets() {
        let ray = Ray { origin: [0.0, 0.0, 0.0], dir: [1.0, 0.0, 0.0] };
        let near = [t(1, 5.0)];
        let far = [t(2, 70.0)];
        let mut out = [HitscanHit {
            target_id: 0, distance: 0.0,
            damage: compute_damage(0.0, HitZone::Body, DamageType::Kinetic, ArmorClass::Unarmored, false),
        }; 1];
        let p = HitscanParams::PISTOL_COMMON;
        let n_near = cast_hitscan(ray, &near, p, &mut out);
        let near_dmg = out[0].damage.final_dmg;
        let n_far = cast_hitscan(ray, &far, p, &mut out);
        let far_dmg = out[0].damage.final_dmg;
        assert_eq!(n_near, 1);
        assert_eq!(n_far, 1);
        assert!(near_dmg > far_dmg);
    }

    #[test]
    fn miss_returns_zero() {
        let ray = Ray { origin: [0.0, 0.0, 0.0], dir: [0.0, 1.0, 0.0] };
        let ts = [t(1, 10.0)];
        let mut out = [HitscanHit {
            target_id: 0, distance: 0.0,
            damage: compute_damage(0.0, HitZone::Body, DamageType::Kinetic, ArmorClass::Unarmored, false),
        }; 1];
        let p = HitscanParams::PISTOL_COMMON;
        assert_eq!(cast_hitscan(ray, &ts, p, &mut out), 0);
    }
}
