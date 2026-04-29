//! § Complex amplitude scalars — `C32` + `C64`
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § PURPOSE
//!   The Wave-Unity solver works in the complex amplitude space ψ ∈ ℂ.
//!   Stage-0 ships its own deterministic complex-number type so the
//!   solver does not pull a fresh dependency for the same surface that
//!   `num-complex` would offer. The scalar fields (`re`, `im`) are
//!   plain f32 / f64 ; serialization is via byte-order-independent
//!   `to_le_bytes()` so the replay-log is bit-equal across hosts.
//!
//! § SURFACE
//!   ```text
//!   pub struct C32 { pub re: f32, pub im: f32 }
//!   pub struct C64 { pub re: f64, pub im: f64 }
//!   ```
//!   Both implement the canonical complex-number ops :
//!     - `+`, `-`, `*`, `/`, `Neg`
//!     - `conj()`, `norm_sqr()`, `norm()` (Euclidean magnitude)
//!     - `from_polar(r, theta)` (Euler-form constructor)
//!     - `arg()` (phase)
//!     - `scale(real_factor)` (cheap real-multiply ; sidesteps the
//!       general `*` cost when the multiplier is real)
//!     - `mul_add(other, accumulator)` (fused multiply-add ; Stage-0
//!       expands to two separate ops to honor the determinism contract
//!       — no FMA on values that affect the psi-tensor).
//!
//! § DETERMINISM
//!   - All ops are bit-deterministic on x86-64 SSE2 (the cssl-rt ABI
//!     default) given identical inputs + denormal-flush state.
//!   - `mul_add` is intentionally NOT lowered to fma — see
//!     `cssl-substrate-omega-step::determinism::fast_math_probe` and
//!     the omega_step DETERMINISM CONTRACT (§ DETERMINISM).
//!
//! § REPLAY
//!   Both types are `Copy` + `PartialEq` ; the replay-log records the
//!   raw `[u8; 8]` (C32) or `[u8; 16]` (C64) byte representations via
//!   the `to_le_bytes` / `from_le_bytes` round-trip.

use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

/// § 32-bit complex amplitude. Used for fast-band ψ values where the
///   memory-budget table in `Wave-Unity §VIII.1` calls for 8 B per cell.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct C32 {
    /// § Real part.
    pub re: f32,
    /// § Imaginary part.
    pub im: f32,
}

impl C32 {
    /// § Zero element.
    pub const ZERO: C32 = C32 { re: 0.0, im: 0.0 };
    /// § Real-axis unit element.
    pub const ONE: C32 = C32 { re: 1.0, im: 0.0 };
    /// § Imaginary-axis unit element.
    pub const I: C32 = C32 { re: 0.0, im: 1.0 };

    /// § Construct a complex from real + imaginary parts.
    #[inline]
    #[must_use]
    pub const fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    /// § Construct from polar (magnitude, phase).
    #[inline]
    #[must_use]
    pub fn from_polar(r: f32, theta: f32) -> Self {
        Self {
            re: r * theta.cos(),
            im: r * theta.sin(),
        }
    }

    /// § Complex conjugate : (re, -im).
    #[inline]
    #[must_use]
    pub const fn conj(self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
        }
    }

    /// § |ψ|² = re² + im². The energy density.
    #[inline]
    #[must_use]
    pub fn norm_sqr(self) -> f32 {
        self.re * self.re + self.im * self.im
    }

    /// § |ψ| = sqrt(re² + im²).
    #[inline]
    #[must_use]
    pub fn norm(self) -> f32 {
        self.norm_sqr().sqrt()
    }

    /// § Phase = atan2(im, re).
    #[inline]
    #[must_use]
    pub fn arg(self) -> f32 {
        self.im.atan2(self.re)
    }

    /// § Multiply by a real scalar ; avoids the general complex-mul cost.
    #[inline]
    #[must_use]
    pub fn scale(self, k: f32) -> Self {
        Self {
            re: self.re * k,
            im: self.im * k,
        }
    }

    /// § self * other + acc. Stage-0 expands to mul + add so determinism
    ///   is preserved (no FMA — see module docs).
    #[inline]
    #[must_use]
    pub fn mul_add(self, other: Self, acc: Self) -> Self {
        let prod = self * other;
        prod + acc
    }

    /// § Promote to f64 for IMEX residual computation.
    #[inline]
    #[must_use]
    pub fn to_c64(self) -> C64 {
        C64 {
            re: self.re as f64,
            im: self.im as f64,
        }
    }

    /// § 8-byte little-endian serialization. Used by replay-log.
    #[inline]
    #[must_use]
    pub fn to_le_bytes(self) -> [u8; 8] {
        let mut out = [0u8; 8];
        out[0..4].copy_from_slice(&self.re.to_le_bytes());
        out[4..8].copy_from_slice(&self.im.to_le_bytes());
        out
    }

    /// § Inverse of `to_le_bytes`.
    #[inline]
    #[must_use]
    pub fn from_le_bytes(b: [u8; 8]) -> Self {
        let mut re = [0u8; 4];
        let mut im = [0u8; 4];
        re.copy_from_slice(&b[0..4]);
        im.copy_from_slice(&b[4..8]);
        Self {
            re: f32::from_le_bytes(re),
            im: f32::from_le_bytes(im),
        }
    }

    /// § True iff both components are finite (no NaN, no ±∞).
    #[inline]
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }
}

impl Add for C32 {
    type Output = C32;
    #[inline]
    fn add(self, rhs: C32) -> C32 {
        C32 {
            re: self.re + rhs.re,
            im: self.im + rhs.im,
        }
    }
}

impl AddAssign for C32 {
    #[inline]
    fn add_assign(&mut self, rhs: C32) {
        self.re += rhs.re;
        self.im += rhs.im;
    }
}

impl Sub for C32 {
    type Output = C32;
    #[inline]
    fn sub(self, rhs: C32) -> C32 {
        C32 {
            re: self.re - rhs.re,
            im: self.im - rhs.im,
        }
    }
}

impl SubAssign for C32 {
    #[inline]
    fn sub_assign(&mut self, rhs: C32) {
        self.re -= rhs.re;
        self.im -= rhs.im;
    }
}

impl Mul for C32 {
    type Output = C32;
    #[inline]
    fn mul(self, rhs: C32) -> C32 {
        C32 {
            re: self.re * rhs.re - self.im * rhs.im,
            im: self.re * rhs.im + self.im * rhs.re,
        }
    }
}

impl MulAssign for C32 {
    #[inline]
    fn mul_assign(&mut self, rhs: C32) {
        let new = *self * rhs;
        *self = new;
    }
}

impl Div for C32 {
    type Output = C32;
    #[inline]
    fn div(self, rhs: C32) -> C32 {
        let d = rhs.norm_sqr();
        C32 {
            re: (self.re * rhs.re + self.im * rhs.im) / d,
            im: (self.im * rhs.re - self.re * rhs.im) / d,
        }
    }
}

impl Neg for C32 {
    type Output = C32;
    #[inline]
    fn neg(self) -> C32 {
        C32 {
            re: -self.re,
            im: -self.im,
        }
    }
}

/// § 64-bit complex amplitude. Used for IMEX residual computation +
///   norm-conservation accounting where the f32 precision is too tight.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct C64 {
    /// § Real part.
    pub re: f64,
    /// § Imaginary part.
    pub im: f64,
}

impl C64 {
    /// § Zero element.
    pub const ZERO: C64 = C64 { re: 0.0, im: 0.0 };
    /// § Real-axis unit element.
    pub const ONE: C64 = C64 { re: 1.0, im: 0.0 };

    /// § Construct a complex from real + imaginary parts.
    #[inline]
    #[must_use]
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    /// § Construct from polar (magnitude, phase).
    #[inline]
    #[must_use]
    pub fn from_polar(r: f64, theta: f64) -> Self {
        Self {
            re: r * theta.cos(),
            im: r * theta.sin(),
        }
    }

    /// § Complex conjugate : (re, -im).
    #[inline]
    #[must_use]
    pub const fn conj(self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
        }
    }

    /// § |ψ|² = re² + im². The energy density.
    #[inline]
    #[must_use]
    pub fn norm_sqr(self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    /// § |ψ| = sqrt(re² + im²).
    #[inline]
    #[must_use]
    pub fn norm(self) -> f64 {
        self.norm_sqr().sqrt()
    }

    /// § Multiply by a real scalar.
    #[inline]
    #[must_use]
    pub fn scale(self, k: f64) -> Self {
        Self {
            re: self.re * k,
            im: self.im * k,
        }
    }

    /// § Truncate to f32 precision.
    #[inline]
    #[must_use]
    pub fn to_c32(self) -> C32 {
        C32 {
            re: self.re as f32,
            im: self.im as f32,
        }
    }
}

impl Add for C64 {
    type Output = C64;
    #[inline]
    fn add(self, rhs: C64) -> C64 {
        C64 {
            re: self.re + rhs.re,
            im: self.im + rhs.im,
        }
    }
}

impl Sub for C64 {
    type Output = C64;
    #[inline]
    fn sub(self, rhs: C64) -> C64 {
        C64 {
            re: self.re - rhs.re,
            im: self.im - rhs.im,
        }
    }
}

impl Mul for C64 {
    type Output = C64;
    #[inline]
    fn mul(self, rhs: C64) -> C64 {
        C64 {
            re: self.re * rhs.re - self.im * rhs.im,
            im: self.re * rhs.im + self.im * rhs.re,
        }
    }
}

impl Neg for C64 {
    type Output = C64;
    #[inline]
    fn neg(self) -> C64 {
        C64 {
            re: -self.re,
            im: -self.im,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close_f32(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn c32_zero_is_default() {
        assert_eq!(C32::default(), C32::ZERO);
    }

    #[test]
    fn c32_arithmetic_basic() {
        let a = C32::new(3.0, 4.0);
        let b = C32::new(1.0, 2.0);
        let s = a + b;
        let d = a - b;
        let p = a * b;
        let q = a / b;
        assert_eq!(s, C32::new(4.0, 6.0));
        assert_eq!(d, C32::new(2.0, 2.0));
        // (3+4i)(1+2i) = 3 + 6i + 4i + 8i² = -5 + 10i
        assert_eq!(p, C32::new(-5.0, 10.0));
        // (3+4i) / (1+2i) = (3+4i)(1-2i)/5 = (3-6i+4i-8i²)/5 = (11-2i)/5
        assert!(close_f32(q.re, 11.0 / 5.0, 1e-6));
        assert!(close_f32(q.im, -2.0 / 5.0, 1e-6));
    }

    #[test]
    fn c32_norm_pythagorean() {
        let a = C32::new(3.0, 4.0);
        assert!(close_f32(a.norm_sqr(), 25.0, 1e-6));
        assert!(close_f32(a.norm(), 5.0, 1e-6));
    }

    #[test]
    fn c32_conj_negates_imaginary() {
        let a = C32::new(3.0, 4.0);
        assert_eq!(a.conj(), C32::new(3.0, -4.0));
    }

    #[test]
    fn c32_polar_round_trip() {
        let a = C32::from_polar(2.0, std::f32::consts::FRAC_PI_4);
        assert!(close_f32(a.norm(), 2.0, 1e-6));
        assert!(close_f32(a.arg(), std::f32::consts::FRAC_PI_4, 1e-6));
    }

    #[test]
    fn c32_scale_real_multiply() {
        let a = C32::new(2.0, 3.0);
        assert_eq!(a.scale(2.0), C32::new(4.0, 6.0));
    }

    #[test]
    fn c32_neg_negates_both() {
        let a = C32::new(2.0, 3.0);
        assert_eq!(-a, C32::new(-2.0, -3.0));
    }

    #[test]
    fn c32_le_bytes_round_trip() {
        let a = C32::new(1.5, -2.25);
        let b = a.to_le_bytes();
        let c = C32::from_le_bytes(b);
        assert_eq!(a, c);
    }

    #[test]
    fn c32_mul_add_matches_separate_ops() {
        let a = C32::new(2.0, 3.0);
        let b = C32::new(1.0, 0.5);
        let acc = C32::new(0.5, 0.5);
        assert_eq!(a.mul_add(b, acc), a * b + acc);
    }

    #[test]
    fn c32_to_c64_preserves_value() {
        let a = C32::new(1.5, 2.5);
        let b = a.to_c64();
        assert_eq!(b.re, 1.5);
        assert_eq!(b.im, 2.5);
    }

    #[test]
    fn c32_finite_detector() {
        assert!(C32::new(1.0, 1.0).is_finite());
        assert!(!C32::new(f32::NAN, 1.0).is_finite());
        assert!(!C32::new(f32::INFINITY, 0.0).is_finite());
    }

    #[test]
    fn c64_arithmetic_basic() {
        let a = C64::new(3.0, 4.0);
        let b = C64::new(1.0, 2.0);
        assert_eq!(a + b, C64::new(4.0, 6.0));
        assert_eq!(a - b, C64::new(2.0, 2.0));
        assert_eq!(a * b, C64::new(-5.0, 10.0));
    }

    #[test]
    fn c64_round_trip_to_c32() {
        let a = C64::new(1.5, 2.5);
        let b = a.to_c32();
        assert_eq!(b.re, 1.5);
        assert_eq!(b.im, 2.5);
    }

    #[test]
    fn c32_addassign_mutates_in_place() {
        let mut a = C32::new(1.0, 2.0);
        a += C32::new(3.0, 4.0);
        assert_eq!(a, C32::new(4.0, 6.0));
    }

    #[test]
    fn c32_subassign_mutates_in_place() {
        let mut a = C32::new(5.0, 7.0);
        a -= C32::new(1.0, 2.0);
        assert_eq!(a, C32::new(4.0, 5.0));
    }

    #[test]
    fn c32_mulassign_mutates_in_place() {
        let mut a = C32::new(2.0, 0.0);
        a *= C32::new(0.0, 1.0); // i
        assert_eq!(a, C32::new(0.0, 2.0));
    }

    #[test]
    fn c32_unit_constants_correct() {
        assert_eq!(C32::ZERO, C32::new(0.0, 0.0));
        assert_eq!(C32::ONE, C32::new(1.0, 0.0));
        assert_eq!(C32::I, C32::new(0.0, 1.0));
        assert_eq!(C32::I * C32::I, -C32::ONE);
    }
}
