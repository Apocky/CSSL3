//! § Vec4 — 4-component f32 vector
//!
//! Used for homogeneous coordinates in clip-space, RGBA color, and any
//! place a `(x, y, z, w)` quadruplet flows. `#[repr(C)]` so a `&[Vec4]`
//! slice casts directly to `&[f32]` of quadruple length for SIMD or GPU
//! upload.

use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::scalar::{lerp, EPSILON_F32};
use crate::vec3::Vec3;

/// 4-component f32 vector with `#[repr(C)]` storage.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct Vec4 {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
    /// Z component.
    pub z: f32,
    /// W component (homogeneous coordinate).
    pub w: f32,
}

impl Vec4 {
    /// All-zero 4-vector.
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0, 0.0);
    /// All-one 4-vector — `(1, 1, 1, 1)`.
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0, 1.0);

    /// Construct from explicit components.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    /// Splat a single scalar across all four components.
    #[must_use]
    pub const fn splat(v: f32) -> Self {
        Self::new(v, v, v, v)
    }

    /// Lift a `Vec3` to a `Vec4` with explicit `w`. `w = 1` for points,
    /// `w = 0` for directions.
    #[must_use]
    pub const fn from_vec3(v: Vec3, w: f32) -> Self {
        Self::new(v.x, v.y, v.z, w)
    }

    /// Drop `w` and return the `xyz` triplet.
    #[must_use]
    pub const fn xyz(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }

    /// Drop `xyz` and return only `w`.
    #[must_use]
    pub const fn w(self) -> f32 {
        self.w
    }

    /// Dot product. Returns a scalar.
    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.x.mul_add(
            other.x,
            self.y
                .mul_add(other.y, self.z.mul_add(other.z, self.w * other.w)),
        )
    }

    /// Squared magnitude.
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    /// Euclidean magnitude.
    #[must_use]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Normalized copy. Returns `Vec4::ZERO` for the zero vector.
    #[must_use]
    pub fn normalize(self) -> Self {
        let len_sq = self.length_squared();
        if len_sq > EPSILON_F32 {
            let inv = len_sq.sqrt().recip();
            Self::new(self.x * inv, self.y * inv, self.z * inv, self.w * inv)
        } else {
            Self::ZERO
        }
    }

    /// Componentwise minimum.
    #[must_use]
    pub fn min(self, other: Self) -> Self {
        Self::new(
            self.x.min(other.x),
            self.y.min(other.y),
            self.z.min(other.z),
            self.w.min(other.w),
        )
    }

    /// Componentwise maximum.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self::new(
            self.x.max(other.x),
            self.y.max(other.y),
            self.z.max(other.z),
            self.w.max(other.w),
        )
    }

    /// Linear interpolation toward `other`.
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::new(
            lerp(self.x, other.x, t),
            lerp(self.y, other.y, t),
            lerp(self.z, other.z, t),
            lerp(self.w, other.w, t),
        )
    }

    /// Perspective divide — divide `xyz` by `w`, returning normalized
    /// device coordinates. Returns `Vec3::ZERO` if `w` is near-zero
    /// rather than producing NaN / infinity (substrate-level totality).
    #[must_use]
    pub fn perspective_divide(self) -> Vec3 {
        if self.w.abs() > EPSILON_F32 {
            let inv = self.w.recip();
            Vec3::new(self.x * inv, self.y * inv, self.z * inv)
        } else {
            Vec3::ZERO
        }
    }
}

impl Add for Vec4 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(
            self.x + rhs.x,
            self.y + rhs.y,
            self.z + rhs.z,
            self.w + rhs.w,
        )
    }
}
impl Sub for Vec4 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(
            self.x - rhs.x,
            self.y - rhs.y,
            self.z - rhs.z,
            self.w - rhs.w,
        )
    }
}
impl Neg for Vec4 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z, -self.w)
    }
}
impl Mul<f32> for Vec4 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs, self.w * rhs)
    }
}
impl Mul<Vec4> for f32 {
    type Output = Vec4;
    fn mul(self, rhs: Vec4) -> Vec4 {
        rhs * self
    }
}
impl Div<f32> for Vec4 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs, self.w / rhs)
    }
}
impl AddAssign for Vec4 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}
impl SubAssign for Vec4 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}
impl MulAssign<f32> for Vec4 {
    fn mul_assign(&mut self, rhs: f32) {
        *self = *self * rhs;
    }
}
impl DivAssign<f32> for Vec4 {
    fn div_assign(&mut self, rhs: f32) {
        *self = *self / rhs;
    }
}

#[cfg(test)]
mod tests {
    use super::Vec4;
    use crate::vec3::Vec3;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }
    fn vec3_approx(a: Vec3, b: Vec3, eps: f32) -> bool {
        approx(a.x, b.x, eps) && approx(a.y, b.y, eps) && approx(a.z, b.z, eps)
    }

    #[test]
    fn vec4_basic_arithmetic() {
        let a = Vec4::new(1.0, 2.0, 3.0, 4.0);
        let b = Vec4::new(5.0, 6.0, 7.0, 8.0);
        assert_eq!(a + b, Vec4::new(6.0, 8.0, 10.0, 12.0));
        assert_eq!(b - a, Vec4::new(4.0, 4.0, 4.0, 4.0));
        assert_eq!(a * 2.0, Vec4::new(2.0, 4.0, 6.0, 8.0));
        assert_eq!(2.0 * a, Vec4::new(2.0, 4.0, 6.0, 8.0));
        assert_eq!(b / 2.0, Vec4::new(2.5, 3.0, 3.5, 4.0));
        assert_eq!(-a, Vec4::new(-1.0, -2.0, -3.0, -4.0));
    }

    #[test]
    fn vec4_dot_known_values() {
        let a = Vec4::new(1.0, 2.0, 3.0, 4.0);
        let b = Vec4::new(5.0, 6.0, 7.0, 8.0);
        // 5 + 12 + 21 + 32 = 70.
        assert!(approx(a.dot(b), 70.0, 1e-5));
    }

    #[test]
    fn vec4_normalize_zero_returns_zero() {
        assert_eq!(Vec4::ZERO.normalize(), Vec4::ZERO);
        let n = Vec4::new(1.0, 2.0, 2.0, 0.0).normalize();
        assert!(approx(n.length(), 1.0, 1e-6));
    }

    #[test]
    fn vec4_perspective_divide_total() {
        // w == 0 must not crash or NaN ; substrate totality.
        let v = Vec4::new(1.0, 2.0, 3.0, 0.0);
        assert_eq!(v.perspective_divide(), Vec3::ZERO);
        // Normal case.
        let v = Vec4::new(2.0, 4.0, 6.0, 2.0);
        assert!(vec3_approx(
            v.perspective_divide(),
            Vec3::new(1.0, 2.0, 3.0),
            1e-6
        ));
    }

    #[test]
    fn vec4_from_vec3_preserves_xyz() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        let p = Vec4::from_vec3(v, 1.0);
        assert_eq!(p, Vec4::new(1.0, 2.0, 3.0, 1.0));
        assert_eq!(p.xyz(), v);
    }

    #[test]
    fn vec4_lerp_midpoint() {
        let a = Vec4::ZERO;
        let b = Vec4::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(a.lerp(b, 0.5), Vec4::new(5.0, 10.0, 15.0, 20.0));
    }

    #[test]
    fn vec4_min_max_componentwise() {
        let a = Vec4::new(1.0, 5.0, 3.0, 7.0);
        let b = Vec4::new(3.0, 2.0, 7.0, 1.0);
        assert_eq!(a.min(b), Vec4::new(1.0, 2.0, 3.0, 1.0));
        assert_eq!(a.max(b), Vec4::new(3.0, 5.0, 7.0, 7.0));
    }

    #[test]
    fn vec4_assign_ops() {
        let mut v = Vec4::new(1.0, 2.0, 3.0, 4.0);
        v += Vec4::ONE;
        assert_eq!(v, Vec4::new(2.0, 3.0, 4.0, 5.0));
        v -= Vec4::ONE;
        assert_eq!(v, Vec4::new(1.0, 2.0, 3.0, 4.0));
        v *= 2.0;
        assert_eq!(v, Vec4::new(2.0, 4.0, 6.0, 8.0));
        v /= 2.0;
        assert_eq!(v, Vec4::new(1.0, 2.0, 3.0, 4.0));
    }

    #[test]
    fn vec4_repr_c_layout() {
        assert_eq!(core::mem::size_of::<Vec4>(), 16);
        assert_eq!(core::mem::align_of::<Vec4>(), 4);
    }
}
