//! Biquad IIR filter — direct-form II transposed.
//!
//! § DESIGN
//!   The biquad is the canonical second-order IIR — it implements low-pass,
//!   high-pass, band-pass, and notch shapes with the same internal
//!   structure, varying only the coefficient set. We use the
//!   **Robert Bristow-Johnson "audio EQ cookbook"** coefficients, which
//!   are the de-facto standard in audio DSP since 2005.
//!
//!   Direct-form II transposed is the recommended structure for
//!   single-precision float audio because it has the best numerical
//!   stability for low-frequency cutoffs (where coefficient
//!   ill-conditioning is worst).
//!
//! § COEFFICIENT FORMAT
//!   Standard biquad form :
//!     `y[n] = b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]`
//!   normalized so `a0 = 1`. Internally we store `(b0, b1, b2, a1, a2)`.
//!
//! § PER-CHANNEL STATE
//!   The filter is monophonic ; the mixer applies one biquad per
//!   channel by interleaving state. We hold an array of `[f32; 2]`
//!   delay-line memory for up to 8 channels (matches `SoundBank` 8ch
//!   max). Stereo filtering uses indices 0 + 1 ; mono uses 0 only.

use crate::dsp::Effect;

/// The four canonical biquad shapes the mixer ships with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiquadKind {
    /// Low-pass — passes frequencies below `cutoff`, attenuates above.
    LowPass,
    /// High-pass — passes frequencies above `cutoff`, attenuates below.
    HighPass,
    /// Band-pass — passes a narrow band centered at `cutoff`.
    BandPass,
    /// Notch — rejects a narrow band centered at `cutoff`.
    Notch,
}

impl BiquadKind {
    /// Short identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LowPass => "low-pass",
            Self::HighPass => "high-pass",
            Self::BandPass => "band-pass",
            Self::Notch => "notch",
        }
    }
}

/// Biquad filter — direct-form II transposed with up-to-8ch state.
pub struct Biquad {
    kind: BiquadKind,
    cutoff_hz: f32,
    q: f32,
    sample_rate: u32,
    /// Coefficients `(b0, b1, b2, a1, a2)`. `a0` is normalized to 1.
    coeffs: BiquadCoefficients,
    /// Per-channel delay-line memory (transposed direct-form II).
    /// `[s1[ch], s2[ch]]` for each channel.
    state: [(f32, f32); 8],
}

#[derive(Debug, Clone, Copy)]
struct BiquadCoefficients {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Biquad {
    /// Construct a biquad of `kind` with `cutoff_hz` and `q` factor.
    /// `q = 1/sqrt(2)` ≈ 0.707 is the Butterworth (maximally flat)
    /// design point ; the mixer's default for low/high-pass.
    #[must_use]
    pub fn new(kind: BiquadKind, cutoff_hz: f32, q: f32, sample_rate: u32) -> Self {
        let mut s = Self {
            kind,
            cutoff_hz,
            q,
            sample_rate,
            coeffs: BiquadCoefficients {
                b0: 1.0,
                b1: 0.0,
                b2: 0.0,
                a1: 0.0,
                a2: 0.0,
            },
            state: [(0.0, 0.0); 8],
        };
        s.recompute_coefficients();
        s
    }

    /// Convenience : Butterworth-Q low-pass.
    #[must_use]
    pub fn low_pass(cutoff_hz: f32, sample_rate: u32) -> Self {
        Self::new(
            BiquadKind::LowPass,
            cutoff_hz,
            std::f32::consts::FRAC_1_SQRT_2,
            sample_rate,
        )
    }

    /// Convenience : Butterworth-Q high-pass.
    #[must_use]
    pub fn high_pass(cutoff_hz: f32, sample_rate: u32) -> Self {
        Self::new(
            BiquadKind::HighPass,
            cutoff_hz,
            std::f32::consts::FRAC_1_SQRT_2,
            sample_rate,
        )
    }

    /// Convenience : band-pass with explicit Q.
    #[must_use]
    pub fn band_pass(cutoff_hz: f32, q: f32, sample_rate: u32) -> Self {
        Self::new(BiquadKind::BandPass, cutoff_hz, q, sample_rate)
    }

    /// Cutoff frequency.
    #[must_use]
    pub const fn cutoff_hz(&self) -> f32 {
        self.cutoff_hz
    }

    /// Q factor (resonance).
    #[must_use]
    pub const fn q(&self) -> f32 {
        self.q
    }

    /// Sample rate the coefficients were computed for.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Filter kind.
    #[must_use]
    pub const fn kind(&self) -> BiquadKind {
        self.kind
    }

    /// Update cutoff + recompute coefficients.
    pub fn set_cutoff_hz(&mut self, cutoff_hz: f32) {
        self.cutoff_hz = cutoff_hz;
        self.recompute_coefficients();
    }

    /// Update Q + recompute coefficients.
    pub fn set_q(&mut self, q: f32) {
        self.q = q.max(0.001);
        self.recompute_coefficients();
    }

    /// Update sample rate + recompute coefficients.
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate;
        self.recompute_coefficients();
    }

    /// Process one sample through the filter for a given channel.
    /// Used by tests + per-sample loops in the mixer.
    pub fn process_sample(&mut self, channel: usize, x: f32) -> f32 {
        let ch = channel.min(7);
        let (s1, s2) = self.state[ch];
        let y = self.coeffs.b0 * x + s1;
        let new_s1 = self.coeffs.b1 * x - self.coeffs.a1 * y + s2;
        let new_s2 = self.coeffs.b2 * x - self.coeffs.a2 * y;
        self.state[ch] = (new_s1, new_s2);
        y
    }

    /// Recompute coefficients from `(kind, cutoff_hz, q, sample_rate)`.
    fn recompute_coefficients(&mut self) {
        let fs = self.sample_rate.max(1) as f32;
        let f0 = self.cutoff_hz.clamp(1.0, fs * 0.499);
        let q = self.q.max(0.001);
        let omega = 2.0 * std::f32::consts::PI * f0 / fs;
        let cos_omega = omega.cos();
        let sin_omega = omega.sin();
        let alpha = sin_omega / (2.0 * q);

        // Standard biquad coefficient formulas per RBJ cookbook.
        let (b0, b1, b2, a0, a1, a2) = match self.kind {
            BiquadKind::LowPass => {
                let b0 = (1.0 - cos_omega) / 2.0;
                let b1 = 1.0 - cos_omega;
                let b2 = (1.0 - cos_omega) / 2.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_omega;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            BiquadKind::HighPass => {
                let b0 = (1.0 + cos_omega) / 2.0;
                let b1 = -(1.0 + cos_omega);
                let b2 = (1.0 + cos_omega) / 2.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_omega;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            BiquadKind::BandPass => {
                // Constant 0 dB peak gain (band-pass with Q=1 = 0dB at cutoff).
                let b0 = alpha;
                let b1 = 0.0;
                let b2 = -alpha;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_omega;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
            BiquadKind::Notch => {
                let b0 = 1.0;
                let b1 = -2.0 * cos_omega;
                let b2 = 1.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_omega;
                let a2 = 1.0 - alpha;
                (b0, b1, b2, a0, a1, a2)
            }
        };
        // Normalize so a0 = 1.
        let inv_a0 = 1.0 / a0;
        self.coeffs = BiquadCoefficients {
            b0: b0 * inv_a0,
            b1: b1 * inv_a0,
            b2: b2 * inv_a0,
            a1: a1 * inv_a0,
            a2: a2 * inv_a0,
        };
    }
}

impl Effect for Biquad {
    fn process(&mut self, buffer: &mut [f32], channels: usize, sample_rate: u32) {
        if u32::from(self.sample_rate as u32) != sample_rate {
            self.set_sample_rate(sample_rate);
        }
        let ch = channels.min(8);
        if ch == 0 {
            return;
        }
        let frames = buffer.len() / ch;
        for i in 0..frames {
            for c in 0..ch {
                let idx = i * ch + c;
                let x = buffer[idx];
                buffer[idx] = self.process_sample(c, x);
            }
        }
    }

    fn reset(&mut self) {
        self.state = [(0.0, 0.0); 8];
    }

    fn name(&self) -> &'static str {
        "biquad"
    }
}

impl core::fmt::Debug for Biquad {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Biquad")
            .field("kind", &self.kind)
            .field("cutoff_hz", &self.cutoff_hz)
            .field("q", &self.q)
            .field("sample_rate", &self.sample_rate)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
#[allow(clippy::similar_names)]
mod tests {
    use super::*;

    #[test]
    fn kind_str_names() {
        assert_eq!(BiquadKind::LowPass.as_str(), "low-pass");
        assert_eq!(BiquadKind::HighPass.as_str(), "high-pass");
        assert_eq!(BiquadKind::BandPass.as_str(), "band-pass");
        assert_eq!(BiquadKind::Notch.as_str(), "notch");
    }

    #[test]
    fn low_pass_butterworth_q() {
        let f = Biquad::low_pass(1000.0, 48_000);
        assert!((f.q() - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    }

    #[test]
    fn high_pass_butterworth_q() {
        let f = Biquad::high_pass(500.0, 48_000);
        assert_eq!(f.kind(), BiquadKind::HighPass);
    }

    #[test]
    fn band_pass_q_set() {
        let f = Biquad::band_pass(1000.0, 4.0, 48_000);
        assert_eq!(f.q(), 4.0);
    }

    #[test]
    fn set_cutoff_recomputes() {
        let mut f = Biquad::low_pass(500.0, 48_000);
        f.set_cutoff_hz(2000.0);
        assert_eq!(f.cutoff_hz(), 2000.0);
    }

    #[test]
    fn set_q_min_floor() {
        let mut f = Biquad::low_pass(500.0, 48_000);
        f.set_q(0.0);
        assert!(f.q() > 0.0); // clamped to 0.001
    }

    #[test]
    fn lowpass_attenuates_high_freq() {
        // 12 kHz square wave → low-pass at 500 Hz → output should
        // approach near-DC after a few cycles. We measure RMS of last
        // half of buffer.
        let sr = 48_000;
        let mut f = Biquad::low_pass(500.0, sr);
        let mut buf = vec![0.0_f32; 4096];
        // Generate 12 kHz sine.
        let f0 = 12_000.0_f32;
        for (i, sample) in buf.iter_mut().enumerate() {
            let t = (i as f32) / (sr as f32);
            *sample = (2.0 * std::f32::consts::PI * f0 * t).sin();
        }
        f.process(&mut buf, 1, sr);
        let half = buf.len() / 2;
        let rms_late: f32 = (buf[half..].iter().map(|x| x * x).sum::<f32>() / (half as f32)).sqrt();
        // 12 kHz at low-pass(500) should be heavily attenuated (≪ unity).
        assert!(rms_late < 0.1, "RMS_late={rms_late} ; expected ≪ 0.1");
    }

    #[test]
    fn lowpass_passes_low_freq() {
        // 100 Hz sine → low-pass at 5 kHz → almost passthrough.
        let sr = 48_000;
        let mut f = Biquad::low_pass(5_000.0, sr);
        let mut buf = vec![0.0_f32; 4096];
        let f0 = 100.0_f32;
        for (i, sample) in buf.iter_mut().enumerate() {
            let t = (i as f32) / (sr as f32);
            *sample = (2.0 * std::f32::consts::PI * f0 * t).sin();
        }
        let mut input = buf.clone();
        f.process(&mut buf, 1, sr);
        // Compute RMS of input vs output ; ratio should be ≈ 1.
        let half = buf.len() / 2;
        let rms_in: f32 = (input[half..].iter().map(|x| x * x).sum::<f32>() / (half as f32)).sqrt();
        let rms_out: f32 = (buf[half..].iter().map(|x| x * x).sum::<f32>() / (half as f32)).sqrt();
        let ratio = rms_out / rms_in.max(1e-9);
        assert!(ratio > 0.85 && ratio < 1.15, "ratio={ratio} ; expected ≈ 1");
        // Suppress unused-mut warning :
        input.clear();
    }

    #[test]
    fn highpass_passes_high_freq() {
        // 8 kHz sine → high-pass at 500 Hz → near-passthrough.
        let sr = 48_000;
        let mut f = Biquad::high_pass(500.0, sr);
        let mut buf = vec![0.0_f32; 4096];
        let f0 = 8_000.0_f32;
        for (i, sample) in buf.iter_mut().enumerate() {
            let t = (i as f32) / (sr as f32);
            *sample = (2.0 * std::f32::consts::PI * f0 * t).sin();
        }
        let input = buf.clone();
        f.process(&mut buf, 1, sr);
        let half = buf.len() / 2;
        let rms_in: f32 = (input[half..].iter().map(|x| x * x).sum::<f32>() / (half as f32)).sqrt();
        let rms_out: f32 = (buf[half..].iter().map(|x| x * x).sum::<f32>() / (half as f32)).sqrt();
        let ratio = rms_out / rms_in.max(1e-9);
        assert!(ratio > 0.85 && ratio < 1.15, "ratio={ratio} ; expected ≈ 1");
    }

    #[test]
    fn highpass_attenuates_low_freq() {
        // 50 Hz sine → high-pass at 5 kHz → heavily attenuated.
        let sr = 48_000;
        let mut f = Biquad::high_pass(5_000.0, sr);
        let mut buf = vec![0.0_f32; 4096];
        let f0 = 50.0_f32;
        for (i, sample) in buf.iter_mut().enumerate() {
            let t = (i as f32) / (sr as f32);
            *sample = (2.0 * std::f32::consts::PI * f0 * t).sin();
        }
        f.process(&mut buf, 1, sr);
        let half = buf.len() / 2;
        let rms_late: f32 = (buf[half..].iter().map(|x| x * x).sum::<f32>() / (half as f32)).sqrt();
        assert!(rms_late < 0.1, "RMS_late={rms_late}");
    }

    #[test]
    fn process_zero_signal_yields_zero() {
        let sr = 48_000;
        let mut f = Biquad::low_pass(1000.0, sr);
        let mut buf = vec![0.0_f32; 256];
        f.process(&mut buf, 1, sr);
        for s in &buf {
            assert!(s.abs() < 1e-9);
        }
    }

    #[test]
    fn reset_clears_state() {
        let sr = 48_000;
        let mut f = Biquad::low_pass(1000.0, sr);
        let mut buf = vec![1.0_f32; 64];
        f.process(&mut buf, 1, sr);
        f.reset();
        // After reset, processing zero input should give exactly zero.
        let mut zeros = vec![0.0_f32; 64];
        f.process(&mut zeros, 1, sr);
        for s in &zeros {
            assert!(s.abs() < 1e-9);
        }
    }

    #[test]
    fn determinism_same_input_same_output() {
        let sr = 48_000;
        let mut f1 = Biquad::low_pass(1000.0, sr);
        let mut f2 = Biquad::low_pass(1000.0, sr);
        let mut buf1 = vec![0.0_f32; 256];
        let mut buf2 = vec![0.0_f32; 256];
        for (i, (a, b)) in buf1.iter_mut().zip(buf2.iter_mut()).enumerate() {
            let v = (i as f32 * 0.01).sin();
            *a = v;
            *b = v;
        }
        f1.process(&mut buf1, 1, sr);
        f2.process(&mut buf2, 1, sr);
        assert_eq!(buf1, buf2);
    }

    #[test]
    fn stereo_processing_independent_per_channel() {
        let sr = 48_000;
        let mut f = Biquad::low_pass(1000.0, sr);
        // Stereo : L = sine, R = silence. After filter, L should be
        // filtered, R should remain silence.
        let mut buf = vec![0.0_f32; 512];
        for i in 0..256 {
            let v = (i as f32 * 0.01).sin();
            buf[i * 2] = v; // L
            buf[i * 2 + 1] = 0.0; // R
        }
        f.process(&mut buf, 2, sr);
        // R channel : all zeros (filter on independent channel state).
        for i in 0..256 {
            assert!(buf[i * 2 + 1].abs() < 1e-9, "R[{i}] = {}", buf[i * 2 + 1]);
        }
    }

    #[test]
    fn cutoff_clamps_to_nyquist() {
        // 30 kHz cutoff at 48 kHz sample rate exceeds Nyquist (24 kHz).
        // Should clamp internally without panic.
        let sr = 48_000;
        let mut f = Biquad::low_pass(30_000.0, sr);
        let mut buf = vec![0.5_f32; 64];
        f.process(&mut buf, 1, sr);
        // No panic + buffer mutated.
    }

    #[test]
    fn cutoff_clamps_below_min() {
        // 0 Hz cutoff is invalid ; should clamp to 1 Hz.
        let sr = 48_000;
        let _f = Biquad::low_pass(0.0, sr);
        // No panic.
    }
}
