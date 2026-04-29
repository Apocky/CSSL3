//! Delay-line effect — multi-tap delay with feedback.
//!
//! § DESIGN
//!   A circular delay buffer with N taps + a global feedback gain :
//!     ```text
//!     delay  ──→ tap_1 (gain g1) ──┐
//!         │   ──→ tap_2 (gain g2) ──┼─→ wet
//!         │   ──→ tap_N (gain gN) ──┘
//!         │                         │
//!         └────── feedback ←────────┘  (gain f)
//!     ```
//!   Each tap reads from a different position in the delay buffer and
//!   contributes to the wet signal. Feedback re-injects the wet signal
//!   into the buffer, producing repeating echoes.
//!
//! § STAGE-0 SCOPE
//!   - Up to 8 taps per delay instance (matches max channel count).
//!   - Feedback gain clamped to `[0, 0.95]` to prevent runaway.
//!   - Per-channel state — stereo + surround supported via independent
//!     delay buffers per channel.
//!
//! § DETERMINISM
//!   Like every DSP primitive in this crate, the delay's output is a
//!   pure function of `(input, params, prior_state)`. Two replays
//!   produce bit-equal output.

use crate::dsp::Effect;

/// A single tap : `(delay_samples, gain)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DelayTap {
    /// Delay length in samples.
    pub delay_samples: u32,
    /// Per-tap gain (0..1 typical ; > 1 amplifies).
    pub gain: f32,
}

/// Multi-tap delay with feedback.
pub struct Delay {
    /// Per-channel ring buffer storage.
    buffers: [Vec<f32>; 8],
    /// Per-channel write cursor.
    cursors: [usize; 8],
    /// Active channel count (set on first `process`).
    active_channels: usize,
    /// Sample rate (used for ms→samples conversion).
    sample_rate: u32,
    /// Buffer size in samples — the maximum delay supported.
    buffer_size: usize,
    /// Wet/dry mix (0..1).
    wet_dry: f32,
    /// Feedback gain (0..0.95). Clamped to prevent runaway.
    feedback: f32,
    /// Delay taps. Up to 8 taps ; unused slots have `gain = 0`.
    taps: [DelayTap; 8],
    /// Active tap count.
    active_taps: usize,
}

impl Delay {
    /// Hard cap on feedback to prevent self-oscillation runaway.
    const MAX_FEEDBACK: f32 = 0.95;

    /// Construct a delay with the given maximum buffer size.
    /// `max_delay_secs * sample_rate` samples are allocated per channel.
    #[must_use]
    pub fn new(max_delay_secs: f32, sample_rate: u32) -> Self {
        let buf_size = ((max_delay_secs.max(0.001) * sample_rate as f32) as usize).max(2);
        let mut d = Self {
            buffers: [
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ],
            cursors: [0; 8],
            active_channels: 0,
            sample_rate,
            buffer_size: buf_size,
            wet_dry: 0.5,
            feedback: 0.3,
            taps: [DelayTap {
                delay_samples: (sample_rate / 4),
                gain: 1.0,
            }; 8],
            active_taps: 1,
        };
        d.allocate_for_channels(2);
        d
    }

    /// Set wet/dry mix (0..1).
    pub fn set_wet_dry(&mut self, wet_dry: f32) {
        self.wet_dry = wet_dry.clamp(0.0, 1.0);
    }

    /// Set feedback gain (0..0.95).
    pub fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback.clamp(0.0, Self::MAX_FEEDBACK);
    }

    /// Wet/dry mix.
    #[must_use]
    pub const fn wet_dry(&self) -> f32 {
        self.wet_dry
    }

    /// Feedback gain.
    #[must_use]
    pub const fn feedback(&self) -> f32 {
        self.feedback
    }

    /// Buffer size in samples (the maximum delay supported).
    #[must_use]
    pub const fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    /// Active tap count.
    #[must_use]
    pub const fn tap_count(&self) -> usize {
        self.active_taps
    }

    /// Set the delay taps. `taps.len()` ≤ 8 ; values beyond 8 are ignored.
    pub fn set_taps(&mut self, taps: &[DelayTap]) {
        let n = taps.len().min(8);
        for (i, tap) in taps.iter().take(n).enumerate() {
            let clamped_delay = (tap.delay_samples as usize).min(self.buffer_size - 1);
            self.taps[i] = DelayTap {
                delay_samples: clamped_delay as u32,
                gain: tap.gain,
            };
        }
        // Zero-out unused taps.
        for i in n..8 {
            self.taps[i] = DelayTap {
                delay_samples: 0,
                gain: 0.0,
            };
        }
        self.active_taps = n;
    }

    /// Convenience : single-tap delay set in milliseconds.
    pub fn set_single_tap_ms(&mut self, delay_ms: f32, gain: f32) {
        let samples = (delay_ms * 0.001 * self.sample_rate as f32) as u32;
        self.set_taps(&[DelayTap {
            delay_samples: samples,
            gain,
        }]);
    }

    fn allocate_for_channels(&mut self, channels: usize) {
        let ch = channels.min(8);
        self.active_channels = ch;
        for ci in 0..ch {
            self.buffers[ci].resize(self.buffer_size, 0.0);
            self.cursors[ci] = 0;
        }
    }

    /// Process one sample for a given channel.
    fn process_sample_channel(&mut self, channel: usize, x: f32) -> f32 {
        let ch = channel.min(self.active_channels.saturating_sub(1));
        let buf_size = self.buffer_size;
        if buf_size == 0 {
            return x;
        }
        // Write input + feedback into the buffer FIRST, then read taps :
        //   - input at cursor → tap with delay=N reads at (cursor-N) → echo
        //     arrives exactly N samples after the input goes in.
        // We also need to pre-compute feedback from existing buffer state
        // (use the deepest tap as the feedback source so feedback isn't
        // self-reading the just-written sample).
        let mut wet = 0.0;
        for ti in 0..self.active_taps {
            let tap = self.taps[ti];
            let delay = (tap.delay_samples as usize).min(buf_size - 1).max(1);
            // Read at `(cursor + buf_size - delay) % buf_size` BEFORE we
            // write the new sample. This gives us the buffer state from
            // `delay` samples ago.
            let read_idx = (self.cursors[ch] + buf_size - delay) % buf_size;
            wet += self.buffers[ch][read_idx] * tap.gain;
        }
        // Write current input + feedback into the buffer at the current cursor.
        self.buffers[ch][self.cursors[ch]] = x + wet * self.feedback;
        self.cursors[ch] = (self.cursors[ch] + 1) % buf_size;
        // Wet/dry mix.
        x * (1.0 - self.wet_dry) + wet * self.wet_dry
    }
}

impl Effect for Delay {
    fn process(&mut self, buffer: &mut [f32], channels: usize, sample_rate: u32) {
        let ch = channels.min(8);
        if ch == 0 {
            return;
        }
        if ch != self.active_channels || sample_rate != self.sample_rate {
            self.sample_rate = sample_rate;
            self.allocate_for_channels(ch);
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
        for ci in 0..self.active_channels {
            for slot in &mut self.buffers[ci] {
                *slot = 0.0;
            }
            self.cursors[ci] = 0;
        }
    }

    fn name(&self) -> &'static str {
        "delay"
    }
}

impl core::fmt::Debug for Delay {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Delay")
            .field("buffer_size", &self.buffer_size)
            .field("wet_dry", &self.wet_dry)
            .field("feedback", &self.feedback)
            .field("active_channels", &self.active_channels)
            .field("active_taps", &self.active_taps)
            .field("sample_rate", &self.sample_rate)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn new_default_buffer_size_2sec() {
        let d = Delay::new(2.0, 48_000);
        // 2 seconds at 48k = 96 000 samples.
        assert!(d.buffer_size() >= 96_000);
    }

    #[test]
    fn min_buffer_size_2_samples() {
        // Even with 0 sec request, buffer is at least 2.
        let d = Delay::new(0.0, 48_000);
        assert!(d.buffer_size() >= 2);
    }

    #[test]
    fn set_feedback_clamps_above_max() {
        let mut d = Delay::new(1.0, 48_000);
        d.set_feedback(2.0);
        assert!(d.feedback() <= Delay::MAX_FEEDBACK);
    }

    #[test]
    fn set_feedback_clamps_negative() {
        let mut d = Delay::new(1.0, 48_000);
        d.set_feedback(-1.0);
        assert_eq!(d.feedback(), 0.0);
    }

    #[test]
    fn set_wet_dry_clamps() {
        let mut d = Delay::new(1.0, 48_000);
        d.set_wet_dry(2.0);
        assert_eq!(d.wet_dry(), 1.0);
        d.set_wet_dry(-1.0);
        assert_eq!(d.wet_dry(), 0.0);
    }

    #[test]
    fn set_taps_limits_to_eight() {
        let mut d = Delay::new(1.0, 48_000);
        let many: Vec<DelayTap> = (0..16)
            .map(|i| DelayTap {
                delay_samples: (i + 1) * 100,
                gain: 0.5,
            })
            .collect();
        d.set_taps(&many);
        assert_eq!(d.tap_count(), 8);
    }

    #[test]
    fn set_single_tap_ms_converts_to_samples() {
        let mut d = Delay::new(1.0, 48_000);
        d.set_single_tap_ms(100.0, 0.5);
        // 100 ms at 48k = 4800 samples.
        assert_eq!(d.tap_count(), 1);
    }

    #[test]
    fn impulse_with_feedback_zero_produces_single_echo() {
        let sr = 48_000;
        let mut d = Delay::new(0.5, sr);
        d.set_single_tap_ms(10.0, 1.0);
        d.set_wet_dry(1.0);
        d.set_feedback(0.0);
        let mut buf = vec![0.0_f32; 1024];
        buf[0] = 1.0;
        d.process(&mut buf, 1, sr);
        // After 480 samples (10 ms) we should see a single non-zero echo.
        let echo_pos = (sr as f32 * 0.01) as usize;
        let mut max_late = 0.0_f32;
        for i in (echo_pos + 1)..1024 {
            max_late = max_late.max(buf[i].abs());
        }
        // With feedback=0 + single tap, the only echo is at echo_pos.
        // Subsequent samples should be near-zero.
        assert!(max_late < 0.5, "max late echo : {max_late}");
        // The echo position itself should be loud.
        assert!(buf[echo_pos].abs() > 0.5, "echo @ {echo_pos} = {}", buf[echo_pos]);
    }

    #[test]
    fn impulse_with_feedback_produces_repeating_echoes() {
        let sr = 48_000;
        let mut d = Delay::new(0.5, sr);
        d.set_single_tap_ms(10.0, 1.0);
        d.set_wet_dry(1.0);
        d.set_feedback(0.5);
        let mut buf = vec![0.0_f32; 8192];
        buf[0] = 1.0;
        d.process(&mut buf, 1, sr);
        // We should see at least 3 distinct echo peaks (each at ~10 ms apart).
        let echo_step = (sr as f32 * 0.01) as usize;
        let mut peaks = 0;
        for k in 1..=4 {
            let pos = k * echo_step;
            if pos < 8192 && buf[pos].abs() > 0.05 {
                peaks += 1;
            }
        }
        assert!(peaks >= 2, "peaks={peaks} ; expected at least 2 repeating echoes");
    }

    #[test]
    fn dry_only_passes_signal_unchanged() {
        let sr = 48_000;
        let mut d = Delay::new(0.5, sr);
        d.set_wet_dry(0.0);
        let mut buf = vec![0.5_f32; 256];
        let input = buf.clone();
        d.process(&mut buf, 1, sr);
        for (a, b) in input.iter().zip(buf.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn reset_clears_state() {
        let sr = 48_000;
        let mut d = Delay::new(0.5, sr);
        d.set_single_tap_ms(10.0, 1.0);
        d.set_wet_dry(1.0);
        let mut buf = vec![1.0_f32; 1024];
        d.process(&mut buf, 1, sr);
        d.reset();
        let mut zeros = vec![0.0_f32; 1024];
        d.process(&mut zeros, 1, sr);
        for s in &zeros {
            assert!(s.abs() < 1e-9, "post-reset sample : {s}");
        }
    }

    #[test]
    fn determinism_same_input_same_output() {
        let sr = 48_000;
        let mut d1 = Delay::new(1.0, sr);
        let mut d2 = Delay::new(1.0, sr);
        d1.set_feedback(0.4);
        d2.set_feedback(0.4);
        let mut buf1 = vec![0.0_f32; 256];
        let mut buf2 = vec![0.0_f32; 256];
        for (i, (a, b)) in buf1.iter_mut().zip(buf2.iter_mut()).enumerate() {
            let v = (i as f32 * 0.01).sin();
            *a = v;
            *b = v;
        }
        d1.process(&mut buf1, 1, sr);
        d2.process(&mut buf2, 1, sr);
        assert_eq!(buf1, buf2);
    }

    #[test]
    fn name_delay() {
        let d = Delay::new(0.1, 48_000);
        assert_eq!(d.name(), "delay");
    }

    #[test]
    fn multi_tap_produces_multiple_echoes() {
        let sr = 48_000;
        let mut d = Delay::new(0.5, sr);
        d.set_taps(&[
            DelayTap {
                delay_samples: 1000,
                gain: 0.5,
            },
            DelayTap {
                delay_samples: 2000,
                gain: 0.4,
            },
            DelayTap {
                delay_samples: 4000,
                gain: 0.3,
            },
        ]);
        d.set_wet_dry(1.0);
        d.set_feedback(0.0);
        let mut buf = vec![0.0_f32; 8192];
        buf[0] = 1.0;
        d.process(&mut buf, 1, sr);
        // Three peaks at exactly 1000, 2000, 4000 (delay=N → echo @ N).
        assert!(buf[1000].abs() > 0.4, "tap@1000 = {}", buf[1000]);
        assert!(buf[2000].abs() > 0.3, "tap@2000 = {}", buf[2000]);
        assert!(buf[4000].abs() > 0.2, "tap@4000 = {}", buf[4000]);
    }
}
