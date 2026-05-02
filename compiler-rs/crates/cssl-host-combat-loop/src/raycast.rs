//! § raycast — sphere-vs-ray intersection (stage-0 hit detection)
//! ══════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!
//! Per `combat_loop.csl` § AXIOMS :
//!   t∞: hit-detection = sphere-vs-sphere stage-0 ; SDF-vs-SDF when render-v2 wires
//!
//! Stage-0 tests a player-fired ray against each crystal's bounding-sphere
//! derived from `(crystal.world_pos, crystal.extent_mm / 2)`. When render-v2
//! lands SDF-vs-SDF intersection, only the body of `raycast_crystals` changes.
//!
//! § COORDINATE SYSTEM
//!   - Crystal `world_pos` is `i32` millimeters (replay-deterministic fixed-point).
//!   - Ray origins/directions are `f32` meters (matches the .csl spec input).
//!   - Convert mm → m at boundary by ×0.001.
//!
//! § ATTESTATION
//! There was no hurt nor harm in the making of this, to anyone, anything, or anybody.

use cssl_host_crystallization::Crystal;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin_x: f32,
    pub origin_y: f32,
    pub origin_z: f32,
    pub dir_x: f32,
    pub dir_y: f32,
    pub dir_z: f32,
    pub max_range_m: f32,
}

impl Ray {
    pub fn from_camera(
        origin_x: f32,
        origin_y: f32,
        origin_z: f32,
        yaw_rad: f32,
        pitch_rad: f32,
        max_range_m: f32,
    ) -> Self {
        let cp = pitch_rad.cos();
        let sp = pitch_rad.sin();
        let cy = yaw_rad.cos();
        let sy = yaw_rad.sin();
        Self {
            origin_x,
            origin_y,
            origin_z,
            dir_x: sy * cp,
            dir_y: sp,
            dir_z: cy * cp,
            max_range_m,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RaycastHit {
    pub target_handle: u32,
    pub hit_t_m: f32,
    pub is_headshot: bool,
}

/// Solve ray-vs-sphere. Direction MUST be a unit vector ; caller normalizes.
pub fn ray_sphere_t(
    ox: f32,
    oy: f32,
    oz: f32,
    dx: f32,
    dy: f32,
    dz: f32,
    cx: f32,
    cy: f32,
    cz: f32,
    radius_m: f32,
) -> Option<f32> {
    let mx = ox - cx;
    let my = oy - cy;
    let mz = oz - cz;
    let b = dx * mx + dy * my + dz * mz;
    let c = mx * mx + my * my + mz * mz - radius_m * radius_m;

    if c > 0.0 && b > 0.0 {
        return None;
    }
    let discr = b * b - c;
    if discr < 0.0 {
        return None;
    }
    let sqrt_d = discr.sqrt();
    let t = -b - sqrt_d;
    if t >= 0.0 {
        Some(t)
    } else {
        let t_far = -b + sqrt_d;
        if t_far >= 0.0 { Some(t_far) } else { None }
    }
}

/// Convert Crystal's `world_pos` (mm) + `extent_mm` (mm) to `(cx, cy, cz, r_m)`.
pub fn crystal_bounding_sphere(crystal: &Crystal) -> (f32, f32, f32, f32) {
    let cx = crystal.world_pos.x_mm as f32 * 0.001;
    let cy = crystal.world_pos.y_mm as f32 * 0.001;
    let cz = crystal.world_pos.z_mm as f32 * 0.001;
    let radius_m = (crystal.extent_mm as f32 * 0.001) * 0.5;
    (cx, cy, cz, radius_m)
}

/// Trace ray against every crystal, returning closest hit ≤ `ray.max_range_m`.
/// Tie-break on smaller handle (deterministic for replay-stability).
pub fn raycast_crystals(ray: &Ray, crystals: &[Crystal]) -> Option<RaycastHit> {
    let len_sq = ray.dir_x * ray.dir_x + ray.dir_y * ray.dir_y + ray.dir_z * ray.dir_z;
    if len_sq < 1.0e-12 {
        return None;
    }
    let len = len_sq.sqrt();
    let dx = ray.dir_x / len;
    let dy = ray.dir_y / len;
    let dz = ray.dir_z / len;

    let mut best: Option<RaycastHit> = None;
    for crystal in crystals {
        if !crystal.aspect_permitted(0) {
            continue;
        }
        let (cx, cy, cz, radius_m) = crystal_bounding_sphere(crystal);
        let Some(t) = ray_sphere_t(
            ray.origin_x, ray.origin_y, ray.origin_z,
            dx, dy, dz,
            cx, cy, cz, radius_m,
        ) else {
            continue;
        };
        if t > ray.max_range_m {
            continue;
        }

        let impact_y = ray.origin_y + dy * t;
        let is_headshot = (impact_y - cy) >= radius_m * 0.4;

        let candidate = RaycastHit {
            target_handle: crystal.handle,
            hit_t_m: t,
            is_headshot,
        };
        match best {
            None => best = Some(candidate),
            Some(prev) => {
                if t < prev.hit_t_m
                    || (t == prev.hit_t_m && crystal.handle < prev.target_handle)
                {
                    best = Some(candidate);
                }
            }
        }
    }
    best
}

// ══════════════════════════════════════════════════════════════════════════
// § TESTS
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_crystallization::{Crystal, CrystalClass, WorldPos};

    fn at(x_mm: i32, y_mm: i32, z_mm: i32) -> Crystal {
        Crystal::allocate(CrystalClass::Entity, 1, WorldPos::new(x_mm, y_mm, z_mm))
    }

    #[test]
    fn ray_sphere_direct_hit() {
        let t = ray_sphere_t(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 10.0, 1.0);
        assert!(t.is_some());
        let t = t.unwrap();
        assert!((t - 9.0).abs() < 0.01, "expected ~9.0 got {}", t);
    }

    #[test]
    fn ray_sphere_miss_above() {
        let t = ray_sphere_t(0.0, 5.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 10.0, 1.0);
        assert!(t.is_none());
    }

    #[test]
    fn ray_sphere_pointing_away_misses() {
        let t = ray_sphere_t(0.0, 0.0, 5.0, 0.0, 0.0, -1.0, 0.0, 0.0, 10.0, 1.0);
        assert!(t.is_none());
    }

    #[test]
    fn raycast_picks_closest() {
        let c1 = at(0, 0, 1_000);
        let c2 = at(0, 0, 5_000);
        let crystals = vec![c1, c2];
        let ray = Ray {
            origin_x: 0.0, origin_y: 0.0, origin_z: 0.0,
            dir_x: 0.0, dir_y: 0.0, dir_z: 1.0,
            max_range_m: 100.0,
        };
        let hit = raycast_crystals(&ray, &crystals).expect("nearest hit");
        assert_eq!(hit.target_handle, crystals[0].handle);
    }

    #[test]
    fn raycast_respects_max_range() {
        let c = at(0, 0, 50_000);
        let crystals = vec![c];
        let ray = Ray {
            origin_x: 0.0, origin_y: 0.0, origin_z: 0.0,
            dir_x: 0.0, dir_y: 0.0, dir_z: 1.0,
            max_range_m: 10.0,
        };
        assert!(raycast_crystals(&ray, &crystals).is_none());
    }

    #[test]
    fn raycast_skips_silhouette_revoked() {
        let mut c = at(0, 0, 5_000);
        c.revoke_aspect(0);
        let crystals = vec![c];
        let ray = Ray {
            origin_x: 0.0, origin_y: 0.0, origin_z: 0.0,
            dir_x: 0.0, dir_y: 0.0, dir_z: 1.0,
            max_range_m: 100.0,
        };
        assert!(raycast_crystals(&ray, &crystals).is_none());
    }

    #[test]
    fn ray_from_camera_yaw_zero_points_pos_z() {
        let r = Ray::from_camera(0.0, 0.0, 0.0, 0.0, 0.0, 100.0);
        assert!(r.dir_x.abs() < 1.0e-6);
        assert!(r.dir_y.abs() < 1.0e-6);
        assert!((r.dir_z - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn raycast_empty_list_returns_none() {
        let ray = Ray {
            origin_x: 0.0, origin_y: 0.0, origin_z: 0.0,
            dir_x: 0.0, dir_y: 0.0, dir_z: 1.0,
            max_range_m: 100.0,
        };
        assert!(raycast_crystals(&ray, &[]).is_none());
    }
}
