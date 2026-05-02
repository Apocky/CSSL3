//! § complex — minimal `(re, im)` complex-f32 type for ℂ-HDC components.
//!
//! Intentionally tiny — we expose just the operations the `cvec` module
//! needs (multiply, add, magnitude, polar-conversion). A full `num-complex`
//! dep would pull a generic-numeric stack we do not need at this slice.
//!
//! § DETERMINISM
//!   IEEE-754 single-precision arithmetic. Order of operations is explicit
//!   in callers — no compiler-fused fma usage that would vary across
//!   targets. Phase wrapping uses `rem_euclid` analog computed manually.

/// § Complex f32 number in cartesian form.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct C32 {
    pub re: f32,
    pub im: f32,
}

impl C32 {
    pub const ZERO: Self = Self { re: 0.0, im: 0.0 };
    pub const ONE: Self = Self { re: 1.0, im: 0.0 };

    #[inline]
    pub const fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    /// § Build from polar `(amplitude, phase)`.
    #[inline]
    pub fn from_polar(amp: f32, phase: f32) -> Self {
        Self {
            re: amp * phase.cos(),
            im: amp * phase.sin(),
        }
    }

    /// § Convert to polar `(amplitude, phase)`. Phase ∈ `[-π, π]` per `atan2`.
    #[inline]
    pub fn to_polar(self) -> (f32, f32) {
        let amp = (self.re * self.re + self.im * self.im).sqrt();
        let phase = self.im.atan2(self.re);
        (amp, phase)
    }

    /// § Magnitude `‖z‖ = √(re² + im²)`.
    #[inline]
    pub fn magnitude(self) -> f32 {
        (self.re * self.re + self.im * self.im).sqrt()
    }

    /// § Phase angle `arg(z) = atan2(im, re)` ∈ `[-π, π]`.
    #[inline]
    pub fn arg(self) -> f32 {
        self.im.atan2(self.re)
    }

    /// § Complex-conjugate `z̄ = (re, -im)`.
    #[inline]
    pub const fn conj(self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
        }
    }

    /// § Pointwise complex-multiply `(a·b)ᵣ = aᵣbᵣ - aᵢbᵢ ; (a·b)ᵢ = aᵣbᵢ + aᵢbᵣ`.
    ///
    /// This is the core of `bind` — note the FMA-pattern compiles to two
    /// `vfmadd*` on AVX-512 when called in a tight loop, which is why the
    /// `[f32; 256]` polar storage is converted to cartesian for bind paths.
    #[inline]
    pub fn mul(self, other: Self) -> Self {
        Self {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }

    /// § Pointwise complex-add `(a + b) = (aᵣ+bᵣ, aᵢ+bᵢ)`.
    #[inline]
    pub fn add(self, other: Self) -> Self {
        Self {
            re: self.re + other.re,
            im: self.im + other.im,
        }
    }

    /// § Scalar-multiply `(αz) = (αre, αim)`.
    #[inline]
    pub fn scale(self, alpha: f32) -> Self {
        Self {
            re: self.re * alpha,
            im: self.im * alpha,
        }
    }
}

/// § Wrap a phase into `[-π, π]`. Deterministic across hosts.
///
/// Uses explicit subtraction loops rather than `rem_euclid` to avoid the
/// rounding-mode subtleties of mixed-sign `%`. Stays in `f32`.
#[inline]
pub fn wrap_phase(mut phi: f32) -> f32 {
    use core::f32::consts::PI;
    let two_pi = 2.0 * PI;
    while phi > PI {
        phi -= two_pi;
    }
    while phi < -PI {
        phi += two_pi;
    }
    phi
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f32::consts::PI;

    #[test]
    fn polar_roundtrip_at_unit_amp() {
        // π/3 phase + amp 1.0 → cart → polar → close to original.
        let z = C32::from_polar(1.0, PI / 3.0);
        let (amp, phase) = z.to_polar();
        assert!((amp - 1.0).abs() < 1e-5);
        assert!((phase - PI / 3.0).abs() < 1e-5);
    }

    #[test]
    fn mul_matches_polar_definition() {
        // (amp_a · amp_b , phase_a + phase_b) under multiplication.
        let a = C32::from_polar(0.7, 0.3);
        let b = C32::from_polar(0.5, 0.8);
        let c = a.mul(b);
        let (amp_c, phase_c) = c.to_polar();
        assert!((amp_c - 0.35).abs() < 1e-5);
        assert!((phase_c - 1.1).abs() < 1e-5);
    }

    #[test]
    fn conj_negates_imaginary() {
        let z = C32::new(0.7, -0.4);
        let zc = z.conj();
        assert_eq!(zc.re, 0.7);
        assert_eq!(zc.im, 0.4);
    }

    #[test]
    fn wrap_phase_into_principal_range() {
        let p1 = wrap_phase(3.0 * PI);
        assert!((p1 - PI).abs() < 1e-5 || (p1 + PI).abs() < 1e-5);
        let p2 = wrap_phase(-3.0 * PI);
        assert!((p2 - PI).abs() < 1e-5 || (p2 + PI).abs() < 1e-5);
        let p3 = wrap_phase(0.5);
        assert!((p3 - 0.5).abs() < 1e-6);
    }
}
