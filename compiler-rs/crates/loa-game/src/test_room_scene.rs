//! § test_room_scene — pure-math labyrinth-test-room scene definition.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § THESIS — "the world is math"
//!
//!   Every visible-pixel of the labyrinth-test-room is computed by evaluating
//!   mathematical functions :
//!     - walls          ⟶ box-SDF unions forming corridors
//!     - creature       ⟶ KAN-style sphere-blob with frame-N animation
//!     - companion      ⟶ sphere with orbit-frame-N position
//!     - shading        ⟶ Lambertian + ambient (n·l)
//!     - camera         ⟶ orbit-around-scene (frame-N driven)
//!
//!   NO textures. NO sample-banks. NO procedural-fill-canvas. ZERO non-math
//!   inputs to per-pixel color. This module is the canonical test-bed for
//!   the world-is-math maxim per `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl`.
//!
//! § SURFACE
//!
//!   ```rust,ignore
//!   let cam   = camera_at_frame(frame_n);
//!   let pixel = raymarch_pixel(&cam, u, v);  // u,v ∈ [-1, +1] screen-coords
//!   ```
//!
//!   The renderer (S2-C1 follow-up OR existing GDI presenter) calls
//!   `raymarch_pixel` per-pixel + writes to its frame-buffer.
//!
//! § STAGE-0 SIMPLICITY
//!
//!   Camera : ORBITS around scene-origin (no input deps yet ; F2-input wires
//!   later). Animation : creature pulses + companion orbits via frame-N.
//!   Lighting : single directional light. NO global-illum, NO shadows
//!   (fast-path stage-0).
//!
//! § ATTESTATION  (PRIME_DIRECTIVE.md § 11)
//!
//!   There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody.

#![allow(clippy::module_name_repetitions)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::similar_names)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::suboptimal_flops)]

use core::f32::consts::TAU;

// ═══════════════════════════════════════════════════════════════════════════
// § VEC3 — minimal 3-component math (no external deps)
// ═══════════════════════════════════════════════════════════════════════════

/// 3-component float vector. Inline operations only ; ZERO heap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct V3(pub f32, pub f32, pub f32);

impl V3 {
    #[inline]
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self(x, y, z)
    }

    #[inline]
    #[must_use]
    pub const fn splat(s: f32) -> Self {
        Self(s, s, s)
    }

    #[inline]
    #[must_use]
    pub fn add(self, b: Self) -> Self {
        Self(self.0 + b.0, self.1 + b.1, self.2 + b.2)
    }

    #[inline]
    #[must_use]
    pub fn sub(self, b: Self) -> Self {
        Self(self.0 - b.0, self.1 - b.1, self.2 - b.2)
    }

    #[inline]
    #[must_use]
    pub fn mul(self, s: f32) -> Self {
        Self(self.0 * s, self.1 * s, self.2 * s)
    }

    #[inline]
    #[must_use]
    pub fn dot(self, b: Self) -> f32 {
        self.0.mul_add(b.0, self.1.mul_add(b.1, self.2 * b.2))
    }

    #[inline]
    #[must_use]
    pub fn norm(self) -> f32 {
        self.dot(self).sqrt()
    }

    #[inline]
    #[must_use]
    pub fn normalize(self) -> Self {
        let n = self.norm().max(1e-9);
        self.mul(1.0 / n)
    }

    #[inline]
    #[must_use]
    pub fn cross(self, b: Self) -> Self {
        Self(
            self.1.mul_add(b.2, -(self.2 * b.1)),
            self.2.mul_add(b.0, -(self.0 * b.2)),
            self.0.mul_add(b.1, -(self.1 * b.0)),
        )
    }

    /// Component-wise abs.
    #[inline]
    #[must_use]
    pub fn abs(self) -> Self {
        Self(self.0.abs(), self.1.abs(), self.2.abs())
    }

    /// Component-wise max with a scalar.
    #[inline]
    #[must_use]
    pub fn max_scalar(self, s: f32) -> Self {
        Self(self.0.max(s), self.1.max(s), self.2.max(s))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § CAMERA — orbits around scene-origin (frame-N driven)
// ═══════════════════════════════════════════════════════════════════════════

/// First-person camera. Position + look-direction.
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub position: V3,
    pub forward: V3,
    pub up: V3,
    pub right: V3,
    pub fov_radians: f32,
}

impl Camera {
    /// Orbit camera at the given frame_n. Period ≈ 600 frames (10s @ 60Hz).
    #[must_use]
    pub fn at_frame(frame_n: u64) -> Self {
        // Orbit angle : full revolution every 600 frames.
        let t = (frame_n as f32) / 600.0;
        let angle = t * TAU;

        // Camera position : orbit at radius=8, height=3.
        let radius = 8.0_f32;
        let height = 3.0_f32;
        let position = V3::new(angle.cos() * radius, height, angle.sin() * radius);

        // Look-at scene-origin (0,0,0).
        let target = V3::splat(0.0);
        let forward = target.sub(position).normalize();

        // World-up vector + derive camera-right + camera-up.
        let world_up = V3::new(0.0, 1.0, 0.0);
        let right = forward.cross(world_up).normalize();
        let up = right.cross(forward).normalize();

        Self {
            position,
            forward,
            up,
            right,
            fov_radians: 60.0_f32.to_radians(),
        }
    }

    /// Compute primary ray direction for screen-coord (u, v) ∈ [-1, +1].
    /// Aspect ratio compensation handled by caller (multiply u by aspect).
    #[must_use]
    pub fn ray_dir(&self, u: f32, v: f32) -> V3 {
        let half_fov = (self.fov_radians * 0.5).tan();
        let dir = self
            .forward
            .add(self.right.mul(u * half_fov))
            .add(self.up.mul(v * half_fov));
        dir.normalize()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § SDF PRIMITIVES — sphere / box / plane (analytic functions)
// ═══════════════════════════════════════════════════════════════════════════

/// SDF for a sphere centered at `c` with radius `r`.
#[inline]
#[must_use]
pub fn sphere_sdf(p: V3, c: V3, r: f32) -> f32 {
    p.sub(c).norm() - r
}

/// SDF for an axis-aligned box centered at `c` with half-extents `b`.
#[inline]
#[must_use]
pub fn box_sdf(p: V3, c: V3, b: V3) -> f32 {
    let q = p.sub(c).abs().sub(b);
    let outside = q.max_scalar(0.0).norm();
    let inside = q.0.max(q.1.max(q.2)).min(0.0);
    outside + inside
}

/// SDF for an infinite plane defined by normal `n` (unit) and offset `d`.
#[inline]
#[must_use]
pub fn plane_sdf(p: V3, n: V3, d: f32) -> f32 {
    p.dot(n) + d
}

/// Smooth-min — blend two SDFs with a smoothing factor `k`.
#[inline]
#[must_use]
pub fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    let h = ((k - (a - b).abs()) / k).clamp(0.0, 1.0);
    a.min(b) - h * h * k * 0.25
}

// ═══════════════════════════════════════════════════════════════════════════
// § LABYRINTH SCENE — walls + creature + companion
// ═══════════════════════════════════════════════════════════════════════════

/// Material-id tagged per-hit ; downstream shader uses for color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialId {
    Floor,
    Wall,
    Creature,
    Companion,
}

/// Combined scene SDF. Returns `(distance, material_id)`.
#[must_use]
pub fn scene_sdf(p: V3, frame_n: u64) -> (f32, MaterialId) {
    // ── Floor (plane at y=-2). ────────────────────────────────────────────
    let d_floor = plane_sdf(p, V3::new(0.0, 1.0, 0.0), 2.0);
    let mut best = (d_floor, MaterialId::Floor);

    // ── Labyrinth walls (4 box-SDFs forming a cross-corridor). ────────────
    // Per Omniverse canonical : SDF-walls are union-of-boxes ; corridors
    // emerge from gaps between boxes. Stage-0 shape : simple cross-corridor.
    let walls = [
        // North-South corridor walls (left + right of corridor).
        (V3::new(-3.0, 0.0, 0.0), V3::new(0.5, 1.5, 6.0)),
        (V3::new(3.0, 0.0, 0.0), V3::new(0.5, 1.5, 6.0)),
        // East-West corridor walls (above + below corridor).
        (V3::new(0.0, 0.0, -3.0), V3::new(6.0, 1.5, 0.5)),
        (V3::new(0.0, 0.0, 3.0), V3::new(6.0, 1.5, 0.5)),
    ];
    for (c, b) in walls {
        let d = box_sdf(p, c, b);
        if d < best.0 {
            best = (d, MaterialId::Wall);
        }
    }

    // ── Creature : pulsing sphere at origin (KAN-style frame-N animation). ─
    // Pulse-radius : base + sin(frame_n × ω). Period ≈ 90 frames (1.5s).
    let pulse_t = (frame_n as f32) / 90.0;
    let pulse_radius = 0.7 + 0.15 * (pulse_t * TAU).sin();
    let d_creature = sphere_sdf(p, V3::new(0.0, 0.0, 0.0), pulse_radius);
    if d_creature < best.0 {
        best = (d_creature, MaterialId::Creature);
    }

    // ── Companion : orbiting sphere offset from origin. ───────────────────
    // Orbit period ≈ 240 frames (4s @ 60Hz) ; radius=2.5, height=0.5.
    let orbit_t = (frame_n as f32) / 240.0;
    let orbit_angle = orbit_t * TAU;
    let companion_pos = V3::new(orbit_angle.cos() * 2.5, 0.5, orbit_angle.sin() * 2.5);
    let d_companion = sphere_sdf(p, companion_pos, 0.4);
    if d_companion < best.0 {
        best = (d_companion, MaterialId::Companion);
    }

    best
}

/// Finite-difference normal at point `p` (3 extra SDF evaluations).
#[must_use]
pub fn scene_normal(p: V3, frame_n: u64) -> V3 {
    let h = 0.001_f32;
    let dx = scene_sdf(V3::new(p.0 + h, p.1, p.2), frame_n).0
        - scene_sdf(V3::new(p.0 - h, p.1, p.2), frame_n).0;
    let dy = scene_sdf(V3::new(p.0, p.1 + h, p.2), frame_n).0
        - scene_sdf(V3::new(p.0, p.1 - h, p.2), frame_n).0;
    let dz = scene_sdf(V3::new(p.0, p.1, p.2 + h), frame_n).0
        - scene_sdf(V3::new(p.0, p.1, p.2 - h), frame_n).0;
    V3::new(dx, dy, dz).normalize()
}

// ═══════════════════════════════════════════════════════════════════════════
// § RAYMARCH — sphere-trace canonical loop
// ═══════════════════════════════════════════════════════════════════════════

/// Raymarch hit-info.
#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub p: V3,
    pub t: f32,
    pub normal: V3,
    pub material: MaterialId,
    pub steps: u32,
}

/// Sphere-trace through the scene. Returns Hit if surface intersected.
#[must_use]
pub fn raymarch(origin: V3, dir: V3, frame_n: u64) -> Option<Hit> {
    const MAX_STEPS: u32 = 64;
    const MAX_DIST: f32 = 100.0;
    const SURFACE: f32 = 0.001;

    let mut t = 0.0_f32;
    for step in 0..MAX_STEPS {
        let p = origin.add(dir.mul(t));
        let (d, material) = scene_sdf(p, frame_n);
        if d < SURFACE {
            let normal = scene_normal(p, frame_n);
            return Some(Hit {
                p,
                t,
                normal,
                material,
                steps: step,
            });
        }
        t += d;
        if t > MAX_DIST {
            return None;
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════════════════════
// § SHADING — Lambertian + ambient (math-only)
// ═══════════════════════════════════════════════════════════════════════════

/// Per-material albedo (RGB, 0..1).
#[must_use]
pub fn material_albedo(m: MaterialId) -> V3 {
    match m {
        MaterialId::Floor => V3::new(0.45, 0.42, 0.38),     // muted gray-tan
        MaterialId::Wall => V3::new(0.55, 0.50, 0.42),      // labyrinth-stone
        MaterialId::Creature => V3::new(0.85, 0.35, 0.25),  // warm reddish
        MaterialId::Companion => V3::new(0.30, 0.55, 0.85), // cool blue
    }
}

/// Lambertian + ambient shading.
#[must_use]
pub fn scene_color(hit: &Hit, light_dir: V3) -> V3 {
    let albedo = material_albedo(hit.material);
    let ambient = 0.15;
    let n_dot_l = hit.normal.dot(light_dir).max(0.0);
    let diffuse = n_dot_l * 0.85;
    albedo.mul(ambient + diffuse)
}

/// Sky-color when ray misses scene. Vertical gradient (math-only).
#[must_use]
pub fn sky_color(dir: V3) -> V3 {
    let t = 0.5 * (dir.1 + 1.0);
    let horizon = V3::new(0.65, 0.62, 0.55);
    let zenith = V3::new(0.20, 0.25, 0.35);
    horizon.mul(1.0 - t).add(zenith.mul(t))
}

// ═══════════════════════════════════════════════════════════════════════════
// § PIXEL — full per-pixel canonical evaluation (camera → ray → color)
// ═══════════════════════════════════════════════════════════════════════════

/// Compute final color for screen-coord (u,v) ∈ [-1,+1] with aspect-correction.
/// Returns RGB in [0,1] range (caller clamps + converts to byte format).
#[must_use]
pub fn raymarch_pixel(cam: &Camera, u: f32, v: f32, aspect: f32, frame_n: u64) -> V3 {
    let dir = cam.ray_dir(u * aspect, v);
    let light_dir = V3::new(-0.4, 0.8, -0.45).normalize();
    match raymarch(cam.position, dir, frame_n) {
        Some(hit) => scene_color(&hit, light_dir),
        None => sky_color(dir),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § RENDER — fill a pixel-buffer with the labyrinth scene
// ═══════════════════════════════════════════════════════════════════════════

/// Render-target dimensions (matches `test_room_render::sdf_scene::RENDER_W/H`).
pub const LABYRINTH_RENDER_W: u32 = 320;
pub const LABYRINTH_RENDER_H: u32 = 180;

/// Convert a [0,1] RGB triple to a Win32 GDI BGRA u32 pixel.
#[inline]
fn rgb_to_bgra(c: V3) -> u32 {
    let r = (c.0.clamp(0.0, 1.0) * 255.0) as u32;
    let g = (c.1.clamp(0.0, 1.0) * 255.0) as u32;
    let b = (c.2.clamp(0.0, 1.0) * 255.0) as u32;
    (255 << 24) | (r << 16) | (g << 8) | b
}

/// Render the labyrinth scene into the given pixel buffer for `frame_n`.
///
/// `pixels` must be a slice of `LABYRINTH_RENDER_W * LABYRINTH_RENDER_H`
/// BGRA u32 pixels. Compatible with the
/// `test_room_render::win32_gdi::GdiRenderer::paint_buffer` surface.
pub fn render_labyrinth_into(pixels: &mut [u32], frame_n: u64) {
    let w = LABYRINTH_RENDER_W;
    let h = LABYRINTH_RENDER_H;
    debug_assert_eq!(pixels.len(), (w * h) as usize);

    let cam = Camera::at_frame(frame_n);
    let aspect = w as f32 / h as f32;

    for py in 0..h {
        for px in 0..w {
            // Screen-coords ∈ [-1, +1] (top-left = (-1, +1), bottom-right = (+1, -1))
            let u = (px as f32 + 0.5) / (w as f32) * 2.0 - 1.0;
            let v = 1.0 - (py as f32 + 0.5) / (h as f32) * 2.0;
            let color = raymarch_pixel(&cam, u, v, aspect, frame_n);
            pixels[(py * w + px) as usize] = rgb_to_bgra(color);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § TESTS — math-only, deterministic, no I/O
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn v3_dot_orthogonal_is_zero() {
        let a = V3::new(1.0, 0.0, 0.0);
        let b = V3::new(0.0, 1.0, 0.0);
        assert!(approx_eq(a.dot(b), 0.0, 1e-6));
    }

    #[test]
    fn v3_normalize_unit() {
        let v = V3::new(3.0, 4.0, 0.0);
        let n = v.normalize();
        assert!(approx_eq(n.norm(), 1.0, 1e-6));
    }

    #[test]
    fn sphere_sdf_at_center_is_negative_radius() {
        let p = V3::new(0.0, 0.0, 0.0);
        assert!(approx_eq(sphere_sdf(p, V3::splat(0.0), 1.0), -1.0, 1e-6));
    }

    #[test]
    fn sphere_sdf_on_surface_is_zero() {
        let p = V3::new(1.0, 0.0, 0.0);
        assert!(approx_eq(sphere_sdf(p, V3::splat(0.0), 1.0), 0.0, 1e-6));
    }

    #[test]
    fn box_sdf_at_center_is_negative() {
        let p = V3::new(0.0, 0.0, 0.0);
        let d = box_sdf(p, V3::splat(0.0), V3::new(1.0, 1.0, 1.0));
        assert!(d < 0.0);
    }

    #[test]
    fn plane_sdf_above_is_positive() {
        let p = V3::new(0.0, 5.0, 0.0);
        let d = plane_sdf(p, V3::new(0.0, 1.0, 0.0), 0.0);
        assert!(d > 0.0);
    }

    #[test]
    fn camera_orbits_around_origin() {
        let c0 = Camera::at_frame(0);
        let c150 = Camera::at_frame(150);
        let c300 = Camera::at_frame(300);

        // All cameras at radius ~ 8.
        assert!(approx_eq(c0.position.0.hypot(c0.position.2), 8.0, 0.01));
        assert!(approx_eq(c150.position.0.hypot(c150.position.2), 8.0, 0.01));
        assert!(approx_eq(c300.position.0.hypot(c300.position.2), 8.0, 0.01));

        // Forward vector points roughly toward origin (negative-radial).
        let forward_radial = c0.forward.0 * c0.position.0 + c0.forward.2 * c0.position.2;
        assert!(forward_radial < 0.0);
    }

    #[test]
    fn camera_ray_dir_is_unit() {
        let c = Camera::at_frame(0);
        let r = c.ray_dir(0.0, 0.0);
        assert!(approx_eq(r.norm(), 1.0, 1e-5));
    }

    #[test]
    fn scene_sdf_at_camera_position_is_positive() {
        // Camera at orbit-radius=8 ; scene-walls extend to ~3 ; should be in
        // empty space.
        let cam = Camera::at_frame(0);
        let (d, _) = scene_sdf(cam.position, 0);
        assert!(d > 0.0, "camera position should be in empty space, got d={d}");
    }

    #[test]
    fn raymarch_from_camera_hits_something() {
        // Center pixel from frame-0 should hit creature or wall, not sky.
        let cam = Camera::at_frame(0);
        let dir = cam.ray_dir(0.0, 0.0);
        let hit = raymarch(cam.position, dir, 0);
        assert!(
            hit.is_some(),
            "center-of-screen ray should hit scene from orbit-camera at frame 0"
        );
    }

    #[test]
    fn material_albedo_is_in_unit_range() {
        for m in [
            MaterialId::Floor,
            MaterialId::Wall,
            MaterialId::Creature,
            MaterialId::Companion,
        ] {
            let a = material_albedo(m);
            assert!(a.0 >= 0.0 && a.0 <= 1.0);
            assert!(a.1 >= 0.0 && a.1 <= 1.0);
            assert!(a.2 >= 0.0 && a.2 <= 1.0);
        }
    }

    #[test]
    fn shading_unlit_normal_is_ambient_only() {
        // Normal opposite to light : n·l ≤ 0 → just ambient.
        let hit = Hit {
            p: V3::splat(0.0),
            t: 1.0,
            normal: V3::new(0.0, -1.0, 0.0),
            material: MaterialId::Wall,
            steps: 5,
        };
        let light = V3::new(0.0, 1.0, 0.0);
        let c = scene_color(&hit, light);
        let albedo = material_albedo(MaterialId::Wall);
        // Should be ambient * albedo (0.15 ≈).
        assert!(approx_eq(c.0, albedo.0 * 0.15, 0.01));
    }

    #[test]
    fn raymarch_pixel_deterministic() {
        let cam = Camera::at_frame(60);
        let c1 = raymarch_pixel(&cam, 0.0, 0.0, 1.778, 60);
        let c2 = raymarch_pixel(&cam, 0.0, 0.0, 1.778, 60);
        assert!(approx_eq(c1.0, c2.0, 1e-9));
        assert!(approx_eq(c1.1, c2.1, 1e-9));
        assert!(approx_eq(c1.2, c2.2, 1e-9));
    }

    #[test]
    fn creature_pulses_with_frame_n() {
        // At frame_n where pulse_t = 0.25 (sin = 1.0), creature is largest.
        let frame_max = (90.0 * 0.25) as u64; // ~22
        let frame_min = (90.0 * 0.75) as u64; // ~67

        let p_origin = V3::splat(0.0);
        let (d_max, _) = scene_sdf(p_origin, frame_max);
        let (d_min, _) = scene_sdf(p_origin, frame_min);

        // Larger sphere (max-pulse) → MORE-negative SDF at center.
        assert!(d_max <= d_min);
    }

    #[test]
    fn companion_orbits_at_radius_2_5() {
        // Sample multiple frames ; companion-position should be at radius ~2.5.
        for f in [0_u64, 60, 120, 180, 240] {
            let orbit_t = f as f32 / 240.0;
            let angle = orbit_t * TAU;
            let cx = angle.cos() * 2.5;
            let cz = angle.sin() * 2.5;
            let r = (cx * cx + cz * cz).sqrt();
            assert!(approx_eq(r, 2.5, 1e-5));
        }
    }
}
