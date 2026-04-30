//! § light_stub — stub light-state ABI for W-S-CORE-2 (cssl-substrate-light).
//!
//! ## Purpose
//! Documents the per-cell light-state contract the CFER iterator depends on,
//! BEFORE the real `cssl-substrate-light` crate lands. The stub provides
//! enough surface area to wire the convergence-loop, multigrid V-cycle, and
//! denoiser without diff-churn at upgrade-time.
//!
//! ## Migration
//! When `cssl-substrate-light` lands :
//!   1. Cargo.toml : add `cssl-substrate-light = { path = ... }` ;
//!      remove the local `light_stub` re-exports.
//!   2. Replace `crate::light_stub::LightState` with the real type.
//!   3. The convergence-loop in `cfer::cfer_render_frame` only depends on
//!      [`LightState::norm_diff_l1`] + [`LightState::scale`] + arithmetic ; the
//!      real type implements the same trait surface so no driver code changes.
//!
//! ## Math
//! Per spec § 36 : `L_c(λ, θφ) ∈ ApockyLight` is the angular-spectral light
//! distribution per cell, compressed via per-cell KAN-band into `k` learned-
//! basis coefficients. We pick `k = 8` for the stub ; the real crate will
//! decide whether 4, 8, or 16 is the production-baseline.

use core::ops::{Add, AddAssign, Mul, Sub};

/// Number of KAN-band coefficients per cell light-state (stub ; production-
/// baseline TBD by W-S-CORE-2).
pub const LIGHT_STATE_COEFS: usize = 8;

/// Stub spectral-band descriptor : (λ_lo, λ_hi, n_coefs).
///
/// Per spec § 36 § Light-state per-cell, the band is the basis-expansion
/// shape used to compress angular-spectral radiance into `n_coefs` floats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectralBand {
    /// Lower wavelength (nm) of the band.
    pub lambda_lo_nm: f32,
    /// Upper wavelength (nm) of the band.
    pub lambda_hi_nm: f32,
    /// Number of KAN-basis coefficients (≤ [`LIGHT_STATE_COEFS`]).
    pub n_coefs: u16,
}

impl SpectralBand {
    /// Visible-light default : 380nm–780nm, 8 KAN-coefs.
    pub const VISIBLE: Self = Self {
        lambda_lo_nm: 380.0,
        lambda_hi_nm: 780.0,
        n_coefs: LIGHT_STATE_COEFS as u16,
    };

    /// Band-width in nm.
    #[inline]
    pub fn width_nm(self) -> f32 {
        self.lambda_hi_nm - self.lambda_lo_nm
    }
}

/// Stub light-state : 8 KAN-band coefficients + dirty-flag.
///
/// Per spec § 36 § Field-evolution PDE the per-cell light-state evolves under
/// the radiance-transport iteration. The CFER driver reads/writes via the
/// minimal surface :
///
///   - [`LightState::zero`] — null-light initializer.
///   - [`LightState::norm_diff_l1`] — convergence-residual.
///   - [`LightState::scale`] — multigrid prolongation/restriction scalar.
///   - `Add` / `Sub` / `Mul` — arithmetic for the evolution step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightState {
    /// 8 KAN-coefs in the cell's local basis.
    pub coefs: [f32; LIGHT_STATE_COEFS],
    /// Convergence-flag : true when last update hit ‖ΔL‖ < ε.
    pub converged: bool,
}

impl LightState {
    /// Null-light state ; all coefs zero, marked converged-trivially.
    pub const fn zero() -> Self {
        Self {
            coefs: [0.0; LIGHT_STATE_COEFS],
            converged: true,
        }
    }

    /// Construct from explicit coefs ; defaults `converged = false`.
    pub fn from_coefs(coefs: [f32; LIGHT_STATE_COEFS]) -> Self {
        Self {
            coefs,
            converged: false,
        }
    }

    /// L1-norm difference : Σ |a_i - b_i| over coefs. Used for the CFER
    /// convergence-test ‖ΔL‖ < ε.
    #[inline]
    pub fn norm_diff_l1(self, other: Self) -> f32 {
        let mut acc = 0.0_f32;
        for i in 0..LIGHT_STATE_COEFS {
            acc += (self.coefs[i] - other.coefs[i]).abs();
        }
        acc
    }

    /// Total radiance (sum of coefs ; stub heuristic — real type uses
    /// quadrature against the basis).
    #[inline]
    pub fn radiance(self) -> f32 {
        let mut acc = 0.0_f32;
        for i in 0..LIGHT_STATE_COEFS {
            acc += self.coefs[i];
        }
        acc
    }

    /// Multigrid scaling : multiply all coefs by `s`.
    #[inline]
    pub fn scale(self, s: f32) -> Self {
        let mut out = self.coefs;
        for i in 0..LIGHT_STATE_COEFS {
            out[i] *= s;
        }
        Self {
            coefs: out,
            converged: self.converged,
        }
    }

    /// Decompress at a given wavelength sample. Stub : linear-mix of coefs by
    /// position in band ; real type evaluates the KAN basis.
    pub fn sample_at(self, band: SpectralBand, lambda_nm: f32) -> f32 {
        let w = band.width_nm().max(1e-3);
        let t = ((lambda_nm - band.lambda_lo_nm) / w).clamp(0.0, 1.0);
        let n = LIGHT_STATE_COEFS as f32;
        let f = (t * (n - 1.0)).max(0.0);
        let lo = f.floor() as usize;
        let hi = (lo + 1).min(LIGHT_STATE_COEFS - 1);
        let a = f - (lo as f32);
        self.coefs[lo] * (1.0 - a) + self.coefs[hi] * a
    }
}

impl Default for LightState {
    fn default() -> Self {
        Self::zero()
    }
}

impl Add for LightState {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        let mut out = self.coefs;
        for i in 0..LIGHT_STATE_COEFS {
            out[i] += rhs.coefs[i];
        }
        Self {
            coefs: out,
            converged: false,
        }
    }
}

impl AddAssign for LightState {
    fn add_assign(&mut self, rhs: Self) {
        for i in 0..LIGHT_STATE_COEFS {
            self.coefs[i] += rhs.coefs[i];
        }
        self.converged = false;
    }
}

impl Sub for LightState {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        let mut out = self.coefs;
        for i in 0..LIGHT_STATE_COEFS {
            out[i] -= rhs.coefs[i];
        }
        Self {
            coefs: out,
            converged: false,
        }
    }
}

impl Mul<f32> for LightState {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        self.scale(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_is_default_and_zero_radiance() {
        let z = LightState::zero();
        assert_eq!(z, LightState::default());
        assert_eq!(z.radiance(), 0.0);
        assert!(z.converged);
    }

    #[test]
    fn norm_diff_l1_zero_for_equal_states() {
        let a = LightState::from_coefs([0.5; LIGHT_STATE_COEFS]);
        let b = LightState::from_coefs([0.5; LIGHT_STATE_COEFS]);
        assert_eq!(a.norm_diff_l1(b), 0.0);
    }

    #[test]
    fn norm_diff_l1_nonzero_for_unequal() {
        let a = LightState::zero();
        let b = LightState::from_coefs([1.0; LIGHT_STATE_COEFS]);
        let d = a.norm_diff_l1(b);
        assert_eq!(d, LIGHT_STATE_COEFS as f32);
    }

    #[test]
    fn scale_multiplies_all_coefs() {
        let a = LightState::from_coefs([1.0; LIGHT_STATE_COEFS]);
        let s = a.scale(0.5);
        for i in 0..LIGHT_STATE_COEFS {
            assert_eq!(s.coefs[i], 0.5);
        }
    }

    #[test]
    fn add_and_sub_inverse() {
        let a = LightState::from_coefs([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        let b = LightState::from_coefs([0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5]);
        let c = a + b;
        let d = c - b;
        assert_eq!(a.norm_diff_l1(d), 0.0);
    }

    #[test]
    fn visible_band_width_is_400nm() {
        assert_eq!(SpectralBand::VISIBLE.width_nm(), 400.0);
        assert_eq!(SpectralBand::VISIBLE.n_coefs as usize, LIGHT_STATE_COEFS);
    }

    #[test]
    fn sample_at_clamps_outside_band() {
        let s = LightState::from_coefs([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        let v_below = s.sample_at(SpectralBand::VISIBLE, 100.0);
        let v_above = s.sample_at(SpectralBand::VISIBLE, 9000.0);
        assert_eq!(v_below, 1.0);
        assert_eq!(v_above, 8.0);
    }
}
