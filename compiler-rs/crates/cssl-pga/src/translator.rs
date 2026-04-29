//! § Translator — pure-translation element of the motor group
//!
//! A translator is the PGA replacement for "add a translation vector". It
//! is an element of the **even subalgebra** restricted to grades 0 +
//! 2-ideal — 4 components (one scalar + three ideal bivectors) :
//!
//!   `T = 1 + (1/2)·(t_x·e₀₁ + t_y·e₀₂ + t_z·e₀₃)`
//!
//! The factor of 1/2 (cf. the rotor `θ/2`) is what makes the sandwich
//! product `T p T̃` translate a point `p` by exactly `(t_x, t_y, t_z)`.
//!
//! § DEGENERATE-NORM PROPERTY
//!   Because `e₀² = 0`, the squared norm of a translator is always 1
//!   regardless of `(t_x, t_y, t_z)` magnitude — the translator subgroup
//!   is non-compact (translations form a real-line group) but the norm
//!   doesn't see that. This is a feature, not a bug : it lets us treat
//!   translators as unit-norm elements without spurious renormalization.

use crate::basis::BLADE_COUNT;
use crate::multivector::Multivector;

/// A unit translator. The scalar component is always `1` ; only the
/// `(t01, t02, t03)` components are stored explicitly. The "1/2" factor
/// from the canonical form is applied by the constructor — pass the raw
/// translation vector and the math is done internally.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Translator {
    /// `e₀₁` coefficient — half the x-translation.
    pub t01: f32,
    /// `e₀₂` coefficient — half the y-translation.
    pub t02: f32,
    /// `e₀₃` coefficient — half the z-translation.
    pub t03: f32,
}

impl Default for Translator {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Translator {
    /// Identity translator — no displacement.
    pub const IDENTITY: Self = Self {
        t01: 0.0,
        t02: 0.0,
        t03: 0.0,
    };

    /// Construct from a translation vector `(tx, ty, tz)`.
    ///
    /// Canonical Klein-PGA convention `T = 1 - (1/2)(tx·e₀₁ + ty·e₀₂ + tz·e₀₃)` :
    /// the negative sign is what makes the sandwich product `T p T̃`
    /// produce positive `(tx, ty, tz)` translation given the
    /// trivector-point sign convention used in [`crate::Point`].
    #[must_use]
    pub const fn from_translation(tx: f32, ty: f32, tz: f32) -> Self {
        Self {
            t01: -0.5 * tx,
            t02: -0.5 * ty,
            t03: -0.5 * tz,
        }
    }

    /// Recover the world-space translation vector. Inverse of
    /// [`Self::from_translation`] — accounts for the canonical sign flip.
    #[must_use]
    pub const fn to_translation(self) -> (f32, f32, f32) {
        (-2.0 * self.t01, -2.0 * self.t02, -2.0 * self.t03)
    }

    /// Reverse `~T` — for a translator this negates the bivector components.
    /// For unit translators this equals the inverse.
    #[must_use]
    pub const fn reverse(self) -> Self {
        Self {
            t01: -self.t01,
            t02: -self.t02,
            t03: -self.t03,
        }
    }

    /// Compose two translators. Translation is commutative (Lie-algebra
    /// is abelian for translation-only) so the result is just the sum
    /// of bivector components.
    #[must_use]
    pub fn compose(self, other: Self) -> Self {
        Self {
            t01: self.t01 + other.t01,
            t02: self.t02 + other.t02,
            t03: self.t03 + other.t03,
        }
    }

    /// Embed this translator as a full 16-component multivector.
    /// `T = 1 + t01·e₀₁ + t02·e₀₂ + t03·e₀₃`.
    #[must_use]
    pub fn to_multivector(self) -> Multivector {
        let mut a = [0.0_f32; BLADE_COUNT];
        a[0] = 1.0;
        a[8] = self.t01;
        a[9] = self.t02;
        a[10] = self.t03;
        Multivector::from_array(a)
    }

    /// Apply this translator to a multivector by sandwich `T v T̃`.
    #[must_use]
    pub fn apply(self, v: &Multivector) -> Multivector {
        self.to_multivector().sandwich(v)
    }
}

impl core::ops::Mul for Translator {
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
    fn translator_identity_is_zero_components() {
        let t = Translator::IDENTITY;
        assert!(approx(t.t01, 0.0));
        assert!(approx(t.t02, 0.0));
        assert!(approx(t.t03, 0.0));
    }

    #[test]
    fn translator_round_trip_translation_vector() {
        let t = Translator::from_translation(3.5, -2.0, 1.5);
        let (x, y, z) = t.to_translation();
        assert!(approx(x, 3.5));
        assert!(approx(y, -2.0));
        assert!(approx(z, 1.5));
    }

    #[test]
    fn translator_compose_sums_translations() {
        let a = Translator::from_translation(1.0, 2.0, 3.0);
        let b = Translator::from_translation(4.0, 5.0, 6.0);
        let c = a * b;
        let (x, y, z) = c.to_translation();
        assert!(approx(x, 5.0));
        assert!(approx(y, 7.0));
        assert!(approx(z, 9.0));
    }

    #[test]
    fn translator_reverse_negates_inverse() {
        let t = Translator::from_translation(3.0, -1.0, 2.0);
        let inv = t.reverse();
        let composed = t * inv;
        let (x, y, z) = composed.to_translation();
        assert!(approx(x, 0.0));
        assert!(approx(y, 0.0));
        assert!(approx(z, 0.0));
    }

    #[test]
    fn translator_apply_to_origin_yields_translation() {
        // Translating the origin (0, 0, 0) by (2, -3, 4) gives (2, -3, 4).
        let t = Translator::from_translation(2.0, -3.0, 4.0);
        let origin = Point::from_xyz(0.0, 0.0, 0.0);
        let translated_mv = t.apply(&origin.to_multivector());
        let p = Point::from_multivector(&translated_mv).normalize();
        let (x, y, z) = p.to_xyz();
        assert!(approx(x, 2.0));
        assert!(approx(y, -3.0));
        assert!(approx(z, 4.0));
    }

    #[test]
    fn translator_apply_translates_arbitrary_point() {
        let t = Translator::from_translation(1.0, 2.0, 3.0);
        let p = Point::from_xyz(5.0, 6.0, 7.0);
        let translated_mv = t.apply(&p.to_multivector());
        let p_out = Point::from_multivector(&translated_mv).normalize();
        let (x, y, z) = p_out.to_xyz();
        assert!(approx(x, 6.0));
        assert!(approx(y, 8.0));
        assert!(approx(z, 10.0));
    }
}
