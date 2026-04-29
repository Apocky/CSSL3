//! § CrossBandCoupler — reads D114 wave-solver coupling-matrix.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl § XI` the cross-band-
//!   coupling table specifies how energy flows between LIGHT, AUDIO, HEAT,
//!   SCENT, MANA bands. Selected rows :
//!
//!   ```text
//!   | From → To    | Strength | Aesthetic Effect |
//!   | LIGHT → AUDIO | 0.001   | hearable-light-fields (sun-burble) |
//!   | AUDIO → LIGHT | 0.001   | visible-sound-fields (orchestra-glow) |
//!   | MANA  → AUDIO | 0.05    | magic-hum |
//!   | LIGHT → MANA  | 0.0     | (AGENCY-INVARIANT enforced) |
//!   ```
//!
//!   `CrossBandCoupler` carries this matrix + applies the AUDIO-band
//!   row to the local ψ-AUDIO field. Specifically :
//!     - read `LIGHT → AUDIO` shimmer from upstream (D114 wave-solver
//!       once landed) and inject it into the AUDIO field.
//!     - read `MANA → AUDIO` magic-hum and inject it.
//!     - apply `AUDIO → LIGHT` energy outflow as a MAGNITUDE
//!       attenuation on the AUDIO field (the LIGHT-band recipient
//!       lives in the upstream solver ; from this crate's POV the
//!       outflow is just a small per-cell amplitude reduction).
//!
//!   Until D114's full multi-band container exists this module exposes
//!   a STUB-MATRIX surface : the caller supplies a `CouplingMatrix`
//!   directly + the coupler applies it. When D114 lands its
//!   `CouplingMatrix` becomes the canonical source.
//!
//! § AGENCY-INVARIANT
//!   Per spec § XI :
//!
//!   ```text
//!   ‼ ¬ permits any-coupling-violating-AGENCY
//!     (Λ-token-creation @ unwitnessed-illumination)
//!   LIGHT → MANA = 0 ⊗ enforces "magic emits, light doesn't make magic"
//!   ```
//!
//!   The coupler REFUSES to load a matrix where `LIGHT → MANA != 0` ;
//!   construction returns [`WaveAudioError::AgencyViolation`] for any
//!   such input. This is a structural enforcement — the offending
//!   matrix never reaches the inner-loop application step.

use crate::complex::Complex;
use crate::error::{Result, WaveAudioError};
use crate::psi_field::PsiAudioField;

/// The five canonical bands per Wave-Unity § II.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Band {
    /// Optical / visible-light band.
    Light,
    /// Audible-acoustic band.
    Audio,
    /// Thermal-radiation band.
    Heat,
    /// Olfactory / scent-diffusion band.
    Scent,
    /// Mana / Λ-token-density band.
    Mana,
}

impl Band {
    /// Stable canonical name for telemetry.
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Audio => "audio",
            Self::Heat => "heat",
            Self::Scent => "scent",
            Self::Mana => "mana",
        }
    }

    /// All five bands in canonical order.
    #[must_use]
    pub const fn all() -> [Band; 5] {
        [
            Band::Light,
            Band::Audio,
            Band::Heat,
            Band::Scent,
            Band::Mana,
        ]
    }

    /// Center wavelength of this band in metres. For LIGHT this is the
    /// SVEA-folded envelope wavelength ; for AUDIO this is the carrier
    /// wavelength at 1 kHz.
    #[must_use]
    pub fn center_wavelength_m(self) -> f32 {
        match self {
            Self::Light => 5.5e-7, // 550 nm green
            Self::Audio => 0.343,  // 1 kHz @ 343 m/s
            Self::Heat => 1.0e-5,  // 10 µm thermal IR
            Self::Scent => 1.0,    // diffusive, no real wavelength
            Self::Mana => 0.25,    // Λ-token granularity
        }
    }
}

/// Cross-band coupling strength matrix per spec § XI.
///
/// § FIELDS
///   `entries[i][j]` = strength of coupling FROM band-i TO band-j.
///   Strength is dimensionless ; spec § XI §strength column.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CouplingMatrix {
    /// 5x5 matrix : `[from_band_idx][to_band_idx] = strength`.
    pub entries: [[f32; 5]; 5],
}

impl Default for CouplingMatrix {
    fn default() -> CouplingMatrix {
        CouplingMatrix::canonical_spec()
    }
}

impl CouplingMatrix {
    /// All-zero matrix (no coupling).
    pub const NONE: CouplingMatrix = CouplingMatrix {
        entries: [[0.0; 5]; 5],
    };

    /// The canonical spec § XI matrix. Verbatim values from the table
    /// in `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl § XI`.
    #[must_use]
    pub const fn canonical_spec() -> CouplingMatrix {
        // Row order : LIGHT (0), AUDIO (1), HEAT (2), SCENT (3), MANA (4).
        // Column order matches.
        let mut m = [[0.0_f32; 5]; 5];
        // LIGHT → AUDIO : 0.001
        m[0][1] = 0.001;
        // AUDIO → LIGHT : 0.001
        m[1][0] = 0.001;
        // LIGHT → HEAT : 0.05
        m[0][2] = 0.05;
        // HEAT → LIGHT : 0.001
        m[2][0] = 0.001;
        // AUDIO → HEAT : 0.0001
        m[1][2] = 0.0001;
        // MANA → LIGHT : 0.1
        m[4][0] = 0.1;
        // MANA → AUDIO : 0.05
        m[4][1] = 0.05;
        // LIGHT → MANA : 0.0 (AGENCY-INVARIANT)
        m[0][4] = 0.0;
        // AUDIO → MANA : 0.0 (AGENCY-INVARIANT)
        m[1][4] = 0.0;
        // SCENT → AUDIO : 0.0
        m[3][1] = 0.0;
        // SCENT → LIGHT : 0.0001
        m[3][0] = 0.0001;
        CouplingMatrix { entries: m }
    }

    /// Read the coupling strength from `from` to `to`.
    #[must_use]
    pub fn strength(&self, from: Band, to: Band) -> f32 {
        self.entries[band_index(from)][band_index(to)]
    }

    /// Enforce the AGENCY-INVARIANT clauses per spec § XI.
    ///
    /// § ERRORS
    ///   - [`WaveAudioError::AgencyViolation`] when LIGHT → MANA != 0.
    ///   - [`WaveAudioError::AgencyViolation`] when AUDIO → MANA != 0.
    ///
    /// § §-XI exact :
    ///   "ALL non-zero strengths above-permit only the listed flow
    ///   directions ; the unspecified are 0 by AGENCY default."
    pub fn validate_agency(&self) -> Result<()> {
        if self.entries[band_index(Band::Light)][band_index(Band::Mana)].abs() > 1e-9 {
            return Err(WaveAudioError::AgencyViolation {
                explanation: "LIGHT → MANA must be zero (light cannot make magic)",
            });
        }
        if self.entries[band_index(Band::Audio)][band_index(Band::Mana)].abs() > 1e-9 {
            return Err(WaveAudioError::AgencyViolation {
                explanation: "AUDIO → MANA must be zero (sound cannot make magic)",
            });
        }
        Ok(())
    }

    /// Test convenience : a matrix where ALL entries are non-zero
    /// (illegal — used to verify validation).
    #[must_use]
    pub const fn all_ones_for_test() -> CouplingMatrix {
        CouplingMatrix {
            entries: [[1.0; 5]; 5],
        }
    }
}

/// Band → array-index helper. `[Light=0, Audio=1, Heat=2, Scent=3, Mana=4]`.
#[inline]
const fn band_index(b: Band) -> usize {
    match b {
        Band::Light => 0,
        Band::Audio => 1,
        Band::Heat => 2,
        Band::Scent => 3,
        Band::Mana => 4,
    }
}

/// CrossBandCoupler — applies cross-band-coupling to the local AUDIO band.
#[derive(Debug, Clone)]
pub struct CrossBandCoupler {
    matrix: CouplingMatrix,
    /// Coupling threshold below which we skip the application pass
    /// (spec § X has `COUPLING_EPSILON` as a coupling-strength
    /// threshold ; we expose it here).
    epsilon: f32,
}

impl Default for CrossBandCoupler {
    fn default() -> CrossBandCoupler {
        CrossBandCoupler::canonical().expect("canonical spec matrix is valid")
    }
}

impl CrossBandCoupler {
    /// Construct with the spec-canonical matrix.
    pub fn canonical() -> Result<CrossBandCoupler> {
        CrossBandCoupler::from_matrix(CouplingMatrix::canonical_spec())
    }

    /// Construct with a custom matrix. Returns
    /// [`WaveAudioError::AgencyViolation`] if the matrix violates
    /// AGENCY-INVARIANT.
    pub fn from_matrix(matrix: CouplingMatrix) -> Result<CrossBandCoupler> {
        matrix.validate_agency()?;
        Ok(CrossBandCoupler {
            matrix,
            epsilon: 1e-6,
        })
    }

    /// Read the active coupling matrix.
    #[must_use]
    pub fn matrix(&self) -> CouplingMatrix {
        self.matrix
    }

    /// Configure the coupling-skip threshold.
    pub fn set_epsilon(&mut self, eps: f32) {
        self.epsilon = eps.max(0.0);
    }

    /// Apply LIGHT → AUDIO coupling : reads a representative LIGHT
    /// amplitude (sampled by the upstream solver) + injects a
    /// proportional AUDIO-band shimmer at the same cells. Until D114
    /// lands the upstream LIGHT field, we accept a "donor" iterator
    /// of `(MortonKey, light_amp)` pairs from the caller.
    pub fn apply_light_to_audio<I>(
        &self,
        audio: &mut PsiAudioField,
        light_donor: I,
        dt: f32,
    ) -> Result<()>
    where
        I: IntoIterator<Item = (cssl_substrate_omega_field::morton::MortonKey, Complex)>,
    {
        let s = self.matrix.strength(Band::Light, Band::Audio);
        if s < self.epsilon {
            return Ok(());
        }
        for (k, light_amp) in light_donor {
            // δψ_AUDIO = strength · ψ_LIGHT · dt
            let delta = light_amp.scale(s * dt);
            if delta.norm_sq() < self.epsilon {
                continue;
            }
            audio.add_at(k, delta)?;
        }
        Ok(())
    }

    /// Apply MANA → AUDIO coupling : reads MANA-band amplitudes (via
    /// the donor iterator) + injects magic-hum into AUDIO. Strength
    /// 0.05 per spec § XI.
    pub fn apply_mana_to_audio<I>(
        &self,
        audio: &mut PsiAudioField,
        mana_donor: I,
        dt: f32,
    ) -> Result<()>
    where
        I: IntoIterator<Item = (cssl_substrate_omega_field::morton::MortonKey, Complex)>,
    {
        let s = self.matrix.strength(Band::Mana, Band::Audio);
        if s < self.epsilon {
            return Ok(());
        }
        for (k, mana_amp) in mana_donor {
            let delta = mana_amp.scale(s * dt);
            if delta.norm_sq() < self.epsilon {
                continue;
            }
            audio.add_at(k, delta)?;
        }
        Ok(())
    }

    /// Apply AUDIO → LIGHT coupling outflow : attenuates the AUDIO
    /// field by a small fraction at cells where the AUDIO amplitude is
    /// above a threshold (the energy flows to LIGHT in the upstream
    /// solver ; from cssl-wave-audio's POV it's a magnitude reduction).
    pub fn apply_audio_outflow_to_light(
        &self,
        audio: &mut PsiAudioField,
        dt: f32,
        threshold: f32,
    ) -> Result<f32> {
        let s = self.matrix.strength(Band::Audio, Band::Light);
        if s < self.epsilon {
            return Ok(0.0);
        }
        let mut total_outflow = 0.0_f32;
        let cells: Vec<_> = audio.iter().map(|(k, c)| (k, c.amplitude)).collect();
        for (k, amp) in cells {
            if amp.norm_sq() < threshold * threshold {
                continue;
            }
            // Outflow rate : δ = - strength · ψ · dt. The energy goes
            // to LIGHT (out of scope here) ; the AUDIO amplitude
            // shrinks by that amount.
            let delta = amp.scale(-s * dt);
            audio.add_at(k, delta)?;
            total_outflow += delta.norm();
        }
        Ok(total_outflow)
    }

    /// Apply AUDIO → HEAT coupling : energy absorbed into HEAT (small
    /// 0.0001 per spec § XI). Returns the total energy outflow.
    pub fn apply_audio_outflow_to_heat(&self, audio: &mut PsiAudioField, dt: f32) -> Result<f32> {
        let s = self.matrix.strength(Band::Audio, Band::Heat);
        if s < self.epsilon {
            return Ok(0.0);
        }
        let mut total_outflow = 0.0_f32;
        let cells: Vec<_> = audio.iter().map(|(k, c)| (k, c.amplitude)).collect();
        for (k, amp) in cells {
            let delta = amp.scale(-s * dt);
            audio.add_at(k, delta)?;
            total_outflow += delta.norm();
        }
        Ok(total_outflow)
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::{Band, CouplingMatrix, CrossBandCoupler};
    use crate::complex::Complex;
    use crate::psi_field::PsiAudioField;
    use cssl_substrate_omega_field::morton::MortonKey;

    #[test]
    fn band_canonical_names() {
        assert_eq!(Band::Light.canonical_name(), "light");
        assert_eq!(Band::Audio.canonical_name(), "audio");
        assert_eq!(Band::Heat.canonical_name(), "heat");
        assert_eq!(Band::Scent.canonical_name(), "scent");
        assert_eq!(Band::Mana.canonical_name(), "mana");
    }

    #[test]
    fn band_all_count_five() {
        assert_eq!(Band::all().len(), 5);
    }

    #[test]
    fn band_audio_wavelength_about_0_343m() {
        let l = Band::Audio.center_wavelength_m();
        assert!((l - 0.343).abs() < 1e-3);
    }

    #[test]
    fn band_light_wavelength_about_550nm() {
        let l = Band::Light.center_wavelength_m();
        assert!((l - 5.5e-7).abs() < 1e-9);
    }

    #[test]
    fn coupling_matrix_none_is_zero() {
        let m = CouplingMatrix::NONE;
        for from in Band::all() {
            for to in Band::all() {
                assert_eq!(m.strength(from, to), 0.0);
            }
        }
    }

    #[test]
    fn coupling_canonical_light_to_audio_is_0_001() {
        let m = CouplingMatrix::canonical_spec();
        assert!((m.strength(Band::Light, Band::Audio) - 0.001).abs() < 1e-9);
    }

    #[test]
    fn coupling_canonical_mana_to_audio_is_0_05() {
        let m = CouplingMatrix::canonical_spec();
        assert!((m.strength(Band::Mana, Band::Audio) - 0.05).abs() < 1e-9);
    }

    #[test]
    fn coupling_canonical_light_to_mana_is_zero() {
        let m = CouplingMatrix::canonical_spec();
        assert_eq!(m.strength(Band::Light, Band::Mana), 0.0);
    }

    #[test]
    fn coupling_canonical_audio_to_mana_is_zero() {
        let m = CouplingMatrix::canonical_spec();
        assert_eq!(m.strength(Band::Audio, Band::Mana), 0.0);
    }

    #[test]
    fn coupling_canonical_validates_agency() {
        let m = CouplingMatrix::canonical_spec();
        assert!(m.validate_agency().is_ok());
    }

    #[test]
    fn coupling_all_ones_violates_agency() {
        let m = CouplingMatrix::all_ones_for_test();
        let r = m.validate_agency();
        assert!(r.is_err());
    }

    #[test]
    fn coupler_from_canonical_matrix_succeeds() {
        let c = CrossBandCoupler::canonical();
        assert!(c.is_ok());
    }

    #[test]
    fn coupler_from_invalid_matrix_refuses() {
        let m = CouplingMatrix::all_ones_for_test();
        let r = CrossBandCoupler::from_matrix(m);
        assert!(r.is_err());
    }

    #[test]
    fn coupler_default_uses_canonical() {
        let c = CrossBandCoupler::default();
        assert!((c.matrix().strength(Band::Light, Band::Audio) - 0.001).abs() < 1e-9);
    }

    #[test]
    fn apply_light_to_audio_below_threshold_noop() {
        let mut audio = PsiAudioField::new();
        let coupler = CrossBandCoupler::canonical().unwrap();
        let donor: Vec<(MortonKey, Complex)> = vec![];
        coupler
            .apply_light_to_audio(&mut audio, donor, 1.0)
            .unwrap();
        assert!(audio.is_silent());
    }

    #[test]
    fn apply_light_to_audio_injects_proportional_shimmer() {
        let mut audio = PsiAudioField::new();
        let coupler = CrossBandCoupler::canonical().unwrap();
        let k = MortonKey::encode(1, 0, 0).unwrap();
        // Big LIGHT amplitude so 0.001 strength * 1.0 dt produces a
        // detectable AUDIO injection.
        let donor = vec![(k, Complex::new(100.0, 0.0))];
        coupler
            .apply_light_to_audio(&mut audio, donor, 1.0)
            .unwrap();
        let injected = audio.at(k);
        // δψ_AUDIO = 0.001 * 100 * 1 = 0.1
        assert!((injected.re - 0.1).abs() < 1e-5);
    }

    #[test]
    fn apply_mana_to_audio_injects_magic_hum() {
        let mut audio = PsiAudioField::new();
        let coupler = CrossBandCoupler::canonical().unwrap();
        let k = MortonKey::encode(2, 0, 0).unwrap();
        let donor = vec![(k, Complex::new(10.0, 0.0))];
        coupler.apply_mana_to_audio(&mut audio, donor, 1.0).unwrap();
        let injected = audio.at(k);
        // δψ_AUDIO = 0.05 * 10 * 1 = 0.5
        assert!((injected.re - 0.5).abs() < 1e-4);
    }

    #[test]
    fn apply_audio_outflow_to_light_attenuates() {
        let mut audio = PsiAudioField::new();
        let coupler = CrossBandCoupler::canonical().unwrap();
        let k = MortonKey::encode(3, 0, 0).unwrap();
        audio.set(k, Complex::new(10.0, 0.0)).unwrap();
        let _outflow = coupler
            .apply_audio_outflow_to_light(&mut audio, 1.0, 0.0)
            .unwrap();
        let after = audio.at(k);
        // δψ_AUDIO = -0.001 * 10 * 1 = -0.01 ; new value ≈ 9.99.
        assert!((after.re - 9.99).abs() < 1e-3);
    }

    #[test]
    fn apply_audio_outflow_below_threshold_skips() {
        let mut audio = PsiAudioField::new();
        let coupler = CrossBandCoupler::canonical().unwrap();
        let k = MortonKey::encode(4, 0, 0).unwrap();
        audio.set(k, Complex::new(0.001, 0.0)).unwrap();
        // High threshold ; should skip.
        let _ = coupler
            .apply_audio_outflow_to_light(&mut audio, 1.0, 1.0)
            .unwrap();
        let after = audio.at(k);
        assert!((after.re - 0.001).abs() < 1e-6);
    }

    #[test]
    fn apply_audio_outflow_to_heat_attenuates() {
        let mut audio = PsiAudioField::new();
        let coupler = CrossBandCoupler::canonical().unwrap();
        let k = MortonKey::encode(5, 0, 0).unwrap();
        audio.set(k, Complex::new(10.0, 0.0)).unwrap();
        coupler
            .apply_audio_outflow_to_heat(&mut audio, 1.0)
            .unwrap();
        let after = audio.at(k);
        // 0.0001 * 10 * 1 = 0.001 outflow ; new value ≈ 9.999.
        assert!((after.re - 9.999).abs() < 1e-3);
    }
}
