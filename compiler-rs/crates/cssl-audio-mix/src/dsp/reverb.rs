//! Schroeder reverberator — 4 parallel comb-filters + 2 series all-pass.
//!
//! § DESIGN
//!   The Schroeder design (Schroeder & Logan 1961) is the textbook
//!   "first synthetic reverb" : 4 comb filters in parallel produce a
//!   diffuse early-reflection cluster, then 2 all-pass filters in
//!   series smooth the comb's resonant ringing into a more natural-
//!   sounding tail.
//!
//!   The four comb-filter delay-line lengths are chosen as mutually-
//!   prime (or near-prime) numbers so the sum-of-combs avoids
//!   periodic-reinforcement artifacts. We use the Freeverb defaults :
//!     `comb_lengths = [1116, 1188, 1277, 1356]` samples @ 44.1 kHz
//!     `allpass_lengths = [225, 556]`           samples @ 44.1 kHz
//!   Scaled to the actual sample rate at construction.
//!
//! § PARAMETERS
//!   - `room_size`  ∈ [0, 1] — comb-filter feedback (0 = dry, 1 = ∞).
//!                              Defaults to 0.7 (medium room).
//!   - `damping`    ∈ [0, 1] — high-frequency rolloff inside the combs.
//!                              0 = bright, 1 = dark. Defaults to 0.5.
//!   - `wet_dry`    ∈ [0, 1] — wet/dry mix. 0 = dry only, 1 = wet only.
//!                              Defaults to 0.3.
//!
//! § STEREO HANDLING
//!   For stereo we run two independent reverb instances (one per
//!   channel) with slightly different delay-line lengths so the
//!   stereo image isn't collapsed. The mixer's two-channel mode
//!   decorrelates left + right by ±23-sample stereo-spread.

use crate::dsp::Effect;

/// Schroeder reverb — parallel-comb + series-allpass.
pub struct Reverb {
    /// Per-channel state — up to 8 channels (max channel count).
    channels: [ReverbChannel; 8],
    /// Active channel count. Set on first `process` based on input layout.
    active_channels: usize,
    /// Sample rate. Coefficients re-scale on rate change.
    sample_rate: u32,
    /// Room size (0..1).
    room_size: f32,
    /// Damping (0..1).
    damping: f32,
    /// Wet/dry mix (0..1).
    wet_dry: f32,
    /// Stereo spread in samples (left vs right delay-line offset).
    /// 0 = mono ; 23 = standard Freeverb stereo.
    stereo_spread: u32,
}

/// Per-channel reverb state — 4 combs + 2 all-pass.
struct ReverbChannel {
    combs: [CombFilter; 4],
    allpasses: [AllPass; 2],
}

impl Default for ReverbChannel {
    fn default() -> Self {
        Self {
            combs: [
                CombFilter::default(),
                CombFilter::default(),
                CombFilter::default(),
                CombFilter::default(),
            ],
            allpasses: [AllPass::default(), AllPass::default()],
        }
    }
}

/// Single comb filter — delay line + feedback + damping low-pass.
struct CombFilter {
    buffer: Vec<f32>,
    cursor: usize,
    feedback: f32,
    damp_state: f32,
    damp_coeff: f32,
}

impl Default for CombFilter {
    fn default() -> Self {
        Self {
            buffer: Vec::new(),
            cursor: 0,
            feedback: 0.5,
            damp_state: 0.0,
            damp_coeff: 0.2,
        }
    }
}

impl CombFilter {
    fn resize(&mut self, length: usize) {
        self.buffer.resize(length.max(1), 0.0);
        self.cursor = 0;
    }

    fn reset(&mut self) {
        for slot in &mut self.buffer {
            *slot = 0.0;
        }
        self.cursor = 0;
        self.damp_state = 0.0;
    }

    fn process_sample(&mut self, x: f32) -> f32 {
        let buf_len = self.buffer.len();
        if buf_len == 0 {
            return x;
        }
        let y = self.buffer[self.cursor];
        // 1-pole low-pass on the feedback path = the "damping" tone control.
        self.damp_state = y * (1.0 - self.damp_coeff) + self.damp_state * self.damp_coeff;
        let new = x + self.damp_state * self.feedback;
        self.buffer[self.cursor] = new;
        self.cursor = (self.cursor + 1) % buf_len;
        y
    }
}

/// Single all-pass filter — delay line + opposite-sign feedback +
/// feedforward.
struct AllPass {
    buffer: Vec<f32>,
    cursor: usize,
    feedback: f32,
}

impl Default for AllPass {
    fn default() -> Self {
        Self {
            buffer: Vec::new(),
            cursor: 0,
            feedback: 0.5,
        }
    }
}

impl AllPass {
    fn resize(&mut self, length: usize) {
        self.buffer.resize(length.max(1), 0.0);
        self.cursor = 0;
    }

    fn reset(&mut self) {
        for slot in &mut self.buffer {
            *slot = 0.0;
        }
        self.cursor = 0;
    }

    fn process_sample(&mut self, x: f32) -> f32 {
        let buf_len = self.buffer.len();
        if buf_len == 0 {
            return x;
        }
        let buffered = self.buffer[self.cursor];
        let new_in = x + buffered * self.feedback;
        self.buffer[self.cursor] = new_in;
        self.cursor = (self.cursor + 1) % buf_len;
        // All-pass classic : output = -input + buffered + feedback*x.
        // Equivalent textbook form : y = -x + (1-feedback²)*buffered.
        // We use the simpler "lattice" form from Freeverb.
        buffered - x * self.feedback
    }
}

impl Reverb {
    /// Standard comb-filter delay-line lengths (Freeverb defaults @ 44.1 kHz).
    const BASE_RATE: u32 = 44_100;
    const COMB_LENGTHS: [u32; 4] = [1116, 1188, 1277, 1356];
    const ALLPASS_LENGTHS: [u32; 2] = [225, 556];
    const DEFAULT_STEREO_SPREAD: u32 = 23;

    /// Construct a reverb with sensible defaults at the given sample rate.
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        let mut r = Self {
            channels: [
                ReverbChannel::default(),
                ReverbChannel::default(),
                ReverbChannel::default(),
                ReverbChannel::default(),
                ReverbChannel::default(),
                ReverbChannel::default(),
                ReverbChannel::default(),
                ReverbChannel::default(),
            ],
            active_channels: 0,
            sample_rate,
            room_size: 0.7,
            damping: 0.5,
            wet_dry: 0.3,
            stereo_spread: Self::DEFAULT_STEREO_SPREAD,
        };
        r.allocate_for_channels(2);
        r.update_parameters();
        r
    }

    /// Construct a reverb tuned for a specific room size + damping.
    #[must_use]
    pub fn with_params(sample_rate: u32, room_size: f32, damping: f32, wet_dry: f32) -> Self {
        let mut r = Self::new(sample_rate);
        r.set_room_size(room_size);
        r.set_damping(damping);
        r.set_wet_dry(wet_dry);
        r
    }

    /// Room size (0..1). Higher = longer reverb tail.
    #[must_use]
    pub const fn room_size(&self) -> f32 {
        self.room_size
    }

    /// Set room size (0..1). Clamps + recomputes feedback.
    pub fn set_room_size(&mut self, room_size: f32) {
        self.room_size = room_size.clamp(0.0, 1.0);
        self.update_parameters();
    }

    /// Damping (0..1). Higher = darker tail (more high-freq rolloff).
    #[must_use]
    pub const fn damping(&self) -> f32 {
        self.damping
    }

    /// Set damping (0..1). Clamps + recomputes per-comb damping coefficient.
    pub fn set_damping(&mut self, damping: f32) {
        self.damping = damping.clamp(0.0, 1.0);
        self.update_parameters();
    }

    /// Wet/dry mix (0..1). 0 = dry, 1 = wet.
    #[must_use]
    pub const fn wet_dry(&self) -> f32 {
        self.wet_dry
    }

    /// Set wet/dry (0..1).
    pub fn set_wet_dry(&mut self, wet_dry: f32) {
        self.wet_dry = wet_dry.clamp(0.0, 1.0);
    }

    /// Allocate buffers for `channels`. Must be called before any
    /// `process_sample` in tests + on first `process` from the Effect
    /// trait. Not called on the hot path.
    fn allocate_for_channels(&mut self, channels: usize) {
        let ch = channels.min(8);
        self.active_channels = ch;
        let scale = self.sample_rate as f32 / Self::BASE_RATE as f32;
        for ci in 0..ch {
            // Per-channel stereo spread — left = base, right = base+spread,
            // others use multiples to decorrelate further.
            let spread = if ci % 2 == 0 {
                0
            } else {
                self.stereo_spread
            };
            for (k, length) in Self::COMB_LENGTHS.iter().enumerate() {
                let scaled = ((*length as f32 * scale) as u32 + spread) as usize;
                self.channels[ci].combs[k].resize(scaled);
            }
            for (k, length) in Self::ALLPASS_LENGTHS.iter().enumerate() {
                let scaled = ((*length as f32 * scale) as u32 + spread) as usize;
                self.channels[ci].allpasses[k].resize(scaled);
            }
        }
    }

    /// Update per-comb feedback + damping coefficients from
    /// `(room_size, damping)`.
    fn update_parameters(&mut self) {
        // Freeverb-derived feedback curve : feedback = 0.7 + room_size * 0.28.
        // Bounded below 1 to keep the system stable.
        let feedback = (0.7 + self.room_size * 0.28).min(0.98);
        let damp_coeff = self.damping;
        for ch in &mut self.channels {
            for c in &mut ch.combs {
                c.feedback = feedback;
                c.damp_coeff = damp_coeff;
            }
            for ap in &mut ch.allpasses {
                ap.feedback = 0.5;
            }
        }
    }

    /// Process a single sample for a given channel.
    fn process_sample_channel(&mut self, channel: usize, x: f32) -> f32 {
        let ch = channel.min(self.active_channels.saturating_sub(1));
        let mut wet = 0.0;
        for c in &mut self.channels[ch].combs {
            wet += c.process_sample(x);
        }
        wet *= 0.25; // average the 4 combs
        for ap in &mut self.channels[ch].allpasses {
            wet = ap.process_sample(wet);
        }
        x * (1.0 - self.wet_dry) + wet * self.wet_dry
    }
}

impl Effect for Reverb {
    fn process(&mut self, buffer: &mut [f32], channels: usize, sample_rate: u32) {
        let ch = channels.min(8);
        if ch == 0 {
            return;
        }
        if ch != self.active_channels || sample_rate != self.sample_rate {
            self.sample_rate = sample_rate;
            self.allocate_for_channels(ch);
            self.update_parameters();
        }
        let frames = buffer.len() / ch;
        for i in 0..frames {
            for c in 0..ch {
                let idx = i * ch + c;
                let x = buffer[idx];
                buffer[idx] = self.process_sample_channel(c, x);
            }
        }
    }

    fn reset(&mut self) {
        for ch in &mut self.channels {
            for c in &mut ch.combs {
                c.reset();
            }
            for ap in &mut ch.allpasses {
                ap.reset();
            }
        }
    }

    fn name(&self) -> &'static str {
        "reverb"
    }
}

impl core::fmt::Debug for Reverb {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Reverb")
            .field("room_size", &self.room_size)
            .field("damping", &self.damping)
            .field("wet_dry", &self.wet_dry)
            .field("sample_rate", &self.sample_rate)
            .field("stereo_spread", &self.stereo_spread)
            .field("active_channels", &self.active_channels)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn new_default_parameters() {
        let r = Reverb::new(48_000);
        assert!((r.room_size() - 0.7).abs() < 1e-6);
        assert!((r.damping() - 0.5).abs() < 1e-6);
        assert!((r.wet_dry() - 0.3).abs() < 1e-6);
    }

    #[test]
    fn with_params_sets_all() {
        let r = Reverb::with_params(48_000, 0.5, 0.2, 0.4);
        assert_eq!(r.room_size(), 0.5);
        assert_eq!(r.damping(), 0.2);
        assert_eq!(r.wet_dry(), 0.4);
    }

    #[test]
    fn room_size_clamps_above_one() {
        let mut r = Reverb::new(48_000);
        r.set_room_size(2.0);
        assert_eq!(r.room_size(), 1.0);
    }

    #[test]
    fn room_size_clamps_below_zero() {
        let mut r = Reverb::new(48_000);
        r.set_room_size(-1.0);
        assert_eq!(r.room_size(), 0.0);
    }

    #[test]
    fn damping_clamps_to_unit_range() {
        let mut r = Reverb::new(48_000);
        r.set_damping(2.0);
        assert_eq!(r.damping(), 1.0);
        r.set_damping(-0.5);
        assert_eq!(r.damping(), 0.0);
    }

    #[test]
    fn wet_dry_clamps_to_unit_range() {
        let mut r = Reverb::new(48_000);
        r.set_wet_dry(2.0);
        assert_eq!(r.wet_dry(), 1.0);
    }

    #[test]
    fn dry_mix_passes_signal_unchanged() {
        // wet_dry = 0 → dry signal exits unchanged.
        let sr = 48_000;
        let mut r = Reverb::with_params(sr, 0.5, 0.5, 0.0);
        let mut buf = vec![0.5_f32; 256];
        let input = buf.clone();
        r.process(&mut buf, 1, sr);
        for (a, b) in input.iter().zip(buf.iter()) {
            assert!((a - b).abs() < 1e-6, "dry path altered : {a} vs {b}");
        }
    }

    #[test]
    fn impulse_produces_decaying_tail() {
        // Single impulse → reverb tail with non-zero late energy.
        let sr = 48_000;
        let mut r = Reverb::with_params(sr, 0.7, 0.3, 1.0); // wet only
        let mut buf = vec![0.0_f32; 8192];
        buf[0] = 1.0;
        r.process(&mut buf, 1, sr);
        // Late tail should have non-zero energy.
        let tail_energy: f32 = buf[2048..].iter().map(|x| x * x).sum();
        assert!(tail_energy > 1e-4, "tail_energy={tail_energy} ; expected > 0");
    }

    #[test]
    fn reset_clears_state() {
        let sr = 48_000;
        let mut r = Reverb::new(sr);
        let mut buf = vec![1.0_f32; 1024];
        r.process(&mut buf, 1, sr);
        r.reset();
        // After reset, processing zeros gives zeros.
        let mut zeros = vec![0.0_f32; 1024];
        r.process(&mut zeros, 1, sr);
        for s in &zeros {
            assert!(s.abs() < 1e-6, "reset failure : {s}");
        }
    }

    #[test]
    fn determinism_same_input_same_output() {
        let sr = 48_000;
        let mut r1 = Reverb::new(sr);
        let mut r2 = Reverb::new(sr);
        let mut buf1 = vec![0.0_f32; 256];
        let mut buf2 = vec![0.0_f32; 256];
        for (i, (a, b)) in buf1.iter_mut().zip(buf2.iter_mut()).enumerate() {
            let v = (i as f32 * 0.01).sin();
            *a = v;
            *b = v;
        }
        r1.process(&mut buf1, 1, sr);
        r2.process(&mut buf2, 1, sr);
        assert_eq!(buf1, buf2);
    }

    #[test]
    fn stereo_decorrelates_channels() {
        // Mono impulse on both channels → reverb should produce
        // different left+right tails (stereo spread). The smallest
        // comb-filter length is ≈ 1116 samples × scale@48k ≈ 1215 samples,
        // so we measure at indices well past that.
        let sr = 48_000;
        let mut r = Reverb::with_params(sr, 0.7, 0.3, 1.0);
        let mut buf = vec![0.0_f32; 16_384];
        buf[0] = 1.0; // L impulse
        buf[1] = 1.0; // R impulse
        r.process(&mut buf, 2, sr);
        // Compare L + R tails after the comb-filter loop has populated.
        let mut diff_count = 0;
        let frames = buf.len() / 2;
        for i in 1500..frames {
            if (buf[i * 2] - buf[i * 2 + 1]).abs() > 1e-6 {
                diff_count += 1;
            }
        }
        assert!(
            diff_count > 100,
            "stereo not decorrelated : diff_count={diff_count} (out of {})",
            frames - 1500
        );
    }

    #[test]
    fn empty_buffer_no_panic() {
        let sr = 48_000;
        let mut r = Reverb::new(sr);
        let mut buf: Vec<f32> = vec![];
        r.process(&mut buf, 2, sr);
    }

    #[test]
    fn name_reverb() {
        let r = Reverb::new(48_000);
        assert_eq!(r.name(), "reverb");
    }

    #[test]
    fn higher_room_size_longer_tail() {
        // Compare tail energy at room_size = 0.2 vs 0.9.
        let sr = 48_000;
        let mut small = Reverb::with_params(sr, 0.2, 0.5, 1.0);
        let mut large = Reverb::with_params(sr, 0.9, 0.5, 1.0);
        let mut buf_s = vec![0.0_f32; 8192];
        let mut buf_l = vec![0.0_f32; 8192];
        buf_s[0] = 1.0;
        buf_l[0] = 1.0;
        small.process(&mut buf_s, 1, sr);
        large.process(&mut buf_l, 1, sr);
        let energy_s: f32 = buf_s[4096..].iter().map(|x| x * x).sum();
        let energy_l: f32 = buf_l[4096..].iter().map(|x| x * x).sum();
        assert!(energy_l > energy_s, "larger room should have longer tail");
    }
}
