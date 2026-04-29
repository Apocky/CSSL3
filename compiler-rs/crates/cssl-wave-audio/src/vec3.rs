//! § Vec3 — minimal 3-vector for wave-audio listener / source positions.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Wave-audio routinely takes 3D listener position + source position +
//!   propagation direction inputs. We roll a small Vec3 here rather than
//!   pull a dep on the legacy `cssl-audio-mix::voice::Vec3` because :
//!
//!     1. cssl-wave-audio MUST NOT depend on the legacy mixer (per
//!        Cargo.toml § DELIBERATELY-NO-DEP) — they are siblings.
//!     2. The wave-unity axis convention is right-handed `+X right /
//!        +Y up / -Z forward`, identical to the mixer's, so a separate
//!        type that documents the same convention is clearer than an
//!        alias that traverses crate boundaries.
//!
//! § AXIS CONVENTION
//!   `+X = right` (listener-relative), `+Y = up`, `-Z = forward`. This
//!   matches the legacy mixer + the standard OpenAL listener orientation.

/// 3-vector in `f32` precision. `repr(C)` so the type can be used in
/// std430 GPU buffers when the LBM kernel migrates to GPU.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct Vec3 {
    /// X-component (right-axis under standard listener orientation).
    pub x: f32,
    /// Y-component (up-axis).
    pub y: f32,
    /// Z-component (forward-axis ; `-Z = forward`).
    pub z: f32,
}

impl Vec3 {
    /// The zero vector (additive identity).
    pub const ZERO: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Forward unit vector under right-handed convention : `(0, 0, -1)`.
    pub const FORWARD: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: -1.0,
    };

    /// Up unit vector : `(0, 1, 0)`.
    pub const UP: Vec3 = Vec3 {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };

    /// Right unit vector : `(1, 0, 0)`.
    pub const RIGHT: Vec3 = Vec3 {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    };

    /// Construct a new 3-vector.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3 { x, y, z }
    }

    /// Construct from a 3-element array.
    #[must_use]
    pub const fn from_array(a: [f32; 3]) -> Vec3 {
        Vec3 {
            x: a[0],
            y: a[1],
            z: a[2],
        }
    }

    /// Convert to a 3-element array.
    #[must_use]
    pub const fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    /// Componentwise addition.
    #[must_use]
    pub fn add(self, rhs: Vec3) -> Vec3 {
        Vec3::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }

    /// Componentwise subtraction.
    #[must_use]
    pub fn sub(self, rhs: Vec3) -> Vec3 {
        Vec3::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }

    /// Scalar multiplication.
    #[must_use]
    pub fn scale(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }

    /// Dot product.
    #[must_use]
    pub fn dot(self, rhs: Vec3) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    /// Cross product (right-handed).
    #[must_use]
    pub fn cross(self, rhs: Vec3) -> Vec3 {
        Vec3::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }

    /// Squared length.
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    /// Euclidean length.
    #[must_use]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Normalize ; returns `Vec3::ZERO` when length is below epsilon to
    /// avoid NaN propagation onto the audio thread.
    #[must_use]
    pub fn normalize(self) -> Vec3 {
        let len = self.length();
        if len < 1e-12 {
            return Vec3::ZERO;
        }
        self.scale(1.0 / len)
    }

    /// Linear interpolation : `(1-t)·a + t·b`.
    #[must_use]
    pub fn lerp(self, rhs: Vec3, t: f32) -> Vec3 {
        Vec3::new(
            self.x + (rhs.x - self.x) * t,
            self.y + (rhs.y - self.y) * t,
            self.z + (rhs.z - self.z) * t,
        )
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::Vec3;

    #[test]
    fn zero_is_default() {
        assert_eq!(Vec3::default(), Vec3::ZERO);
    }

    #[test]
    fn forward_is_minus_z() {
        assert_eq!(Vec3::FORWARD, Vec3::new(0.0, 0.0, -1.0));
    }

    #[test]
    fn up_is_plus_y() {
        assert_eq!(Vec3::UP, Vec3::new(0.0, 1.0, 0.0));
    }

    #[test]
    fn right_is_plus_x() {
        assert_eq!(Vec3::RIGHT, Vec3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn from_to_array_roundtrip() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(Vec3::from_array(v.to_array()), v);
    }

    #[test]
    fn add_componentwise() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert_eq!(a.add(b), Vec3::new(5.0, 7.0, 9.0));
    }

    #[test]
    fn sub_componentwise() {
        let a = Vec3::new(5.0, 7.0, 9.0);
        let b = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(a.sub(b), Vec3::new(4.0, 5.0, 6.0));
    }

    #[test]
    fn scale_componentwise() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(v.scale(2.0), Vec3::new(2.0, 4.0, 6.0));
    }

    #[test]
    fn dot_orthogonal_zero() {
        assert_eq!(Vec3::FORWARD.dot(Vec3::UP), 0.0);
        assert_eq!(Vec3::FORWARD.dot(Vec3::RIGHT), 0.0);
        assert_eq!(Vec3::UP.dot(Vec3::RIGHT), 0.0);
    }

    #[test]
    fn dot_self_is_length_squared() {
        let v = Vec3::new(3.0, 4.0, 0.0);
        assert_eq!(v.dot(v), 25.0);
    }

    #[test]
    fn cross_x_y_is_z() {
        // Right-handed : (+X) × (+Y) = (+Z).
        let r = Vec3::RIGHT.cross(Vec3::UP);
        assert!((r.z - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cross_forward_up_is_minus_right() {
        // (-Z) × (+Y) = (+X) using right-hand rule.
        let r = Vec3::FORWARD.cross(Vec3::UP);
        assert!((r.x - 1.0).abs() < 1e-6);
        assert!(r.y.abs() < 1e-6);
        assert!(r.z.abs() < 1e-6);
    }

    #[test]
    fn length_345_triangle() {
        let v = Vec3::new(3.0, 4.0, 0.0);
        assert!((v.length() - 5.0).abs() < 1e-6);
    }

    #[test]
    fn normalize_preserves_direction_unit_length() {
        let v = Vec3::new(2.0, 0.0, 0.0);
        let n = v.normalize();
        assert!((n.length() - 1.0).abs() < 1e-6);
        assert!((n.x - 1.0).abs() < 1e-6);
    }

    #[test]
    fn normalize_zero_returns_zero() {
        let v = Vec3::ZERO;
        assert_eq!(v.normalize(), Vec3::ZERO);
    }

    #[test]
    fn lerp_t_zero_is_self() {
        let a = Vec3::new(1.0, 0.0, 0.0);
        let b = Vec3::new(0.0, 1.0, 0.0);
        assert_eq!(a.lerp(b, 0.0), a);
    }

    #[test]
    fn lerp_t_one_is_other() {
        let a = Vec3::new(1.0, 0.0, 0.0);
        let b = Vec3::new(0.0, 1.0, 0.0);
        assert_eq!(a.lerp(b, 1.0), b);
    }

    #[test]
    fn lerp_midpoint_average() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(2.0, 2.0, 2.0);
        assert_eq!(a.lerp(b, 0.5), Vec3::new(1.0, 1.0, 1.0));
    }

    #[test]
    fn determinism_replay_bit_equal() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(0.5, 0.7, -0.3);
        let r1 = a.add(b).cross(Vec3::FORWARD).normalize();
        let r2 = a.add(b).cross(Vec3::FORWARD).normalize();
        assert_eq!(r1.x.to_bits(), r2.x.to_bits());
        assert_eq!(r1.y.to_bits(), r2.y.to_bits());
        assert_eq!(r1.z.to_bits(), r2.z.to_bits());
    }
}
