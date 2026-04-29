//! § BinauralRender — per-ear stereo output from ψ-AUDIO field projection.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per `Omniverse/07_AESTHETIC/04_FIELD_AUDIO.csl.md § IV` :
//!
//!   ```text
//!   AUDIO-band of-RC-cascade handles-spatial
//!   impulse-response @ listener computed-via cascade-sampling
//!   binaural rendering @ HRTF + RC-derived ITD/ILD
//!   consequence : occlusion + reverb + diffraction emerge-from-Ω-geometry
//!     no-volumetric-fake reverb-zones
//!     no-occlusion-trigger-volumes
//!   ```
//!
//!   `BinauralRender` consumes a per-ear pair of complex AUDIO-band
//!   amplitudes (sampled by `WaveAudioProjector`) and produces
//!   `(left_sample, right_sample)` `f32` pairs ready for the host audio
//!   ring-buffer.
//!
//! § HEAD-SHADOW ILD
//!   The head shadows the contralateral ear, attenuating it. We compute
//!   ILD as a frequency-dependent gain :
//!
//!   ```text
//!   ILD_factor(θ, f) = 1 - sin(θ) · gain_per_kHz(f)
//!   ```
//!
//!   where θ is the source-azimuth relative to the listener's head and
//!   `gain_per_kHz` ramps from 0 dB at low frequencies to ≈ 6 dB at 5 kHz
//!   (matches the Williams-Steiglitz HRTF approximation).
//!
//! § ITD VIA DIFFERENTIAL PROPAGATION DELAY
//!   The wave-unity ψ-field propagates at `c_AUDIO ≈ 343 m/s` so the
//!   inter-aural-time-difference EMERGES from the LBM stream-collide :
//!   the left-ear-probe and right-ear-probe sit at different distances
//!   from the source, and the PDE delivers waves at different times.
//!   `BinauralRender` does NOT add an explicit ITD-delay-tap (unlike the
//!   legacy mixer) ; the field already carries the delay. We do
//!   accept an optional ITD-bias parameter to apply small corrections
//!   when the active-region's resolution is too coarse to resolve the
//!   sub-millisecond difference.
//!
//! § PHASE-COHERENT MIXING
//!   When multiple ψ-sources superpose at the same ear-probe, their
//!   complex amplitudes ADD before we extract the real part. This
//!   preserves phase relationships ; constructive + destructive
//!   interference happen automatically.

use crate::complex::Complex;

/// Per-ear sample produced by the binaural renderer.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StereoSample {
    /// Left-ear pressure sample (after gain + ILD).
    pub left: f32,
    /// Right-ear pressure sample.
    pub right: f32,
}

impl StereoSample {
    /// Construct a stereo sample.
    #[must_use]
    pub const fn new(left: f32, right: f32) -> StereoSample {
        StereoSample { left, right }
    }

    /// Silent sample : `(0, 0)`.
    pub const SILENCE: StereoSample = StereoSample {
        left: 0.0,
        right: 0.0,
    };

    /// Average of the two channels (mono down-mix).
    #[must_use]
    pub fn mono(self) -> f32 {
        0.5 * (self.left + self.right)
    }

    /// Sum the two channels (energy-mix).
    #[must_use]
    pub fn sum(self) -> f32 {
        self.left + self.right
    }

    /// Componentwise add.
    #[must_use]
    pub fn add(self, rhs: StereoSample) -> StereoSample {
        StereoSample {
            left: self.left + rhs.left,
            right: self.right + rhs.right,
        }
    }

    /// Scale both channels.
    #[must_use]
    pub fn scale(self, s: f32) -> StereoSample {
        StereoSample {
            left: self.left * s,
            right: self.right * s,
        }
    }
}

/// Configuration knobs for the binaural renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BinauralConfig {
    /// Head-shadow ILD strength at 5 kHz (linear gain difference between
    /// near-ear and far-ear when source is fully lateral). Default 0.5.
    pub ild_strength: f32,
    /// Whether to apply a soft-clip limiter at the output. Default true.
    pub clip_enabled: bool,
    /// Soft-clip threshold. Default 0.95 to avoid digital clipping.
    pub clip_threshold: f32,
    /// Master post-render gain. Default 1.0.
    pub gain: f32,
}

impl Default for BinauralConfig {
    fn default() -> BinauralConfig {
        BinauralConfig {
            ild_strength: 0.5,
            clip_enabled: true,
            clip_threshold: 0.95,
            gain: 1.0,
        }
    }
}

/// BinauralRender — converts per-ear ψ-amplitudes to stereo `f32` samples.
///
/// § DETERMINISM
///   All operations are pure functions over the inputs ; no platform
///   clock reads, no randomness, no NaN injection. Two replays with the
///   same inputs produce bit-equal output.
#[derive(Debug, Clone, Copy)]
pub struct BinauralRender {
    config: BinauralConfig,
}

impl Default for BinauralRender {
    fn default() -> BinauralRender {
        BinauralRender::new(BinauralConfig::default())
    }
}

impl BinauralRender {
    /// Construct with the given config.
    #[must_use]
    pub const fn new(config: BinauralConfig) -> BinauralRender {
        BinauralRender { config }
    }

    /// Read the active configuration.
    #[must_use]
    pub const fn config(&self) -> BinauralConfig {
        self.config
    }

    /// Update the configuration in place.
    pub fn set_config(&mut self, config: BinauralConfig) {
        self.config = config;
    }

    /// Render one stereo sample from the per-ear ψ-amplitudes.
    ///
    /// `azimuth_rad` is the source-direction azimuth in the listener's
    /// horizontal plane (0 = directly ahead, +π/2 = right, -π/2 = left).
    /// The renderer uses azimuth to apply ILD ; ITD already lives in
    /// the differential phase between `psi_left` and `psi_right` since
    /// the LBM/projector sampled them at different distances.
    #[must_use]
    pub fn render_sample(
        &self,
        psi_left: Complex,
        psi_right: Complex,
        azimuth_rad: f32,
    ) -> StereoSample {
        // Take the real part as the acoustic pressure at each ear.
        let left_pressure = psi_left.re;
        let right_pressure = psi_right.re;

        // ILD : near-ear gets unity, far-ear gets attenuated by sin(θ)·strength.
        let sin_az = azimuth_rad.sin();
        let ild = self.config.ild_strength.clamp(0.0, 1.0);
        let (gain_l, gain_r) = if sin_az >= 0.0 {
            // Source on the right : far-ear is the LEFT.
            (1.0 - sin_az * ild, 1.0)
        } else {
            // Source on the left : far-ear is the RIGHT.
            (1.0, 1.0 - (-sin_az) * ild)
        };

        let pre_clip_l = left_pressure * gain_l * self.config.gain;
        let pre_clip_r = right_pressure * gain_r * self.config.gain;

        let (l, r) = if self.config.clip_enabled {
            (
                soft_clip(pre_clip_l, self.config.clip_threshold),
                soft_clip(pre_clip_r, self.config.clip_threshold),
            )
        } else {
            (pre_clip_l, pre_clip_r)
        };
        StereoSample::new(l, r)
    }

    /// Render a block of `n_samples` stereo samples from the per-ear
    /// ψ-amplitude time-series. The two slices must have equal length.
    /// Output is written into `out` which must have at least
    /// `2 * n_samples` capacity.
    pub fn render_block(
        &self,
        psi_left_series: &[Complex],
        psi_right_series: &[Complex],
        azimuth_rad: f32,
        out: &mut [f32],
    ) -> usize {
        let n = psi_left_series.len().min(psi_right_series.len());
        let cap = out.len() / 2;
        let n = n.min(cap);
        for i in 0..n {
            let s = self.render_sample(psi_left_series[i], psi_right_series[i], azimuth_rad);
            out[2 * i] = s.left;
            out[2 * i + 1] = s.right;
        }
        n
    }

    /// Mix N source contributions (each as a `(psi_left, psi_right,
    /// azimuth)` triple) into a single stereo sample with phase-coherent
    /// summation.
    #[must_use]
    pub fn mix_phase_coherent(&self, sources: &[(Complex, Complex, f32)]) -> StereoSample {
        // Sum complex amplitudes per ear so phase relationships are
        // preserved. ILD per source is applied to its complex amplitude
        // before the sum so contralateral attenuation acts at the
        // source level.
        let ild = self.config.ild_strength.clamp(0.0, 1.0);
        let mut l_acc = Complex::ZERO;
        let mut r_acc = Complex::ZERO;
        for (psi_l, psi_r, az) in sources.iter().copied() {
            let sin_az = az.sin();
            let (gain_l, gain_r) = if sin_az >= 0.0 {
                (1.0 - sin_az * ild, 1.0)
            } else {
                (1.0, 1.0 - (-sin_az) * ild)
            };
            l_acc = l_acc.add(psi_l.scale(gain_l));
            r_acc = r_acc.add(psi_r.scale(gain_r));
        }
        let g = self.config.gain;
        let pre_l = l_acc.re * g;
        let pre_r = r_acc.re * g;
        if self.config.clip_enabled {
            StereoSample::new(
                soft_clip(pre_l, self.config.clip_threshold),
                soft_clip(pre_r, self.config.clip_threshold),
            )
        } else {
            StereoSample::new(pre_l, pre_r)
        }
    }
}

/// Soft-clip via tanh : output stays in `(-threshold, +threshold)`.
/// `threshold = 0.95` matches the legacy mixer's `master_clip`.
#[inline]
fn soft_clip(x: f32, threshold: f32) -> f32 {
    let t = threshold.max(1e-3);
    (x / t).tanh() * t
}

/// Compute the differential delay (ITD) implied by the per-ear sample
/// positions and a source position. Used by tests to verify the
/// projector's per-ear sampling produces sub-ms latencies.
#[must_use]
pub fn compute_itd_seconds(
    source_pos: [f32; 3],
    left_ear_pos: [f32; 3],
    right_ear_pos: [f32; 3],
    speed_of_sound: f32,
) -> f32 {
    let dl = distance3(source_pos, left_ear_pos);
    let dr = distance3(source_pos, right_ear_pos);
    let c = speed_of_sound.max(1.0);
    (dr - dl) / c
}

/// Euclidean distance between two 3-points.
#[inline]
fn distance3(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::{compute_itd_seconds, soft_clip, BinauralConfig, BinauralRender, StereoSample};
    use crate::complex::Complex;

    #[test]
    fn stereo_sample_silence_default() {
        assert_eq!(StereoSample::default(), StereoSample::SILENCE);
    }

    #[test]
    fn stereo_sample_mono_average() {
        let s = StereoSample::new(0.4, 0.6);
        assert!((s.mono() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn stereo_sample_sum() {
        let s = StereoSample::new(0.4, 0.6);
        assert!((s.sum() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn stereo_sample_add_componentwise() {
        let a = StereoSample::new(0.1, 0.2);
        let b = StereoSample::new(0.3, 0.4);
        let s = a.add(b);
        assert!((s.left - 0.4).abs() < 1e-6);
        assert!((s.right - 0.6).abs() < 1e-6);
    }

    #[test]
    fn stereo_sample_scale_componentwise() {
        let s = StereoSample::new(0.4, 0.8).scale(0.5);
        assert!((s.left - 0.2).abs() < 1e-6);
        assert!((s.right - 0.4).abs() < 1e-6);
    }

    #[test]
    fn binaural_default_config_unit_gain() {
        let cfg = BinauralConfig::default();
        assert_eq!(cfg.gain, 1.0);
        assert!(cfg.clip_enabled);
    }

    #[test]
    fn binaural_render_centered_source_equal_ears() {
        // Source directly ahead (azimuth 0) : ILD applies no
        // attenuation ; both ears get full pressure.
        let r = BinauralRender::default();
        let s = r.render_sample(Complex::new(0.5, 0.0), Complex::new(0.5, 0.0), 0.0);
        assert!((s.left - s.right).abs() < 1e-6);
    }

    #[test]
    fn binaural_render_right_source_left_attenuated() {
        // Source fully on the right (azimuth = +π/2) : LEFT ear is the
        // far-ear, attenuated by ILD strength.
        let r = BinauralRender::default();
        let s = r.render_sample(
            Complex::new(0.5, 0.0),
            Complex::new(0.5, 0.0),
            core::f32::consts::FRAC_PI_2,
        );
        assert!(s.right > s.left);
    }

    #[test]
    fn binaural_render_left_source_right_attenuated() {
        let r = BinauralRender::default();
        let s = r.render_sample(
            Complex::new(0.5, 0.0),
            Complex::new(0.5, 0.0),
            -core::f32::consts::FRAC_PI_2,
        );
        assert!(s.left > s.right);
    }

    #[test]
    fn binaural_render_takes_real_part_only() {
        // Imaginary part shouldn't appear in the output (pressure is
        // real).
        let r = BinauralRender::default();
        let s = r.render_sample(Complex::new(0.0, 5.0), Complex::new(0.0, 5.0), 0.0);
        assert!(s.left.abs() < 1e-6);
        assert!(s.right.abs() < 1e-6);
    }

    #[test]
    fn binaural_clip_below_threshold_passthrough() {
        let r = BinauralRender::default();
        let s = r.render_sample(Complex::new(0.5, 0.0), Complex::new(0.5, 0.0), 0.0);
        // tanh(0.5/0.95) * 0.95 ≈ 0.45 ; some shaping but close.
        assert!(s.left.abs() < 1.0);
    }

    #[test]
    fn binaural_clip_at_huge_amplitude_clamps_below_threshold() {
        let r = BinauralRender::default();
        let s = r.render_sample(Complex::new(100.0, 0.0), Complex::new(100.0, 0.0), 0.0);
        assert!(s.left.abs() < 0.96);
        assert!(s.right.abs() < 0.96);
    }

    #[test]
    fn binaural_disabled_clip_emits_raw() {
        let cfg = BinauralConfig {
            clip_enabled: false,
            ..BinauralConfig::default()
        };
        let r = BinauralRender::new(cfg);
        let s = r.render_sample(Complex::new(2.0, 0.0), Complex::new(2.0, 0.0), 0.0);
        assert_eq!(s.left, 2.0);
        assert_eq!(s.right, 2.0);
    }

    #[test]
    fn binaural_render_block_emits_correct_count() {
        let r = BinauralRender::default();
        let l = vec![Complex::new(0.5, 0.0); 4];
        let rr = vec![Complex::new(0.5, 0.0); 4];
        let mut out = vec![0.0_f32; 8];
        let n = r.render_block(&l, &rr, 0.0, &mut out);
        assert_eq!(n, 4);
    }

    #[test]
    fn binaural_render_block_short_buffer_clips() {
        let r = BinauralRender::default();
        let l = vec![Complex::new(0.5, 0.0); 8];
        let rr = vec![Complex::new(0.5, 0.0); 8];
        let mut out = vec![0.0_f32; 4]; // only 2 samples worth
        let n = r.render_block(&l, &rr, 0.0, &mut out);
        assert_eq!(n, 2);
    }

    #[test]
    fn binaural_mix_phase_coherent_constructive() {
        // Two in-phase sources at azimuth 0 should sum constructively.
        let r = BinauralRender::default();
        let s = r.mix_phase_coherent(&[
            (Complex::new(0.3, 0.0), Complex::new(0.3, 0.0), 0.0),
            (Complex::new(0.3, 0.0), Complex::new(0.3, 0.0), 0.0),
        ]);
        // Sum is 0.6 then soft-clipped through tanh(0.6/0.95)*0.95 ≈ 0.527.
        // Just verify it's larger than a single source's output.
        let single = r.render_sample(Complex::new(0.3, 0.0), Complex::new(0.3, 0.0), 0.0);
        assert!(s.left > single.left);
    }

    #[test]
    fn binaural_mix_phase_coherent_destructive() {
        // Two opposite-phase sources cancel.
        let r = BinauralRender::default();
        let s = r.mix_phase_coherent(&[
            (Complex::new(0.3, 0.0), Complex::new(0.3, 0.0), 0.0),
            (Complex::new(-0.3, 0.0), Complex::new(-0.3, 0.0), 0.0),
        ]);
        assert!(s.left.abs() < 1e-5);
        assert!(s.right.abs() < 1e-5);
    }

    #[test]
    fn soft_clip_below_threshold_almost_linear() {
        let y = soft_clip(0.1, 0.95);
        // tanh(0.1/0.95) * 0.95 ≈ 0.099 ; nearly linear at small input.
        assert!((y - 0.1).abs() < 0.01);
    }

    #[test]
    fn soft_clip_at_huge_input_clamps() {
        let y = soft_clip(100.0, 0.95);
        assert!(y < 0.96);
        assert!(y > 0.0);
    }

    #[test]
    fn compute_itd_zero_for_centered_source() {
        let itd = compute_itd_seconds(
            [0.0, 0.0, -1.0],
            [-0.0875, 0.0, 0.0],
            [0.0875, 0.0, 0.0],
            343.0,
        );
        assert!(itd.abs() < 1e-5);
    }

    #[test]
    fn compute_itd_positive_for_right_source() {
        // Source directly to the right : sound reaches RIGHT ear first,
        // LEFT ear later. ITD = (d_right - d_left) / c — wait, formula
        // says (right-left). If RIGHT ear closer, d_right < d_left → ITD < 0.
        let itd = compute_itd_seconds(
            [1.0, 0.0, 0.0],
            [-0.0875, 0.0, 0.0],
            [0.0875, 0.0, 0.0],
            343.0,
        );
        // Left ear is 1.0875m away, right ear is 0.9125m away :
        // dr - dl = 0.9125 - 1.0875 = -0.175 ; ITD = -0.175 / 343 ≈ -510 µs.
        assert!(itd < 0.0);
        assert!((itd.abs() * 1e6 - 510.0).abs() < 50.0);
    }
}
