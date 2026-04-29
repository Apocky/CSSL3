//! § Point — grade-3 element of G(3,0,1) (Klein-PGA)
//!
//! In Klein-style PGA, **points are grade-3 trivectors** — 4 components on
//! the basis `e₀₃₂ e₀₁₃ e₀₂₁ e₁₂₃`. A finite point at world coordinates
//! `(x, y, z)` maps to the multivector
//!   `x·e₀₃₂ + y·e₀₁₃ + z·e₀₂₁ + 1·e₁₂₃`.
//!
//! The `e₁₂₃` "weight" component plays the role of the homogeneous
//! coordinate `w` from projective geometry — points at infinity have
//! `e₁₂₃ = 0`.
//!
//! § WHY GRADE-3 AND NOT GRADE-1
//!   In the dual algebra (Klein-style) the natural representation of a
//!   point is grade-3 because a point is the **meet** of three planes —
//!   and the meet of grade-1 elements is grade-3 in the wedge-product
//!   sense (in plane-based PGA, the operations `^` and `&` swap roles
//!   relative to point-based PGA). See `Omniverse/01_AXIOMS/10_OPUS_MATH §
//!   I` and the Bivector.net "Plane-Based PGA Cheat Sheet" for the
//!   pedagogical derivation.

use crate::basis::BLADE_COUNT;
use crate::multivector::Multivector;

/// A point in 3D space, stored as a grade-3 trivector. The components
/// `(e₀₃₂, e₀₁₃, e₀₂₁, e₁₂₃)` map to `(x, y, z, w)` where `w` is the
/// homogeneous weight.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct Point {
    /// `e₀₃₂` coefficient — x.
    pub e032: f32,
    /// `e₀₁₃` coefficient — y.
    pub e013: f32,
    /// `e₀₂₁` coefficient — z.
    pub e021: f32,
    /// `e₁₂₃` coefficient — w (homogeneous weight ; 1 for finite points).
    pub e123: f32,
}

impl Point {
    /// Construct a finite point at `(x, y, z)` with weight 1.
    #[must_use]
    pub const fn from_xyz(x: f32, y: f32, z: f32) -> Self {
        Self {
            e032: x,
            e013: y,
            e021: z,
            e123: 1.0,
        }
    }

    /// Construct from explicit `(x, y, z, w)` — useful for points at
    /// infinity (`w = 0`) or for un-normalized homogeneous points.
    #[must_use]
    pub const fn from_xyzw(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self {
            e032: x,
            e013: y,
            e021: z,
            e123: w,
        }
    }

    /// Project to Euclidean `(x, y, z)` by dividing through by the weight.
    /// For finite points (`w = 1`) this is identity ; for un-normalized
    /// homogeneous points this performs the perspective divide. Returns
    /// `(0, 0, 0)` for points at infinity (totality).
    #[must_use]
    pub fn to_xyz(self) -> (f32, f32, f32) {
        if self.e123.abs() > 1e-12 {
            let inv = self.e123.recip();
            (self.e032 * inv, self.e013 * inv, self.e021 * inv)
        } else {
            (0.0, 0.0, 0.0)
        }
    }

    /// Embed this point as a full 16-component multivector.
    #[must_use]
    pub fn to_multivector(self) -> Multivector {
        let mut a = [0.0_f32; BLADE_COUNT];
        a[11] = self.e032;
        a[12] = self.e013;
        a[13] = self.e021;
        a[14] = self.e123;
        Multivector::from_array(a)
    }

    /// Try to extract a point from a general multivector — the grade-3
    /// projection. Components on other grades are silently dropped.
    #[must_use]
    pub fn from_multivector(mv: &Multivector) -> Self {
        Self {
            e032: mv.e032(),
            e013: mv.e013(),
            e021: mv.e021(),
            e123: mv.e123(),
        }
    }

    /// Renormalize so the homogeneous weight is exactly 1. For finite
    /// points (non-zero weight) this is the perspective-divide path ; for
    /// points at infinity it is identity.
    #[must_use]
    pub fn normalize(self) -> Self {
        if self.e123.abs() > 1e-12 {
            let inv = self.e123.recip();
            Self {
                e032: self.e032 * inv,
                e013: self.e013 * inv,
                e021: self.e021 * inv,
                e123: 1.0,
            }
        } else {
            self
        }
    }

    /// True if this point is at infinity (zero weight).
    #[must_use]
    pub fn is_at_infinity(self) -> bool {
        self.e123.abs() < 1e-12
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn point_finite_constructor_has_weight_one() {
        let p = Point::from_xyz(1.0, 2.0, 3.0);
        assert!(approx(p.e123, 1.0));
        assert!(approx(p.e032, 1.0));
        assert!(approx(p.e013, 2.0));
        assert!(approx(p.e021, 3.0));
    }

    #[test]
    fn point_to_xyz_recovers_input() {
        let p = Point::from_xyz(1.5, -2.0, 3.25);
        let (x, y, z) = p.to_xyz();
        assert!(approx(x, 1.5));
        assert!(approx(y, -2.0));
        assert!(approx(z, 3.25));
    }

    #[test]
    fn point_normalize_is_perspective_divide() {
        let p = Point::from_xyzw(2.0, 4.0, 6.0, 2.0);
        let n = p.normalize();
        assert!(approx(n.e032, 1.0));
        assert!(approx(n.e013, 2.0));
        assert!(approx(n.e021, 3.0));
        assert!(approx(n.e123, 1.0));
    }

    #[test]
    fn point_at_infinity_detected() {
        let p = Point::from_xyzw(1.0, 0.0, 0.0, 0.0);
        assert!(p.is_at_infinity());
    }

    #[test]
    fn point_round_trips_through_multivector() {
        let p = Point::from_xyz(1.5, -2.0, 3.25);
        let mv = p.to_multivector();
        let p2 = Point::from_multivector(&mv);
        assert_eq!(p, p2);
    }
}
