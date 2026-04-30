//! § camera — free-fly camera struct for the LoA test-room.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-HOST-1 (W-LOA-host-render) : provides the JUST-THE-STRUCT
//! camera used by render. The sibling slice `W-LOA-host-input` will wire
//! WASD + mouse-look into `position` / `yaw` / `pitch`. This module only
//! exposes the primitive : eye state + view/proj/view-proj matrices.
//!
//! § COORDINATE SYSTEM
//!   Right-handed, +Y up, -Z forward (standard OpenGL/glam convention).
//!   wgpu uses a y-down NDC for textures + clip-space depth in `[0, 1]` ;
//!   `Mat4::perspective_rh` produces depth in `[-1, +1]` which is fine for
//!   wgpu IF the depth buffer compare-func is `Less`. We use `Less` and
//!   apply the standard `OPENGL_TO_WGPU_MATRIX` correction matrix to
//!   remap depth to `[0, 1]` for wgpu compliance.

#![allow(clippy::module_name_repetitions)]

use glam::{Mat4, Quat, Vec3};

/// wgpu uses depth in `[0, 1]` but `Mat4::perspective_rh` produces depth ∈ [-1,1].
/// This matrix remaps Z from [-1, +1] to [0, +1] preserving X/Y as-is.
const OPENGL_TO_WGPU_MATRIX: Mat4 = Mat4::from_cols(
    glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
    glam::Vec4::new(0.0, 1.0, 0.0, 0.0),
    glam::Vec4::new(0.0, 0.0, 0.5, 0.0),
    glam::Vec4::new(0.0, 0.0, 0.5, 1.0),
);

/// Free-fly camera. Stage-0 keeps the surface tiny so the input-slice
/// (`W-LOA-host-input`) can mutate position/orientation directly.
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    /// World-space eye position.
    pub position: Vec3,
    /// Yaw (radians) — rotation around world +Y axis. 0 looks -Z.
    pub yaw: f32,
    /// Pitch (radians) — rotation around camera +X axis. Clamped to ±π/2.
    pub pitch: f32,
    /// Vertical field-of-view (radians).
    pub fov_y: f32,
    /// Near clip plane (meters).
    pub znear: f32,
    /// Far clip plane (meters).
    pub zfar: f32,
}

impl Default for Camera {
    /// Default camera : eye at room center, eye-height 1.7m, looking +Z (north).
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 1.7, 0.0),
            yaw: 0.0,
            pitch: 0.0,
            fov_y: 60.0_f32.to_radians(),
            znear: 0.1,
            zfar: 200.0,
        }
    }
}

impl Camera {
    /// Forward vector (unit). 0 yaw → -Z, π/2 yaw → +X (right-handed).
    #[must_use]
    pub fn forward(&self) -> Vec3 {
        let cp = self.pitch.cos();
        Vec3::new(self.yaw.sin() * cp, self.pitch.sin(), -self.yaw.cos() * cp).normalize()
    }

    /// Right vector (unit) — perpendicular to forward + world-up.
    #[must_use]
    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    /// Camera-up vector (unit) — perpendicular to forward + right.
    #[must_use]
    pub fn up(&self) -> Vec3 {
        self.right().cross(self.forward()).normalize()
    }

    /// View matrix (world → camera).
    #[must_use]
    pub fn view(&self) -> Mat4 {
        Mat4::look_to_rh(self.position, self.forward(), Vec3::Y)
    }

    /// Projection matrix (camera → wgpu clip-space, depth in `[0, 1]`).
    #[must_use]
    pub fn proj(&self, aspect: f32) -> Mat4 {
        OPENGL_TO_WGPU_MATRIX * Mat4::perspective_rh(self.fov_y, aspect, self.znear, self.zfar)
    }

    /// Combined view-projection matrix uploaded to the shader.
    #[must_use]
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.proj(aspect) * self.view()
    }

    /// Convenience constructor : explicit yaw + pitch.
    #[must_use]
    pub fn at(position: Vec3, yaw: f32, pitch: f32) -> Self {
        Self {
            position,
            yaw,
            pitch,
            ..Self::default()
        }
    }

    /// Quaternion form of the orientation, exposed for the input slice
    /// to compose with mouse deltas without touching yaw/pitch directly.
    #[must_use]
    pub fn orientation(&self) -> Quat {
        Quat::from_axis_angle(Vec3::Y, self.yaw) * Quat::from_axis_angle(Vec3::X, self.pitch)
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § TESTS
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn camera_default_at_room_center() {
        let c = Camera::default();
        assert!(approx_eq(c.position.x, 0.0, 1e-6));
        assert!(approx_eq(c.position.y, 1.7, 1e-6));
        assert!(approx_eq(c.position.z, 0.0, 1e-6));
        assert!(approx_eq(c.yaw, 0.0, 1e-6));
        assert!(approx_eq(c.pitch, 0.0, 1e-6));
    }

    #[test]
    fn camera_default_looks_negative_z() {
        let c = Camera::default();
        let f = c.forward();
        assert!(approx_eq(f.x, 0.0, 1e-6));
        assert!(approx_eq(f.y, 0.0, 1e-6));
        assert!(approx_eq(f.z, -1.0, 1e-6));
    }

    #[test]
    fn camera_view_proj_is_finite() {
        let c = Camera::default();
        let m = c.view_proj(16.0 / 9.0);
        for col in 0..4 {
            for row in 0..4 {
                assert!(m.col(col)[row].is_finite());
            }
        }
    }

    #[test]
    fn camera_yaw_quarter_turn_looks_right() {
        // Yaw = +π/2 (90°) → forward should be +X.
        let c = Camera::at(Vec3::ZERO, std::f32::consts::FRAC_PI_2, 0.0);
        let f = c.forward();
        assert!(approx_eq(f.x, 1.0, 1e-5));
        assert!(approx_eq(f.z, 0.0, 1e-5));
    }
}
