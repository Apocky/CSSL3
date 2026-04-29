//! Sound storage — PCM sample buffers + addressable bank.
//!
//! § DESIGN
//!   `PcmData` is the canonical sample container : a `Vec<f32>` of
//!   interleaved samples + a `(rate, channels)` metadata pair. Sounds
//!   are stored in a `SoundBank` keyed by an opaque `SoundHandle`.
//!   The mixer holds a `SoundHandle` per voice rather than the buffer
//!   itself so multiple voices can share a single PCM source.
//!
//! § SOUND VARIANTS
//!   - `OneShot(PcmData)`  — plays once, voice retires when done.
//!   - `Looping(PcmData)`  — plays forever ; loop boundary is
//!                            sample-accurate (no click).
//!   - `Streaming(...)`    — chunked playback for long sources. At
//!                            stage-0 the chunk-source is a `Box<dyn
//!                            SoundSource>` ; the trait is the seam
//!                            future slices use to wire OGG/MP3
//!                            streaming + procedural synthesis.
//!
//! § FORMAT MATCHING
//!   At stage-0 the mixer requires every `PcmData` it touches to share
//!   the mixer's output `(rate, channels)`. A `MixError::FormatMismatch`
//!   surfaces whenever a sound is `play()`-ed with mismatched format ;
//!   a future slice will land linear-resampling for off-rate playback.

use core::fmt;

use crate::error::{MixError, Result};

/// Opaque handle to a sound stored in a `SoundBank`. Stable for the
/// lifetime of the bank ; `SoundBank::clear()` invalidates all handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SoundHandle(pub u64);

/// Inline PCM sample buffer.
///
/// § INVARIANTS (checked at `new` / builder)
///   - `samples.len() % channels == 0` — interleaved samples align to
///     a frame boundary.
///   - `rate > 0`.
///   - `channels >= 1` and `channels <= 8`.
#[derive(Debug, Clone, PartialEq)]
pub struct PcmData {
    /// Interleaved samples. `samples[i*channels + c]` = sample at frame `i`
    /// for channel `c`.
    samples: Vec<f32>,
    /// Sample rate in Hz.
    rate: u32,
    /// Channel count.
    channels: u16,
}

impl PcmData {
    /// Construct a new `PcmData`. Validates the invariants. Stage-0 cap
    /// at 8 channels (the `cssl-host-audio::ChannelLayout::Surround71`
    /// upper bound).
    pub fn new(samples: Vec<f32>, rate: u32, channels: u16) -> Result<Self> {
        if rate == 0 {
            return Err(MixError::invalid("PcmData::new", "rate must be > 0"));
        }
        if !(1..=8).contains(&channels) {
            return Err(MixError::invalid(
                "PcmData::new",
                format!("channels {channels} out of supported range 1..=8"),
            ));
        }
        if samples.len() % usize::from(channels) != 0 {
            return Err(MixError::invalid(
                "PcmData::new",
                format!(
                    "samples.len() {} not divisible by channels {channels}",
                    samples.len()
                ),
            ));
        }
        Ok(Self {
            samples,
            rate,
            channels,
        })
    }

    /// Build a silent `PcmData` of `frame_count` frames @ `(rate, channels)`.
    pub fn silence(frame_count: usize, rate: u32, channels: u16) -> Result<Self> {
        let total = frame_count * usize::from(channels);
        Self::new(vec![0.0; total], rate, channels)
    }

    /// Sample-rate.
    #[must_use]
    pub const fn rate(&self) -> u32 {
        self.rate
    }

    /// Channel count.
    #[must_use]
    pub const fn channels(&self) -> u16 {
        self.channels
    }

    /// Number of frames = `samples.len() / channels`.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    /// Read-only access to the interleaved samples.
    #[must_use]
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// Read a sample at `(frame_index, channel)`. Returns `0.0` if out
    /// of bounds (sample-accurate stop ; never panics).
    #[must_use]
    pub fn sample(&self, frame_index: usize, channel: usize) -> f32 {
        if frame_index >= self.frames() || channel >= usize::from(self.channels) {
            return 0.0;
        }
        self.samples[frame_index * usize::from(self.channels) + channel]
    }

    /// Read a frame as a slice of channel-samples ; returns an empty
    /// slice if the index is out of bounds.
    #[must_use]
    pub fn frame_slice(&self, frame_index: usize) -> &[f32] {
        if frame_index >= self.frames() {
            return &[];
        }
        let ch = usize::from(self.channels);
        &self.samples[frame_index * ch..(frame_index + 1) * ch]
    }

    /// Duration in seconds (frames / rate).
    #[must_use]
    pub fn duration_secs(&self) -> f32 {
        if self.rate == 0 {
            return 0.0;
        }
        (self.frames() as f32) / (self.rate as f32)
    }
}

/// Builder for `PcmData` — mutable accumulator that finalizes into a
/// validated `PcmData`. Useful for procedural synthesis tests + future
/// chunk-decoder paths.
#[derive(Debug, Clone)]
pub struct PcmDataBuilder {
    samples: Vec<f32>,
    rate: u32,
    channels: u16,
}

impl PcmDataBuilder {
    /// Construct a fresh builder targeted at `(rate, channels)`.
    #[must_use]
    pub fn new(rate: u32, channels: u16) -> Self {
        Self {
            samples: Vec::new(),
            rate,
            channels,
        }
    }

    /// Reserve capacity for `frame_count` frames.
    #[must_use]
    pub fn with_capacity(rate: u32, channels: u16, frame_count: usize) -> Self {
        Self {
            samples: Vec::with_capacity(frame_count * usize::from(channels)),
            rate,
            channels,
        }
    }

    /// Append a single frame of `channels` samples. Panics if the slice
    /// length doesn't match `channels` — this is a builder-time invariant
    /// the caller controls.
    pub fn push_frame(&mut self, frame: &[f32]) {
        assert!(
            frame.len() == usize::from(self.channels),
            "push_frame : got {} samples, expected {} (channels)",
            frame.len(),
            self.channels
        );
        self.samples.extend_from_slice(frame);
    }

    /// Append `count` silence frames.
    pub fn push_silence(&mut self, count: usize) {
        let total = count * usize::from(self.channels);
        self.samples.extend(core::iter::repeat(0.0).take(total));
    }

    /// Finalize into a `PcmData`. Returns the validated container.
    pub fn finish(self) -> Result<PcmData> {
        PcmData::new(self.samples, self.rate, self.channels)
    }
}

/// A sample source that the mixer pulls from frame-by-frame.
///
/// § STREAMING DESIGN
///   The mixer's render loop calls `next_frame(out)` for each voice that
///   wraps a streaming source. The trait is `Send` so future slices can
///   move sources between threads (e.g., decoder thread → mixer thread).
///   Sources that decode on the audio thread MUST honor the
///   `Realtime<Crit>` invariants (`{NoAlloc, NoUnbounded, Deadline<1ms>}`).
pub trait SoundSource: Send {
    /// Sample rate the source emits.
    fn rate(&self) -> u32;
    /// Channel count the source emits.
    fn channels(&self) -> u16;
    /// Pull `out.len() / channels` frames into `out` (interleaved).
    /// Returns the number of *frames* actually filled. Returning fewer
    /// than requested signals end-of-source ; the mixer retires the
    /// voice on the next render call.
    fn next_frames(&mut self, out: &mut [f32]) -> usize;
    /// Whether the source has more frames to emit. Mixer queries this
    /// after each `next_frames` to avoid invoking exhausted sources.
    fn has_more(&self) -> bool;
    /// Optional reset — not all sources support this. Default is no-op
    /// (returns `false` ; caller should treat as un-loopable).
    fn reset(&mut self) -> bool {
        false
    }
}

/// PCM-backed source that can be reset for looping playback. Used by
/// the mixer to wrap `Sound::Looping` variants in a uniform interface.
#[derive(Debug, Clone)]
#[allow(dead_code)] // `pcm()` and `reset()` reserved for replay-introspection slice
pub(crate) struct PcmSource {
    pcm: PcmData,
    cursor: usize,
}

impl PcmSource {
    pub(crate) fn new(pcm: PcmData) -> Self {
        Self { pcm, cursor: 0 }
    }

    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    pub(crate) fn frames(&self) -> usize {
        self.pcm.frames()
    }

    pub(crate) fn rate(&self) -> u32 {
        self.pcm.rate()
    }

    pub(crate) fn channels(&self) -> u16 {
        self.pcm.channels()
    }

    /// Advance + read into `out`. Returns frames read.
    pub(crate) fn read_frames(&mut self, out: &mut [f32]) -> usize {
        let ch = usize::from(self.pcm.channels());
        debug_assert!(out.len() % ch == 0, "read_frames : misaligned out");
        let frames_requested = out.len() / ch;
        let frames_avail = self.pcm.frames().saturating_sub(self.cursor);
        let frames_to_read = frames_requested.min(frames_avail);
        let samples_to_read = frames_to_read * ch;
        let src = &self.pcm.samples()[self.cursor * ch..self.cursor * ch + samples_to_read];
        out[..samples_to_read].copy_from_slice(src);
        // Zero the remainder.
        for slot in out[samples_to_read..].iter_mut() {
            *slot = 0.0;
        }
        self.cursor += frames_to_read;
        frames_to_read
    }

    /// Read with looping — wraps the cursor when EOF is hit.
    pub(crate) fn read_frames_looping(&mut self, out: &mut [f32]) -> usize {
        let ch = usize::from(self.pcm.channels());
        debug_assert!(out.len() % ch == 0, "read_frames_looping : misaligned out");
        let frames_requested = out.len() / ch;
        let mut frames_done = 0;
        while frames_done < frames_requested {
            if self.pcm.frames() == 0 {
                // Empty PCM ; fill rest with silence.
                for slot in out[frames_done * ch..].iter_mut() {
                    *slot = 0.0;
                }
                return frames_requested;
            }
            if self.cursor >= self.pcm.frames() {
                self.cursor = 0;
            }
            let frames_avail = self.pcm.frames() - self.cursor;
            let frames_to_read = (frames_requested - frames_done).min(frames_avail);
            let samples_to_read = frames_to_read * ch;
            let src_start = self.cursor * ch;
            let src = &self.pcm.samples()[src_start..src_start + samples_to_read];
            let dst_start = frames_done * ch;
            out[dst_start..dst_start + samples_to_read].copy_from_slice(src);
            self.cursor += frames_to_read;
            frames_done += frames_to_read;
        }
        frames_done
    }

    /// Reserved for replay-mode rewind ; not yet wired (S9-O2 will use it).
    #[allow(dead_code)]
    pub(crate) fn reset(&mut self) {
        self.cursor = 0;
    }

    pub(crate) fn is_exhausted(&self) -> bool {
        self.cursor >= self.pcm.frames()
    }
}

/// Sound — playback-mode + sample data.
///
/// Wraps a `SoundHandle` reference + the playback-mode discipline. The
/// mixer holds a `Sound` per voice ; the underlying PCM lives in the
/// `SoundBank` so multiple voices can share storage.
pub enum Sound {
    /// Plays once. Voice retires when sample exhausted.
    OneShot(SoundHandle),
    /// Loops indefinitely. Sample-accurate loop boundary.
    Looping(SoundHandle),
    /// Streaming source — pulled frame-by-frame on the audio thread.
    /// The streaming source is owned by the mixer voice ; sources MUST
    /// be `Send` + honor `Realtime<Crit>` invariants on `next_frames`.
    Streaming(Box<dyn SoundSource>),
}

impl fmt::Debug for Sound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OneShot(h) => f.debug_tuple("OneShot").field(h).finish(),
            Self::Looping(h) => f.debug_tuple("Looping").field(h).finish(),
            Self::Streaming(_) => f
                .debug_struct("Streaming")
                .field("source", &"<dyn SoundSource>")
                .finish(),
        }
    }
}

/// Bank that stores `PcmData` keyed by `SoundHandle`. Stage-0 caps at
/// 4096 sounds — covers a generous SFX library + music + UI without
/// runtime allocation churn.
#[derive(Debug, Default)]
pub struct SoundBank {
    sounds: Vec<Option<PcmData>>,
    next_id: u64,
    capacity_max: usize,
}

impl SoundBank {
    /// Default capacity — 4096 sounds.
    pub const DEFAULT_MAX: usize = 4096;

    /// Construct an empty bank with the default capacity.
    #[must_use]
    pub fn new() -> Self {
        Self::with_max(Self::DEFAULT_MAX)
    }

    /// Construct an empty bank with the given hard cap.
    #[must_use]
    pub fn with_max(capacity_max: usize) -> Self {
        Self {
            sounds: Vec::new(),
            next_id: 0,
            capacity_max,
        }
    }

    /// Number of currently-allocated sounds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sounds.iter().filter(|s| s.is_some()).count()
    }

    /// Whether the bank is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Insert a `PcmData`. Returns the `SoundHandle` for later
    /// `play()` calls. Returns `BankFull` if capacity is exhausted.
    ///
    /// § STAGE-0 SLOT DISCIPLINE
    ///   Handles are slot-indices ; `next_id` increments monotonically
    ///   and serves as both the new handle's id AND the slot index.
    ///   Removed sounds leave a `None` in their slot ; the bank does
    ///   NOT reuse slots (handles are stable for the bank's lifetime
    ///   modulo `clear`). This keeps `get(handle)` an O(1) Vec lookup.
    pub fn insert(&mut self, pcm: PcmData) -> Result<SoundHandle> {
        if self.len() >= self.capacity_max {
            return Err(MixError::bank_full(self.len(), self.capacity_max));
        }
        let handle = SoundHandle(self.next_id);
        self.next_id += 1;
        self.sounds.push(Some(pcm));
        Ok(handle)
    }

    /// Remove a sound. Subsequent `get()` for the same handle returns `None`.
    ///
    /// § STAGE-0 SIMPLIFICATION
    ///   `SoundHandle` is the slot index ; insertions append + never reuse
    ///   slots, so `handle.0 == slot_index`. Removal nulls the slot in place ;
    ///   subsequent inserts find the empty slot via `position(Option::is_none)`
    ///   and reuse it (then a fresh handle is allocated from `next_id` —
    ///   the slot is not the same handle, just the same Vec position).
    pub fn remove(&mut self, handle: SoundHandle) -> Result<()> {
        let idx = handle.0 as usize;
        let slot = self
            .sounds
            .get_mut(idx)
            .ok_or(MixError::SoundNotFound(handle))?;
        if slot.is_none() {
            return Err(MixError::SoundNotFound(handle));
        }
        *slot = None;
        Ok(())
    }

    /// Retrieve a `PcmData` by handle. Returns `None` if removed.
    #[must_use]
    pub fn get(&self, handle: SoundHandle) -> Option<&PcmData> {
        // Stage-0 : id N → slot N (we don't reuse slots).
        let idx = handle.0 as usize;
        self.sounds.get(idx).and_then(|s| s.as_ref())
    }

    /// Mutable retrieval — used by streaming-source resets.
    pub fn get_mut(&mut self, handle: SoundHandle) -> Option<&mut PcmData> {
        let idx = handle.0 as usize;
        self.sounds.get_mut(idx).and_then(|s| s.as_mut())
    }

    /// Clear all sounds. All previously-issued handles are invalidated.
    pub fn clear(&mut self) {
        self.sounds.clear();
        self.next_id = 0;
    }

    /// Return the bank's hard capacity.
    #[must_use]
    pub const fn capacity_max(&self) -> usize {
        self.capacity_max
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // PCM samples are bit-equal verbatim by design
mod tests {
    use super::*;

    #[test]
    fn sound_handle_ord_and_eq() {
        let a = SoundHandle(1);
        let b = SoundHandle(2);
        assert!(a < b);
        assert_eq!(a, SoundHandle(1));
    }

    #[test]
    fn pcm_new_validates_rate_zero() {
        assert!(PcmData::new(vec![0.0; 4], 0, 2).is_err());
    }

    #[test]
    fn pcm_new_validates_channels_zero() {
        assert!(PcmData::new(Vec::new(), 48_000, 0).is_err());
    }

    #[test]
    fn pcm_new_validates_channels_overflow() {
        assert!(PcmData::new(Vec::new(), 48_000, 16).is_err());
    }

    #[test]
    fn pcm_new_validates_misaligned_samples() {
        assert!(PcmData::new(vec![0.0; 5], 48_000, 2).is_err());
    }

    #[test]
    fn pcm_silence_is_zero_filled() {
        let p = PcmData::silence(8, 48_000, 2).expect("valid");
        assert_eq!(p.frames(), 8);
        assert!(p.samples().iter().all(|s| *s == 0.0));
    }

    #[test]
    fn pcm_frames_count_matches_samples_div_channels() {
        let p = PcmData::new(vec![0.5; 16], 48_000, 2).unwrap();
        assert_eq!(p.frames(), 8);
    }

    #[test]
    fn pcm_sample_out_of_bounds_returns_zero() {
        let p = PcmData::new(vec![0.5; 16], 48_000, 2).unwrap();
        assert_eq!(p.sample(0, 0), 0.5);
        assert_eq!(p.sample(99, 0), 0.0);
        assert_eq!(p.sample(0, 99), 0.0);
    }

    #[test]
    fn pcm_frame_slice_returns_correct_range() {
        let mut samples = Vec::new();
        for i in 0..8 {
            samples.push(i as f32);
            samples.push((i as f32) + 0.5);
        }
        let p = PcmData::new(samples, 48_000, 2).unwrap();
        let slice = p.frame_slice(2);
        assert_eq!(slice.len(), 2);
        assert_eq!(slice[0], 2.0);
        assert_eq!(slice[1], 2.5);
    }

    #[test]
    fn pcm_duration_secs_at_48k() {
        let p = PcmData::silence(48_000, 48_000, 2).unwrap();
        let d = p.duration_secs();
        assert!((d - 1.0).abs() < 1e-6);
    }

    #[test]
    fn pcm_builder_accumulates_frames() {
        let mut b = PcmDataBuilder::new(48_000, 2);
        b.push_frame(&[1.0, -1.0]);
        b.push_frame(&[0.5, -0.5]);
        let p = b.finish().unwrap();
        assert_eq!(p.frames(), 2);
        assert_eq!(p.samples(), &[1.0, -1.0, 0.5, -0.5]);
    }

    #[test]
    fn pcm_builder_push_silence_zeros_frames() {
        let mut b = PcmDataBuilder::with_capacity(48_000, 2, 4);
        b.push_silence(4);
        let p = b.finish().unwrap();
        assert_eq!(p.frames(), 4);
        assert!(p.samples().iter().all(|s| *s == 0.0));
    }

    #[test]
    #[should_panic(expected = "push_frame")]
    fn pcm_builder_push_frame_misaligned_panics() {
        let mut b = PcmDataBuilder::new(48_000, 2);
        b.push_frame(&[1.0]); // missing R sample
    }

    #[test]
    fn pcm_source_read_frames_advances_cursor() {
        let pcm = PcmData::new((0..16).map(|i| i as f32).collect(), 48_000, 2).unwrap();
        let mut src = PcmSource::new(pcm);
        let mut out = vec![0.0; 8]; // 4 frames
        let n = src.read_frames(&mut out);
        assert_eq!(n, 4);
        assert_eq!(src.cursor(), 4);
    }

    #[test]
    fn pcm_source_read_frames_zero_pad_on_eof() {
        let pcm = PcmData::new(vec![1.0, 2.0, 3.0, 4.0], 48_000, 2).unwrap();
        let mut src = PcmSource::new(pcm);
        let mut out = vec![0.5; 8]; // 4 frames requested, only 2 available
        let n = src.read_frames(&mut out);
        assert_eq!(n, 2);
        assert_eq!(out[0], 1.0);
        assert_eq!(out[1], 2.0);
        assert_eq!(out[2], 3.0);
        assert_eq!(out[3], 4.0);
        // Remainder zeroed.
        assert_eq!(out[4], 0.0);
        assert_eq!(out[5], 0.0);
    }

    #[test]
    fn pcm_source_looping_wraps() {
        let pcm = PcmData::new(vec![1.0, -1.0, 2.0, -2.0], 48_000, 2).unwrap();
        // 2 frames in source ; request 5 frames looping = 1+1+1+1+1.
        let mut src = PcmSource::new(pcm);
        let mut out = vec![0.0; 10]; // 5 frames stereo
        let n = src.read_frames_looping(&mut out);
        assert_eq!(n, 5);
        // Pattern : 1,-1, 2,-2, 1,-1, 2,-2, 1,-1.
        assert_eq!(
            out,
            &[1.0, -1.0, 2.0, -2.0, 1.0, -1.0, 2.0, -2.0, 1.0, -1.0]
        );
    }

    #[test]
    fn pcm_source_reset_returns_to_start() {
        let pcm = PcmData::new(vec![1.0, 2.0], 48_000, 1).unwrap();
        let mut src = PcmSource::new(pcm);
        let mut out = vec![0.0; 1];
        src.read_frames(&mut out);
        assert_eq!(src.cursor(), 1);
        src.reset();
        assert_eq!(src.cursor(), 0);
    }

    #[test]
    fn pcm_source_is_exhausted_after_full_read() {
        let pcm = PcmData::new(vec![1.0, 2.0], 48_000, 1).unwrap();
        let mut src = PcmSource::new(pcm);
        assert!(!src.is_exhausted());
        let mut out = vec![0.0; 4]; // Read more than available.
        src.read_frames(&mut out);
        assert!(src.is_exhausted());
    }

    #[test]
    fn bank_default_capacity_4096() {
        let bank = SoundBank::new();
        assert_eq!(bank.capacity_max(), SoundBank::DEFAULT_MAX);
        assert!(bank.is_empty());
    }

    #[test]
    fn bank_insert_returns_handle() {
        let mut bank = SoundBank::new();
        let h = bank
            .insert(PcmData::silence(8, 48_000, 2).unwrap())
            .expect("insert");
        // First handle = 0.
        assert_eq!(h.0, 0);
        assert_eq!(bank.len(), 1);
        let next = bank
            .insert(PcmData::silence(4, 48_000, 2).unwrap())
            .expect("insert");
        assert_eq!(next.0, 1);
    }

    #[test]
    fn bank_get_returns_pcm() {
        let mut bank = SoundBank::new();
        let pcm = PcmData::new(vec![0.7; 4], 48_000, 2).unwrap();
        let h = bank.insert(pcm).unwrap();
        let stored = bank.get(h).expect("present");
        assert_eq!(stored.frames(), 2);
        assert_eq!(stored.samples()[0], 0.7);
    }

    #[test]
    fn bank_get_unknown_returns_none() {
        let bank = SoundBank::new();
        assert!(bank.get(SoundHandle(99)).is_none());
    }

    #[test]
    fn bank_full_returns_error() {
        let mut bank = SoundBank::with_max(2);
        bank.insert(PcmData::silence(1, 48_000, 1).unwrap())
            .unwrap();
        bank.insert(PcmData::silence(1, 48_000, 1).unwrap())
            .unwrap();
        let err = bank.insert(PcmData::silence(1, 48_000, 1).unwrap());
        match err {
            Err(MixError::BankFull { capacity, max }) => {
                assert_eq!(capacity, 2);
                assert_eq!(max, 2);
            }
            other => panic!("wrong: {other:?}"),
        }
    }

    #[test]
    fn bank_clear_invalidates_handles() {
        let mut bank = SoundBank::new();
        let h = bank
            .insert(PcmData::silence(1, 48_000, 1).unwrap())
            .unwrap();
        bank.clear();
        assert!(bank.get(h).is_none());
    }

    #[test]
    fn sound_oneshot_debug_renders_handle() {
        let s = Sound::OneShot(SoundHandle(13));
        let dbg = format!("{s:?}");
        assert!(dbg.contains("OneShot"));
        assert!(dbg.contains("13"));
    }

    #[test]
    fn sound_looping_debug_renders_handle() {
        let s = Sound::Looping(SoundHandle(7));
        let dbg = format!("{s:?}");
        assert!(dbg.contains("Looping"));
        assert!(dbg.contains('7'));
    }
}
