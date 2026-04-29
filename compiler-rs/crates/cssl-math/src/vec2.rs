//! § Vec2 — 2-component f32 vector
//!
//! Used for screen-space coordinates, UVs, 2D physics impulses, and any
//! place a `(x, y)` pair flows through the math pipeline. `#[repr(C)]`
//! so `&[Vec2]` casts to `&[f32]` of double length for SIMD or GPU upload.

use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::scalar::{lerp, EPSILON_F32};

/// 2-component f32 vector with `#[repr(C)]` storage.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct Vec2 {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
}

impl Vec2 {
    /// All-zero vector — the 2D origin.
    pub const ZERO: Self = Self::new(0.0, 0.0);
    /// All-one vector — `(1, 1)`.
    pub const ONE: Self = Self::new(1.0, 1.0);
    /// Positive X axis.
    pub const X: Self = Self::new(1.0, 0.0);
    /// Positive Y axis.
    pub const Y: Self = Self::new(0.0, 1.0);

    /// Construct from components.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Splat a scalar across both components.
    #[must_use]
    pub const fn splat(v: f32) -> Self {
        Self::new(v, v)
    }

    /// Dot product.
    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.x.mul_add(other.x, self.y * other.y)
    }

    /// 2D cross product — returns the z-component of the 3D cross of the
    /// two lifted vectors. Sign indicates orientation : positive ⇒ `other`
    /// is to the left of `self` (CCW). Useful for line-side tests +
    /// 2D angular impulses.
    #[must_use]
    pub fn perp_dot(self, other: Self) -> f32 {
        self.x.mul_add(other.y, -(self.y * other.x))
    }

    /// 90-degree counter-clockwise rotation. Returns `(-y, x)`.
    #[must_use]
    pub const fn perp(self) -> Self {
        Self::new(-self.y, self.x)
    }

    /// Squared magnitude — avoids the sqrt when ordering / threshold tests
    /// are sufficient.
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    /// Euclidean magnitude.
    #[must_use]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Normalized copy. Returns `Vec2::ZERO` for the zero vector — total
    /// behavior on degenerate input.
    #[must_use]
    pub fn normalize(self) -> Self {
        let len_sq = self.length_squared();
        if len_sq > EPSILON_F32 {
            let inv = len_sq.sqrt().recip();
            Self::new(self.x * inv, self.y * inv)
        } else {
            Self::ZERO
        }
    }

    /// Componentwise minimum.
    #[must_use]
    pub fn min(self, other: Self) -> Self {
        Self::new(self.x.min(other.x), self.y.min(other.y))
    }

    /// Componentwise maximum.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self::new(self.x.max(other.x), self.y.max(other.y))
    }

    /// Componentwise clamp into the box `[lo, hi]`.
    #[must_use]
    pub fn clamp(self, lo: Self, hi: Self) -> Self {
        self.max(lo).min(hi)
    }

    /// Linear interpolation toward `other`.
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::new(lerp(self.x, other.x, t), lerp(self.y, other.y, t))
    }

    /// Componentwise absolute value.
    #[must_use]
    pub fn abs(self) -> Self {
        Self::new(self.x.abs(), self.y.abs())
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
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}
impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}
impl Neg for Vec2 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}
impl Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}
impl Mul<Vec2> for f32 {
    type Output = Vec2;
    fn mul(self, rhs: Vec2) -> Vec2 {
        rhs * self
    }
}
impl Div<f32> for Vec2 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self::new(self.x / rhs, self.y / rhs)
    }
}
impl AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}
impl SubAssign for Vec2 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}
impl MulAssign<f32> for Vec2 {
    fn mul_assign(&mut self, rhs: f32) {
        *self = *self * rhs;
    }
}
impl DivAssign<f32> for Vec2 {
    fn div_assign(&mut self, rhs: f32) {
        *self = *self / rhs;
    }
}

#[cfg(test)]
mod tests {
    use super::Vec2;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }
    fn vec_approx(a: Vec2, b: Vec2, eps: f32) -> bool {
        approx(a.x, b.x, eps) && approx(a.y, b.y, eps)
    }

    #[test]
    fn vec2_basic_arithmetic() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, 4.0);
        assert_eq!(a + b, Vec2::new(4.0, 6.0));
        assert_eq!(b - a, Vec2::new(2.0, 2.0));
        assert_eq!(a * 2.0, Vec2::new(2.0, 4.0));
        assert_eq!(2.0 * a, Vec2::new(2.0, 4.0));
        assert_eq!(b / 2.0, Vec2::new(1.5, 2.0));
        assert_eq!(-a, Vec2::new(-1.0, -2.0));
    }

    #[test]
    fn vec2_dot_and_perp_dot() {
        let a = Vec2::new(1.0, 0.0);
        let b = Vec2::new(0.0, 1.0);
        // dot of perpendicular = 0.
        assert!(approx(a.dot(b), 0.0, 1e-6));
        // perp_dot of CCW pair is +1.
        assert!(approx(a.perp_dot(b), 1.0, 1e-6));
        // perp_dot of CW pair is -1.
        assert!(approx(b.perp_dot(a), -1.0, 1e-6));
    }

    #[test]
    fn vec2_perp_rotates_90_ccw() {
        let v = Vec2::X;
        assert_eq!(v.perp(), Vec2::Y);
        let v = Vec2::Y;
        assert_eq!(v.perp(), -Vec2::X);
    }

    #[test]
    fn vec2_normalize_zero_returns_zero() {
        assert_eq!(Vec2::ZERO.normalize(), Vec2::ZERO);
        let v = Vec2::new(3.0, 4.0).normalize();
        assert!(approx(v.length(), 1.0, 1e-6));
    }

    #[test]
    fn vec2_min_max_componentwise() {
        let a = Vec2::new(1.0, 5.0);
        let b = Vec2::new(3.0, 2.0);
        assert_eq!(a.min(b), Vec2::new(1.0, 2.0));
        assert_eq!(a.max(b), Vec2::new(3.0, 5.0));
    }

    #[test]
    fn vec2_clamp_box() {
        let v = Vec2::new(2.0, -3.0);
        let lo = Vec2::splat(-1.0);
        let hi = Vec2::splat(1.0);
        assert_eq!(v.clamp(lo, hi), Vec2::new(1.0, -1.0));
    }

    #[test]
    fn vec2_lerp_midpoint() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(10.0, 20.0);
        assert!(vec_approx(a.lerp(b, 0.5), Vec2::new(5.0, 10.0), 1e-6));
    }

    #[test]
    fn vec2_distance_and_squared() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(3.0, 4.0);
        assert!(approx(a.distance(b), 5.0, 1e-6));
        assert!(approx(a.distance_squared(b), 25.0, 1e-6));
    }

    #[test]
    fn vec2_assign_ops() {
        let mut v = Vec2::new(1.0, 2.0);
        v += Vec2::new(3.0, 4.0);
        assert_eq!(v, Vec2::new(4.0, 6.0));
        v -= Vec2::new(1.0, 1.0);
        assert_eq!(v, Vec2::new(3.0, 5.0));
        v *= 2.0;
        assert_eq!(v, Vec2::new(6.0, 10.0));
        v /= 2.0;
        assert_eq!(v, Vec2::new(3.0, 5.0));
    }

    #[test]
    fn vec2_repr_c_layout() {
        // Sanity-check : Vec2 is exactly two floats, contiguous.
        assert_eq!(core::mem::size_of::<Vec2>(), 8);
        assert_eq!(core::mem::align_of::<Vec2>(), 4);
    }
}
