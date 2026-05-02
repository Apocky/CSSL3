//! § ray — observer-ray walking through ω-field.
//!
//! For each pixel, we cast a ray from the observer through the pixel's
//! direction in observer-space, and walk it through the world. At each
//! sample point, we ask the crystallization layer "what crystal is near
//! here?". Stage-0 uses a simple isotropic bounding-sphere visibility
//! check ; future iterations swap in a substrate-spatial-index.

use cssl_host_crystallization::{Crystal, WorldPos};

use crate::observer::ObserverCoord;

/// Number of sample points along each ray. More samples = higher fidelity
/// at proportionally higher cost. Stage-0 default = 8 (fits in L1 cache).
pub const RAY_SAMPLES: usize = 8;
pub const RAY_MAX_DIST_MM: i32 = 16_000; // 16 meters

/// One sample point on a ray.
#[derive(Debug, Clone, Copy)]
pub struct RaySample {
    pub world: WorldPos,
    /// Distance from observer in mm.
    pub dist_mm: i32,
}

/// Compute the world-space direction (in mm-millunits per step) the pixel
/// `(px, py)` should sample, given the observer's facing + a virtual sensor.
/// Stage-0 uses a simple pinhole camera with 90° horizontal FOV.
pub fn pixel_direction(
    observer: ObserverCoord,
    px: u32,
    py: u32,
    width: u32,
    height: u32,
) -> (i32, i32, i32) {
    // Convert pixel coords to NDC -1..1.
    let nx = (px as i32 * 2) - (width as i32);
    let ny = (height as i32) - (py as i32 * 2);
    // Apply pinhole projection (assume 1000-unit virtual sensor distance).
    // Returned direction is unnormalized but proportional ; ray-walk will
    // normalize implicitly via step accumulation.
    let z_unit = 1000_i32;
    let x_unit = nx;
    let y_unit = ny;
    // Apply yaw (rotate around Y axis).
    let yaw = observer.yaw_milli as i64;
    // Stage-0 small-angle approximation : rotate by milliradians.
    // sin(θ) ≈ θ_mrad/1000 ; cos(θ) ≈ 1.
    let cos_y = 1000i64;
    let sin_y = (yaw as i64).clamp(-1000, 1000);
    let xr = ((x_unit as i64) * cos_y - (z_unit as i64) * sin_y) / 1000;
    let zr = ((x_unit as i64) * sin_y + (z_unit as i64) * cos_y) / 1000;
    // Apply pitch (rotate around X axis).
    let pitch = observer.pitch_milli as i64;
    let cos_p = 1000i64;
    let sin_p = pitch.clamp(-1000, 1000);
    let yr = ((y_unit as i64) * cos_p - (zr) * sin_p) / 1000;
    let zr2 = ((y_unit as i64) * sin_p + (zr) * cos_p) / 1000;
    (xr as i32, yr as i32, zr2 as i32)
}

/// Generate `RAY_SAMPLES` evenly-spaced sample points along the ray. The
/// step size depends on RAY_MAX_DIST_MM / RAY_SAMPLES.
///
/// The input direction (dx, dy, dz) is roughly unit-magnitude scaled by
/// 1000 (because pixel_direction uses z_unit = 1000). We step along it
/// in integer-mm using `step_mm` per step.
pub fn walk_ray(observer: ObserverCoord, dx: i32, dy: i32, dz: i32) -> [RaySample; RAY_SAMPLES] {
    let mut out = [RaySample {
        world: WorldPos::new(0, 0, 0),
        dist_mm: 0,
    }; RAY_SAMPLES];
    let step_mm = RAY_MAX_DIST_MM / (RAY_SAMPLES as i32 + 1);
    // (dx, dy, dz) magnitude is approximately 1000 (pinhole projection
    // uses z_unit = 1000). To avoid integer-truncation, multiply BEFORE
    // dividing : per-step offset = (component × step_mm × i) / 1000.
    for i in 0..RAY_SAMPLES {
        let i_i64 = (i as i64) + 1;
        let dist_mm = (i as i32 + 1) * step_mm;
        let off_x = ((dx as i64) * (step_mm as i64) * i_i64) / 1000;
        let off_y = ((dy as i64) * (step_mm as i64) * i_i64) / 1000;
        let off_z = ((dz as i64) * (step_mm as i64) * i_i64) / 1000;
        out[i] = RaySample {
            world: WorldPos::new(
                observer.x_mm.saturating_add(off_x as i32),
                observer.y_mm.saturating_add(off_y as i32),
                observer.z_mm.saturating_add(off_z as i32),
            ),
            dist_mm,
        };
    }
    out
}

/// Cheap proximity test : list of crystal indices whose bounding-sphere
/// contains `world`. Stage-0 brute-force (linear scan); future: spatial-
/// index via cssl-substrate-omega-field.
pub fn crystals_near(
    crystals: &[Crystal],
    world: WorldPos,
    radius_mm: i32,
) -> impl Iterator<Item = usize> + '_ {
    let radius_sq = (radius_mm as i64) * (radius_mm as i64);
    crystals.iter().enumerate().filter_map(move |(i, c)| {
        let dx = (c.world_pos.x_mm - world.x_mm) as i64;
        let dy = (c.world_pos.y_mm - world.y_mm) as i64;
        let dz = (c.world_pos.z_mm - world.z_mm) as i64;
        let d_sq = dx * dx + dy * dy + dz * dz;
        let r_total = (c.extent_mm as i64) + (radius_mm as i64);
        let r_total_sq = r_total * r_total;
        if d_sq <= r_total_sq.min(radius_sq * 4) {
            Some(i)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_direction_center_is_forward() {
        let observer = ObserverCoord::default();
        let (dx, _dy, dz) = pixel_direction(observer, 50, 50, 100, 100);
        // Center pixel → (0, 0, +z) approximately (some rounding).
        assert!(dx.abs() <= 10);
        assert!(dz > 500);
    }

    #[test]
    fn walk_ray_produces_n_samples() {
        let observer = ObserverCoord::default();
        let samples = walk_ray(observer, 0, 0, 1000);
        assert_eq!(samples.len(), RAY_SAMPLES);
        // Distances are monotonic.
        for w in samples.windows(2) {
            assert!(w[1].dist_mm > w[0].dist_mm);
        }
    }
}
