//! § IMEX implicit-explicit step for stiff bands.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § THESIS (Wave-Unity §VI.1)
//!   Slow-modes (HEAT, SCENT, MANA-envelope) and stiff fast-modes (LIGHT
//!   envelope under SVEA) are advanced via an **IMEX** (implicit-explicit)
//!   split :
//!
//!     `(I + Δt · A_implicit) ψ_new = ψ + Δt · F_explicit(ψ)`
//!
//!   where `A_implicit` is the stiff diagonal-dominant operator (linear
//!   absorption + envelope decay) and `F_explicit(ψ)` is the explicit
//!   nonlinear remainder (cross-band-coupling source + boundary
//!   forcing). Because `A_implicit` is diagonal-dominant in the wave-LBM
//!   formulation, the implicit solve reduces to a per-cell point-update :
//!
//!     `ψ_new = (ψ + Δt · F_explicit) / (1 + Δt · α_b)`
//!
//!   where `α_b ∈ ℝ⁺` is the per-band absorption coefficient. The
//!   omega_step DETERMINISM CONTRACT is honoured because the per-cell
//!   division is bit-deterministic on x86-64 SSE2.
//!
//! § STAGE-0 SCOPE
//!   - Diagonal-dominant absorption per band (single `α_b` parameter).
//!   - LIGHT envelope under SVEA uses `α_b ≈ 0.01` (small absorption).
//!   - AUDIO uses `α_b ≈ 0.001` (very small absorption ; air-loss).
//!   - Cross-band coupling enters as `F_explicit` ; computed by
//!     [`crate::coupling`].
//!
//! § FLOP COUNT (§ IX.1)
//!   Per cell per substep :
//!     - 1 complex multiply (Δt · F_explicit).
//!     - 1 complex add (ψ + Δt · F).
//!     - 1 complex divide (point-update).
//!   approximately 20 FLOP / cell. At 1 M cells × 5 bands = 100 MF/frame.
//!
//! § DETERMINISM
//!   - All updates are computed from `prev` into `next` (no in-place).
//!   - Iteration walks `prev` in Morton-sorted order.
//!   - The division is the only step where SSE2 round-to-nearest-even
//!     matters ; the cssl-rt ABI default honours this.

use crate::band::BandClass;
use crate::complex::C32;
use crate::psi_field::WaveField;

use cssl_substrate_omega_field::MortonKey;

/// § Per-band default absorption coefficient. Stage-0 picks conservative
///   values per spec §XI ; the runtime version reads from the
///   KAN-impedance `R(λ, embedding)` once D115 lands.
#[must_use]
pub fn default_absorption(class: BandClass) -> f32 {
    match class {
        BandClass::FastDirect => 0.001,  // audio : 0.1 % per substep
        BandClass::FastEnvelope => 0.01, // light envelope : 1 % per substep
        BandClass::SlowEnvelope => 0.05, // heat/scent/mana
    }
}

/// § Run one IMEX implicit-explicit substep. `forcing` carries any
///   explicit cross-band-coupling source that has been pre-computed by
///   [`crate::coupling::apply_cross_coupling`] into a separate buffer.
///   At Stage-0 the forcing buffer is taken as zero (see `imex_implicit_step_no_forcing`).
pub fn imex_implicit_step_with_forcing<const C: usize>(
    prev: &WaveField<C>,
    next: &mut WaveField<C>,
    band_idx: usize,
    dt: f64,
    absorption: f32,
    forcing: impl Fn(MortonKey) -> C32,
) -> usize {
    if band_idx >= prev.band_count() {
        return 0;
    }
    let dt_alpha = (dt as f32) * absorption;
    let denom = 1.0 + dt_alpha;
    let cells: Vec<(MortonKey, C32)> = prev.cells_in_band(band_idx).collect();
    let touched = cells.len();
    for (k, psi_here) in &cells {
        let f = forcing(*k);
        // ψ_new = (ψ + Δt · f) / (1 + Δt · α).
        let numerator = *psi_here + f.scale(dt as f32);
        let psi_new = numerator.scale(1.0 / denom);
        if psi_new.is_finite() {
            next.set(band_idx, *k, psi_new);
        } else {
            next.set(band_idx, *k, C32::ZERO);
        }
    }
    touched
}

/// § Convenience wrapper : IMEX step with zero explicit forcing. Used
///   when the band is advanced in isolation (no cross-band coupling).
pub fn imex_implicit_step<const C: usize>(
    prev: &WaveField<C>,
    next: &mut WaveField<C>,
    band_idx: usize,
    dt: f64,
    absorption: f32,
) -> usize {
    imex_implicit_step_with_forcing(prev, next, band_idx, dt, absorption, |_| C32::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::band::Band;

    fn key(x: u64, y: u64, z: u64) -> MortonKey {
        MortonKey::encode(x, y, z).unwrap()
    }

    #[test]
    fn default_absorption_audio_is_minimal() {
        assert!(default_absorption(BandClass::FastDirect) < 0.01);
    }

    #[test]
    fn default_absorption_light_is_moderate() {
        assert!(default_absorption(BandClass::FastEnvelope) > 0.0);
        assert!(default_absorption(BandClass::FastEnvelope) < 0.1);
    }

    #[test]
    fn default_absorption_slow_is_higher() {
        assert!(default_absorption(BandClass::SlowEnvelope) > 0.0);
        assert!(default_absorption(BandClass::SlowEnvelope) < 0.5);
    }

    #[test]
    fn imex_no_forcing_decays_amplitude() {
        let mut prev = WaveField::<5>::with_default_bands();
        let k = key(0, 0, 0);
        prev.set_band(Band::AudioSubKHz, k, C32::new(1.0, 0.0));
        let mut next = WaveField::<5>::with_default_bands();
        // ψ_new = ψ / (1 + dt·α) ; with dt=1.0, α=0.1, denom=1.1.
        // ψ_new = 1/1.1 ≈ 0.909.
        imex_implicit_step(&prev, &mut next, 0, 1.0, 0.1);
        let v = next.at_band(Band::AudioSubKHz, k);
        assert!((v.re - 1.0 / 1.1).abs() < 1e-5);
        assert!(v.im.abs() < 1e-9);
    }

    #[test]
    fn imex_zero_absorption_is_identity() {
        let mut prev = WaveField::<5>::with_default_bands();
        let k = key(0, 0, 0);
        prev.set_band(Band::AudioSubKHz, k, C32::new(0.7, 0.3));
        let mut next = WaveField::<5>::with_default_bands();
        imex_implicit_step(&prev, &mut next, 0, 1.0, 0.0);
        let v = next.at_band(Band::AudioSubKHz, k);
        assert!((v.re - 0.7).abs() < 1e-9);
        assert!((v.im - 0.3).abs() < 1e-9);
    }

    #[test]
    fn imex_with_forcing_adds_source() {
        // The IMEX iterator only walks cells PRESENT in `prev` (the
        // canonical "advance existing cells" pattern). To verify the
        // forcing-add behavior we seed the cell with a small amplitude.
        let mut prev = WaveField::<5>::with_default_bands();
        let k = key(0, 0, 0);
        prev.set_band(Band::AudioSubKHz, k, C32::new(0.5, 0.0));
        let mut next = WaveField::<5>::with_default_bands();
        let force = |kq: MortonKey| {
            if kq == k {
                C32::new(0.5, 0.0)
            } else {
                C32::ZERO
            }
        };
        imex_implicit_step_with_forcing(&prev, &mut next, 0, 1.0, 0.0, force);
        // ψ_new = (0.5 + 1·0.5) / (1 + 0) = 1.
        let v = next.at_band(Band::AudioSubKHz, k);
        assert!((v.re - 1.0).abs() < 1e-6);
    }

    #[test]
    fn imex_step_total_norm_decreases_with_absorption() {
        let mut prev = WaveField::<5>::with_default_bands();
        for i in 0..10_u64 {
            prev.set_band(Band::LightRed, key(i, 0, 0), C32::new(1.0, 0.0));
        }
        let n_before = prev.band_norm_sqr_band(Band::LightRed);
        let mut next = WaveField::<5>::with_default_bands();
        imex_implicit_step(&prev, &mut next, 1, 1.0, 0.1);
        let n_after = next.band_norm_sqr_band(Band::LightRed);
        // Norm should decrease monotonically with absorption.
        assert!(n_after < n_before);
        assert!(n_after > 0.0);
    }

    #[test]
    fn imex_replay_deterministic() {
        let mut prev = WaveField::<5>::with_default_bands();
        for i in 0..5_u64 {
            prev.set_band(Band::LightGreen, key(i, 0, 0), C32::new(i as f32, i as f32));
        }
        let mut n1 = WaveField::<5>::with_default_bands();
        let mut n2 = WaveField::<5>::with_default_bands();
        imex_implicit_step(&prev, &mut n1, 2, 0.001, 0.05);
        imex_implicit_step(&prev, &mut n2, 2, 0.001, 0.05);
        for i in 0..5_u64 {
            let k = key(i, 0, 0);
            let v1 = n1.at_band(Band::LightGreen, k);
            let v2 = n2.at_band(Band::LightGreen, k);
            assert_eq!(v1, v2);
        }
    }

    #[test]
    fn imex_handles_oob_band_index() {
        let prev = WaveField::<5>::with_default_bands();
        let mut next = WaveField::<5>::with_default_bands();
        let touched = imex_implicit_step(&prev, &mut next, 99, 1.0, 0.1);
        assert_eq!(touched, 0);
    }

    #[test]
    fn imex_blowup_clamps_to_zero() {
        let mut prev = WaveField::<5>::with_default_bands();
        let k = key(0, 0, 0);
        prev.set_band(Band::AudioSubKHz, k, C32::new(f32::NAN, 0.0));
        let mut next = WaveField::<5>::with_default_bands();
        imex_implicit_step(&prev, &mut next, 0, 1.0, 0.1);
        let v = next.at_band(Band::AudioSubKHz, k);
        assert_eq!(v, C32::ZERO);
    }
}
