//! Bounded ring buffer for audio sample streaming.
//!
//! § DESIGN
//!   A power-of-two-sized SPSC (single-producer single-consumer) ring
//!   for `f32` samples. The producer is the application's render fn
//!   (which calls `submit_frames` on the AudioStream) ; the consumer
//!   is the platform layer (WASAPI / ALSA / CoreAudio) which drains
//!   the ring into the device buffer.
//!
//!   The ring tracks two cursors :
//!     - `head` — write index ; advances when the producer pushes samples.
//!     - `tail` — read index ; advances when the consumer pulls samples.
//!   The mask `(capacity - 1)` masks indices ; capacity must be a power
//!   of two for the mask trick to work.
//!
//! § DETERMINISTIC LATENCY
//!   The ring's capacity defines the worst-case latency budget. At
//!   48 kHz stereo with a 256-frame ring, the latency budget is
//!   `256 frames / 48000 Hz = 5.3ms`. Default config matches this.
//!   Callers building larger buffers can pass `RingBufferConfig`
//!   with `frame_capacity` overridden.
//!
//! § STAGE-0 SCOPE
//!   The ring is not lock-free at stage-0 — it relies on `&mut self`
//!   for both push + pop. A future slice will introduce atomic head
//!   + tail cursors so the producer thread + consumer thread can run
//!   concurrently without a mutex. At stage-0 the platform layer
//!   serializes access via the AudioStream's internal lock.

use crate::error::{AudioError, Result};
use crate::format::AudioFormat;

/// Configuration for the ring buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingBufferConfig {
    /// Capacity in *frames* (samples = frames × channels).
    pub frame_capacity: usize,
}

impl RingBufferConfig {
    /// Default 256-frame capacity = 5.3ms latency budget at 48 kHz.
    #[must_use]
    pub const fn default_latency() -> Self {
        Self {
            frame_capacity: 256,
        }
    }

    /// Build a config with a specific frame-capacity. Capacity is
    /// rounded UP to the next power of two for the index-mask trick.
    /// Returns `InvalidArgument` for capacity < 16 (smaller is not
    /// useful) or > 65_536 (4 channels × 65k = 256k samples = 1MB).
    pub fn new(frame_capacity: usize) -> Result<Self> {
        if frame_capacity < 16 {
            return Err(AudioError::invalid(
                "RingBufferConfig::new",
                format!("frame_capacity {frame_capacity} below minimum 16"),
            ));
        }
        if frame_capacity > 65_536 {
            return Err(AudioError::invalid(
                "RingBufferConfig::new",
                format!("frame_capacity {frame_capacity} above maximum 65536"),
            ));
        }
        let rounded = frame_capacity.next_power_of_two();
        Ok(Self {
            frame_capacity: rounded,
        })
    }

    /// Latency budget at the given sample rate (in milliseconds).
    /// `frame_capacity` is bounded ≤ 65536 (audio-domain sane), so the
    /// usize→f64 cast cannot lose precision in practice — but clippy
    /// can't see the bound, so the allow is local + documented.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn latency_ms(&self, sample_rate_hz: u32) -> f32 {
        if sample_rate_hz == 0 {
            return 0.0;
        }
        let frames = self.frame_capacity as f64;
        let rate = f64::from(sample_rate_hz);
        ((frames * 1000.0) / rate) as f32
    }
}

impl Default for RingBufferConfig {
    fn default() -> Self {
        Self::default_latency()
    }
}

/// SPSC ring buffer for f32 samples.
///
/// At stage-0 the ring uses a single mutable-reference protocol —
/// callers must synchronize externally if producer + consumer run on
/// different threads. The AudioStream layer serializes access.
pub struct RingBuffer {
    /// Sample storage. Length = `frame_capacity * channels`.
    storage: Vec<f32>,
    /// Channel count (samples per frame).
    channels: usize,
    /// Capacity in frames (always a power of two).
    frame_capacity: usize,
    /// Sample mask (`frame_capacity * channels - 1`).
    sample_mask: usize,
    /// Write cursor (sample index ; wrapped via `sample_mask`).
    head: usize,
    /// Read cursor (sample index ; wrapped via `sample_mask`).
    tail: usize,
    /// Number of frames currently buffered (head - tail in frames).
    fill_frames: usize,
}

impl RingBuffer {
    /// Build a new ring sized for `format` × `config`.
    pub fn new(format: AudioFormat, config: RingBufferConfig) -> Result<Self> {
        let frame_capacity = config.frame_capacity;
        if !frame_capacity.is_power_of_two() {
            return Err(AudioError::invalid(
                "RingBuffer::new",
                format!("frame_capacity {frame_capacity} not a power of two"),
            ));
        }
        let channels = format.layout.channel_count() as usize;
        let total_samples = frame_capacity
            .checked_mul(channels)
            .ok_or_else(|| AudioError::invalid("RingBuffer::new", "size overflow"))?;
        Ok(Self {
            storage: vec![0.0; total_samples],
            channels,
            frame_capacity,
            sample_mask: total_samples - 1,
            head: 0,
            tail: 0,
            fill_frames: 0,
        })
    }

    /// Capacity in frames.
    #[must_use]
    pub const fn frame_capacity(&self) -> usize {
        self.frame_capacity
    }

    /// Channel count (samples per frame).
    #[must_use]
    pub const fn channels(&self) -> usize {
        self.channels
    }

    /// Capacity in samples = `frame_capacity * channels`.
    #[must_use]
    pub const fn sample_capacity(&self) -> usize {
        self.frame_capacity * self.channels
    }

    /// Frames currently buffered (head - tail).
    #[must_use]
    pub const fn fill_frames(&self) -> usize {
        self.fill_frames
    }

    /// Free space in frames.
    #[must_use]
    pub const fn free_frames(&self) -> usize {
        self.frame_capacity - self.fill_frames
    }

    /// Is the ring empty ?
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.fill_frames == 0
    }

    /// Is the ring full ?
    #[must_use]
    pub const fn is_full(&self) -> bool {
        self.fill_frames == self.frame_capacity
    }

    /// Push `samples` into the ring. Returns the number of *frames*
    /// actually accepted (may be less than `samples.len() / channels`
    /// if the ring fills before all samples fit).
    ///
    /// Panics if `samples.len() % channels != 0`.
    pub fn push(&mut self, samples: &[f32]) -> usize {
        assert!(
            samples.len() % self.channels == 0,
            "push : samples.len() {} not divisible by channels {}",
            samples.len(),
            self.channels
        );
        let frames_in = samples.len() / self.channels;
        let frames_to_write = frames_in.min(self.free_frames());
        let samples_to_write = frames_to_write * self.channels;
        for (i, sample) in samples.iter().enumerate().take(samples_to_write) {
            let idx = (self.head + i) & self.sample_mask;
            self.storage[idx] = *sample;
        }
        self.head = (self.head + samples_to_write) & self.sample_mask;
        self.fill_frames += frames_to_write;
        frames_to_write
    }

    /// Pop frames from the ring into `out`. Returns the number of
    /// *frames* actually drained.
    ///
    /// Panics if `out.len() % channels != 0`.
    pub fn pop(&mut self, out: &mut [f32]) -> usize {
        assert!(
            out.len() % self.channels == 0,
            "pop : out.len() {} not divisible by channels {}",
            out.len(),
            self.channels
        );
        let frames_requested = out.len() / self.channels;
        let frames_to_read = frames_requested.min(self.fill_frames);
        let samples_to_read = frames_to_read * self.channels;
        for (i, slot) in out.iter_mut().enumerate().take(samples_to_read) {
            let idx = (self.tail + i) & self.sample_mask;
            *slot = self.storage[idx];
        }
        self.tail = (self.tail + samples_to_read) & self.sample_mask;
        self.fill_frames -= frames_to_read;
        frames_to_read
    }

    /// Drain `frame_count` frames into `out`, filling missing frames
    /// with silence (0.0). Returns the number of *real* frames drained
    /// (the rest are silence). This is the underrun-fill path used by
    /// platform layers when the ring underflows ; the caller is
    /// expected to record an `AudioEvent::Underrun` when the returned
    /// count < `frame_count`.
    ///
    /// Panics if `out.len() != frame_count * channels`.
    pub fn drain_with_silence(&mut self, out: &mut [f32], frame_count: usize) -> usize {
        let expected_len = frame_count * self.channels;
        assert!(
            out.len() == expected_len,
            "drain_with_silence : out.len() {} != frame_count {} * channels {}",
            out.len(),
            frame_count,
            self.channels
        );
        let frames_drained = self.pop(out);
        // Zero out the remainder.
        let drained_samples = frames_drained * self.channels;
        for sample in out.iter_mut().skip(drained_samples) {
            *sample = 0.0;
        }
        frames_drained
    }

    /// Reset the ring (drop all buffered data).
    pub const fn reset(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.fill_frames = 0;
    }
}

impl core::fmt::Debug for RingBuffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RingBuffer")
            .field("frame_capacity", &self.frame_capacity)
            .field("channels", &self.channels)
            .field("fill_frames", &self.fill_frames)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // ring stores + returns f32 verbatim ; bit-equality is intentional
#[allow(clippy::cast_precision_loss)] // small int → f32 in test fixtures
mod tests {
    use super::*;

    #[test]
    fn config_default_is_256_frames() {
        let cfg = RingBufferConfig::default();
        assert_eq!(cfg.frame_capacity, 256);
    }

    #[test]
    fn config_latency_at_48k_is_5_3ms() {
        let cfg = RingBufferConfig::default_latency();
        let latency = cfg.latency_ms(48_000);
        assert!((latency - 5.333).abs() < 0.01, "latency={latency}");
    }

    #[test]
    fn config_rounds_up_to_pow2() {
        let cfg = RingBufferConfig::new(200).expect("valid");
        assert_eq!(cfg.frame_capacity, 256);
    }

    #[test]
    fn config_already_pow2_unchanged() {
        let cfg = RingBufferConfig::new(512).expect("valid");
        assert_eq!(cfg.frame_capacity, 512);
    }

    #[test]
    fn config_rejects_below_min() {
        assert!(RingBufferConfig::new(8).is_err());
    }

    #[test]
    fn config_rejects_above_max() {
        assert!(RingBufferConfig::new(1_000_000).is_err());
    }

    #[test]
    fn ring_new_initial_state() {
        let r = RingBuffer::new(AudioFormat::default(), RingBufferConfig::default()).unwrap();
        assert_eq!(r.frame_capacity(), 256);
        assert_eq!(r.channels(), 2);
        assert_eq!(r.sample_capacity(), 512);
        assert!(r.is_empty());
        assert!(!r.is_full());
        assert_eq!(r.fill_frames(), 0);
        assert_eq!(r.free_frames(), 256);
    }

    #[test]
    fn ring_push_advances_head_and_fill() {
        let mut r =
            RingBuffer::new(AudioFormat::default(), RingBufferConfig::new(64).unwrap()).unwrap();
        let samples = vec![0.5_f32; 16]; // 16 samples = 8 stereo frames
        let pushed = r.push(&samples);
        assert_eq!(pushed, 8);
        assert_eq!(r.fill_frames(), 8);
        assert_eq!(r.free_frames(), 64 - 8);
    }

    #[test]
    fn ring_push_over_capacity_caps_at_capacity() {
        let mut r =
            RingBuffer::new(AudioFormat::default(), RingBufferConfig::new(64).unwrap()).unwrap();
        // Push 100 stereo frames into a 64-frame ring.
        let samples = vec![0.5_f32; 200];
        let pushed = r.push(&samples);
        assert_eq!(pushed, 64);
        assert!(r.is_full());
    }

    #[test]
    fn ring_pop_drains_in_fifo_order() {
        let mut r =
            RingBuffer::new(AudioFormat::default(), RingBufferConfig::new(64).unwrap()).unwrap();
        let samples: Vec<f32> = (0..16).map(|i| i as f32).collect();
        r.push(&samples);
        let mut out = vec![0.0_f32; 16];
        let drained = r.pop(&mut out);
        assert_eq!(drained, 8);
        assert_eq!(out, samples);
        assert!(r.is_empty());
    }

    #[test]
    fn ring_pop_more_than_buffered_returns_partial() {
        let mut r =
            RingBuffer::new(AudioFormat::default(), RingBufferConfig::new(64).unwrap()).unwrap();
        r.push(&[0.5_f32; 8]); // 4 frames
        let mut out = vec![0.0_f32; 16];
        let drained = r.pop(&mut out);
        assert_eq!(drained, 4);
        // First 8 samples = the data ; rest = unchanged 0.0.
        for sample in out.iter().take(8) {
            assert_eq!(*sample, 0.5);
        }
        for sample in out.iter().skip(8) {
            assert_eq!(*sample, 0.0);
        }
    }

    #[test]
    fn ring_drain_with_silence_zero_fills_remainder() {
        let mut r =
            RingBuffer::new(AudioFormat::default(), RingBufferConfig::new(64).unwrap()).unwrap();
        r.push(&[1.0_f32; 8]); // 4 frames
        let mut out = vec![0.7_f32; 16]; // Pre-fill non-zero to verify zeroing.
        let drained = r.drain_with_silence(&mut out, 8);
        assert_eq!(drained, 4);
        // First 8 = 1.0, rest = 0.0.
        for sample in out.iter().take(8) {
            assert_eq!(*sample, 1.0);
        }
        for sample in out.iter().skip(8) {
            assert_eq!(*sample, 0.0);
        }
    }

    #[test]
    fn ring_wraparound_works_correctly() {
        let mut r =
            RingBuffer::new(AudioFormat::default(), RingBufferConfig::new(16).unwrap()).unwrap();
        // 16-frame ring × 2 channels = 32 sample storage, 5-bit mask.
        let pat1 = vec![1.0_f32; 24]; // 12 frames
        r.push(&pat1);
        let mut out1 = vec![0.0_f32; 16]; // 8 frames
        r.pop(&mut out1);
        // Now head=24 (sample), tail=16, fill=4 frames.
        let pat2 = vec![2.0_f32; 16]; // 8 frames — wraps around past index 32.
        let pushed = r.push(&pat2);
        assert_eq!(pushed, 8);
        // Drain everything : should be 4 frames of 1.0 + 8 frames of 2.0.
        let mut out2 = vec![0.0_f32; 24];
        let drained = r.pop(&mut out2);
        assert_eq!(drained, 12);
        for sample in out2.iter().take(8) {
            assert_eq!(*sample, 1.0);
        }
        for sample in out2.iter().skip(8).take(16) {
            assert_eq!(*sample, 2.0);
        }
    }

    #[test]
    fn ring_reset_clears_state() {
        let mut r =
            RingBuffer::new(AudioFormat::default(), RingBufferConfig::new(64).unwrap()).unwrap();
        r.push(&[0.5_f32; 16]);
        r.reset();
        assert!(r.is_empty());
        assert_eq!(r.fill_frames(), 0);
    }

    #[test]
    #[should_panic(expected = "samples.len()")]
    fn ring_push_misaligned_panics() {
        let mut r =
            RingBuffer::new(AudioFormat::default(), RingBufferConfig::new(64).unwrap()).unwrap();
        // 5 samples on a 2-channel ring is misaligned.
        r.push(&[0.5_f32; 5]);
    }
}
