//! Canonical audio format types.
//!
//! § DESIGN
//!   The CSSLv3 audio surface speaks **f32 interleaved across channels**
//!   exclusively. Platform layers convert to / from device-native formats
//!   (typically i16 / i24 / f32) at the boundary. This keeps user code
//!   simple : a stereo 48-kHz buffer is `&[f32]` of length
//!   `frames * 2`, with samples laid out `L0 R0 L1 R1 ... Ln Rn`.
//!
//! § SAMPLE RATES
//!   The `SampleRate` enum names the most common rates that CSSLv3 audio
//!   targets (`Hz44100` through `Hz192000`). Arbitrary rates are still
//!   supported via [`SampleRate::custom`] ; the enum simply gives names
//!   to the rates that audio-engineering practice has standardized on.
//!
//! § CHANNEL LAYOUTS
//!   `ChannelLayout` covers the canonical surround layouts up to 7.1 ;
//!   exotic layouts (Atmos, ambisonics) are deferred to a follow-up
//!   slice. The platform layer maps each layout to the device channel
//!   mask at open time.

use crate::error::{AudioError, Result};

/// Canonical sample-rate enum + custom-rate escape hatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SampleRate {
    /// 44.1 kHz — CD audio standard.
    Hz44100,
    /// 48 kHz — DVD / professional audio standard. Default.
    Hz48000,
    /// 88.2 kHz — high-resolution 2× CD.
    Hz88200,
    /// 96 kHz — high-resolution 2× DVD.
    Hz96000,
    /// 176.4 kHz — high-resolution 4× CD.
    Hz176400,
    /// 192 kHz — high-resolution 4× DVD.
    Hz192000,
    /// Arbitrary sample-rate (escape hatch).
    Custom(u32),
}

impl SampleRate {
    /// Build a custom sample-rate. Returns `InvalidArgument` if 0 or
    /// above `768000` (4× max-supported = 768 kHz).
    pub fn custom(hz: u32) -> Result<Self> {
        if hz == 0 {
            return Err(AudioError::invalid(
                "SampleRate::custom",
                "rate must be > 0",
            ));
        }
        if hz > 768_000 {
            return Err(AudioError::invalid(
                "SampleRate::custom",
                format!("rate {hz} exceeds 768000 Hz"),
            ));
        }
        Ok(Self::Custom(hz))
    }

    /// Raw Hertz value.
    #[must_use]
    pub const fn as_hz(self) -> u32 {
        match self {
            Self::Hz44100 => 44_100,
            Self::Hz48000 => 48_000,
            Self::Hz88200 => 88_200,
            Self::Hz96000 => 96_000,
            Self::Hz176400 => 176_400,
            Self::Hz192000 => 192_000,
            Self::Custom(hz) => hz,
        }
    }

    /// Default rate for new streams (48 kHz).
    #[must_use]
    pub const fn default_rate() -> Self {
        Self::Hz48000
    }
}

impl Default for SampleRate {
    fn default() -> Self {
        Self::default_rate()
    }
}

/// Canonical channel-layout enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelLayout {
    /// 1.0 — mono.
    Mono,
    /// 2.0 — stereo (L, R). Default.
    Stereo,
    /// 2.1 — stereo + LFE.
    Stereo21,
    /// 5.1 — surround (FL, FR, FC, LFE, BL, BR).
    Surround51,
    /// 7.1 — surround (FL, FR, FC, LFE, BL, BR, SL, SR).
    Surround71,
}

impl ChannelLayout {
    /// Number of audio channels in the layout.
    #[must_use]
    pub const fn channel_count(self) -> u16 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
            Self::Stereo21 => 3,
            Self::Surround51 => 6,
            Self::Surround71 => 8,
        }
    }

    /// Short human-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mono => "mono",
            Self::Stereo => "stereo",
            Self::Stereo21 => "stereo-2.1",
            Self::Surround51 => "surround-5.1",
            Self::Surround71 => "surround-7.1",
        }
    }

    /// Default layout (stereo).
    #[must_use]
    pub const fn default_layout() -> Self {
        Self::Stereo
    }
}

impl Default for ChannelLayout {
    fn default() -> Self {
        Self::default_layout()
    }
}

/// Canonical audio format : f32 interleaved across `channels` at `rate`.
///
/// The format is what the CSSLv3 surface speaks ; the platform layer
/// negotiates with the device + converts at the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AudioFormat {
    /// Sample rate (e.g., 48 kHz).
    pub rate: SampleRate,
    /// Channel layout (e.g., stereo).
    pub layout: ChannelLayout,
}

impl AudioFormat {
    /// Build a new format. Validates that channel count is non-zero
    /// (always true for the enum-defined layouts) + that rate is non-zero.
    pub fn new(rate: SampleRate, layout: ChannelLayout) -> Result<Self> {
        if rate.as_hz() == 0 {
            return Err(AudioError::invalid("AudioFormat::new", "rate must be > 0"));
        }
        Ok(Self { rate, layout })
    }

    /// Number of bytes per sample (always 4 for f32).
    #[must_use]
    pub const fn bytes_per_sample() -> usize {
        4
    }

    /// Number of bytes per frame = `channels * bytes_per_sample`.
    #[must_use]
    pub const fn bytes_per_frame(&self) -> usize {
        (self.layout.channel_count() as usize) * Self::bytes_per_sample()
    }

    /// Number of bytes a `frame_count`-frame buffer occupies.
    #[must_use]
    pub const fn buffer_bytes(&self, frame_count: usize) -> usize {
        frame_count * self.bytes_per_frame()
    }

    /// Sample-count for a given frame-count = `frames * channels`.
    #[must_use]
    pub const fn sample_count(&self, frame_count: usize) -> usize {
        frame_count * (self.layout.channel_count() as usize)
    }

    /// Default format : 48 kHz stereo.
    #[must_use]
    pub fn default_output() -> Self {
        Self {
            rate: SampleRate::default_rate(),
            layout: ChannelLayout::default_layout(),
        }
    }
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self::default_output()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_rate_default_is_48k() {
        assert_eq!(SampleRate::default().as_hz(), 48_000);
    }

    #[test]
    fn sample_rate_named_values() {
        assert_eq!(SampleRate::Hz44100.as_hz(), 44_100);
        assert_eq!(SampleRate::Hz48000.as_hz(), 48_000);
        assert_eq!(SampleRate::Hz88200.as_hz(), 88_200);
        assert_eq!(SampleRate::Hz96000.as_hz(), 96_000);
        assert_eq!(SampleRate::Hz176400.as_hz(), 176_400);
        assert_eq!(SampleRate::Hz192000.as_hz(), 192_000);
    }

    #[test]
    fn sample_rate_custom_accepts_arbitrary() {
        let sr = SampleRate::custom(32_000).expect("valid rate");
        assert_eq!(sr.as_hz(), 32_000);
    }

    #[test]
    fn sample_rate_custom_rejects_zero() {
        assert!(SampleRate::custom(0).is_err());
    }

    #[test]
    fn sample_rate_custom_rejects_above_max() {
        assert!(SampleRate::custom(1_000_000).is_err());
    }

    #[test]
    fn channel_layout_counts() {
        assert_eq!(ChannelLayout::Mono.channel_count(), 1);
        assert_eq!(ChannelLayout::Stereo.channel_count(), 2);
        assert_eq!(ChannelLayout::Stereo21.channel_count(), 3);
        assert_eq!(ChannelLayout::Surround51.channel_count(), 6);
        assert_eq!(ChannelLayout::Surround71.channel_count(), 8);
    }

    #[test]
    fn channel_layout_default_is_stereo() {
        assert_eq!(ChannelLayout::default(), ChannelLayout::Stereo);
    }

    #[test]
    fn channel_layout_str_names() {
        assert_eq!(ChannelLayout::Mono.as_str(), "mono");
        assert_eq!(ChannelLayout::Surround51.as_str(), "surround-5.1");
    }

    #[test]
    fn audio_format_default_is_48k_stereo() {
        let f = AudioFormat::default();
        assert_eq!(f.rate.as_hz(), 48_000);
        assert_eq!(f.layout.channel_count(), 2);
    }

    #[test]
    fn audio_format_bytes_per_frame_stereo_f32_is_8() {
        let f = AudioFormat::default();
        assert_eq!(f.bytes_per_frame(), 8);
    }

    #[test]
    fn audio_format_bytes_per_frame_5_1_is_24() {
        let f = AudioFormat::new(SampleRate::Hz48000, ChannelLayout::Surround51).unwrap();
        assert_eq!(f.bytes_per_frame(), 24);
    }

    #[test]
    fn audio_format_buffer_bytes_for_256_frames_stereo() {
        let f = AudioFormat::default();
        // 256 frames * 2 channels * 4 bytes = 2048
        assert_eq!(f.buffer_bytes(256), 2048);
    }

    #[test]
    fn audio_format_sample_count_matches_frames_times_channels() {
        let f = AudioFormat::default();
        assert_eq!(f.sample_count(256), 512);
    }
}
