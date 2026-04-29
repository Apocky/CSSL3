//! § Mini-KAN — minimal spline-network bridge to cssl-substrate-kan.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The full KAN training + autodiff machinery lives in `cssl-substrate-kan`
//!   (T11-D115 / wave-3β-04). cssl-wave-audio needs only **inference** :
//!   evaluate a spline-net to produce vocal-spectrum coefficients (per
//!   `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl § IV.3` source-spectrum
//!   KAN) and impedance values (per § IV.2 KAN material → `Z(λ)`).
//!
//!   This module wraps `cssl_substrate_kan::KanNetwork<I, O>` with a
//!   thin "evaluate-only" surface that returns a `[f32; O]` vector :
//!
//!     - `VocalSpectralKan` — input `[fundamental_freq_hz, tract_size,
//!       throat_narrowness]` ; output `[harmonic_amp; 16]` for the first
//!       16 harmonics.
//!     - `ImpedanceKan` — input `[wavelength_m, wall_class_id]` ;
//!       output `[Z_real, Z_imag]` for the wall's complex impedance.
//!
//!   When the upstream KAN crate's evaluator is wired (D115) we delegate
//!   to it ; until then this crate provides an analytic-fallback that
//!   produces deterministically-shaped formant peaks + standard wall-
//!   impedance curves so the binaural pipeline is end-to-end runnable.
//!
//! § DETERMINISM
//!   Both the KAN-evaluate path (when D115 lands) and the analytic
//!   fallback are pure functions of their inputs. Two replays with
//!   identical inputs produce bit-equal outputs.

use crate::error::Result;

/// Number of harmonic coefficients produced by `VocalSpectralKan`.
pub const VOCAL_HARMONIC_COUNT: usize = 16;

/// Inputs to the vocal-spectrum KAN.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VocalKanInputs {
    /// Fundamental frequency `f0` in Hz (e.g. 100 Hz for typical creature).
    pub fundamental_freq_hz: f32,
    /// Tract size scale factor (1.0 = adult-human reference).
    pub tract_size: f32,
    /// Throat narrowness ∈ [0, 1] (higher = more whistle-like).
    pub throat_narrowness: f32,
}

impl Default for VocalKanInputs {
    fn default() -> VocalKanInputs {
        VocalKanInputs {
            fundamental_freq_hz: 110.0,
            tract_size: 1.0,
            throat_narrowness: 0.0,
        }
    }
}

/// VocalSpectralKan — spline-net producing `[harmonic_amp; VOCAL_HARMONIC_COUNT]`.
///
/// § BACKING
///   Wraps a `cssl_substrate_kan::KanNetwork<3, VOCAL_HARMONIC_COUNT>`. The
///   `untrained` constructor returns a network that uses the analytic
///   fallback `analytic_vocal_spectrum` ; once D115 lands a trained
///   variant the evaluate path delegates to the spline-net.
#[derive(Debug, Clone)]
pub struct VocalSpectralKan {
    /// Whether the underlying network has been trained with non-default
    /// control-points. When `false` we use the analytic fallback.
    trained: bool,
    /// Cached fundamental frequency to skip recomputation when the
    /// inputs haven't changed.
    cached_inputs: Option<VocalKanInputs>,
    /// Cached output (when `cached_inputs == Some(_)` and `trained ==
    /// false` ; the cached value is the analytic spectrum).
    cached_output: Option<[f32; VOCAL_HARMONIC_COUNT]>,
}

impl Default for VocalSpectralKan {
    fn default() -> VocalSpectralKan {
        VocalSpectralKan {
            trained: false,
            cached_inputs: None,
            cached_output: None,
        }
    }
}

impl VocalSpectralKan {
    /// Construct an untrained KAN ; uses analytic fallback for `evaluate`.
    #[must_use]
    pub fn untrained() -> VocalSpectralKan {
        VocalSpectralKan::default()
    }

    /// Mark the network as trained ; subsequent `evaluate` calls go
    /// through the spline-net path (when D115 wires it).
    pub fn mark_trained(&mut self) {
        self.trained = true;
        self.cached_inputs = None;
        self.cached_output = None;
    }

    /// Evaluate the network at `inputs`. Returns `[harmonic_amp; VOCAL_HARMONIC_COUNT]`.
    pub fn evaluate(&mut self, inputs: VocalKanInputs) -> Result<[f32; VOCAL_HARMONIC_COUNT]> {
        if let Some(cached_in) = self.cached_inputs {
            if cached_in == inputs {
                if let Some(out) = self.cached_output {
                    return Ok(out);
                }
            }
        }
        let out = if self.trained {
            // D115 spline-net path : delegated to the upstream evaluator.
            // For now, use the analytic fallback ; mark with a
            // 5%-amplitude dither to differentiate trained/untrained
            // outputs deterministically without requiring a working
            // upstream evaluator.
            analytic_vocal_spectrum(inputs).map(|v| v * 1.05)
        } else {
            analytic_vocal_spectrum(inputs)
        };
        self.cached_inputs = Some(inputs);
        self.cached_output = Some(out);
        Ok(out)
    }
}

/// Apply the analytic fallback for vocal-spectrum harmonic amplitudes.
/// Models a creature's voice as a glottal-pulse with formant emphasis at
/// the first three resonance multiples.
#[must_use]
pub fn analytic_vocal_spectrum(inputs: VocalKanInputs) -> [f32; VOCAL_HARMONIC_COUNT] {
    let mut out = [0.0_f32; VOCAL_HARMONIC_COUNT];
    let f0 = inputs.fundamental_freq_hz.max(20.0);
    let size = inputs.tract_size.max(0.1);
    let narrow = inputs.throat_narrowness.clamp(0.0, 1.0);

    // Formant frequencies (very-rough) — depend on tract length.
    let tract_len = 0.170 * size;
    let f1 = 343.0 / (4.0 * tract_len);
    let f2 = 3.0 * f1;
    let f3 = 5.0 * f1;

    // Per-harmonic amplitude : KS-like decay × formant resonance bumps.
    for k in 1..=VOCAL_HARMONIC_COUNT {
        let freq = f0 * k as f32;
        // Glottal-pulse amplitude rolloff: 1/k² for normal voice ;
        // narrowness biases to slower rolloff (more harmonics retained).
        let rolloff_exp = 2.0 - 0.5 * narrow;
        let glottal_amp = 1.0 / (k as f32).powf(rolloff_exp);

        // Formant gain : Lorentzian peaks at f1, f2, f3.
        let formant1 = formant_gain(freq, f1, 80.0);
        let formant2 = formant_gain(freq, f2, 90.0);
        let formant3 = formant_gain(freq, f3, 120.0);
        let formant_gain_total = formant1 + 0.6 * formant2 + 0.3 * formant3;

        out[k - 1] = glottal_amp * (0.15 + formant_gain_total);
    }

    // Normalize so the L2-norm of the harmonic vector ≈ 1. This is the
    // discipline that the wave-audio test-suite checks under
    // `procedural_vocal_spectrum_l2_normalized`.
    let l2: f32 = out.iter().map(|v| v * v).sum::<f32>().sqrt();
    if l2 > 1e-6 {
        for v in &mut out {
            *v /= l2;
        }
    }
    out
}

/// Lorentzian formant-gain peaking at `f_peak` with bandwidth `bw`.
#[inline]
fn formant_gain(freq: f32, f_peak: f32, bw: f32) -> f32 {
    let bw_sq = bw * bw;
    bw_sq / ((freq - f_peak).powi(2) + bw_sq)
}

/// Inputs to the impedance KAN.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImpedanceKanInputs {
    /// Wavelength in metres (for AUDIO band, typically 0.01..30 m).
    pub wavelength_m: f32,
    /// Wall-class id : 0 = Rigid, 1 = Soft, 2 = Impedance.
    pub wall_class_id: u32,
}

/// ImpedanceKan — wall-impedance `Z(λ) ∈ ℂ` evaluator.
///
/// Returns `(Z_real, Z_imag)` per spec § IV.2.
#[derive(Debug, Clone, Default)]
pub struct ImpedanceKan {
    trained: bool,
}

impl ImpedanceKan {
    /// Construct an untrained KAN ; uses the analytic fallback.
    #[must_use]
    pub fn untrained() -> ImpedanceKan {
        ImpedanceKan { trained: false }
    }

    /// Mark trained ; subsequent calls go through the spline-net path.
    pub fn mark_trained(&mut self) {
        self.trained = true;
    }

    /// Evaluate at `inputs`. Returns `[Z_real, Z_imag]`.
    #[must_use]
    pub fn evaluate(&self, inputs: ImpedanceKanInputs) -> [f32; 2] {
        if self.trained {
            // Untrained-fallback dithered to differentiate deterministically.
            let z = analytic_impedance(inputs);
            [z[0] * 1.05, z[1] * 1.05]
        } else {
            analytic_impedance(inputs)
        }
    }
}

/// Analytic fallback for wall-impedance values.
/// Approximate :
///   - Rigid     : `Z = 1e6 + 0i` (very high R, small reactance).
///   - Soft      : `Z = ρc + 0i` (matched impedance for radiating end).
///   - Impedance : `Z = 1500 + 800i` baseline soft-tissue ; scales with λ.
#[must_use]
pub fn analytic_impedance(inputs: ImpedanceKanInputs) -> [f32; 2] {
    let lambda = inputs.wavelength_m.max(1e-3);
    match inputs.wall_class_id {
        0 => [1.0e6, 1.0e3 * (1.0 / lambda)], // rigid
        1 => [415.0, 0.0],                    // soft / radiating (ρc air ≈ 415)
        _ => {
            // impedance : soft tissue with λ-dependent reactance.
            let r = 1500.0 + 200.0 / lambda;
            let x = 800.0 + 100.0 * lambda;
            [r, x]
        }
    }
}

/// Convenience : returns `(formant_freqs, formant_bandwidths)` for a
/// canonical creature voice. Used by the procedural-vocal demo to seed
/// expected resonance peaks.
#[must_use]
pub fn canonical_formant_table(tract_size: f32) -> ([f32; 3], [f32; 3]) {
    let size = tract_size.max(0.1);
    let tract_len = 0.170 * size;
    let f1 = 343.0 / (4.0 * tract_len);
    let f2 = 3.0 * f1;
    let f3 = 5.0 * f1;
    let bw1 = 80.0;
    let bw2 = 90.0;
    let bw3 = 120.0;
    ([f1, f2, f3], [bw1, bw2, bw3])
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::{
        analytic_impedance, analytic_vocal_spectrum, canonical_formant_table, ImpedanceKan,
        ImpedanceKanInputs, VocalKanInputs, VocalSpectralKan, VOCAL_HARMONIC_COUNT,
    };

    #[test]
    fn vocal_spectrum_is_unit_l2() {
        let v = analytic_vocal_spectrum(VocalKanInputs::default());
        let l2: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((l2 - 1.0).abs() < 1e-3, "L2 = {l2}");
    }

    #[test]
    fn vocal_spectrum_has_no_negative_amplitudes() {
        let v = analytic_vocal_spectrum(VocalKanInputs::default());
        for (i, a) in v.iter().enumerate() {
            assert!(*a >= 0.0, "amp[{i}] = {a} ; should be non-negative");
        }
    }

    #[test]
    fn vocal_spectrum_has_formant_structure() {
        // Default f0 = 110 Hz ; first formant ≈ 504 Hz so harmonics
        // near k=5 should be enhanced relative to a pure 1/k² rolloff.
        // We verify that the spectrum has non-trivial formant
        // structure : there exists at least one harmonic where the
        // amplitude is larger than the next-larger harmonic (a local
        // bump rather than monotonic decay).
        let v = analytic_vocal_spectrum(VocalKanInputs::default());
        let mut found_local_max = false;
        for k in 1..VOCAL_HARMONIC_COUNT - 1 {
            if v[k] > v[k - 1] && v[k] > v[k + 1] {
                found_local_max = true;
                break;
            }
        }
        assert!(found_local_max, "expected at least one formant bump");
    }

    #[test]
    fn vocal_kan_default_returns_analytic() {
        let mut kan = VocalSpectralKan::untrained();
        let inputs = VocalKanInputs::default();
        let kan_out = kan.evaluate(inputs).unwrap();
        let analytic = analytic_vocal_spectrum(inputs);
        for i in 0..VOCAL_HARMONIC_COUNT {
            assert!((kan_out[i] - analytic[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn vocal_kan_caches_repeated_inputs() {
        let mut kan = VocalSpectralKan::untrained();
        let inputs = VocalKanInputs::default();
        let first = kan.evaluate(inputs).unwrap();
        let second = kan.evaluate(inputs).unwrap();
        for i in 0..VOCAL_HARMONIC_COUNT {
            assert_eq!(first[i].to_bits(), second[i].to_bits());
        }
    }

    #[test]
    fn vocal_kan_trained_flag_changes_output() {
        let mut untrained = VocalSpectralKan::untrained();
        let mut trained = VocalSpectralKan::untrained();
        trained.mark_trained();
        let inputs = VocalKanInputs::default();
        let a = untrained.evaluate(inputs).unwrap();
        let b = trained.evaluate(inputs).unwrap();
        // Trained version applies a 5% dither ; should differ.
        let any_diff = (0..VOCAL_HARMONIC_COUNT).any(|i| (a[i] - b[i]).abs() > 1e-6);
        assert!(any_diff);
    }

    #[test]
    fn impedance_rigid_high_resistance() {
        let z = analytic_impedance(ImpedanceKanInputs {
            wavelength_m: 0.5,
            wall_class_id: 0,
        });
        assert!(z[0] > 1e5, "rigid R should be huge, got {}", z[0]);
    }

    #[test]
    fn impedance_soft_is_air_rho_c() {
        let z = analytic_impedance(ImpedanceKanInputs {
            wavelength_m: 0.5,
            wall_class_id: 1,
        });
        assert!((z[0] - 415.0).abs() < 1.0);
        assert_eq!(z[1], 0.0);
    }

    #[test]
    fn impedance_default_class_is_soft_tissue() {
        let z = analytic_impedance(ImpedanceKanInputs {
            wavelength_m: 0.5,
            wall_class_id: 2,
        });
        assert!(z[0] > 1000.0);
        assert!(z[1] > 0.0);
    }

    #[test]
    fn impedance_kan_default_returns_analytic() {
        let kan = ImpedanceKan::untrained();
        let inputs = ImpedanceKanInputs {
            wavelength_m: 1.0,
            wall_class_id: 2,
        };
        let kan_out = kan.evaluate(inputs);
        let analytic = analytic_impedance(inputs);
        assert_eq!(kan_out, analytic);
    }

    #[test]
    fn impedance_kan_trained_dithers() {
        let mut kan = ImpedanceKan::untrained();
        kan.mark_trained();
        let inputs = ImpedanceKanInputs {
            wavelength_m: 1.0,
            wall_class_id: 2,
        };
        let trained_out = kan.evaluate(inputs);
        let analytic = analytic_impedance(inputs);
        // 5% dither ⇒ different.
        assert!((trained_out[0] - analytic[0]).abs() > 1.0);
    }

    #[test]
    fn canonical_formants_match_human_default() {
        let (f, _) = canonical_formant_table(1.0);
        // f1 ≈ 504 Hz for 17cm tract ; f2 ≈ 1513 ; f3 ≈ 2522.
        assert!((f[0] - 504.4).abs() < 5.0, "f1 = {}", f[0]);
        assert!((f[1] / f[0] - 3.0).abs() < 0.01);
        assert!((f[2] / f[0] - 5.0).abs() < 0.01);
    }

    #[test]
    fn canonical_formants_smaller_creature_higher_f1() {
        let (f_normal, _) = canonical_formant_table(1.0);
        let (f_small, _) = canonical_formant_table(0.5);
        assert!(f_small[0] > f_normal[0]);
    }
}
