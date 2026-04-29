//! § ProceduralVocal — creature vocalization from SDF + KAN spectral coeffs.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl § V.6` :
//!
//!   ```text
//!   instrument @ M-facet ⊗ shape + material ⊗ KAN-impedance ⊗ resonant-modes
//!   bow-string @ S-facet ⊗ excites ψ @ string-mode
//!   body @ M-facet ⊗ amplifies ψ @ body-modes
//!   listener @ AudioListener ⊗ samples ψ @ ear-position
//!   ‼ N! sample-library ⊗ ✓ FULL ψ-PDE on-instrument-domain
//!   consequence : violin SOUNDS LIKE THIS specific-violin's-physics
//!   ```
//!
//!   `ProceduralVocal` extends this principle to creatures : the
//!   vocalization is NOT a sample-library lookup ; it is the ψ-PDE
//!   driven through a KAN-derived glottal-pulse source on a SDF-vocal-
//!   tract domain, with the wall-impedance KAN providing per-segment
//!   `Z(λ)` for the Robin-BC. The result : creature vocalizations that
//!   sound like the creature's specific-vocal-tract.
//!
//! § PIPELINE
//!   1. **Glottal pulse** : KAN-derived spectral coefficients per harmonic
//!      based on `(fundamental_freq, tract_size, throat_narrowness)`.
//!   2. **Source injection** : at the glottis cell of the tract SDF,
//!      inject ψ-amplitude at each harmonic frequency.
//!   3. **LBM propagation** : the ψ-AUDIO field carries the source
//!      through the tract via the wave-LBM stream-collide.
//!   4. **Boundary application** : Robin-BC at tract walls (impedance
//!      KAN) ; rigid-BC at tract closure (e.g. velum) ; soft-BC at lip
//!      aperture.
//!   5. **Lip output** : the projector samples ψ-AUDIO at the lip
//!      position to recover the radiated waveform.
//!
//! § DETERMINISM
//!   All steps are pure functions over the inputs. Two replays with
//!   identical creature-spec inputs produce bit-equal output frames.

use crate::complex::Complex;
use crate::error::{Result, WaveAudioError};
use crate::kan::{canonical_formant_table, VocalKanInputs, VocalSpectralKan, VOCAL_HARMONIC_COUNT};
use crate::lbm::{LbmConfig, LbmSpatialAudio};
use crate::sdf::{VocalTractSdf, WallClass};
use cssl_substrate_omega_field::morton::MortonKey;

/// Specification of a creature's vocalization.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CreatureVocalSpec {
    /// Fundamental frequency `f0` in Hz.
    pub fundamental_freq_hz: f32,
    /// Tract size scale (1.0 = adult human).
    pub tract_size: f32,
    /// Throat narrowness ∈ [0, 1].
    pub throat_narrowness: f32,
    /// Phonation duration (seconds). The source remains active for this
    /// duration ; longer phonations are produced via repeated calls.
    pub duration_seconds: f32,
}

impl Default for CreatureVocalSpec {
    fn default() -> CreatureVocalSpec {
        CreatureVocalSpec {
            fundamental_freq_hz: 150.0,
            tract_size: 0.8,
            throat_narrowness: 0.2,
            duration_seconds: 0.5,
        }
    }
}

/// One frame of vocal-source state : the per-harmonic amplitudes at
/// the current synthesis tick + the time within the phonation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VocalSourceFrame {
    /// Per-harmonic complex amplitudes (real = sin component, imag =
    /// cos component) computed at the current tick.
    pub harmonics: [Complex; VOCAL_HARMONIC_COUNT],
    /// Current synthesis time in seconds.
    pub t_seconds: f32,
}

impl Default for VocalSourceFrame {
    fn default() -> VocalSourceFrame {
        VocalSourceFrame {
            harmonics: [Complex::ZERO; VOCAL_HARMONIC_COUNT],
            t_seconds: 0.0,
        }
    }
}

/// ProceduralVocal — synthesizes creature vocalization through the LBM
/// solver.
#[derive(Debug, Clone)]
pub struct ProceduralVocal {
    spec: CreatureVocalSpec,
    sdf: VocalTractSdf,
    spectral_kan: VocalSpectralKan,
    /// Current synthesis time within the phonation.
    t_seconds: f32,
    /// Cached per-harmonic amplitude vector from the KAN (re-evaluated
    /// only when spec changes).
    harmonic_amps: [f32; VOCAL_HARMONIC_COUNT],
    /// Cached formant-table (for diagnostics).
    formants: [f32; 3],
    /// Cached harmonic frequency stack (`f0 * k`) for fast injection.
    harmonic_freqs: [f32; VOCAL_HARMONIC_COUNT],
    /// Glottis-cell coordinates in lattice units.
    glottis_cell: (u32, u32, u32),
}

impl ProceduralVocal {
    /// Construct a procedural-vocal synthesizer for the given creature
    /// spec, vocal tract SDF, and lattice glottis position.
    ///
    /// § ERRORS
    ///   - [`WaveAudioError::VocalTract`] propagated from SDF
    ///     construction failures.
    ///   - [`WaveAudioError::OutOfBand`] when `fundamental_freq_hz` is
    ///     outside the AUDIO band [20, 20000] Hz.
    pub fn new(
        spec: CreatureVocalSpec,
        sdf: VocalTractSdf,
        glottis_cell: (u32, u32, u32),
    ) -> Result<ProceduralVocal> {
        if spec.fundamental_freq_hz < 20.0 || spec.fundamental_freq_hz > 20_000.0 {
            return Err(WaveAudioError::OutOfBand {
                freq_hz: spec.fundamental_freq_hz,
                band_lo: 20.0,
                band_hi: 20_000.0,
            });
        }
        let mut spectral_kan = VocalSpectralKan::untrained();
        let kan_in = VocalKanInputs {
            fundamental_freq_hz: spec.fundamental_freq_hz,
            tract_size: spec.tract_size,
            throat_narrowness: spec.throat_narrowness,
        };
        let harmonic_amps = spectral_kan.evaluate(kan_in)?;
        let (formants, _) = canonical_formant_table(spec.tract_size);
        let mut harmonic_freqs = [0.0_f32; VOCAL_HARMONIC_COUNT];
        for k in 1..=VOCAL_HARMONIC_COUNT {
            harmonic_freqs[k - 1] = spec.fundamental_freq_hz * k as f32;
        }
        Ok(ProceduralVocal {
            spec,
            sdf,
            spectral_kan,
            t_seconds: 0.0,
            harmonic_amps,
            formants,
            harmonic_freqs,
            glottis_cell,
        })
    }

    /// Construct a default human-vocal synthesizer.
    pub fn default_human() -> Result<ProceduralVocal> {
        let sdf = VocalTractSdf::human_default();
        let spec = CreatureVocalSpec::default();
        ProceduralVocal::new(spec, sdf, (5, 5, 5))
    }

    /// Construct a creature variant with given size/narrowness.
    pub fn creature(
        spec: CreatureVocalSpec,
        glottis_cell: (u32, u32, u32),
    ) -> Result<ProceduralVocal> {
        let sdf = VocalTractSdf::creature(spec.tract_size, spec.throat_narrowness)?;
        ProceduralVocal::new(spec, sdf, glottis_cell)
    }

    /// Read the active spec.
    #[must_use]
    pub fn spec(&self) -> CreatureVocalSpec {
        self.spec
    }

    /// Update the spec ; re-evaluates the KAN spectral coefficients.
    pub fn set_spec(&mut self, spec: CreatureVocalSpec) -> Result<()> {
        self.spec = spec;
        let kan_in = VocalKanInputs {
            fundamental_freq_hz: spec.fundamental_freq_hz,
            tract_size: spec.tract_size,
            throat_narrowness: spec.throat_narrowness,
        };
        self.harmonic_amps = self.spectral_kan.evaluate(kan_in)?;
        let (formants, _) = canonical_formant_table(spec.tract_size);
        self.formants = formants;
        for k in 1..=VOCAL_HARMONIC_COUNT {
            self.harmonic_freqs[k - 1] = spec.fundamental_freq_hz * k as f32;
        }
        Ok(())
    }

    /// Read the SDF.
    #[must_use]
    pub fn sdf(&self) -> &VocalTractSdf {
        &self.sdf
    }

    /// Read the formant peaks (Hz).
    #[must_use]
    pub fn formants(&self) -> [f32; 3] {
        self.formants
    }

    /// Read the harmonic-amplitude vector.
    #[must_use]
    pub fn harmonic_amps(&self) -> [f32; VOCAL_HARMONIC_COUNT] {
        self.harmonic_amps
    }

    /// Read the harmonic-frequency vector (Hz).
    #[must_use]
    pub fn harmonic_freqs(&self) -> [f32; VOCAL_HARMONIC_COUNT] {
        self.harmonic_freqs
    }

    /// Read the current synthesis time.
    #[must_use]
    pub fn t_seconds(&self) -> f32 {
        self.t_seconds
    }

    /// Reset the synthesis time to zero.
    pub fn reset(&mut self) {
        self.t_seconds = 0.0;
    }

    /// True iff phonation is still active (within the spec's duration).
    #[must_use]
    pub fn is_phonating(&self) -> bool {
        self.t_seconds < self.spec.duration_seconds
    }

    /// Glottis-cell coordinates.
    #[must_use]
    pub fn glottis_cell(&self) -> (u32, u32, u32) {
        self.glottis_cell
    }

    /// Compute the glottal-pulse source amplitude at the current time
    /// `t_seconds` as the sum of `VOCAL_HARMONIC_COUNT` sinusoids with
    /// the KAN-derived amplitudes.
    #[must_use]
    pub fn glottal_pulse_at_t(&self, t: f32) -> Complex {
        let mut acc = Complex::ZERO;
        for k in 0..VOCAL_HARMONIC_COUNT {
            let freq = self.harmonic_freqs[k];
            let amp = self.harmonic_amps[k];
            if amp.abs() < 1e-6 {
                continue;
            }
            let phase = 2.0 * core::f32::consts::PI * freq * t;
            // Each harmonic is a complex exponential ; real part is
            // the audible pressure.
            let h = Complex::from_polar(amp, phase);
            acc = acc.add(h);
        }
        acc
    }

    /// Generate one frame of the source waveform at the current time.
    #[must_use]
    pub fn current_frame(&self) -> VocalSourceFrame {
        let mut harmonics = [Complex::ZERO; VOCAL_HARMONIC_COUNT];
        let t = self.t_seconds;
        for k in 0..VOCAL_HARMONIC_COUNT {
            let freq = self.harmonic_freqs[k];
            let amp = self.harmonic_amps[k];
            let phase = 2.0 * core::f32::consts::PI * freq * t;
            harmonics[k] = Complex::from_polar(amp, phase);
        }
        VocalSourceFrame {
            harmonics,
            t_seconds: t,
        }
    }

    /// Drive `n_substeps` of the LBM solver, injecting the current-time
    /// glottal-pulse at the glottis cell each substep + advancing
    /// `t_seconds`.
    pub fn drive_lbm(&mut self, lbm: &mut LbmSpatialAudio, n_substeps: u32) -> Result<u32> {
        if !self.is_phonating() {
            return Ok(0);
        }
        let dt_sub = lbm.config().dt();
        if dt_sub <= 0.0 {
            return Err(WaveAudioError::Storage(
                "LBM substep timestep is non-positive".into(),
            ));
        }
        let glottis = MortonKey::encode(
            self.glottis_cell.0 as u64,
            self.glottis_cell.1 as u64,
            self.glottis_cell.2 as u64,
        )
        .map_err(|e| WaveAudioError::Storage(format!("{e}")))?;
        lbm.begin_frame();
        let mut steps_taken = 0_u32;
        for _ in 0..n_substeps {
            if !self.is_phonating() {
                break;
            }
            let amp = self.glottal_pulse_at_t(self.t_seconds);
            lbm.inject_source(glottis, amp)?;
            lbm.substep()?;
            self.t_seconds += dt_sub;
            steps_taken += 1;
        }
        Ok(steps_taken)
    }

    /// Mark the SDF-derived vocal-tract walls as boundaries on the LBM
    /// solver. Convenience for setup.
    pub fn install_boundaries(&self, lbm: &mut LbmSpatialAudio) {
        lbm.mark_vocal_tract_boundary(&self.sdf, self.glottis_cell);
    }

    /// Returns `(lip_x, lip_y, lip_z)` lattice coordinates : the cell
    /// at the lip aperture, downstream of glottis along +X.
    #[must_use]
    pub fn lip_cell(&self, voxel_size: f32) -> (u32, u32, u32) {
        let voxel = voxel_size.max(1e-3);
        let dx_cells = (self.sdf.total_length() / voxel).ceil() as u32;
        (
            self.glottis_cell.0 + dx_cells,
            self.glottis_cell.1,
            self.glottis_cell.2,
        )
    }
}

/// Generate a complete demo of a creature vocalization at the standard
/// 48 kHz audio-rate from a fresh LBM solver. Returns the LBM solver
/// after the phonation completes (so callers can sample the lip-cell
/// for the resulting waveform).
///
/// § OUTPUT
///   The returned solver has the ψ-AUDIO field populated such that
///   `solver.current().sample_world(lip_world_pos, voxel)` recovers
///   the radiated pressure.
pub fn vocalization_demo(spec: CreatureVocalSpec) -> Result<(LbmSpatialAudio, ProceduralVocal)> {
    let mut vocal = ProceduralVocal::creature(spec, (5, 5, 5))?;
    let mut lbm = LbmSpatialAudio::new(LbmConfig {
        max_substeps: 32,
        ..LbmConfig::default()
    });
    vocal.install_boundaries(&mut lbm);
    // Mark the velum (just upstream of glottis) as a rigid wall so
    // pulses radiate outward through the tract.
    let velum = MortonKey::encode(
        vocal.glottis_cell.0 as u64 - 1,
        vocal.glottis_cell.1 as u64,
        vocal.glottis_cell.2 as u64,
    )
    .map_err(|e| WaveAudioError::Storage(format!("{e}")))?;
    lbm.mark_boundary(velum, WallClass::Rigid);

    // Run a short batch to populate the field. We DON'T iterate the
    // entire phonation duration here ; the demo just fills the field
    // with a few substeps so callers can sample it.
    let _ = vocal.drive_lbm(&mut lbm, 8)?;
    Ok((lbm, vocal))
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::{
        vocalization_demo, CreatureVocalSpec, ProceduralVocal, VocalSourceFrame,
        VOCAL_HARMONIC_COUNT,
    };
    use crate::lbm::LbmSpatialAudio;
    use cssl_substrate_omega_field::morton::MortonKey;

    #[test]
    fn spec_default_in_audio_range() {
        let s = CreatureVocalSpec::default();
        assert!(s.fundamental_freq_hz > 20.0);
        assert!(s.fundamental_freq_hz < 20_000.0);
    }

    #[test]
    fn vocal_default_human_constructs() {
        let v = ProceduralVocal::default_human();
        assert!(v.is_ok());
    }

    #[test]
    fn vocal_out_of_band_freq_rejected() {
        let bad = CreatureVocalSpec {
            fundamental_freq_hz: 30_000.0,
            ..CreatureVocalSpec::default()
        };
        let r = ProceduralVocal::creature(bad, (5, 5, 5));
        assert!(r.is_err());
    }

    #[test]
    fn vocal_harmonic_amps_have_unit_l2_norm() {
        let v = ProceduralVocal::default_human().unwrap();
        let amps = v.harmonic_amps();
        let l2: f32 = amps.iter().map(|a| a * a).sum::<f32>().sqrt();
        assert!((l2 - 1.0).abs() < 1e-3);
    }

    #[test]
    fn vocal_harmonic_freqs_are_integer_multiples() {
        let v = ProceduralVocal::default_human().unwrap();
        let freqs = v.harmonic_freqs();
        let f0 = v.spec().fundamental_freq_hz;
        for k in 0..VOCAL_HARMONIC_COUNT {
            assert!((freqs[k] - f0 * (k + 1) as f32).abs() < 1e-3);
        }
    }

    #[test]
    fn vocal_formants_in_human_range() {
        let v = ProceduralVocal::default_human().unwrap();
        let f = v.formants();
        // Default human spec : tract_size = 1.0 → f1 ≈ 504 Hz.
        assert!(f[0] > 200.0 && f[0] < 1000.0);
        assert!(f[1] > f[0]);
        assert!(f[2] > f[1]);
    }

    #[test]
    fn vocal_glottal_pulse_at_t_zero_is_real_sum() {
        let v = ProceduralVocal::default_human().unwrap();
        let amp = v.glottal_pulse_at_t(0.0);
        // At t=0 phase is 0 for every harmonic ; complex polar with
        // theta=0 has imag ≈ 0.
        assert!(amp.im.abs() < 1e-3);
        // Real part should be the L1 norm of the harmonic amplitudes.
        let l1: f32 = v.harmonic_amps().iter().sum();
        assert!((amp.re - l1).abs() < 1e-3);
    }

    #[test]
    fn vocal_current_frame_at_initial_time_zero_phase() {
        let v = ProceduralVocal::default_human().unwrap();
        let f: VocalSourceFrame = v.current_frame();
        assert_eq!(f.t_seconds, 0.0);
        // First harmonic at t=0 should have positive real, near-zero
        // imag.
        assert!(f.harmonics[0].im.abs() < 1e-3);
    }

    #[test]
    fn vocal_set_spec_updates_harmonics() {
        let mut v = ProceduralVocal::default_human().unwrap();
        let original = v.harmonic_amps();
        let new_spec = CreatureVocalSpec {
            fundamental_freq_hz: 250.0,
            ..CreatureVocalSpec::default()
        };
        v.set_spec(new_spec).unwrap();
        let updated = v.harmonic_amps();
        // Different fundamental → different harmonic-frequency stack →
        // different formant emphasis → different per-harmonic amps.
        let any_diff = (0..VOCAL_HARMONIC_COUNT).any(|i| (original[i] - updated[i]).abs() > 1e-3);
        assert!(any_diff);
    }

    #[test]
    fn vocal_is_phonating_resets_correctly() {
        let mut v = ProceduralVocal::default_human().unwrap();
        assert!(v.is_phonating());
        // Drive a few substeps to advance time.
        let mut lbm = LbmSpatialAudio::default();
        v.install_boundaries(&mut lbm);
        let _ = v.drive_lbm(&mut lbm, 4).unwrap();
        v.reset();
        assert_eq!(v.t_seconds(), 0.0);
        assert!(v.is_phonating());
    }

    #[test]
    fn vocal_drive_lbm_advances_time() {
        let mut v = ProceduralVocal::default_human().unwrap();
        let mut lbm = LbmSpatialAudio::default();
        v.install_boundaries(&mut lbm);
        let t_before = v.t_seconds();
        let n = v.drive_lbm(&mut lbm, 4).unwrap();
        assert!(n > 0);
        assert!(v.t_seconds() > t_before);
    }

    #[test]
    fn vocal_drive_lbm_injects_into_glottis_cell() {
        let mut v = ProceduralVocal::default_human().unwrap();
        let mut lbm = LbmSpatialAudio::default();
        v.install_boundaries(&mut lbm);
        let _ = v.drive_lbm(&mut lbm, 1).unwrap();
        // The glottis cell should have non-zero amplitude after one
        // step.
        let g = MortonKey::encode(
            v.glottis_cell().0 as u64,
            v.glottis_cell().1 as u64,
            v.glottis_cell().2 as u64,
        )
        .unwrap();
        // After substep the source-injection has been streamed away ;
        // some amplitude remains at the glottis from the rest-direction
        // weight.
        let neighbor = MortonKey::encode(
            v.glottis_cell().0 as u64 + 1,
            v.glottis_cell().1 as u64,
            v.glottis_cell().2 as u64,
        )
        .unwrap();
        let g_amp = lbm.current().at(g);
        let n_amp = lbm.current().at(neighbor);
        assert!(g_amp.norm() + n_amp.norm() > 0.0);
    }

    #[test]
    fn vocal_drive_lbm_stops_after_duration() {
        let spec = CreatureVocalSpec {
            duration_seconds: 1e-6, // immediately complete
            ..CreatureVocalSpec::default()
        };
        let mut v = ProceduralVocal::creature(spec, (5, 5, 5)).unwrap();
        let mut lbm = LbmSpatialAudio::default();
        v.install_boundaries(&mut lbm);
        // Take one substep to advance time past duration.
        let _ = v.drive_lbm(&mut lbm, 1).unwrap();
        // Now next call should yield 0 substeps.
        let n = v.drive_lbm(&mut lbm, 4).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn vocalization_demo_returns_active_solver() {
        let (lbm, vocal) = vocalization_demo(CreatureVocalSpec::default()).unwrap();
        assert!(lbm.has_active_cells());
        assert!(vocal.t_seconds() > 0.0);
    }

    #[test]
    fn vocal_install_boundaries_records_walls() {
        let v = ProceduralVocal::default_human().unwrap();
        let mut lbm = LbmSpatialAudio::default();
        v.install_boundaries(&mut lbm);
        assert!(lbm.boundary_count() > 0);
    }

    #[test]
    fn vocal_lip_cell_downstream_of_glottis() {
        let v = ProceduralVocal::default_human().unwrap();
        let g = v.glottis_cell();
        let lip = v.lip_cell(0.5);
        // Lip is downstream along +X.
        assert!(lip.0 > g.0);
        assert_eq!(lip.1, g.1);
        assert_eq!(lip.2, g.2);
    }

    #[test]
    fn vocal_determinism_two_replays_same_amps() {
        let v1 = ProceduralVocal::default_human().unwrap();
        let v2 = ProceduralVocal::default_human().unwrap();
        let a1 = v1.harmonic_amps();
        let a2 = v2.harmonic_amps();
        for i in 0..VOCAL_HARMONIC_COUNT {
            assert_eq!(a1[i].to_bits(), a2[i].to_bits());
        }
    }

    #[test]
    fn vocal_creature_smaller_size_has_higher_formants() {
        let big = CreatureVocalSpec {
            tract_size: 1.5,
            ..CreatureVocalSpec::default()
        };
        let small = CreatureVocalSpec {
            tract_size: 0.5,
            ..CreatureVocalSpec::default()
        };
        let v_big = ProceduralVocal::creature(big, (5, 5, 5)).unwrap();
        let v_small = ProceduralVocal::creature(small, (5, 5, 5)).unwrap();
        assert!(v_small.formants()[0] > v_big.formants()[0]);
    }
}
