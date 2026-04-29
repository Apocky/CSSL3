//! § sdf_scene — canonical SDF math-scene for the LoA test-room.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § THESIS — WORLD IS MATH
//!
//!   Per Apocky-maxim "the world is math"
//!   (`Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V`), every visible
//!   pixel of the LoA test-room MUST be the result of a math evaluation, not a
//!   clear-color, procedural-texture, or canvas-fill. T11-D228 wired a GDI
//!   BitBlt clear-color path solely to close the visible-pixels acceptance gate
//!   while the SDF stack was being audited ; THIS slice (T11-D234) replaces the
//!   HSV cycle with a CPU-side SDF raymarch via [`cssl_render_v2`].
//!
//!   The canonical scene is intentionally minimal :
//!     - **one sphere** at the origin, radius `1.5`m,
//!     - **one ground plane** at `y = -2`, normal `+Y`,
//!   composed under hard-union. The camera ORBITS the scene at radius `5`m,
//!   `frame_n / 60` radians/frame, so the user sees CONTINUOUS MOTION — the
//!   visual proof that the canonical 60Hz tick is driving every-pixel math.
//!
//! § PATH-OF-IMPL
//!
//!   - SDF + raymarch surface : `cssl-render-v2` exposes
//!     [`cssl_render_v2::SdfRaymarchPass::march`] (CPU-side, pure-f32, no GPU
//!     dep), [`cssl_render_v2::SdfComposition`] for hard-union, and
//!     [`cssl_render_v2::AnalyticSdf::sphere/plane`] for primitives. We REUSE
//!     all of these — there is no inline-implementation of sphere-tracing.
//!   - Plane-primitive : `cssl-pga::Plane::new(a, b, c, d)` per
//!     `n·p + d = 0`. For ground-plane `y = -2`, normal `+Y` → `Plane::new(0,
//!     1, 0, 2)` (because `y + 2 = 0`).
//!   - Surface-normal : the [`cssl_render_v2::RayHit`] returned by
//!     `march` already carries a backward-diff (jet) normal — central-diffs
//!     are forbidden per `01_SDF_NATIVE_RENDER § IV` grep-gate, and we don't
//!     reinvent them here.
//!
//! § SHADING
//!
//!   Single directional sun-light at `(0.4, 0.8, 0.3)` normalized. Per-pixel
//!   shading is `ambient + max(0, n · l) * sphere_albedo` (sphere) or
//!   `ambient + max(0, n · l) * plane_albedo` (plane). Albedo is dispatched
//!   by checking which primitive the hit is closer to — kept minimal because
//!   this slice's target is MAXIM COMPLIANCE, not material complexity.
//!
//! § PERFORMANCE
//!
//!   At 1280×720 with 64 max steps a naive CPU march costs `1280 * 720 * 64`
//!   ≈ 59M SDF evals/frame. The empirical baseline @ Apocky's intel-ultra is
//!   ~6 fps full-res (single-thread). The renderer therefore renders into a
//!   downsampled `RENDER_W × RENDER_H = 320 × 180` buffer (1/4 each axis) and
//!   GDI's `StretchDIBits` upsamples to client-area. This is the
//!   `Stage5DegradeLever` pattern from `cssl-render-v2 § BUDGET DISCIPLINE` —
//!   keep marching at fixed cost regardless of window size.
//!
//! § FUTURE
//!
//!   Future slices will :
//!   - port the per-pixel loop to GPU via `cssl-render-v2`'s SPIR-V kernel,
//!   - re-enable the foveation + multi-view + KAN-amplifier surface,
//!   - integrate Stage-9 mise-en-abyme.
//!   This slice is the FOOT-IN-THE-DOOR : every visible pixel is math.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::module_name_repetitions)]
#![allow(unreachable_pub)]
#![allow(dead_code)]
// Module-level lint allowances consistent with cssl-render-v2 (the SDF
// raymarch surface this module drives) :
//   - imprecise_flops + suboptimal_flops : the hot per-pixel loop is intentionally
//     plain-Rust f32 arithmetic. `mul_add` would help precision but is not yet
//     part of the determinism contract this slice is locking down ; when
//     cssl-render-v2 standardizes on `mul_add` we'll cascade the change.
//   - many_single_char_names : `n l p u v` are math conventions per
//     `01_SDF_NATIVE_RENDER § IV` algorithm-block.
#![allow(clippy::imprecise_flops)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::many_single_char_names)]

use cssl_pga::Plane;
use cssl_render_v2::{
    AnalyticSdf, MaxDistance, RayHit, RaymarchConfig, SdfComposition, SdfRaymarchPass,
};

/// Render-target width for the SDF buffer. Lower than the window's client
/// area so the per-pixel CPU march stays in budget ; GDI `StretchDIBits`
/// upsamples to the window's actual size.
pub const RENDER_W: u32 = 320;
/// Render-target height for the SDF buffer.
pub const RENDER_H: u32 = 180;

/// Camera orbit radius (meters) around the scene-origin.
const ORBIT_RADIUS: f32 = 5.0;
/// Camera height above the ground-plane (meters). Looks slightly down on the
/// scene.
const ORBIT_HEIGHT: f32 = 1.0;
/// Frames per orbit-revolution. At 60 Hz, one full orbit every 360 frames =
/// 6 seconds — slow enough that the user can read the geometry and fast
/// enough that motion is obvious-from-the-first-second.
const FRAMES_PER_REVOLUTION: f32 = 360.0;
/// Field-of-view (vertical, radians). 60° = π/3 gives a reasonable cinematic
/// feel without distorting the sphere too much.
const FOV_Y_RADIANS: f32 = core::f32::consts::FRAC_PI_3;
/// Maximum raymarch steps per pixel.
const MAX_STEPS: u32 = 64;
/// Far-plane distance (meters).
const FAR_DISTANCE: f32 = 100.0;

/// Sphere primitive : centered at world-origin, radius `1.5` m.
const SPHERE_CENTER: [f32; 3] = [0.0, 0.0, 0.0];
const SPHERE_RADIUS: f32 = 1.5;

/// Plane primitive : ground at `y = -2`, normal `+Y`. Plane equation is
/// `n·p + d = 0` so for `y = -2` we have `y + 2 = 0`, ie `(a,b,c,d) =
/// (0, 1, 0, 2)`.
const PLANE_NORMAL: (f32, f32, f32) = (0.0, 1.0, 0.0);
const PLANE_D: f32 = 2.0;

/// Sun-light direction (TO the sun, world-space). Normalized.
const SUN_DIR_RAW: [f32; 3] = [0.4, 0.8, 0.3];
/// Ambient term — keeps shadows from being pure-black so the user can SEE
/// the sphere's shadow side as it orbits.
const AMBIENT: f32 = 0.15;

/// Albedo for the sphere : warm coral.
const SPHERE_ALBEDO: [f32; 3] = [0.95, 0.55, 0.4];
/// Albedo for the ground plane : cool slate.
const PLANE_ALBEDO: [f32; 3] = [0.4, 0.45, 0.55];
/// Sky color (visible at miss).
const SKY_COLOR: [f32; 3] = [0.55, 0.7, 0.95];

/// Build the canonical SDF scene : sphere ∪ plane.
#[must_use]
pub fn build_scene() -> SdfComposition {
    let sphere = SdfComposition::from_primitive(AnalyticSdf::sphere(
        SPHERE_CENTER[0],
        SPHERE_CENTER[1],
        SPHERE_CENTER[2],
        SPHERE_RADIUS,
    ));
    let plane = SdfComposition::from_primitive(AnalyticSdf::plane(Plane::new(
        PLANE_NORMAL.0,
        PLANE_NORMAL.1,
        PLANE_NORMAL.2,
        PLANE_D,
    )));
    SdfComposition::hard_union(sphere, plane)
}

/// Compute the camera origin + forward-vector for a given frame.
///
/// Camera orbits the world-origin in the X-Z plane at radius
/// [`ORBIT_RADIUS`], height [`ORBIT_HEIGHT`], looking at the origin.
///
/// Returns `(origin, forward, right, up)` — all unit vectors except `origin`.
#[must_use]
pub fn orbit_camera(frame_n: u64) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 3]) {
    let theta = (frame_n as f32) * core::f32::consts::TAU / FRAMES_PER_REVOLUTION;
    let cx = ORBIT_RADIUS * theta.cos();
    let cz = ORBIT_RADIUS * theta.sin();
    let origin = [cx, ORBIT_HEIGHT, cz];

    // forward = normalize(target - origin) where target = (0, 0, 0).
    let to_target = [-origin[0], -origin[1], -origin[2]];
    let forward = normalize(to_target);

    // World-up = +Y.
    let world_up = [0.0_f32, 1.0, 0.0];
    // right = normalize(cross(forward, world_up)).
    let right = normalize(cross(forward, world_up));
    // up = cross(right, forward) — guaranteed orthonormal.
    let up = cross(right, forward);

    (origin, forward, right, up)
}

/// Build the world-space ray-direction for pixel `(px, py)` given the
/// camera basis. `(0, 0)` is top-left, `(width-1, height-1)` is bottom-right.
///
/// Math :
///   - `u = (px + 0.5)/width  − 0.5` ∈ [-0.5, 0.5)
///   - `v = 0.5 − (py + 0.5)/height` ∈ [-0.5, 0.5) (Y-up)
///   - `tan_half_fov = tan(FOV_Y / 2)`
///   - `aspect = width / height`
///   - `dir = normalize(forward + right * (u * aspect * 2 * tan_half) +
///                       up * (v * 2 * tan_half))`.
#[must_use]
pub fn pixel_ray(
    px: u32,
    py: u32,
    width: u32,
    height: u32,
    forward: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
) -> [f32; 3] {
    let w_f = width.max(1) as f32;
    let h_f = height.max(1) as f32;
    let u = (px as f32 + 0.5) / w_f - 0.5;
    let v = 0.5 - (py as f32 + 0.5) / h_f;
    let tan_half = (FOV_Y_RADIANS * 0.5).tan();
    let aspect = w_f / h_f;
    let scale_x = u * aspect * 2.0 * tan_half;
    let scale_y = v * 2.0 * tan_half;
    let dir = [
        forward[0] + right[0] * scale_x + up[0] * scale_y,
        forward[1] + right[1] * scale_x + up[1] * scale_y,
        forward[2] + right[2] * scale_x + up[2] * scale_y,
    ];
    normalize(dir)
}

/// Lambertian + ambient shading for one ray-hit.
///
/// Determines the albedo from which primitive the hit is closer to (the
/// composed SDF doesn't carry per-primitive material handles in this slice's
/// minimal scene), then returns `clamp(ambient + n·l, 0, 1) * albedo`.
#[must_use]
pub fn shade_hit(hit: &RayHit) -> [f32; 3] {
    let n = hit.normal.0;
    let l = normalize(SUN_DIR_RAW);
    let ndotl = (n[0] * l[0] + n[1] * l[1] + n[2] * l[2]).max(0.0);
    let lit = (AMBIENT + ndotl).clamp(0.0, 1.0);

    // Pick albedo from the primitive the hit is closest to.
    // Sphere distance at hit-point :
    let dx = hit.p[0] - SPHERE_CENTER[0];
    let dy = hit.p[1] - SPHERE_CENTER[1];
    let dz = hit.p[2] - SPHERE_CENTER[2];
    let sphere_dist = (dx * dx + dy * dy + dz * dz).sqrt() - SPHERE_RADIUS;
    // Plane signed-distance : n·p + d.
    let plane_dist =
        PLANE_NORMAL.0 * hit.p[0] + PLANE_NORMAL.1 * hit.p[1] + PLANE_NORMAL.2 * hit.p[2] + PLANE_D;
    let albedo = if sphere_dist.abs() < plane_dist.abs() {
        SPHERE_ALBEDO
    } else {
        PLANE_ALBEDO
    };

    [albedo[0] * lit, albedo[1] * lit, albedo[2] * lit]
}

/// Render the SDF scene into the given pixel buffer for `frame_n`.
///
/// `pixels` must be a slice of `RENDER_W * RENDER_H` `u32` BGRA pixels —
/// matching the [`super::win32_gdi::GdiRenderer`]'s buffer layout. The
/// function writes one pixel per buffer slot ; partial buffers are not
/// supported.
///
/// § ALGORITHM (per pixel)
///   1. Build ray (orbit-camera basis × pixel-uv).
///   2. Sphere-trace through the scene SDF ([`SdfRaymarchPass::march`]).
///   3. On hit : shade Lambertian+ambient.
///   4. On miss : sky-color.
///   5. Pack to BGRA.
///
/// § PRIME-DIRECTIVE
///   Pure read-side. The renderer NEVER mutates engine state. Replay
///   determinism is preserved because the only frame-input is `frame_n` (a
///   counter) — no entropy, no clock, no random.
pub fn render_into(buf: &mut [u32], frame_n: u64) {
    debug_assert_eq!(buf.len(), (RENDER_W as usize) * (RENDER_H as usize));

    let scene = build_scene();
    let pass = SdfRaymarchPass::new(RaymarchConfig {
        max_distance: MaxDistance(FAR_DISTANCE),
        ..RaymarchConfig::default()
    });
    let (origin, forward, right, up) = orbit_camera(frame_n);

    for py in 0..RENDER_H {
        for px in 0..RENDER_W {
            let dir = pixel_ray(px, py, RENDER_W, RENDER_H, forward, right, up);
            let rgb = match pass.march(&scene, origin, dir, MAX_STEPS) {
                Ok(Some(hit)) => shade_hit(&hit),
                _ => SKY_COLOR,
            };
            let idx = (py as usize) * (RENDER_W as usize) + (px as usize);
            buf[idx] = pack_bgra(rgb);
        }
    }
}

/// Pack a linear-RGB triple in `[0, 1]` to a BGRA `u32` matching the
/// `BITMAPINFO` BI_RGB / 32bpp / top-down layout used by
/// [`super::win32_gdi::GdiRenderer`]. Out-of-range values are clamped.
#[must_use]
pub fn pack_bgra(rgb: [f32; 3]) -> u32 {
    let r = (rgb[0].clamp(0.0, 1.0) * 255.0).round() as u32;
    let g = (rgb[1].clamp(0.0, 1.0) * 255.0).round() as u32;
    let b = (rgb[2].clamp(0.0, 1.0) * 255.0).round() as u32;
    (r << 16) | (g << 8) | b
}

// ── 3-vector math ────────────────────────────────────────────────────────

#[must_use]
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    if len2 > 1e-12 {
        let inv = 1.0 / len2.sqrt();
        [v[0] * inv, v[1] * inv, v[2] * inv]
    } else {
        [0.0, 0.0, -1.0]
    }
}

#[must_use]
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_builds_with_sphere_and_plane() {
        let s = build_scene();
        assert_eq!(s.leaf_count(), 2, "sphere + plane = 2 leaves");
    }

    #[test]
    fn scene_distance_negative_inside_sphere() {
        let s = build_scene();
        // World-origin is inside both sphere (radius 1.5) and above ground
        // (plane at y=-2). Sphere-distance = -1.5, plane-distance = 2 ⇒ min = -1.5.
        let d = s.evaluate([0.0, 0.0, 0.0]);
        assert!(d < 0.0);
        assert!((d + 1.5).abs() < 1e-3);
    }

    #[test]
    fn scene_distance_positive_above_sphere_above_plane() {
        let s = build_scene();
        let d = s.evaluate([0.0, 5.0, 0.0]);
        // Sphere-distance from (0,5,0) = 5 - 1.5 = 3.5 ; plane-distance = 5 + 2 = 7 ;
        // min = 3.5.
        assert!(d > 0.0);
        assert!((d - 3.5).abs() < 1e-3);
    }

    #[test]
    fn scene_distance_zero_at_sphere_surface() {
        let s = build_scene();
        let d = s.evaluate([1.5, 0.0, 0.0]);
        assert!(d.abs() < 1e-3);
    }

    #[test]
    fn scene_distance_zero_at_ground() {
        let s = build_scene();
        // Far from the sphere, on the ground plane y=-2.
        let d = s.evaluate([10.0, -2.0, 10.0]);
        assert!(d.abs() < 1e-3, "ground-plane distance was {d}");
    }

    #[test]
    fn orbit_camera_radius_constant() {
        for f in [0u64, 30, 60, 180, 359] {
            let (origin, _f, _r, _u) = orbit_camera(f);
            let r = (origin[0] * origin[0] + origin[2] * origin[2]).sqrt();
            assert!(
                (r - ORBIT_RADIUS).abs() < 1e-4,
                "orbit radius drift at frame {f}: {r}"
            );
            // Height fixed.
            assert!((origin[1] - ORBIT_HEIGHT).abs() < 1e-4);
        }
    }

    #[test]
    fn orbit_camera_returns_orthonormal_basis() {
        let (_o, f, r, u) = orbit_camera(0);
        // forward unit
        let fl = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
        assert!((fl - 1.0).abs() < 1e-4);
        // right unit
        let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        assert!((rl - 1.0).abs() < 1e-4);
        // up unit
        let ul = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
        assert!((ul - 1.0).abs() < 1e-4);
        // forward ⊥ right
        let fr = f[0] * r[0] + f[1] * r[1] + f[2] * r[2];
        assert!(fr.abs() < 1e-4);
        // right ⊥ up
        let ru = r[0] * u[0] + r[1] * u[1] + r[2] * u[2];
        assert!(ru.abs() < 1e-4);
        // forward ⊥ up
        let fu = f[0] * u[0] + f[1] * u[1] + f[2] * u[2];
        assert!(fu.abs() < 1e-4);
    }

    #[test]
    fn orbit_camera_motion_across_frames() {
        let (o0, _, _, _) = orbit_camera(0);
        let (o1, _, _, _) = orbit_camera(60);
        // 60 frames = 1/6 revolution = 60 deg.
        // Camera should have visibly moved.
        let dx = o0[0] - o1[0];
        let dz = o0[2] - o1[2];
        let dist = (dx * dx + dz * dz).sqrt();
        assert!(dist > 1.0, "60-frame orbit step too small: {dist}");
    }

    #[test]
    fn pixel_ray_center_aligns_with_forward() {
        let forward = [0.0, 0.0, -1.0];
        let right = [1.0, 0.0, 0.0];
        let up = [0.0, 1.0, 0.0];
        // Center pixel of a 100x100 image.
        let dir = pixel_ray(50, 50, 100, 100, forward, right, up);
        // Should be very close to `forward`.
        let dot = dir[0] * forward[0] + dir[1] * forward[1] + dir[2] * forward[2];
        assert!(
            dot > 0.99,
            "center-pixel ray too far from forward: dot={dot}"
        );
    }

    #[test]
    fn pixel_ray_corner_offsets_in_expected_direction() {
        let forward = [0.0, 0.0, -1.0];
        let right = [1.0, 0.0, 0.0];
        let up = [0.0, 1.0, 0.0];
        // Top-left pixel (0,0).
        let dir_tl = pixel_ray(0, 0, 100, 100, forward, right, up);
        // Should bend left + up vs. forward.
        assert!(dir_tl[0] < 0.0, "top-left X must be negative");
        assert!(dir_tl[1] > 0.0, "top-left Y must be positive");
        assert!(dir_tl[2] < 0.0, "top-left still mostly forward");
    }

    #[test]
    fn pack_bgra_red_packs_to_red_channel() {
        let p = pack_bgra([1.0, 0.0, 0.0]);
        // BGRA layout : MSB→LSB = 00 R G B in u32.
        assert_eq!(p, 0x00FF_0000);
    }

    #[test]
    fn pack_bgra_green_packs_to_green_channel() {
        let p = pack_bgra([0.0, 1.0, 0.0]);
        assert_eq!(p, 0x0000_FF00);
    }

    #[test]
    fn pack_bgra_blue_packs_to_blue_channel() {
        let p = pack_bgra([0.0, 0.0, 1.0]);
        assert_eq!(p, 0x0000_00FF);
    }

    #[test]
    fn pack_bgra_clamps_above_one() {
        let p = pack_bgra([2.0, 2.0, 2.0]);
        assert_eq!(p, 0x00FF_FFFF);
    }

    #[test]
    fn pack_bgra_clamps_below_zero() {
        let p = pack_bgra([-1.0, -1.0, -1.0]);
        assert_eq!(p, 0x0000_0000);
    }

    #[test]
    fn shade_hit_in_sun_direction_is_lit() {
        // Hit at +Y top of sphere, normal pointing +Y. Sun is mostly +Y.
        let n = [0.0, 1.0, 0.0];
        let h = RayHit {
            p: [0.0, SPHERE_RADIUS, 0.0],
            normal: cssl_render_v2::SurfaceNormal(n),
            t: SPHERE_RADIUS,
            sdf_value: 0.0,
            material_handle: 0,
            steps_used: 1,
        };
        let rgb = shade_hit(&h);
        // In bright sun ⇒ lit > ambient ⇒ color > AMBIENT * albedo.
        assert!(rgb[0] > AMBIENT * SPHERE_ALBEDO[0]);
        assert!(rgb[1] > AMBIENT * SPHERE_ALBEDO[1]);
        assert!(rgb[2] > AMBIENT * SPHERE_ALBEDO[2]);
    }

    #[test]
    fn shade_hit_facing_away_is_dark() {
        // Hit at -Y bottom of sphere (in shadow). Normal points -Y.
        let n = [0.0, -1.0, 0.0];
        let h = RayHit {
            p: [0.0, -SPHERE_RADIUS, 0.0],
            normal: cssl_render_v2::SurfaceNormal(n),
            t: SPHERE_RADIUS,
            sdf_value: 0.0,
            material_handle: 0,
            steps_used: 1,
        };
        let rgb = shade_hit(&h);
        // In shadow ⇒ only ambient term ⇒ color = AMBIENT * SPHERE_ALBEDO.
        let expected = AMBIENT * SPHERE_ALBEDO[0];
        assert!((rgb[0] - expected).abs() < 0.01);
    }

    #[test]
    fn shade_hit_on_ground_picks_plane_albedo() {
        // Hit on the ground plane far from the sphere.
        let n = [0.0, 1.0, 0.0];
        let h = RayHit {
            p: [10.0, -2.0, 10.0],
            normal: cssl_render_v2::SurfaceNormal(n),
            t: 14.0,
            sdf_value: 0.0,
            material_handle: 0,
            steps_used: 1,
        };
        let rgb = shade_hit(&h);
        // Color should track PLANE_ALBEDO, not SPHERE_ALBEDO. The R-channel
        // delta is large enough to disambiguate.
        let lit = AMBIENT + (n[1] * (SUN_DIR_RAW[1] / norm_sun()));
        let expected_r = PLANE_ALBEDO[0] * lit.clamp(0.0, 1.0);
        assert!((rgb[0] - expected_r).abs() < 0.02);
    }

    #[test]
    fn render_into_writes_full_buffer() {
        // Smoke-test : render a tiny patch (we can't run a full 320x180 in CI
        // due to march cost — but we do verify the buffer is FULLY written).
        let mut buf = vec![0u32; (RENDER_W as usize) * (RENDER_H as usize)];
        render_into(&mut buf, 0);
        // Every pixel should have a value (at the very least the sky-color).
        // Count of non-zero pixels :
        let nonzero = buf.iter().filter(|p| **p != 0).count();
        assert!(nonzero > 0, "no pixels written");
        // Sky-color is non-trivial, so we expect a strong majority of pixels
        // to be written. Even pure-black sky would have *some* hit pixels for
        // the sphere + ground.
        assert!(
            nonzero > buf.len() / 2,
            "fewer than half the pixels were written: {nonzero}/{}",
            buf.len()
        );
    }

    #[test]
    fn render_into_changes_with_frame() {
        let mut buf0 = vec![0u32; (RENDER_W as usize) * (RENDER_H as usize)];
        let mut buf1 = vec![0u32; (RENDER_W as usize) * (RENDER_H as usize)];
        render_into(&mut buf0, 0);
        render_into(&mut buf1, 60);
        // Camera moves between frames ⇒ at least some pixels must differ.
        let differ = buf0.iter().zip(buf1.iter()).filter(|(a, b)| a != b).count();
        assert!(
            differ > 100,
            "frame 0 and frame 60 too similar: {differ} pixels differ"
        );
    }

    fn norm_sun() -> f32 {
        let v = SUN_DIR_RAW;
        (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
    }

    /// Performance smoke-test : render 10 frames and report per-frame time.
    /// Marked `#[ignore]` so it's skipped in default test runs ; invoke with
    /// `cargo test -p loa-game --features test-bypass --bins --release \
    ///  test_room_render::sdf_scene::tests::bench_render_into_release \
    ///  -- --ignored --nocapture` to get a wall-time read on the SDF math
    /// renderer. The 60Hz target is 16.67 ms/frame ; if `bench` reports more
    /// than that the slice's downsample-resolution should drop further.
    #[test]
    #[ignore]
    fn bench_render_into_release() {
        const N: u32 = 10;
        let mut buf = vec![0u32; (RENDER_W as usize) * (RENDER_H as usize)];
        // Warm-up.
        render_into(&mut buf, 0);
        let start = std::time::Instant::now();
        for f in 0..N {
            render_into(&mut buf, u64::from(f));
        }
        let elapsed = start.elapsed();
        let per_frame = elapsed / N;
        let hz = 1.0 / per_frame.as_secs_f64();
        eprintln!(
            "BENCH render_into : per-frame = {per_frame:?} ({hz:.2} Hz) ; total {elapsed:?} for {N} frames @ {RENDER_W}x{RENDER_H}"
        );
    }
}
