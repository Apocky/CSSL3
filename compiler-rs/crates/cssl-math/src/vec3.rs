//! § Vec3 — 3-component f32 vector
//!
//! The workhorse of the math library. Used for positions, directions,
//! velocities, normals, color triplets — anywhere `(x, y, z)` flows.
//! `#[repr(C)]` so a `&[Vec3]` slice casts to `&[f32]` of triple length
//! for SIMD or GPU upload paths.
//!
//! § HANDEDNESS : right-handed, Y-up. `cross(X, Y) = Z` ; `cross(Y, Z) = X` ;
//! `cross(Z, X) = Y`. View-space forward is `-Z`.

use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::scalar::{lerp, EPSILON_F32};

/// 3-component f32 vector with `#[repr(C)]` storage.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct Vec3 {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
    /// Z component.
    pub z: f32,
}

impl Vec3 {
    /// All-zero vector — the world origin.
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    /// All-one vector — `(1, 1, 1)`.
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0);
    /// Positive X axis (substrate canonical "right").
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    /// Positive Y axis (substrate canonical "up").
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    /// Positive Z axis. Note : view-space forward is `-Z` in RH.
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);
    /// Negative X axis ("left").
    pub const NEG_X: Self = Self::new(-1.0, 0.0, 0.0);
    /// Negative Y axis ("down").
    pub const NEG_Y: Self = Self::new(0.0, -1.0, 0.0);
    /// Negative Z axis ("forward" in RH view-space).
    pub const NEG_Z: Self = Self::new(0.0, 0.0, -1.0);

    /// Construct from explicit components.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Splat a single scalar across all three components.
    #[must_use]
    pub const fn splat(v: f32) -> Self {
        Self::new(v, v, v)
    }

    /// Dot product. Returns a scalar.
    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.x
            .mul_add(other.x, self.y.mul_add(other.y, self.z * other.z))
    }

    /// Cross product, RH convention. The result is perpendicular to both
    /// inputs ; the magnitude is `|self| * |other| * sin(theta)`.
    #[must_use]
    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y.mul_add(other.z, -(self.z * other.y)),
            self.z.mul_add(other.x, -(self.x * other.z)),
            self.x.mul_add(other.y, -(self.y * other.x)),
        )
    }

    /// Squared magnitude. Avoids the sqrt for ordering / threshold tests.
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    /// Euclidean magnitude.
    #[must_use]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Reciprocal magnitude. Returns 0 for the zero vector — total
    /// behavior on degenerate input.
    #[must_use]
    pub fn length_recip(self) -> f32 {
        let len_sq = self.length_squared();
        if len_sq > EPSILON_F32 {
            len_sq.sqrt().recip()
        } else {
            0.0
        }
    }

    /// Normalized copy. Returns `Vec3::ZERO` for the zero vector — total
    /// behavior on degenerate input. Use [`Self::try_normalize`] if you
    /// need to detect the failure path explicitly.
    #[must_use]
    pub fn normalize(self) -> Self {
        let len_sq = self.length_squared();
        if len_sq > EPSILON_F32 {
            let inv = len_sq.sqrt().recip();
            Self::new(self.x * inv, self.y * inv, self.z * inv)
        } else {
            Self::ZERO
        }
    }

    /// Fallible normalize. Returns `None` for the zero vector ; otherwise
    /// returns the unit-length copy.
    #[must_use]
    pub fn try_normalize(self) -> Option<Self> {
        let len_sq = self.length_squared();
        if len_sq > EPSILON_F32 {
            let inv = len_sq.sqrt().recip();
            Some(Self::new(self.x * inv, self.y * inv, self.z * inv))
        } else {
            None
        }
    }

    /// Componentwise minimum.
    #[must_use]
    pub fn min(self, other: Self) -> Self {
        Self::new(
            self.x.min(other.x),
            self.y.min(other.y),
            self.z.min(other.z),
        )
    }

    /// Componentwise maximum.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self::new(
            self.x.max(other.x),
            self.y.max(other.y),
            self.z.max(other.z),
        )
    }

    /// Componentwise clamp into the box `[lo, hi]`.
    #[must_use]
    pub fn clamp(self, lo: Self, hi: Self) -> Self {
        self.max(lo).min(hi)
    }

    /// Linear interpolation toward `other`.
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::new(
            lerp(self.x, other.x, t),
            lerp(self.y, other.y, t),
            lerp(self.z, other.z, t),
        )
    }

    /// Componentwise absolute value.
    #[must_use]
    pub fn abs(self) -> Self {
        Self::new(self.x.abs(), self.y.abs(), self.z.abs())
    }

    /// Distance to another point.
    #[must_use]
    pub fn distance(self, other: Self) -> f32 {
        (self - other).length()
    }

    /// Squared distance — avoids the sqrt for ordering / threshold tests.
    #[must_use]
    pub fn distance_squared(self, other: Self) -> f32 {
        (self - other).length_squared()
    }

    /// Project `self` onto `other`. Returns `Vec3::ZERO` if `other` is
    /// the zero vector.
    #[must_use]
    pub fn project_onto(self, other: Self) -> Self {
        let len_sq = other.length_squared();
        if len_sq > EPSILON_F32 {
            other * (self.dot(other) / len_sq)
        } else {
            Self::ZERO
        }
    }

    /// Reflect `self` across the plane with normal `normal` (assumed unit-
    /// length). Returns `self - 2 * dot(self, n) * n`.
    #[must_use]
    pub fn reflect(self, normal: Self) -> Self {
        self - normal * (2.0 * self.dot(normal))
    }

    /// Componentwise reciprocal. Returns 0 for any near-zero component
    /// (totality on degenerate input).
    #[must_use]
    pub fn recip(self) -> Self {
        let safe = |v: f32| if v.abs() > EPSILON_F32 { v.recip() } else { 0.0 };
        Self::new(safe(self.x), safe(self.y), safe(self.z))
    }

    /// Element-wise multiplication (Hadamard product).
    #[must_use]
    pub fn mul_componentwise(self, other: Self) -> Self {
        Self::new(self.x * other.x, self.y * other.y, self.z * other.z)
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}
impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}
impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}
impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}
impl Mul<Vec3> for f32 {
    type Output = Vec3;
    fn mul(self, rhs: Vec3) -> Vec3 {
        rhs * self
    }
}
impl Div<f32> for Vec3 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}
impl AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}
impl SubAssign for Vec3 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}
impl MulAssign<f32> for Vec3 {
    fn mul_assign(&mut self, rhs: f32) {
        *self = *self * rhs;
    }
}
impl DivAssign<f32> for Vec3 {
    fn div_assign(&mut self, rhs: f32) {
        *self = *self / rhs;
    }
}

#[cfg(test)]
mod tests {
    use super::Vec3;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }
    fn vec_approx(a: Vec3, b: Vec3, eps: f32) -> bool {
        approx(a.x, b.x, eps) && approx(a.y, b.y, eps) && approx(a.z, b.z, eps)
    }

    #[test]
    fn vec3_basic_arithmetic() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert_eq!(a + b, Vec3::new(5.0, 7.0, 9.0));
        assert_eq!(b - a, Vec3::new(3.0, 3.0, 3.0));
        assert_eq!(a * 2.0, Vec3::new(2.0, 4.0, 6.0));
        assert_eq!(2.0 * a, Vec3::new(2.0, 4.0, 6.0));
        assert_eq!(b / 2.0, Vec3::new(2.0, 2.5, 3.0));
        assert_eq!(-a, Vec3::new(-1.0, -2.0, -3.0));
    }

    #[test]
    fn vec3_dot_known_values() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        // 1*4 + 2*5 + 3*6 = 4 + 10 + 18 = 32.
        assert!(approx(a.dot(b), 32.0, 1e-5));
    }

    #[test]
    fn vec3_cross_obeys_right_hand_rule() {
        // RH : cross(X, Y) = Z, cross(Y, Z) = X, cross(Z, X) = Y.
        assert!(vec_approx(Vec3::X.cross(Vec3::Y), Vec3::Z, 1e-6));
        assert!(vec_approx(Vec3::Y.cross(Vec3::Z), Vec3::X, 1e-6));
        assert!(vec_approx(Vec3::Z.cross(Vec3::X), Vec3::Y, 1e-6));
        // Anti-symmetry.
        assert!(vec_approx(Vec3::Y.cross(Vec3::X), -Vec3::Z, 1e-6));
    }

    #[test]
    fn vec3_normalize_zero_returns_zero() {
        assert_eq!(Vec3::ZERO.normalize(), Vec3::ZERO);
        let n = Vec3::new(3.0, 4.0, 0.0).normalize();
        assert!(approx(n.length(), 1.0, 1e-6));
    }

    #[test]
    fn vec3_try_normalize_distinguishes_failure() {
        assert_eq!(Vec3::ZERO.try_normalize(), None);
        let v = Vec3::new(3.0, 4.0, 0.0);
        let n = v.try_normalize().expect("non-zero");
        assert!(approx(n.length(), 1.0, 1e-6));
    }

    #[test]
    fn vec3_min_max_componentwise() {
        let a = Vec3::new(1.0, 5.0, 3.0);
        let b = Vec3::new(3.0, 2.0, 7.0);
        assert_eq!(a.min(b), Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(a.max(b), Vec3::new(3.0, 5.0, 7.0));
    }

    #[test]
    fn vec3_clamp_box() {
        let v = Vec3::new(2.0, -3.0, 0.5);
        let lo = Vec3::splat(-1.0);
        let hi = Vec3::splat(1.0);
        assert_eq!(v.clamp(lo, hi), Vec3::new(1.0, -1.0, 0.5));
    }

    #[test]
    fn vec3_lerp_midpoint() {
        let a = Vec3::ZERO;
        let b = Vec3::new(10.0, 20.0, 30.0);
        assert!(vec_approx(a.lerp(b, 0.5), Vec3::new(5.0, 10.0, 15.0), 1e-6));
    }

    #[test]
    fn vec3_distance_known_value() {
        let a = Vec3::ZERO;
        let b = Vec3::new(2.0, 3.0, 6.0);
        // 2² + 3² + 6² = 4 + 9 + 36 = 49 ⇒ length 7.
        assert!(approx(a.distance(b), 7.0, 1e-6));
        assert!(approx(a.distance_squared(b), 49.0, 1e-6));
    }

    #[test]
    fn vec3_project_onto_axis() {
        let v = Vec3::new(3.0, 4.0, 0.0);
        let onto_x = v.project_onto(Vec3::X);
        assert!(vec_approx(onto_x, Vec3::new(3.0, 0.0, 0.0), 1e-6));
    }

    #[test]
    fn vec3_project_onto_zero_returns_zero() {
        let v = Vec3::new(3.0, 4.0, 0.0);
        assert_eq!(v.project_onto(Vec3::ZERO), Vec3::ZERO);
    }

    #[test]
    fn vec3_reflect_across_y_normal_flips_y() {
        let v = Vec3::new(1.0, -2.0, 3.0);
        let n = Vec3::Y;
        // v - 2 * dot(v, n) * n = v - 2 * -2 * Y = v + 4Y = (1, 2, 3).
        assert!(vec_approx(v.reflect(n), Vec3::new(1.0, 2.0, 3.0), 1e-6));
    }

    #[test]
    fn vec3_recip_safe_on_zero_component() {
        let v = Vec3::new(2.0, 0.0, 4.0);
        assert_eq!(v.recip(), Vec3::new(0.5, 0.0, 0.25));
    }

    #[test]
    fn vec3_mul_componentwise() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert_eq!(a.mul_componentwise(b), Vec3::new(4.0, 10.0, 18.0));
    }

    #[test]
    fn vec3_assign_ops() {
        let mut v = Vec3::new(1.0, 2.0, 3.0);
        v += Vec3::new(1.0, 1.0, 1.0);
        assert_eq!(v, Vec3::new(2.0, 3.0, 4.0));
        v -= Vec3::new(1.0, 1.0, 1.0);
        assert_eq!(v, Vec3::new(1.0, 2.0, 3.0));
        v *= 2.0;
        assert_eq!(v, Vec3::new(2.0, 4.0, 6.0));
        v /= 2.0;
        assert_eq!(v, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn vec3_repr_c_layout() {
        // Sanity-check : 3 contiguous f32s.
        assert_eq!(core::mem::size_of::<Vec3>(), 12);
        assert_eq!(core::mem::align_of::<Vec3>(), 4);
    }
}
