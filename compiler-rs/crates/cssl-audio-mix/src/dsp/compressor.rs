//! Dynamics processors — Compressor + Limiter.
//!
//! § DESIGN
//!   The Compressor is a feed-forward, peak-detection dynamics processor :
//!     ```text
//!       ┌─────────────┐    ┌──────────────┐
//!     ─►│ peak detect ├───►│ gain compute ├──→ apply →
//!       └─────────────┘    └──────────────┘
//!     ```
//!   Above `threshold`, the gain is reduced by `(input - threshold) *
//!   (1 - 1/ratio)`. Below threshold, gain = 1. The envelope follower
//!   uses standard attack + release time-constants to smooth the gain
//!   trajectory.
//!
//!   The Limiter is a special-case Compressor with `ratio = ∞` and a
//!   short attack — typically used as a master-bus brick-wall.
//!
//! § PARAMETERS
//!   - `threshold` ∈ [-60 dB, 0 dB] — level above which compression
//!                                    kicks in. Internally stored as
//!                                    linear amplitude (0.0..1.0).
//!   - `ratio`     ∈ [1, ∞]         — `1` = no compression ;
//!                                    `4` = 4:1 ratio (typical).
//!   - `attack_ms` ∈ [0.1, 1000]    — time to reach 63 % gain reduction.
//!   - `release_ms` ∈ [1, 5000]     — time to release 63 % of the
//!                                    reduction.
//!
//! § DETERMINISM
//!   Pure function over `(input, params, prior_envelope)`. Two replays
//!   with identical inputs produce bit-equal output.

use crate::dsp::Effect;

/// Feed-forward peak compressor with attack + release smoothing.
pub struct Compressor {
    /// Threshold (linear amplitude, 0..1).
    threshold: f32,
    /// Compression ratio (≥ 1).
    ratio: f32,
    /// Attack coefficient (precomputed from attack_ms + sample_rate).
    attack_coeff: f32,
    /// Release coefficient (precomputed from release_ms + sample_rate).
    release_coeff: f32,
    /// Makeup gain (1.0 = unity).
    makeup_gain: f32,
    /// Per-channel envelope state (peak follower).
    envelope: [f32; 8],
    /// Sample rate.
    sample_rate: u32,
    /// Original attack_ms (kept for set_sample_rate recompute).
    attack_ms: f32,
    /// Original release_ms (kept for set_sample_rate recompute).
    release_ms: f32,
}

impl Compressor {
    /// Construct a compressor with sensible defaults.
    /// (-12 dBFS threshold, 4:1 ratio, 5ms attack, 100ms release).
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        let mut c = Self {
            threshold: db_to_linear(-12.0),
            ratio: 4.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            makeup_gain: 1.0,
            envelope: [0.0; 8],
            sample_rate,
            attack_ms: 5.0,
            release_ms: 100.0,
        };
        c.recompute_coefficients();
        c
    }

    /// Construct with explicit parameters.
    #[must_use]
    pub fn with_params(
        sample_rate: u32,
        threshold_db: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
    ) -> Self {
        let mut c = Self::new(sample_rate);
        c.set_threshold_db(threshold_db);
        c.set_ratio(ratio);
        c.set_attack_ms(attack_ms);
        c.set_release_ms(release_ms);
        c
    }

    /// Threshold in dBFS.
    #[must_use]
    pub fn threshold_db(&self) -> f32 {
        linear_to_db(self.threshold)
    }

    /// Set threshold in dBFS (-60..0).
    pub fn set_threshold_db(&mut self, threshold_db: f32) {
        let clamped = threshold_db.clamp(-60.0, 0.0);
        self.threshold = db_to_linear(clamped);
    }

    /// Ratio (≥ 1).
    #[must_use]
    pub const fn ratio(&self) -> f32 {
        self.ratio
    }

    /// Set ratio (≥ 1).
    pub fn set_ratio(&mut self, ratio: f32) {
        self.ratio = ratio.max(1.0);
    }

    /// Attack time in ms.
    #[must_use]
    pub const fn attack_ms(&self) -> f32 {
        self.attack_ms
    }

    /// Set attack in ms (0.1..1000).
    pub fn set_attack_ms(&mut self, attack_ms: f32) {
        self.attack_ms = attack_ms.clamp(0.1, 1000.0);
        self.recompute_coefficients();
    }

    /// Release time in ms.
    #[must_use]
    pub const fn release_ms(&self) -> f32 {
        self.release_ms
    }

    /// Set release in ms (1..5000).
    pub fn set_release_ms(&mut self, release_ms: f32) {
        self.release_ms = release_ms.clamp(1.0, 5000.0);
        self.recompute_coefficients();
    }

    /// Makeup gain (linear).
    #[must_use]
    pub const fn makeup_gain(&self) -> f32 {
        self.makeup_gain
    }

    /// Set makeup gain (linear ; 1.0 = unity).
    pub fn set_makeup_gain(&mut self, gain: f32) {
        self.makeup_gain = gain.max(0.0);
    }

    /// Set sample rate + recompute coefficients.
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate;
        self.recompute_coefficients();
    }

    fn recompute_coefficients(&mut self) {
        let sr = self.sample_rate.max(1) as f32;
        // Standard one-pole lowpass smoothing : exp(-1 / (tau * sr)).
        let attack_tau = self.attack_ms * 0.001;
        let release_tau = self.release_ms * 0.001;
        self.attack_coeff = (-1.0 / (attack_tau * sr)).exp();
        self.release_coeff = (-1.0 / (release_tau * sr)).exp();
    }

    /// Process one sample for a given channel ; returns the compressed value.
    fn process_sample_channel(&mut self, channel: usize, x: f32) -> f32 {
        let ch = channel.min(7);
        let abs = x.abs();
        // Peak follower : attack on rising, release on falling.
        let coeff = if abs > self.envelope[ch] {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.envelope[ch] = coeff * (self.envelope[ch] - abs) + abs;
        // Gain computation.
        let env = self.envelope[ch];
        let gain = if env > self.threshold && self.threshold > 1e-9 {
            // Above threshold : reduce by ratio.
            //   over_db = 20*log10(env / threshold)
            //   reduction_db = over_db * (1 - 1/ratio)
            //   gain = 10^(-reduction_db / 20)
            // Linear-domain :
            //   gain = (threshold/env) ^ (1 - 1/ratio)
            let over = env / self.threshold;
            let exponent = 1.0 - 1.0 / self.ratio;
            (1.0 / over).powf(exponent)
        } else {
            1.0
        };
        x * gain * self.makeup_gain
    }
}

impl Effect for Compressor {
    fn process(&mut self, buffer: &mut [f32], channels: usize, sample_rate: u32) {
        if sample_rate != self.sample_rate {
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
                buffer[idx] = self.process_sample_channel(c, buffer[idx]);
            }
        }
    }

    fn reset(&mut self) {
        self.envelope = [0.0; 8];
    }

    fn name(&self) -> &'static str {
        "compressor"
    }
}

impl core::fmt::Debug for Compressor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Compressor")
            .field("threshold_db", &self.threshold_db())
            .field("ratio", &self.ratio)
            .field("attack_ms", &self.attack_ms)
            .field("release_ms", &self.release_ms)
            .field("makeup_gain", &self.makeup_gain)
            .field("sample_rate", &self.sample_rate)
            .finish_non_exhaustive()
    }
}

/// Brick-wall limiter — Compressor at ratio = ∞ with short attack.
pub struct Limiter {
    inner: Compressor,
}

impl Limiter {
    /// Construct a limiter at -1 dBFS threshold with 0.5 ms attack +
    /// 50 ms release. Ratio is set to a very high value (1000) which
    /// approximates brick-wall behavior.
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        Self {
            inner: Compressor::with_params(sample_rate, -1.0, 1000.0, 0.5, 50.0),
        }
    }

    /// Set threshold in dBFS.
    pub fn set_threshold_db(&mut self, threshold_db: f32) {
        self.inner.set_threshold_db(threshold_db);
    }

    /// Threshold in dBFS.
    #[must_use]
    pub fn threshold_db(&self) -> f32 {
        self.inner.threshold_db()
    }
}

impl Effect for Limiter {
    fn process(&mut self, buffer: &mut [f32], channels: usize, sample_rate: u32) {
        self.inner.process(buffer, channels, sample_rate);
    }

    fn reset(&mut self) {
        self.inner.reset();
    }

    fn name(&self) -> &'static str {
        "limiter"
    }
}

impl core::fmt::Debug for Limiter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Limiter")
            .field("threshold_db", &self.threshold_db())
            .finish_non_exhaustive()
    }
}

/// Convert dBFS → linear amplitude.
fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Convert linear amplitude → dBFS.
fn linear_to_db(linear: f32) -> f32 {
    if linear <= 1e-9 {
        return -180.0;
    }
    20.0 * linear.log10()
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn db_linear_roundtrip() {
        assert!((db_to_linear(0.0) - 1.0).abs() < 1e-6);
        assert!((linear_to_db(1.0) - 0.0).abs() < 1e-6);
        assert!((db_to_linear(-6.0) - 0.501_19).abs() < 1e-3);
        assert!((linear_to_db(0.5) - -6.02).abs() < 0.01);
    }

    #[test]
    fn linear_to_db_zero_safe() {
        assert!(linear_to_db(0.0) < -100.0);
    }

    #[test]
    fn compressor_default_threshold_minus_12() {
        let c = Compressor::new(48_000);
        assert!((c.threshold_db() - -12.0).abs() < 0.01);
    }

    #[test]
    fn compressor_default_ratio_4() {
        let c = Compressor::new(48_000);
        assert_eq!(c.ratio(), 4.0);
    }

    #[test]
    fn compressor_default_attack_release() {
        let c = Compressor::new(48_000);
        assert_eq!(c.attack_ms(), 5.0);
        assert_eq!(c.release_ms(), 100.0);
    }

    #[test]
    fn ratio_clamps_below_one() {
        let mut c = Compressor::new(48_000);
        c.set_ratio(0.5);
        assert_eq!(c.ratio(), 1.0);
    }

    #[test]
    fn threshold_clamps_above_zero() {
        let mut c = Compressor::new(48_000);
        c.set_threshold_db(10.0);
        assert!(c.threshold_db() <= 0.01);
    }

    #[test]
    fn threshold_clamps_below_minus_60() {
        let mut c = Compressor::new(48_000);
        c.set_threshold_db(-100.0);
        assert!(c.threshold_db() >= -60.01);
    }

    #[test]
    fn attack_clamps() {
        let mut c = Compressor::new(48_000);
        c.set_attack_ms(0.0);
        assert!(c.attack_ms() >= 0.1);
        c.set_attack_ms(2000.0);
        assert!(c.attack_ms() <= 1000.0);
    }

    #[test]
    fn release_clamps() {
        let mut c = Compressor::new(48_000);
        c.set_release_ms(0.0);
        assert!(c.release_ms() >= 1.0);
        c.set_release_ms(10_000.0);
        assert!(c.release_ms() <= 5000.0);
    }

    #[test]
    fn makeup_gain_non_negative() {
        let mut c = Compressor::new(48_000);
        c.set_makeup_gain(-1.0);
        assert_eq!(c.makeup_gain(), 0.0);
    }

    #[test]
    fn signal_below_threshold_passes_unchanged() {
        let sr = 48_000;
        let mut c = Compressor::with_params(sr, -6.0, 4.0, 5.0, 100.0);
        // Signal at -20 dBFS = 0.1 amplitude. Threshold = -6 dBFS = 0.501.
        let mut buf = vec![0.1_f32; 1024];
        let input = buf.clone();
        c.process(&mut buf, 1, sr);
        // Slight envelope smoothing, but output should be ≈ input.
        for (a, b) in input.iter().zip(buf.iter()) {
            assert!((a - b).abs() < 1e-3, "{a} → {b}");
        }
    }

    #[test]
    fn signal_above_threshold_is_compressed() {
        let sr = 48_000;
        let mut c = Compressor::with_params(sr, -12.0, 4.0, 1.0, 100.0);
        // Hot signal : 1.0 amplitude (0 dBFS). Compressed by 4:1 above
        // -12 dBFS. We measure peak after the envelope has stabilized.
        let mut buf = vec![1.0_f32; 4096];
        c.process(&mut buf, 1, sr);
        let peak_late: f32 = buf[2048..].iter().fold(0.0_f32, |a, b| a.max(b.abs()));
        // Expected : ~ -3 dBFS (≈ 0.7) post-compression.
        // 0 dBFS - (12 dB excess - 12/ratio) = -3 dBFS.
        assert!(peak_late < 0.85, "peak_late={peak_late} ; expected < 0.85");
    }

    #[test]
    fn compressor_reset_clears_envelope() {
        let sr = 48_000;
        let mut c = Compressor::new(sr);
        let mut buf = vec![1.0_f32; 1024];
        c.process(&mut buf, 1, sr);
        c.reset();
        // After reset, envelope is zero ; very-low signal should pass.
        let mut quiet = vec![0.05_f32; 1024];
        c.process(&mut quiet, 1, sr);
        for s in &quiet {
            assert!((s.abs() - 0.05).abs() < 0.01);
        }
    }

    #[test]
    fn determinism_same_input_same_output() {
        let sr = 48_000;
        let mut c1 = Compressor::new(sr);
        let mut c2 = Compressor::new(sr);
        let mut buf1 = vec![0.0_f32; 256];
        let mut buf2 = vec![0.0_f32; 256];
        for (i, (a, b)) in buf1.iter_mut().zip(buf2.iter_mut()).enumerate() {
            let v = (i as f32 * 0.01).sin() * 0.8;
            *a = v;
            *b = v;
        }
        c1.process(&mut buf1, 1, sr);
        c2.process(&mut buf2, 1, sr);
        assert_eq!(buf1, buf2);
    }

    #[test]
    fn limiter_clamps_hot_signal() {
        let sr = 48_000;
        let mut l = Limiter::new(sr);
        let mut buf = vec![3.0_f32; 4096];
        l.process(&mut buf, 1, sr);
        // After limiter at -1 dBFS = 0.891, peaks should not exceed
        // ~0.9 (with some envelope ramp).
        let peak_late: f32 = buf[2048..].iter().fold(0.0_f32, |a, b| a.max(b.abs()));
        assert!(peak_late < 1.0, "peak_late={peak_late}");
    }

    #[test]
    fn limiter_default_threshold_minus_one_db() {
        let l = Limiter::new(48_000);
        assert!((l.threshold_db() - -1.0).abs() < 0.1);
    }

    #[test]
    fn limiter_set_threshold() {
        let mut l = Limiter::new(48_000);
        l.set_threshold_db(-3.0);
        assert!((l.threshold_db() - -3.0).abs() < 0.1);
    }

    #[test]
    fn limiter_name() {
        let l = Limiter::new(48_000);
        assert_eq!(l.name(), "limiter");
    }

    #[test]
    fn compressor_name() {
        let c = Compressor::new(48_000);
        assert_eq!(c.name(), "compressor");
    }

    #[test]
    fn stereo_processing_independent_envelopes() {
        // L = hot, R = quiet → L compressed, R unaffected.
        let sr = 48_000;
        let mut c = Compressor::with_params(sr, -12.0, 4.0, 1.0, 50.0);
        let mut buf = vec![0.0_f32; 2048];
        for i in 0..1024 {
            buf[i * 2] = 1.0; // L hot
            buf[i * 2 + 1] = 0.05; // R quiet
        }
        c.process(&mut buf, 2, sr);
        // L should be compressed.
        let l_peak: f32 = (256..1024)
            .map(|i| buf[i * 2].abs())
            .fold(0.0_f32, f32::max);
        let r_peak: f32 = (256..1024)
            .map(|i| buf[i * 2 + 1].abs())
            .fold(0.0_f32, f32::max);
        assert!(l_peak < 0.95, "L_peak={l_peak} ; expected compressed");
        assert!(
            (r_peak - 0.05).abs() < 0.02,
            "R_peak={r_peak} ; expected ≈ 0.05"
        );
    }
}
