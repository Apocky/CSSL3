//! § multivector_f64 — `f64` precision Multivector for G(3,0,1)
//!
//! Identical surface to [`crate::multivector::Multivector`] but with `f64`
//! coefficients. Used by physics solvers and Lie-group integrators where
//! `f32` accumulation drifts off-manifold over many compositions.
//!
//! § BLADE TABLE SHARED
//!   The 16×16 blade-product index/sign table is the same as the f32
//!   variant (the algebra is the same — only precision changes). This
//!   module re-uses the integer-typed [`crate::multivector::blade_product`]
//!   table accessor to avoid duplicating the Cayley computation.

use core::ops::{Add, Mul, Neg, Sub};

use crate::basis::{Grade, BLADE_COUNT};

/// 16-component multivector in G(3,0,1), `f64` precision.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct Multivector {
    /// 16 component f64s in canonical-blade order.
    coeffs: [f64; BLADE_COUNT],
}

impl Multivector {
    /// Construct from a raw `[f64; 16]` array.
    #[must_use]
    pub const fn from_array(coeffs: [f64; BLADE_COUNT]) -> Self {
        Self { coeffs }
    }

    /// Borrow the raw component array.
    #[must_use]
    pub const fn as_array(&self) -> &[f64; BLADE_COUNT] {
        &self.coeffs
    }

    /// Read coefficient at canonical-blade index `i`.
    ///
    /// # Panics
    /// Panics if `i >= 16`.
    #[must_use]
    pub const fn coefficient(&self, i: usize) -> f64 {
        self.coeffs[i]
    }

    /// Lower precision : convert to the `f32` [`crate::multivector::Multivector`].
    #[must_use]
    pub fn to_f32(&self) -> crate::multivector::Multivector {
        let mut a = [0.0_f32; BLADE_COUNT];
        for i in 0..BLADE_COUNT {
            a[i] = self.coeffs[i] as f32;
        }
        crate::multivector::Multivector::from_array(a)
    }

    /// Raise precision : convert from the `f32` variant.
    #[must_use]
    pub fn from_f32(mv: &crate::multivector::Multivector) -> Self {
        let src = mv.as_array();
        let mut a = [0.0_f64; BLADE_COUNT];
        for i in 0..BLADE_COUNT {
            a[i] = f64::from(src[i]);
        }
        Self::from_array(a)
    }

    /// Grade projection.
    #[must_use]
    pub fn grade_project(&self, g: Grade) -> Self {
        let mut out = Self::default();
        let (lo, hi) = g.index_range();
        for i in lo..hi {
            out.coeffs[i] = self.coeffs[i];
        }
        out
    }

    /// Reverse `~A` — same sign pattern as f32 variant.
    #[must_use]
    pub fn reverse(&self) -> Self {
        let c = &self.coeffs;
        Self::from_array([
            c[0], c[1], c[2], c[3], c[4], -c[5], -c[6], -c[7], -c[8], -c[9], -c[10], -c[11],
            -c[12], -c[13], -c[14], c[15],
        ])
    }

    /// Geometric product. Re-uses the same f32-derived blade table.
    #[must_use]
    pub fn geometric(&self, b: &Self) -> Self {
        let a = &self.coeffs;
        let b = &b.coeffs;
        let mut r = [0.0_f64; BLADE_COUNT];
        for i in 0..BLADE_COUNT {
            if a[i] == 0.0 {
                continue;
            }
            for j in 0..BLADE_COUNT {
                if b[j] == 0.0 {
                    continue;
                }
                let (idx, sign) = crate::multivector::blade_product_entry(i, j);
                if sign != 0 {
                    r[idx] += f64::from(sign) * a[i] * b[j];
                }
            }
        }
        Self::from_array(r)
    }

    /// Sandwich product `M v M̃`.
    #[must_use]
    pub fn sandwich(&self, v: &Self) -> Self {
        self.geometric(v).geometric(&self.reverse())
    }

    /// Squared norm `‖A‖² = ⟨A Ā⟩₀`.
    #[must_use]
    pub fn norm_squared(&self) -> f64 {
        let c = &self.coeffs;
        c[0] * c[0]
            + c[1] * c[1]
            + c[2] * c[2]
            + c[3] * c[3]
            + c[5] * c[5]
            + c[6] * c[6]
            + c[7] * c[7]
            + c[14] * c[14]
    }
}

impl Add for Multivector {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        let mut a = self.coeffs;
        for i in 0..BLADE_COUNT {
            a[i] += rhs.coeffs[i];
        }
        Self::from_array(a)
    }
}

impl Sub for Multivector {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        let mut a = self.coeffs;
        for i in 0..BLADE_COUNT {
            a[i] -= rhs.coeffs[i];
        }
        Self::from_array(a)
    }
}

impl Neg for Multivector {
    type Output = Self;
    fn neg(self) -> Self {
        let mut a = self.coeffs;
        for v in &mut a {
            *v = -*v;
        }
        Self::from_array(a)
    }
}

impl Mul for Multivector {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        self.geometric(&rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    #[test]
    fn f64_default_is_zero() {
        let mv = Multivector::default();
        for i in 0..BLADE_COUNT {
            assert_eq!(mv.coefficient(i), 0.0);
        }
    }

    #[test]
    fn f64_reverse_is_involutive() {
        let mut a = [0.0_f64; BLADE_COUNT];
        for i in 0..BLADE_COUNT {
            a[i] = (i as f64) + 1.0;
        }
        let mv = Multivector::from_array(a);
        let r = mv.reverse().reverse();
        for i in 0..BLADE_COUNT {
            assert!(approx(r.coefficient(i), mv.coefficient(i)));
        }
    }

    #[test]
    fn f64_to_f32_round_trip_preserves_value() {
        let mut a = [0.0_f64; BLADE_COUNT];
        a[0] = 1.5;
        a[5] = 0.25;
        let mv64 = Multivector::from_array(a);
        let mv32 = mv64.to_f32();
        let back = Multivector::from_f32(&mv32);
        assert!(approx(back.coefficient(0), 1.5));
        assert!(approx(back.coefficient(5), 0.25));
    }

    #[test]
    fn f64_geometric_e1_squared_is_scalar_one() {
        // e₁ at index 1 — e₁² = +1.
        let mut a = [0.0_f64; BLADE_COUNT];
        a[1] = 1.0;
        let e1 = Multivector::from_array(a);
        let sq = e1 * e1;
        assert!(approx(sq.coefficient(0), 1.0));
        for i in 1..BLADE_COUNT {
            assert!(approx(sq.coefficient(i), 0.0));
        }
    }

    #[test]
    fn f64_geometric_e0_squared_is_zero() {
        // e₀ at index 4 — e₀² = 0 (degenerate).
        let mut a = [0.0_f64; BLADE_COUNT];
        a[4] = 1.0;
        let e0 = Multivector::from_array(a);
        let sq = e0 * e0;
        for i in 0..BLADE_COUNT {
            assert!(approx(sq.coefficient(i), 0.0));
        }
    }
}
