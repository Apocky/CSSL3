//! § Rotor — unit-norm even-graded rotation in G(3,0,1)
//!
//! A rotor is the PGA replacement for a quaternion. It is an element of
//! the **even subalgebra** restricted to grades 0 + 2-spatial — 4
//! components (one scalar + three spatial bivectors) :
//!
//!   `R = s + b₁·e₂₃ + b₂·e₃₁ + b₃·e₁₂`
//!
//! For a unit rotor representing a rotation by `θ` about a unit axis
//! `(nx, ny, nz)`, the canonical form is :
//!
//!   `R = cos(θ/2) - sin(θ/2) · (nx·e₂₃ + ny·e₃₁ + nz·e₁₂)`
//!
//! § QUATERNION CORRESPONDENCE
//!   `q = (qx, qy, qz, qw)` Hamilton quaternion ↔ rotor `(s, b₁, b₂, b₃)
//!   = (qw, -qx, -qy, -qz)`. The sign flip on the bivector components
//!   matches the negative-bivector form `R = cos(θ/2) - sin(θ/2)·B̂`
//!   (the Klein-PGA convention chosen so two reflections compose to a
//!   rotor without an extra sign flip — see `Omniverse/01_AXIOMS/
//!   10_OPUS_MATH § I`).

use crate::basis::BLADE_COUNT;
use crate::multivector::Multivector;

/// A unit rotor — element of the rotation subgroup of the motor group.
///
/// Storage `(s, b1, b2, b3)` where `b1, b2, b3` are the `e₂₃, e₃₁, e₁₂`
/// coefficients respectively.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Rotor {
    /// Scalar (cos θ/2) component.
    pub s: f32,
    /// `e₂₃` coefficient — x-axis rotation generator weight.
    pub b1: f32,
    /// `e₃₁` coefficient — y-axis rotation generator weight.
    pub b2: f32,
    /// `e₁₂` coefficient — z-axis rotation generator weight.
    pub b3: f32,
}

impl Default for Rotor {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Rotor {
    /// Identity rotor — no rotation.
    pub const IDENTITY: Self = Self {
        s: 1.0,
        b1: 0.0,
        b2: 0.0,
        b3: 0.0,
    };

    /// Construct from raw `(s, b₁, b₂, b₃)`. Caller is responsible for
    /// unit-norm — most users should prefer [`Self::from_axis_angle`].
    #[must_use]
    pub const fn from_components(s: f32, b1: f32, b2: f32, b3: f32) -> Self {
        Self { s, b1, b2, b3 }
    }

    /// Construct from a unit-length rotation axis and an angle.
    /// The axis is normalized internally for safety.
    #[must_use]
    pub fn from_axis_angle(ax: f32, ay: f32, az: f32, angle_rad: f32) -> Self {
        let n2 = ax * ax + ay * ay + az * az;
        let (nx, ny, nz) = if n2 > 1e-12 {
            let inv = n2.sqrt().recip();
            (ax * inv, ay * inv, az * inv)
        } else {
            // Degenerate axis — return identity.
            return Self::IDENTITY;
        };
        let half = angle_rad * 0.5;
        let s = half.cos();
        let neg_sin = -half.sin();
        Self {
            s,
            b1: neg_sin * nx,
            b2: neg_sin * ny,
            b3: neg_sin * nz,
        }
    }

    /// Squared norm `s² + b₁² + b₂² + b₃²`. A unit rotor has
    /// `norm_squared == 1` modulo accumulated drift.
    #[must_use]
    pub fn norm_squared(self) -> f32 {
        self.s * self.s + self.b1 * self.b1 + self.b2 * self.b2 + self.b3 * self.b3
    }

    /// Euclidean norm.
    #[must_use]
    pub fn norm(self) -> f32 {
        self.norm_squared().sqrt()
    }

    /// Renormalize to unit length. Returns identity for the zero rotor.
    #[must_use]
    pub fn normalize(self) -> Self {
        let n2 = self.norm_squared();
        if n2 > 1e-12 {
            let inv = n2.sqrt().recip();
            Self {
                s: self.s * inv,
                b1: self.b1 * inv,
                b2: self.b2 * inv,
                b3: self.b3 * inv,
            }
        } else {
            Self::IDENTITY
        }
    }

    /// Reverse `~R` — conjugate by the reverse involution. For a unit
    /// rotor this equals the inverse.
    #[must_use]
    pub fn reverse(self) -> Self {
        Self {
            s: self.s,
            b1: -self.b1,
            b2: -self.b2,
            b3: -self.b3,
        }
    }

    /// Compose two rotors `self * other`. The result is the rotation that
    /// first applies `other` then `self` (matching the matrix-multiplication
    /// reading direction).
    #[must_use]
    pub fn compose(self, other: Self) -> Self {
        // Closed-form even-subalgebra product, restricted to grade-0 +
        // grade-2-spatial. Derived from the geometric-product blade-table
        // restricted to the rotor subspace.
        // (s + b₁ e₂₃ + b₂ e₃₁ + b₃ e₁₂) * (s' + b'₁ e₂₃ + b'₂ e₃₁ + b'₃ e₁₂)
        // = ss' - b₁b'₁ - b₂b'₂ - b₃b'₃                             [scalar]
        //   + (sb'₁ + b₁s' - b₂b'₃ + b₃b'₂) e₂₃                     [b₁]
        //   + (sb'₂ + b₁b'₃ + b₂s' - b₃b'₁) e₃₁                     [b₂]
        //   + (sb'₃ - b₁b'₂ + b₂b'₁ + b₃s') e₁₂                     [b₃]
        let (s, b1, b2, b3) = (self.s, self.b1, self.b2, self.b3);
        let (s2, c1, c2, c3) = (other.s, other.b1, other.b2, other.b3);
        Self {
            s: s * s2 - b1 * c1 - b2 * c2 - b3 * c3,
            b1: s * c1 + b1 * s2 - b2 * c3 + b3 * c2,
            b2: s * c2 + b1 * c3 + b2 * s2 - b3 * c1,
            b3: s * c3 - b1 * c2 + b2 * c1 + b3 * s2,
        }
    }

    /// Embed this rotor as a full 16-component multivector.
    #[must_use]
    pub fn to_multivector(self) -> Multivector {
        let mut a = [0.0_f32; BLADE_COUNT];
        a[0] = self.s;
        a[5] = self.b1;
        a[6] = self.b2;
        a[7] = self.b3;
        Multivector::from_array(a)
    }

    /// Extract a rotor from a general multivector — grade-0 + spatial-
    /// bivector projection. Components on other blades are silently
    /// dropped.
    #[must_use]
    pub fn from_multivector(mv: &Multivector) -> Self {
        Self {
            s: mv.s(),
            b1: mv.e23(),
            b2: mv.e31(),
            b3: mv.e12(),
        }
    }

    /// Apply this rotor to a multivector by sandwich `R v R̃`.
    #[must_use]
    pub fn apply(self, v: &Multivector) -> Multivector {
        self.to_multivector().sandwich(v)
    }
}

impl core::ops::Mul for Rotor {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        self.compose(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::point::Point;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn rotor_identity_has_unit_norm() {
        assert!(approx(Rotor::IDENTITY.norm_squared(), 1.0));
    }

    #[test]
    fn rotor_axis_angle_yields_unit_rotor() {
        let r = Rotor::from_axis_angle(0.0, 1.0, 0.0, 1.234);
        assert!(approx(r.norm_squared(), 1.0));
    }

    #[test]
    fn rotor_axis_angle_degenerate_axis_returns_identity() {
        let r = Rotor::from_axis_angle(0.0, 0.0, 0.0, 1.0);
        assert!(approx(r.s, 1.0));
        assert!(approx(r.b1, 0.0));
    }

    #[test]
    fn rotor_compose_with_identity_is_identity() {
        let r = Rotor::from_axis_angle(0.0, 1.0, 0.0, 0.7);
        let composed = r * Rotor::IDENTITY;
        assert!(approx(composed.s, r.s));
        assert!(approx(composed.b1, r.b1));
        assert!(approx(composed.b2, r.b2));
        assert!(approx(composed.b3, r.b3));
    }

    #[test]
    fn rotor_double_application_doubles_angle() {
        // Two applications of a 45-deg rotation = 90-deg rotation.
        let r45 = Rotor::from_axis_angle(0.0, 1.0, 0.0, core::f32::consts::FRAC_PI_4);
        let r90 = Rotor::from_axis_angle(0.0, 1.0, 0.0, core::f32::consts::FRAC_PI_2);
        let r2 = r45 * r45;
        let p = Point::from_xyz(1.0, 0.0, 0.0).to_multivector();
        let target_r2 = r2.apply(&p);
        let target_r90 = r90.apply(&p);
        for i in 0..BLADE_COUNT {
            assert!(
                approx(target_r2.coefficient(i), target_r90.coefficient(i)),
                "blade {i} : r45² != r90"
            );
        }
    }

    #[test]
    fn rotor_apply_to_origin_keeps_origin() {
        // Origin (0,0,0) is fixed by any rotation.
        let r = Rotor::from_axis_angle(0.0, 1.0, 0.0, 0.7);
        let origin = Point::from_xyz(0.0, 0.0, 0.0);
        let rotated_mv = r.apply(&origin.to_multivector());
        let p = Point::from_multivector(&rotated_mv);
        let (x, y, z) = p.to_xyz();
        assert!(approx(x, 0.0));
        assert!(approx(y, 0.0));
        assert!(approx(z, 0.0));
    }

    #[test]
    fn rotor_y_axis_90deg_rotates_x_to_negz() {
        let r = Rotor::from_axis_angle(0.0, 1.0, 0.0, core::f32::consts::FRAC_PI_2);
        let p = Point::from_xyz(1.0, 0.0, 0.0);
        let rotated_mv = r.apply(&p.to_multivector());
        let (x, y, z) = Point::from_multivector(&rotated_mv).to_xyz();
        // 90deg around Y in RH : X → -Z (matching `cssl-math::Quat`).
        assert!(approx(x, 0.0));
        assert!(approx(y, 0.0));
        assert!(approx(z, -1.0));
    }

    #[test]
    fn rotor_reverse_inverts_unit_rotor() {
        let r = Rotor::from_axis_angle(0.0, 1.0, 0.0, 0.7);
        let inv = r.reverse();
        let prod = r * inv;
        // R R̃ = identity for unit R.
        assert!(approx(prod.s, 1.0));
        assert!(approx(prod.b1, 0.0));
        assert!(approx(prod.b2, 0.0));
        assert!(approx(prod.b3, 0.0));
    }
}
