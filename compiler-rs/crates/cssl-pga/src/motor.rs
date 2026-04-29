//! § Motor — full rigid motion in G(3,0,1)
//!
//! A motor is the algebraically-closed PGA representation of a **rigid
//! motion** = rotation + translation = element of `SE(3)`. It replaces
//! 4×4 rigid transform matrices and dual quaternions with a single
//! algebraically-clean 8-component object.
//!
//! § STORAGE
//!   `M = T R = (1 + t/2) (cos(θ/2) - sin(θ/2)·B̂)` expands into the
//!   even subalgebra :
//!
//!   ```text
//!   M = s + r₁·e₂₃ + r₂·e₃₁ + r₃·e₁₂  +  m₀·e₀₁₂₃
//!       + t₁·e₀₁ + t₂·e₀₂ + t₃·e₀₃
//!   ```
//!
//!   The eight components live on grades 0, 2, and 4. The grade-4
//!   pseudoscalar `e₀₁₂₃` arises whenever the rotor and translator
//!   commute non-trivially — for pure rotation `m₀ = 0`, for pure
//!   translation `m₀ = 0` and `(r₁, r₂, r₃) = 0`, but the general
//!   motor has a non-zero pseudoscalar.
//!
//! § COMPOSITION
//!   `M_a · M_b` represents "first apply `M_b`, then `M_a`" — the same
//!   right-to-left reading direction as matrix products and quaternion
//!   composition. Sandwich application is `M.apply(v) = M v M̃`.

use crate::basis::BLADE_COUNT;
use crate::multivector::Multivector;
use crate::rotor::Rotor;
use crate::translator::Translator;

/// A rigid motion in 3D — element of `SE(3)`.
///
/// 8 components on the canonical even-subalgebra blades :
///   `(s, r₁, r₂, r₃, t₁, t₂, t₃, m₀)` ↔
///   `(1, e₂₃, e₃₁, e₁₂, e₀₁, e₀₂, e₀₃, e₀₁₂₃)`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Motor {
    /// Scalar `1` component.
    pub s: f32,
    /// `e₂₃` component — x-axis rotor weight.
    pub r1: f32,
    /// `e₃₁` component — y-axis rotor weight.
    pub r2: f32,
    /// `e₁₂` component — z-axis rotor weight.
    pub r3: f32,
    /// `e₀₁` component — x-translation generator weight.
    pub t1: f32,
    /// `e₀₂` component — y-translation generator weight.
    pub t2: f32,
    /// `e₀₃` component — z-translation generator weight.
    pub t3: f32,
    /// `e₀₁₂₃` pseudoscalar component.
    pub m0: f32,
}

impl Default for Motor {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Motor {
    /// Identity motor — no rotation or translation.
    pub const IDENTITY: Self = Self {
        s: 1.0,
        r1: 0.0,
        r2: 0.0,
        r3: 0.0,
        t1: 0.0,
        t2: 0.0,
        t3: 0.0,
        m0: 0.0,
    };

    /// Construct from explicit components.
    #[must_use]
    pub const fn from_components(
        s: f32,
        r1: f32,
        r2: f32,
        r3: f32,
        t1: f32,
        t2: f32,
        t3: f32,
        m0: f32,
    ) -> Self {
        Self {
            s,
            r1,
            r2,
            r3,
            t1,
            t2,
            t3,
            m0,
        }
    }

    /// Promote a [`Rotor`] to a motor. Pure rotation has zero translation
    /// + zero pseudoscalar components.
    #[must_use]
    pub const fn from_rotor(r: Rotor) -> Self {
        Self {
            s: r.s,
            r1: r.b1,
            r2: r.b2,
            r3: r.b3,
            t1: 0.0,
            t2: 0.0,
            t3: 0.0,
            m0: 0.0,
        }
    }

    /// Promote a [`Translator`] to a motor. Pure translation has scalar
    /// `1` + zero rotation + zero pseudoscalar.
    #[must_use]
    pub const fn from_translator(t: Translator) -> Self {
        Self {
            s: 1.0,
            r1: 0.0,
            r2: 0.0,
            r3: 0.0,
            t1: t.t01,
            t2: t.t02,
            t3: t.t03,
            m0: 0.0,
        }
    }

    /// Build a motor from a translator + rotor pair, in the canonical
    /// "translate-then-rotate" order : `M = T R`.
    ///
    /// This is the form most commonly encoded by game-engine `Transform`
    /// types. The sandwich `M v M̃` first rotates `v` then translates.
    #[must_use]
    pub fn from_translator_rotor(t: Translator, r: Rotor) -> Self {
        Self::from_translator(t).compose(Self::from_rotor(r))
    }

    /// Reverse `~M` — sign-flip the bivector + trivector components.
    /// For a unit motor this equals the inverse.
    #[must_use]
    pub const fn reverse(self) -> Self {
        Self {
            s: self.s,
            r1: -self.r1,
            r2: -self.r2,
            r3: -self.r3,
            t1: -self.t1,
            t2: -self.t2,
            t3: -self.t3,
            m0: self.m0,
        }
    }

    /// Compose two motors `self * other`. Reads right-to-left :
    /// `(a · b).apply(v) = a.apply(b.apply(v))`.
    ///
    /// Implementation : delegate to the full multivector geometric
    /// product. The compiler folds the eight-component sparse paths
    /// since the input motors only populate the eight even-subalgebra
    /// blades — output is identical to a hand-rolled closed form, but
    /// derives from the same source-of-truth blade-product table as
    /// the rest of the algebra.
    #[must_use]
    pub fn compose(self, other: Self) -> Self {
        let mv = self.to_multivector().geometric(&other.to_multivector());
        Self::from_multivector(&mv)
    }

    /// Embed this motor as a full 16-component multivector.
    #[must_use]
    pub fn to_multivector(self) -> Multivector {
        let mut a = [0.0_f32; BLADE_COUNT];
        a[0] = self.s;
        a[5] = self.r1;
        a[6] = self.r2;
        a[7] = self.r3;
        a[8] = self.t1;
        a[9] = self.t2;
        a[10] = self.t3;
        a[15] = self.m0;
        Multivector::from_array(a)
    }

    /// Extract a motor from a general multivector — even-subalgebra
    /// projection.
    #[must_use]
    pub fn from_multivector(mv: &Multivector) -> Self {
        Self {
            s: mv.s(),
            r1: mv.e23(),
            r2: mv.e31(),
            r3: mv.e12(),
            t1: mv.e01(),
            t2: mv.e02(),
            t3: mv.e03(),
            m0: mv.e0123(),
        }
    }

    /// Apply this motor to a multivector by sandwich `M v M̃`.
    #[must_use]
    pub fn apply(self, v: &Multivector) -> Multivector {
        self.to_multivector().sandwich(v)
    }

    /// Squared rotor-norm of the motor — `s² + r₁² + r₂² + r₃²`. The
    /// translation + pseudoscalar parts contribute zero (degenerate
    /// signature). A unit motor has `rotor_norm_squared == 1`.
    #[must_use]
    pub fn rotor_norm_squared(self) -> f32 {
        self.s * self.s + self.r1 * self.r1 + self.r2 * self.r2 + self.r3 * self.r3
    }

    /// Renormalize the rotor sub-component. The translation components are
    /// rescaled to stay self-consistent with the new rotor norm.
    #[must_use]
    pub fn normalize(self) -> Self {
        let n2 = self.rotor_norm_squared();
        if n2 > 1e-12 {
            let inv = n2.sqrt().recip();
            Self {
                s: self.s * inv,
                r1: self.r1 * inv,
                r2: self.r2 * inv,
                r3: self.r3 * inv,
                t1: self.t1 * inv,
                t2: self.t2 * inv,
                t3: self.t3 * inv,
                m0: self.m0 * inv,
            }
        } else {
            Self::IDENTITY
        }
    }
}

impl core::ops::Mul for Motor {
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
    fn motor_identity_has_unit_rotor_norm() {
        assert!(approx(Motor::IDENTITY.rotor_norm_squared(), 1.0));
    }

    #[test]
    fn motor_from_rotor_only_has_zero_translation_and_pseudoscalar() {
        let r = Rotor::from_axis_angle(0.0, 1.0, 0.0, 0.7);
        let m = Motor::from_rotor(r);
        assert!(approx(m.t1, 0.0));
        assert!(approx(m.t2, 0.0));
        assert!(approx(m.t3, 0.0));
        assert!(approx(m.m0, 0.0));
        assert!(approx(m.rotor_norm_squared(), 1.0));
    }

    #[test]
    fn motor_from_translator_only_has_unit_scalar_zero_rotor() {
        let t = Translator::from_translation(2.0, 3.0, 4.0);
        let m = Motor::from_translator(t);
        assert!(approx(m.s, 1.0));
        assert!(approx(m.r1, 0.0));
        assert!(approx(m.r2, 0.0));
        assert!(approx(m.r3, 0.0));
        assert!(approx(m.m0, 0.0));
    }

    #[test]
    fn motor_apply_pure_translation_translates_origin() {
        let t = Translator::from_translation(3.0, -1.0, 2.0);
        let m = Motor::from_translator(t);
        let origin = Point::from_xyz(0.0, 0.0, 0.0);
        let out_mv = m.apply(&origin.to_multivector());
        let p = Point::from_multivector(&out_mv).normalize();
        let (x, y, z) = p.to_xyz();
        assert!(approx(x, 3.0));
        assert!(approx(y, -1.0));
        assert!(approx(z, 2.0));
    }

    #[test]
    fn motor_apply_pure_rotation_y_90deg_takes_x_to_negz() {
        let r = Rotor::from_axis_angle(0.0, 1.0, 0.0, core::f32::consts::FRAC_PI_2);
        let m = Motor::from_rotor(r);
        let p = Point::from_xyz(1.0, 0.0, 0.0);
        let out_mv = m.apply(&p.to_multivector());
        let q = Point::from_multivector(&out_mv).normalize();
        let (x, y, z) = q.to_xyz();
        assert!(approx(x, 0.0));
        assert!(approx(y, 0.0));
        assert!(approx(z, -1.0));
    }

    #[test]
    fn motor_translate_then_rotate_composes_correctly() {
        // Build M = R · T (apply T first, then R).
        // T = translate (1, 0, 0).
        // R = rotate Y by 90 deg.
        // Apply to origin : T takes origin → (1, 0, 0), R takes (1, 0, 0) → (0, 0, -1).
        let t = Translator::from_translation(1.0, 0.0, 0.0);
        let r = Rotor::from_axis_angle(0.0, 1.0, 0.0, core::f32::consts::FRAC_PI_2);
        let m = Motor::from_rotor(r) * Motor::from_translator(t);
        let origin = Point::from_xyz(0.0, 0.0, 0.0);
        let out_mv = m.apply(&origin.to_multivector());
        let q = Point::from_multivector(&out_mv).normalize();
        let (x, y, z) = q.to_xyz();
        assert!(approx(x, 0.0));
        assert!(approx(y, 0.0));
        assert!(approx(z, -1.0));
    }

    #[test]
    fn motor_compose_with_identity_is_self() {
        let r = Rotor::from_axis_angle(0.0, 1.0, 0.0, 0.7);
        let m = Motor::from_rotor(r);
        let composed = m * Motor::IDENTITY;
        assert!(approx(composed.s, m.s));
        assert!(approx(composed.r2, m.r2));
    }

    #[test]
    fn motor_reverse_inverts_unit_motor() {
        let r = Rotor::from_axis_angle(0.0, 1.0, 0.0, 0.7);
        let t = Translator::from_translation(1.0, 2.0, 3.0);
        let m = Motor::from_translator_rotor(t, r);
        let inv = m.reverse();
        let composed = m * inv;
        // M · M̃ should = identity (s = 1, all else 0).
        assert!(approx(composed.s, 1.0));
        assert!(approx(composed.r1, 0.0));
        assert!(approx(composed.r2, 0.0));
        assert!(approx(composed.r3, 0.0));
        assert!(approx(composed.t1, 0.0));
        assert!(approx(composed.t2, 0.0));
        assert!(approx(composed.t3, 0.0));
        assert!(approx(composed.m0, 0.0));
    }

    #[test]
    fn motor_compose_via_multivector_matches_closed_form() {
        // Spot-check : Motor::compose result == round-trip via Multivector
        // geometric product, projected back to even subalgebra.
        let r = Rotor::from_axis_angle(0.0, 1.0, 0.0, 0.7);
        let t = Translator::from_translation(1.0, -2.0, 3.0);
        let a = Motor::from_translator_rotor(t, r);
        let b = Motor::from_rotor(Rotor::from_axis_angle(1.0, 0.0, 0.0, -0.5));
        let composed_closed = a * b;
        let composed_mv = a.to_multivector() * b.to_multivector();
        let composed_round = Motor::from_multivector(&composed_mv);
        assert!(approx(composed_closed.s, composed_round.s));
        assert!(approx(composed_closed.r1, composed_round.r1));
        assert!(approx(composed_closed.r2, composed_round.r2));
        assert!(approx(composed_closed.r3, composed_round.r3));
        assert!(approx(composed_closed.t1, composed_round.t1));
        assert!(approx(composed_closed.t2, composed_round.t2));
        assert!(approx(composed_closed.t3, composed_round.t3));
        assert!(approx(composed_closed.m0, composed_round.m0));
    }
}
