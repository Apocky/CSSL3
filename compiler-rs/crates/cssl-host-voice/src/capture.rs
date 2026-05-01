//! Fixed-capacity audio-sample ring-buffer.
//!
//! § INVARIANTS
//!   - capacity is fixed at construction (never grows).
//!   - on overflow, oldest samples are overwritten in-place (FIFO).
//!   - `most_recent` returns samples in time-order regardless of
//!     internal write-cursor position.
//!   - all operations are O(N) in the slice length, no hidden allocations
//!     beyond the initial fixed buffer.
//!
//! § DETERMINISM
//!   The ring is allocator-touched once at `new()` and never resized.
//!   This matches the Sawyer/Pokemon-OG pre-allocation discipline —
//!   pick the worst-case capacity, allocate once, run forever.

/// Fixed-capacity sample ring buffer.
///
/// Stores `f32` PCM samples interleaved across `channel_count` channels.
/// The capacity is computed as `seconds * sample_rate_hz * channel_count`
/// at construction.
#[derive(Debug, Clone)]
pub struct AudioRingBuffer {
    samples: Vec<f32>,
    write_idx: usize,
    capacity_samples: usize,
    sample_rate_hz: u32,
    channel_count: u8,
    /// Number of samples written total (saturating at usize::MAX).
    /// Used to distinguish "ring not yet full" from "ring full + wrapped".
    total_pushed: usize,
}

impl AudioRingBuffer {
    /// Construct a ring sized for `seconds` of audio at the given rate +
    /// channel count. Capacity in samples = `seconds * sample_rate_hz *
    /// channel_count`. Returns a buffer where every slot is initialized
    /// to silence (0.0).
    #[must_use]
    pub fn new(seconds: u32, sample_rate_hz: u32, channels: u8) -> Self {
        let capacity_samples = (seconds as usize)
            .saturating_mul(sample_rate_hz as usize)
            .saturating_mul(channels.max(1) as usize);
        Self {
            samples: vec![0.0; capacity_samples],
            write_idx: 0,
            capacity_samples,
            sample_rate_hz,
            channel_count: channels.max(1),
            total_pushed: 0,
        }
    }

    /// Push interleaved sample frame into ring. Overwrites oldest
    /// samples in FIFO order on overflow. No-op when capacity is zero.
    pub fn push_samples(&mut self, frame: &[f32]) {
        if self.capacity_samples == 0 {
            return;
        }
        for &sample in frame {
            self.samples[self.write_idx] = sample;
            self.write_idx = (self.write_idx + 1) % self.capacity_samples;
            self.total_pushed = self.total_pushed.saturating_add(1);
        }
    }

    /// Copy out the most-recent `samples` in time-order (oldest-first).
    /// Returns at most `min(samples, valid_samples)` where
    /// `valid_samples = min(total_pushed, capacity_samples)`.
    #[must_use]
    pub fn most_recent(&self, samples: usize) -> Vec<f32> {
        let valid = self.total_pushed.min(self.capacity_samples);
        let want = samples.min(valid);
        if want == 0 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(want);
        // The most-recent `want` samples end at write_idx (exclusive),
        // wrapping back. Compute the start index and walk forward in
        // time-order.
        let start = (self.write_idx + self.capacity_samples - want) % self.capacity_samples;
        for i in 0..want {
            let idx = (start + i) % self.capacity_samples;
            out.push(self.samples[idx]);
        }
        out
    }

    /// Number of seconds currently buffered (clamped at capacity).
    #[must_use]
    pub fn duration_seconds(&self) -> f32 {
        let valid = self.total_pushed.min(self.capacity_samples);
        let denom = (self.sample_rate_hz as f32) * (self.channel_count as f32);
        if denom <= 0.0 {
            0.0
        } else {
            (valid as f32) / denom
        }
    }

    /// Maximum seconds the ring can hold.
    #[must_use]
    pub fn capacity_seconds(&self) -> f32 {
        let denom = (self.sample_rate_hz as f32) * (self.channel_count as f32);
        if denom <= 0.0 {
            0.0
        } else {
            (self.capacity_samples as f32) / denom
        }
    }

    /// Reset ring to silence + zero write-cursor + zero pushed-count.
    pub fn clear(&mut self) {
        for slot in &mut self.samples {
            *slot = 0.0;
        }
        self.write_idx = 0;
        self.total_pushed = 0;
    }

    /// Capacity in samples (interleaved).
    #[must_use]
    pub fn capacity_samples(&self) -> usize {
        self.capacity_samples
    }

    /// Sample rate the ring was constructed with.
    #[must_use]
    pub fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }

    /// Channel count the ring was constructed with.
    #[must_use]
    pub fn channel_count(&self) -> u8 {
        self.channel_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_zeros() {
        let r = AudioRingBuffer::new(1, 8, 1);
        assert_eq!(r.capacity_samples, 8);
        assert!(r.samples.iter().all(|&s| s == 0.0));
        assert_eq!(r.write_idx, 0);
        assert_eq!(r.total_pushed, 0);
    }

    #[test]
    fn push_fills() {
        let mut r = AudioRingBuffer::new(1, 4, 1); // cap = 4
        r.push_samples(&[0.1, 0.2, 0.3]);
        assert_eq!(r.write_idx, 3);
        assert_eq!(r.total_pushed, 3);
        let recent = r.most_recent(3);
        assert_eq!(recent, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn ring_overwrites_on_overflow() {
        let mut r = AudioRingBuffer::new(1, 4, 1); // cap = 4
        r.push_samples(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        // After 6 pushes into cap-4 ring : write_idx wrapped twice.
        // Most-recent 4 should be [3.0, 4.0, 5.0, 6.0] in time-order.
        let recent = r.most_recent(4);
        assert_eq!(recent, vec![3.0, 4.0, 5.0, 6.0]);
        assert_eq!(r.total_pushed, 6);
    }

    #[test]
    fn most_recent_time_ordered() {
        let mut r = AudioRingBuffer::new(1, 4, 1); // cap = 4
        r.push_samples(&[10.0, 20.0, 30.0, 40.0, 50.0]); // wraps once
        // Internal layout : samples = [50.0, 20.0, 30.0, 40.0], write_idx = 1.
        // Most-recent 3 in time-order = [30.0, 40.0, 50.0].
        let recent = r.most_recent(3);
        assert_eq!(recent, vec![30.0, 40.0, 50.0]);
        // Most-recent 1 = [50.0].
        assert_eq!(r.most_recent(1), vec![50.0]);
        // Asking for more than valid clamps to valid.
        assert_eq!(r.most_recent(100).len(), 4);
    }

    #[test]
    fn duration_vs_capacity() {
        let mut r = AudioRingBuffer::new(2, 1000, 1); // cap = 2000
        assert!((r.capacity_seconds() - 2.0).abs() < 1e-6);
        assert!((r.duration_seconds() - 0.0).abs() < 1e-6);
        let frame: Vec<f32> = vec![0.0; 500];
        r.push_samples(&frame);
        assert!((r.duration_seconds() - 0.5).abs() < 1e-6);
        // Push enough to overflow ; duration clamps at capacity_seconds.
        let big: Vec<f32> = vec![0.0; 5000];
        r.push_samples(&big);
        assert!((r.duration_seconds() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn clear_resets() {
        let mut r = AudioRingBuffer::new(1, 4, 1);
        r.push_samples(&[1.0, 2.0, 3.0]);
        r.clear();
        assert_eq!(r.write_idx, 0);
        assert_eq!(r.total_pushed, 0);
        assert_eq!(r.duration_seconds(), 0.0);
        assert!(r.samples.iter().all(|&s| s == 0.0));
        assert_eq!(r.most_recent(10), Vec::<f32>::new());
    }

    #[test]
    fn zero_capacity_no_panic() {
        let mut r = AudioRingBuffer::new(0, 0, 0);
        r.push_samples(&[1.0, 2.0, 3.0]);
        assert_eq!(r.capacity_samples, 0);
        assert!(r.most_recent(5).is_empty());
        assert_eq!(r.duration_seconds(), 0.0);
        assert_eq!(r.capacity_seconds(), 0.0);
    }
}
