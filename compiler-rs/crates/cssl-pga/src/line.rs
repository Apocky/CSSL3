//! § Line — grade-2 element of G(3,0,1) (Klein-PGA)
//!
//! Lines in Klein-PGA are grade-2 bivectors with 6 components on the
//! basis `e₂₃ e₃₁ e₁₂ e₀₁ e₀₂ e₀₃`. The first three are the spatial
//! direction (Plücker direction); the last three are the moment (the
//! cross product of any point on the line with the direction).
//!
//! § PLÜCKER ↔ PGA
//!   Plücker line coordinates `(d : m) ∈ ℝ⁶` with the constraint
//!   `d · m = 0` map naturally to PGA bivectors :
//!     `d = (d₁, d₂, d₃) → (e₂₃, e₃₁, e₁₂)` direction component
//!     `m = (m₁, m₂, m₃) → (e₀₁, e₀₂, e₀₃)` moment component
//!
//! § GENERATOR DUAL-USE
//!   The same grade-2 subspace also encodes **rigid-motion generators** :
//!   a unit rotor is `cos(θ/2) - sin(θ/2) · L̂` for a unit-line `L̂` (the
//!   axis of rotation). A unit translator is `1 - (t/2) · M̂` for a unit
//!   moment-bivector `M̂` (the axis of translation in ideal-line form).
//!   See [`crate::motor`] for the composition.

use crate::basis::BLADE_COUNT;
use crate::multivector::Multivector;

/// A line in 3D space, stored as a grade-2 bivector. Equivalently a
/// **rigid-motion bivector generator** — see [`crate::motor::Motor`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct Line {
    /// `e₂₃` coefficient — x-direction Plücker component.
    pub e23: f32,
    /// `e₃₁` coefficient — y-direction Plücker component.
    pub e31: f32,
    /// `e₁₂` coefficient — z-direction Plücker component.
    pub e12: f32,
    /// `e₀₁` coefficient — x-moment component.
    pub e01: f32,
    /// `e₀₂` coefficient — y-moment component.
    pub e02: f32,
    /// `e₀₃` coefficient — z-moment component.
    pub e03: f32,
}

impl Line {
    /// Construct from explicit Plücker `(direction, moment)`.
    #[must_use]
    pub const fn from_plucker(d: (f32, f32, f32), m: (f32, f32, f32)) -> Self {
        Self {
            e23: d.0,
            e31: d.1,
            e12: d.2,
            e01: m.0,
            e02: m.1,
            e03: m.2,
        }
    }

    /// Construct from explicit components in canonical order.
    #[must_use]
    pub const fn from_components(
        e23: f32,
        e31: f32,
        e12: f32,
        e01: f32,
        e02: f32,
        e03: f32,
    ) -> Self {
        Self {
            e23,
            e31,
            e12,
            e01,
            e02,
            e03,
        }
    }

    /// Squared norm — only the spatial direction `(e₂₃, e₃₁, e₁₂)`
    /// contributes. The ideal moment squares to zero (degenerate
    /// signature).
    #[must_use]
    pub fn norm_squared(self) -> f32 {
        self.e23 * self.e23 + self.e31 * self.e31 + self.e12 * self.e12
    }

    /// Renormalize so the spatial direction is unit length. Returns the
    /// input unchanged if the direction is degenerate.
    #[must_use]
    pub fn normalize(self) -> Self {
        let n2 = self.norm_squared();
        if n2 > 1e-12 {
            let inv = n2.sqrt().recip();
            Self {
                e23: self.e23 * inv,
                e31: self.e31 * inv,
                e12: self.e12 * inv,
                e01: self.e01 * inv,
                e02: self.e02 * inv,
                e03: self.e03 * inv,
            }
        } else {
            self
        }
    }

    /// Embed as a 16-component multivector.
    #[must_use]
    pub fn to_multivector(self) -> Multivector {
        let mut a = [0.0_f32; BLADE_COUNT];
        a[5] = self.e23;
        a[6] = self.e31;
        a[7] = self.e12;
        a[8] = self.e01;
        a[9] = self.e02;
        a[10] = self.e03;
        Multivector::from_array(a)
    }

    /// Extract from a general multivector via grade-2 projection.
    #[must_use]
    pub fn from_multivector(mv: &Multivector) -> Self {
        Self {
            e23: mv.e23(),
            e31: mv.e31(),
            e12: mv.e12(),
            e01: mv.e01(),
            e02: mv.e02(),
            e03: mv.e03(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn line_constructor_round_trip_components() {
        let l = Line::from_components(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
        let mv = l.to_multivector();
        let l2 = Line::from_multivector(&mv);
        assert_eq!(l, l2);
    }

    #[test]
    fn line_norm_squared_uses_only_spatial_direction() {
        // Setting only ideal components ⇒ norm² = 0.
        let l = Line::from_plucker((0.0, 0.0, 0.0), (1.0, 1.0, 1.0));
        assert!(approx(l.norm_squared(), 0.0));
    }

    #[test]
    fn line_normalize_yields_unit_spatial_direction() {
        let l = Line::from_plucker((2.0, 0.0, 0.0), (3.0, 0.0, 0.0));
        let n = l.normalize();
        assert!(approx(n.norm_squared(), 1.0));
    }
}
