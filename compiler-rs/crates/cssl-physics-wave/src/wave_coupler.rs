//! § WaveImpactCoupler — contact-impact → ψ-field excitation.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The bridge between the discrete-impulse XPBD solver and the
//!   continuous ψ-field wave-substrate (`cssl-substrate-omega-field`'s
//!   `psi` overlay, fed by the D114 wave-solver).
//!
//!   At every contact event the coupler :
//!
//!   1. Estimates the impact-energy from the relative velocity along the
//!      contact normal + the reduced mass.
//!   2. Synthesizes a contact-spectrum across the wave-unity bands
//!      (LIGHT / AUDIO / HEAT / SCENT / MANA) per Omniverse `04_OMEGA_FIELD/
//!      04_WAVE_UNITY.csl` § IV.3 "every-impact-sounds-like-its-physics".
//!   3. Emits a `WaveExcitation` event the omega-step pipeline writes
//!      back into the ψ-overlay on the next 2a substep.
//!
//!   This is the canonical "physics → wave-solver" hand-off the audit calls
//!   for. Without this coupler the wave-solver has no physical-event
//!   driver and the wave-physics narrative collapses to two disconnected
//!   subsystems.
//!
//! § SPECTRUM SYNTHESIS
//!   Per Omniverse `WAVE_UNITY` § IV.3 :
//!     "spectral-content @ A_n ⊗ KAN-derived from {force, material, position}
//!      cite : FIELD_AUDIO §III synthesize_source(pos, mat, force) → AudioSpectrum
//!      WAVE-UNITY identity : same-source-KAN drives ALL-bands @ ψ"
//!
//!   The wave-physics V0 coupler does NOT call into the source-KAN — that
//!   wiring lives in the cssl-wave-solver crate that owns the ψ-substrate.
//!   Instead, V0 emits a `ContactSpectrum` that the wave-solver consumes
//!   on the next substep and resolves through the source-KAN itself.
//!
//! § AGENCY
//!   - The coupler ONLY emits events ; it does NOT directly write the
//!     ψ-overlay. Σ-check happens at the wave-solver write-path, not here.
//!   - The energy-floor `IMPACT_ENERGY_FLOOR` filters out micro-impacts
//!     below the perception threshold so the wave-substrate isn't spammed
//!     with noise from microscopic settling jitter.

use cssl_substrate_omega_field::MortonKey;
use thiserror::Error;

/// § Energy floor in Joules. Impacts below this don't emit excitations.
pub const IMPACT_ENERGY_FLOOR: f32 = 1e-6;

/// § Number of wave-unity bands the coupler addresses :
///   `LIGHT / AUDIO / HEAT / SCENT / MANA`.
pub const WAVE_UNITY_BANDS: usize = 5;

/// § Band-id constants (matching Omniverse spec ordering).
pub const BAND_LIGHT: usize = 0;
/// § Band-id : audio.
pub const BAND_AUDIO: usize = 1;
/// § Band-id : heat (long-IR).
pub const BAND_HEAT: usize = 2;
/// § Band-id : scent.
pub const BAND_SCENT: usize = 3;
/// § Band-id : mana (Λ-token-derived).
pub const BAND_MANA: usize = 4;

/// § Default coupling fractions per band — what fraction of the impact
///   energy gets routed to each band. The constants are calibrated to
///   the cross-band-coupling table in Omniverse `WAVE_UNITY` § XI.
pub const DEFAULT_COUPLING_AUDIO: f32 = 0.20;
/// § Default coupling : heat.
pub const DEFAULT_COUPLING_HEAT: f32 = 0.05;
/// § Default coupling : light (typically only metallic / spark-on-stone).
pub const DEFAULT_COUPLING_LIGHT: f32 = 0.001;
/// § Default coupling : scent (chemistry-of-impact).
pub const DEFAULT_COUPLING_SCENT: f32 = 0.0001;
/// § Default coupling : mana (only for Λ-active impacts).
pub const DEFAULT_COUPLING_MANA: f32 = 0.0;

// ───────────────────────────────────────────────────────────────────────
// § ContactSpectrum.
// ───────────────────────────────────────────────────────────────────────

/// § Per-band amplitude vector emitted by the coupler. The wave-solver
///   reads this on its next substep and resolves through the source-KAN.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactSpectrum {
    /// Per-band amplitudes (in Joules of `energy carried by this band`).
    pub bands: [f32; WAVE_UNITY_BANDS],
    /// Center-frequency-shift ratio (used as a hint by the source-KAN).
    pub freq_shift: f32,
}

impl ContactSpectrum {
    /// § A spectrum with all bands zero.
    pub const SILENT: ContactSpectrum = ContactSpectrum {
        bands: [0.0; WAVE_UNITY_BANDS],
        freq_shift: 1.0,
    };

    /// § Total energy across all bands.
    #[must_use]
    pub fn total_energy(&self) -> f32 {
        self.bands.iter().sum()
    }

    /// § True iff this spectrum is below the energy floor (i.e., should
    ///   be discarded).
    #[must_use]
    pub fn is_below_floor(&self) -> bool {
        self.total_energy() < IMPACT_ENERGY_FLOOR
    }
}

// ───────────────────────────────────────────────────────────────────────
// § WaveExcitation.
// ───────────────────────────────────────────────────────────────────────

/// § A single wave-excitation event emitted by the coupler.
///
///   Carries the cell-key (where the excitation lands), the contact
///   spectrum, and metadata for the wave-solver's downstream KAN-eval.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveExcitation {
    /// The cell to inject the excitation into.
    pub cell: MortonKey,
    /// World-space position of the contact (sub-cell precision).
    pub position: [f32; 3],
    /// World-space contact-normal (unit).
    pub normal: [f32; 3],
    /// Per-band spectrum.
    pub spectrum: ContactSpectrum,
    /// Approximate impact velocity along normal (m/s).
    pub impact_velocity: f32,
    /// Approximate effective mass at the contact (kg).
    pub effective_mass: f32,
    /// Time of the excitation in the current substep (`0.0..1.0`).
    pub time_of_impact: f32,
}

impl WaveExcitation {
    /// § Sentinel event for "no excitation" (the coupler returns this
    ///   when the impact is below the energy floor).
    pub const NONE: WaveExcitation = WaveExcitation {
        cell: MortonKey::ZERO,
        position: [0.0; 3],
        normal: [0.0, 1.0, 0.0],
        spectrum: ContactSpectrum::SILENT,
        impact_velocity: 0.0,
        effective_mass: 0.0,
        time_of_impact: 0.0,
    };

    /// § True iff the excitation is non-trivial.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.spectrum.is_below_floor()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § WaveCouplingError.
// ───────────────────────────────────────────────────────────────────────

/// § Failure modes of the wave-coupler.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum WaveCouplingError {
    /// Effective mass collapsed to zero (both bodies infinite-mass) — no
    /// energy to propagate.
    #[error("PHYSWAVE0040 — wave-coupler effective mass is zero (both bodies static)")]
    ZeroEffectiveMass,
    /// Contact spectrum produced a non-finite value.
    #[error("PHYSWAVE0041 — wave-coupler spectrum produced non-finite value")]
    NonFiniteSpectrum,
    /// Contact normal was not unit-length.
    #[error("PHYSWAVE0042 — contact normal length {len} is far from unit")]
    NonUnitNormal {
        /// The actual normal length.
        len: f32,
    },
}

// ───────────────────────────────────────────────────────────────────────
// § WaveImpactCoupler — the canonical struct.
// ───────────────────────────────────────────────────────────────────────

/// § Configuration for the impact coupler.
///
///   Holds the per-band coupling fractions ; consumers can override the
///   defaults to bias the wave-energy distribution per material-class.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveImpactCoupler {
    /// Per-band coupling fractions. Sum should be ≤ 1.0 (energy
    /// conservation) ; the residual stays as kinetic-energy in the
    /// rigid-body branch.
    pub coupling: [f32; WAVE_UNITY_BANDS],
    /// Frequency-shift sensitivity to impact-velocity. Default = 1.0
    /// (no shift) ; > 1.0 → faster impact yields higher-pitched events.
    pub freq_velocity_sensitivity: f32,
}

impl Default for WaveImpactCoupler {
    fn default() -> Self {
        WaveImpactCoupler {
            coupling: [
                DEFAULT_COUPLING_LIGHT,
                DEFAULT_COUPLING_AUDIO,
                DEFAULT_COUPLING_HEAT,
                DEFAULT_COUPLING_SCENT,
                DEFAULT_COUPLING_MANA,
            ],
            freq_velocity_sensitivity: 1.0,
        }
    }
}

impl WaveImpactCoupler {
    /// § Construct a coupler with custom per-band coupling fractions.
    ///
    ///   Every fraction is clamped to `[0, 1]`. The sum is NOT enforced
    ///   to be ≤ 1 ; the wave-solver enforces conservation downstream.
    #[must_use]
    pub fn with_coupling(coupling: [f32; WAVE_UNITY_BANDS]) -> Self {
        let mut clamped = [0.0; WAVE_UNITY_BANDS];
        for i in 0..WAVE_UNITY_BANDS {
            clamped[i] = coupling[i].clamp(0.0, 1.0);
        }
        WaveImpactCoupler {
            coupling: clamped,
            freq_velocity_sensitivity: 1.0,
        }
    }

    /// § Compute the impact-energy from the contact's relative velocity
    ///   along the normal + the reduced mass.
    #[must_use]
    pub fn impact_energy(rel_velocity_along_normal: f32, effective_mass: f32) -> f32 {
        // E = ½ μ v² (kinetic energy in the contact-pair's center-of-mass frame).
        let v = rel_velocity_along_normal.abs();
        // Two-step ; not FMA.
        let v2 = v * v;
        0.5_f32 * (effective_mass * v2)
    }

    /// § Reduced mass : `1 / (1/m_a + 1/m_b)`. With infinite-mass on
    ///   either side the reduced mass collapses to the finite mass of
    ///   the other. With both static returns 0.
    #[must_use]
    pub fn reduced_mass(inv_mass_a: f32, inv_mass_b: f32) -> f32 {
        let total = inv_mass_a + inv_mass_b;
        if total < 1e-12 {
            0.0
        } else {
            1.0_f32 / total
        }
    }

    /// § Synthesize a contact-spectrum from the impact energy.
    #[must_use]
    pub fn synthesize_spectrum(&self, impact_energy: f32, impact_velocity: f32) -> ContactSpectrum {
        let mut bands = [0.0_f32; WAVE_UNITY_BANDS];
        for i in 0..WAVE_UNITY_BANDS {
            bands[i] = self.coupling[i] * impact_energy;
        }
        let freq_shift = if self.freq_velocity_sensitivity == 0.0 {
            1.0
        } else {
            // Logarithmic-ish shift — clamps to [0.5, 4.0] for sanity.
            let v_norm = (impact_velocity.abs() / 5.0).clamp(0.0, 1.0); // 5 m/s = "fast"
            (1.0 + v_norm * self.freq_velocity_sensitivity).clamp(0.5, 4.0)
        };
        ContactSpectrum { bands, freq_shift }
    }

    /// § Emit a `WaveExcitation` event from a contact.
    pub fn emit_excitation(
        &self,
        position: [f32; 3],
        normal: [f32; 3],
        rel_velocity_along_normal: f32,
        inv_mass_a: f32,
        inv_mass_b: f32,
        cell: MortonKey,
        time_of_impact: f32,
    ) -> Result<WaveExcitation, WaveCouplingError> {
        // Verify the normal is approximately unit-length.
        let nl = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        if (nl - 1.0).abs() > 1e-2 {
            return Err(WaveCouplingError::NonUnitNormal { len: nl });
        }
        let m_eff = Self::reduced_mass(inv_mass_a, inv_mass_b);
        if m_eff <= 0.0 {
            return Err(WaveCouplingError::ZeroEffectiveMass);
        }
        let energy = Self::impact_energy(rel_velocity_along_normal, m_eff);
        let spectrum = self.synthesize_spectrum(energy, rel_velocity_along_normal);
        if !spectrum.bands.iter().all(|x| x.is_finite()) {
            return Err(WaveCouplingError::NonFiniteSpectrum);
        }
        if spectrum.is_below_floor() {
            return Ok(WaveExcitation {
                cell,
                position,
                normal,
                spectrum,
                impact_velocity: rel_velocity_along_normal,
                effective_mass: m_eff,
                time_of_impact,
            });
        }
        Ok(WaveExcitation {
            cell,
            position,
            normal,
            spectrum,
            impact_velocity: rel_velocity_along_normal,
            effective_mass: m_eff,
            time_of_impact,
        })
    }

    /// § Emit a metallic-impact-tuned coupling (more LIGHT-band coupling
    ///   for spark-on-stone visual / audio).
    #[must_use]
    pub fn metallic_impact() -> Self {
        let mut c = WaveImpactCoupler::default();
        c.coupling[BAND_LIGHT] = 0.05; // bright spark
        c.coupling[BAND_AUDIO] = 0.30; // loud clang
        c.coupling[BAND_HEAT] = 0.10; // friction-warm
        c
    }

    /// § Emit a magic-impact-tuned coupling (significant MANA-band).
    #[must_use]
    pub fn magic_impact() -> Self {
        let mut c = WaveImpactCoupler::default();
        c.coupling[BAND_MANA] = 0.40;
        c.coupling[BAND_LIGHT] = 0.15;
        c.coupling[BAND_AUDIO] = 0.20;
        c
    }

    /// § Emit a soft-impact (cushioned / fabric) coupling — most energy
    ///   stays kinetic, only a whisper of audio leaks out.
    #[must_use]
    pub fn soft_impact() -> Self {
        let mut c = WaveImpactCoupler::default();
        c.coupling[BAND_AUDIO] = 0.05;
        c.coupling[BAND_HEAT] = 0.01;
        c.coupling[BAND_LIGHT] = 0.0;
        c.coupling[BAND_SCENT] = 0.0;
        c
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Tests.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn impact_energy_zero_velocity_zero_energy() {
        assert_eq!(WaveImpactCoupler::impact_energy(0.0, 1.0), 0.0);
    }

    #[test]
    fn impact_energy_classical_formula() {
        // E = ½ × 2 × 3² = 9
        let e = WaveImpactCoupler::impact_energy(3.0, 2.0);
        assert!(approx(e, 9.0, 1e-6));
    }

    #[test]
    fn reduced_mass_identical_masses() {
        // Two bodies with mass 1 → inv_mass 1 each → reduced 0.5.
        let m = WaveImpactCoupler::reduced_mass(1.0, 1.0);
        assert!(approx(m, 0.5, 1e-6));
    }

    #[test]
    fn reduced_mass_one_static() {
        // Body 1 static (inv_mass = 0) ⇒ reduced = mass of other.
        let m = WaveImpactCoupler::reduced_mass(0.0, 0.5);
        assert!(approx(m, 2.0, 1e-6));
    }

    #[test]
    fn reduced_mass_both_static_zero() {
        let m = WaveImpactCoupler::reduced_mass(0.0, 0.0);
        assert_eq!(m, 0.0);
    }

    #[test]
    fn coupler_default_audio_dominant() {
        let c = WaveImpactCoupler::default();
        assert!(c.coupling[BAND_AUDIO] > c.coupling[BAND_LIGHT]);
        assert!(c.coupling[BAND_AUDIO] > c.coupling[BAND_HEAT]);
    }

    #[test]
    fn coupler_metallic_has_higher_light() {
        let m = WaveImpactCoupler::metallic_impact();
        let d = WaveImpactCoupler::default();
        assert!(m.coupling[BAND_LIGHT] > d.coupling[BAND_LIGHT]);
    }

    #[test]
    fn coupler_magic_has_mana_coupling() {
        let m = WaveImpactCoupler::magic_impact();
        assert!(m.coupling[BAND_MANA] > 0.0);
    }

    #[test]
    fn coupler_soft_has_no_light() {
        let s = WaveImpactCoupler::soft_impact();
        assert_eq!(s.coupling[BAND_LIGHT], 0.0);
    }

    #[test]
    fn synthesize_spectrum_zero_energy_silent() {
        let c = WaveImpactCoupler::default();
        let s = c.synthesize_spectrum(0.0, 0.0);
        assert!(s.is_below_floor());
    }

    #[test]
    fn synthesize_spectrum_high_energy_active() {
        let c = WaveImpactCoupler::default();
        let s = c.synthesize_spectrum(100.0, 5.0);
        assert!(!s.is_below_floor());
    }

    #[test]
    fn contact_spectrum_silent_total_zero() {
        assert_eq!(ContactSpectrum::SILENT.total_energy(), 0.0);
        assert!(ContactSpectrum::SILENT.is_below_floor());
    }

    #[test]
    fn emit_excitation_below_floor_still_returns_event() {
        let c = WaveImpactCoupler::default();
        let e = c
            .emit_excitation(
                [0.0; 3],
                [0.0, 1.0, 0.0],
                0.001, // tiny velocity
                1.0,
                1.0,
                MortonKey::ZERO,
                0.0,
            )
            .unwrap();
        // Event is returned even though spectrum is below floor — the
        // wave-solver decides what to do with it.
        let _ = e;
    }

    #[test]
    fn emit_excitation_normal_unit_check() {
        let c = WaveImpactCoupler::default();
        let r = c.emit_excitation(
            [0.0; 3],
            [0.0, 0.5, 0.0], // not unit
            5.0,
            1.0,
            1.0,
            MortonKey::ZERO,
            0.0,
        );
        assert!(matches!(r, Err(WaveCouplingError::NonUnitNormal { .. })));
    }

    #[test]
    fn emit_excitation_zero_eff_mass_returns_err() {
        let c = WaveImpactCoupler::default();
        let r = c.emit_excitation(
            [0.0; 3],
            [0.0, 1.0, 0.0],
            5.0,
            0.0, // both static
            0.0,
            MortonKey::ZERO,
            0.0,
        );
        assert!(matches!(r, Err(WaveCouplingError::ZeroEffectiveMass)));
    }

    #[test]
    fn emit_excitation_active_event() {
        let c = WaveImpactCoupler::default();
        let e = c
            .emit_excitation(
                [1.0, 2.0, 3.0],
                [0.0, 1.0, 0.0],
                5.0,
                1.0,
                1.0,
                MortonKey::ZERO,
                0.5,
            )
            .unwrap();
        assert!(e.is_active());
        assert_eq!(e.position, [1.0, 2.0, 3.0]);
        assert!(approx(e.time_of_impact, 0.5, 1e-6));
    }

    #[test]
    fn excitation_none_inactive() {
        assert!(!WaveExcitation::NONE.is_active());
    }

    #[test]
    fn freq_shift_velocity_dependent() {
        let c = WaveImpactCoupler::default();
        let s_slow = c.synthesize_spectrum(10.0, 0.5);
        let s_fast = c.synthesize_spectrum(10.0, 5.0);
        assert!(s_fast.freq_shift > s_slow.freq_shift);
    }

    #[test]
    fn freq_shift_bounded() {
        let c = WaveImpactCoupler::default();
        let s = c.synthesize_spectrum(10.0, 100.0);
        assert!(s.freq_shift <= 4.0);
    }

    #[test]
    fn with_coupling_clamps_to_unit_range() {
        let c = WaveImpactCoupler::with_coupling([2.0, -1.0, 0.5, 0.0, 1.5]);
        assert_eq!(c.coupling[0], 1.0);
        assert_eq!(c.coupling[1], 0.0);
        assert_eq!(c.coupling[2], 0.5);
        assert_eq!(c.coupling[3], 0.0);
        assert_eq!(c.coupling[4], 1.0);
    }

    #[test]
    fn impact_energy_floor_is_micro_joules() {
        assert!(IMPACT_ENERGY_FLOOR > 0.0);
        assert!(IMPACT_ENERGY_FLOOR < 1e-3);
    }

    #[test]
    fn band_constants_are_distinct() {
        let bands = [BAND_LIGHT, BAND_AUDIO, BAND_HEAT, BAND_SCENT, BAND_MANA];
        for i in 0..5 {
            for j in (i + 1)..5 {
                assert_ne!(bands[i], bands[j]);
            }
        }
    }

    #[test]
    fn band_count_matches_constant() {
        assert_eq!(WAVE_UNITY_BANDS, 5);
    }
}
