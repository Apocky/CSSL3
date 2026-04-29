//! § Complex<f32> — minimal complex-amplitude arithmetic for ψ-AUDIO band.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The wave-unity ψ-field is **complex-valued** : `ψ : ℝ³ × ℝ → ℂ`. Per
//!   `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl § II.1` :
//!
//!   ```text
//!   real-part Re(ψ) ⊗ scalar-pressure-or-E-field-amplitude
//!   imag-part Im(ψ) ⊗ phase-conjugate-or-B-field-amplitude
//!   complex-magnitude |ψ|² ⊗ energy-density @ band
//!   ```
//!
//!   For the AUDIO band specifically, Re(ψ) is the instantaneous acoustic
//!   pressure and Im(ψ) is the analytic-signal Hilbert-conjugate (carrying
//!   phase). The LBM stream-collide sweeps `f_i ∈ ℂ` per direction so
//!   complex arithmetic is the inner-loop operation.
//!
//! § DESIGN — own copy, no upstream dep
//!   We could pull in the `num-complex` crate but the wave-audio hot path
//!   is small enough to roll our own : add / sub / mul / scalar-mul / norm /
//!   norm-sq / conj / arg / from_polar. This keeps the dependency surface
//!   minimal + matches the workspace `cssl-pga` precedent of inlining
//!   tiny math primitives rather than depending on `num-traits`.
//!
//! § DETERMINISM
//!   All operations are pure functions over the inputs ; no platform clock
//!   reads, no RNG, no NaN-injection. The LBM stream-collide kernel + the
//!   binaural projection kernel both rely on bit-equal output for two
//!   replays with identical inputs.

/// Complex number with `f32` real + imaginary parts. `repr(C)` so the
/// type can be stored directly in std430 GPU buffers (when the LBM
/// kernel migrates to GPU in a follow-up slice — for now this is CPU-
/// native).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct Complex {
    /// Real part — instantaneous pressure for AUDIO-band ψ.
    pub re: f32,
    /// Imaginary part — Hilbert-conjugate phase for AUDIO-band ψ.
    pub im: f32,
}

impl Complex {
    /// The additive identity : `0 + 0i`.
    pub const ZERO: Complex = Complex { re: 0.0, im: 0.0 };

    /// The multiplicative identity : `1 + 0i`.
    pub const ONE: Complex = Complex { re: 1.0, im: 0.0 };

    /// The imaginary unit : `0 + 1i`.
    pub const I: Complex = Complex { re: 0.0, im: 1.0 };

    /// Construct from real + imaginary parts.
    #[must_use]
    pub const fn new(re: f32, im: f32) -> Complex {
        Complex { re, im }
    }

    /// Construct from a real-only value (`im = 0`). Useful when injecting
    /// a real-valued source amplitude into the ψ-field.
    #[must_use]
    pub const fn from_real(re: f32) -> Complex {
        Complex { re, im: 0.0 }
    }

    /// Construct from polar form : `r * e^(iθ) = r·cos(θ) + i·r·sin(θ)`.
    #[must_use]
    pub fn from_polar(r: f32, theta: f32) -> Complex {
        Complex {
            re: r * theta.cos(),
            im: r * theta.sin(),
        }
    }

    /// Magnitude squared : `|z|² = re² + im²`. The energy-density per
    /// spec § II.1 : `|ψ|² ⊗ energy-density @ band`. Avoids the sqrt
    /// when the consumer only needs the relative ordering or the
    /// quadratic energy form.
    #[must_use]
    pub fn norm_sq(self) -> f32 {
        self.re * self.re + self.im * self.im
    }

    /// Magnitude : `|z| = sqrt(re² + im²)`. The acoustic-pressure-
    /// amplitude per band. Use `norm_sq` if you only need the squared
    /// form.
    #[must_use]
    pub fn norm(self) -> f32 {
        self.norm_sq().sqrt()
    }

    /// Phase angle : `arg(z) = atan2(im, re)`. The phase-coherence metric
    /// per spec § XII.2 reads this across cells to detect numerical
    /// decoherence beyond physical decoherence.
    #[must_use]
    pub fn arg(self) -> f32 {
        self.im.atan2(self.re)
    }

    /// Complex conjugate : `re - i·im`. The Helmholtz adjoint operator
    /// uses this when forming the Hermitian inner product `⟨ψ, ψ⟩`.
    #[must_use]
    pub fn conj(self) -> Complex {
        Complex {
            re: self.re,
            im: -self.im,
        }
    }

    /// Complex addition.
    #[must_use]
    pub fn add(self, rhs: Complex) -> Complex {
        Complex {
            re: self.re + rhs.re,
            im: self.im + rhs.im,
        }
    }

    /// Complex subtraction.
    #[must_use]
    pub fn sub(self, rhs: Complex) -> Complex {
        Complex {
            re: self.re - rhs.re,
            im: self.im - rhs.im,
        }
    }

    /// Complex multiplication : `(a+bi)(c+di) = (ac-bd) + (ad+bc)i`.
    #[must_use]
    pub fn mul(self, rhs: Complex) -> Complex {
        Complex {
            re: self.re * rhs.re - self.im * rhs.im,
            im: self.re * rhs.im + self.im * rhs.re,
        }
    }

    /// Scalar multiplication : `r·z = (r·re) + (r·im)i`.
    #[must_use]
    pub fn scale(self, r: f32) -> Complex {
        Complex {
            re: self.re * r,
            im: self.im * r,
        }
    }

    /// Linear interpolation in the complex plane : `(1-t)·a + t·b`. Used
    /// by the boundary-condition Robin-BC application when tier-A and
    /// tier-B cells meet (tier-boundary trilinear interpolation).
    #[must_use]
    pub fn lerp(self, rhs: Complex, t: f32) -> Complex {
        Complex {
            re: self.re + (rhs.re - self.re) * t,
            im: self.im + (rhs.im - self.im) * t,
        }
    }

    /// Apply a phase rotation : `z * e^(iθ)`. Equivalent to multiplying
    /// by `Complex::from_polar(1.0, theta)` but inlined for the LBM
    /// stream-step inner loop.
    #[must_use]
    pub fn rotate_phase(self, theta: f32) -> Complex {
        let c = theta.cos();
        let s = theta.sin();
        Complex {
            re: self.re * c - self.im * s,
            im: self.re * s + self.im * c,
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::Complex;

    #[test]
    fn zero_is_default() {
        assert_eq!(Complex::default(), Complex::ZERO);
    }

    #[test]
    fn from_real_zeros_imag() {
        let z = Complex::from_real(2.5);
        assert_eq!(z.re, 2.5);
        assert_eq!(z.im, 0.0);
    }

    #[test]
    fn one_norm_is_one() {
        assert!((Complex::ONE.norm() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn i_norm_is_one() {
        assert!((Complex::I.norm() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn norm_sq_pythagorean() {
        let z = Complex::new(3.0, 4.0);
        assert_eq!(z.norm_sq(), 25.0);
        assert_eq!(z.norm(), 5.0);
    }

    #[test]
    fn arg_real_axis_zero() {
        let z = Complex::new(1.0, 0.0);
        assert!(z.arg().abs() < 1e-6);
    }

    #[test]
    fn arg_imag_axis_pi_over_two() {
        let z = Complex::new(0.0, 1.0);
        assert!((z.arg() - core::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn conj_negates_imag() {
        let z = Complex::new(1.0, -2.0);
        let zc = z.conj();
        assert_eq!(zc.re, 1.0);
        assert_eq!(zc.im, 2.0);
    }

    #[test]
    fn conj_self_is_norm_sq_real() {
        let z = Complex::new(3.0, 4.0);
        let prod = z.mul(z.conj());
        assert!((prod.re - 25.0).abs() < 1e-5);
        assert!(prod.im.abs() < 1e-5);
    }

    #[test]
    fn add_componentwise() {
        let a = Complex::new(1.0, 2.0);
        let b = Complex::new(3.0, 4.0);
        let s = a.add(b);
        assert_eq!(s, Complex::new(4.0, 6.0));
    }

    #[test]
    fn sub_componentwise() {
        let a = Complex::new(5.0, 7.0);
        let b = Complex::new(3.0, 2.0);
        let s = a.sub(b);
        assert_eq!(s, Complex::new(2.0, 5.0));
    }

    #[test]
    fn mul_distributive() {
        // (1+2i)(3+4i) = 3+4i+6i+8i² = 3+10i-8 = -5+10i
        let a = Complex::new(1.0, 2.0);
        let b = Complex::new(3.0, 4.0);
        let p = a.mul(b);
        assert!((p.re - (-5.0)).abs() < 1e-5);
        assert!((p.im - 10.0).abs() < 1e-5);
    }

    #[test]
    fn mul_one_identity() {
        let z = Complex::new(0.5, -1.5);
        let p = z.mul(Complex::ONE);
        assert_eq!(p, z);
    }

    #[test]
    fn mul_i_rotates_90_degrees() {
        // i·(1+0i) = 0+1i
        let z = Complex::new(1.0, 0.0);
        let r = z.mul(Complex::I);
        assert!((r.re - 0.0).abs() < 1e-6);
        assert!((r.im - 1.0).abs() < 1e-6);
    }

    #[test]
    fn scale_doubles_components() {
        let z = Complex::new(1.0, -1.0);
        let s = z.scale(2.0);
        assert_eq!(s, Complex::new(2.0, -2.0));
    }

    #[test]
    fn from_polar_unity_at_zero_phase() {
        let z = Complex::from_polar(1.0, 0.0);
        assert!((z.re - 1.0).abs() < 1e-6);
        assert!(z.im.abs() < 1e-6);
    }

    #[test]
    fn from_polar_unity_at_pi_over_two() {
        let z = Complex::from_polar(1.0, core::f32::consts::FRAC_PI_2);
        assert!(z.re.abs() < 1e-6);
        assert!((z.im - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rotate_phase_by_zero_is_identity() {
        let z = Complex::new(0.7, -0.3);
        let r = z.rotate_phase(0.0);
        assert!((r.re - z.re).abs() < 1e-6);
        assert!((r.im - z.im).abs() < 1e-6);
    }

    #[test]
    fn rotate_phase_pi_negates() {
        let z = Complex::new(1.0, 0.5);
        let r = z.rotate_phase(core::f32::consts::PI);
        assert!((r.re - (-1.0)).abs() < 1e-5);
        assert!((r.im - (-0.5)).abs() < 1e-5);
    }

    #[test]
    fn lerp_t_zero_is_self() {
        let a = Complex::new(1.0, 2.0);
        let b = Complex::new(3.0, 4.0);
        let r = a.lerp(b, 0.0);
        assert_eq!(r, a);
    }

    #[test]
    fn lerp_t_one_is_other() {
        let a = Complex::new(1.0, 2.0);
        let b = Complex::new(3.0, 4.0);
        let r = a.lerp(b, 1.0);
        assert_eq!(r, b);
    }

    #[test]
    fn lerp_midpoint_average() {
        let a = Complex::new(0.0, 0.0);
        let b = Complex::new(2.0, 4.0);
        let r = a.lerp(b, 0.5);
        assert_eq!(r, Complex::new(1.0, 2.0));
    }

    #[test]
    fn determinism_replay_bit_equal() {
        // Two evaluations with identical inputs must produce bit-equal output.
        let a = Complex::new(1.234, -5.678);
        let b = Complex::new(0.5, 1.5);
        let p1 = a.mul(b).rotate_phase(0.7);
        let p2 = a.mul(b).rotate_phase(0.7);
        assert_eq!(p1.re.to_bits(), p2.re.to_bits());
        assert_eq!(p1.im.to_bits(), p2.im.to_bits());
    }
}
