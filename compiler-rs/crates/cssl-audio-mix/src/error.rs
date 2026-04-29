//! Central error type for the audio mixer.
//!
//! § DESIGN
//!   `MixError` mirrors the d3d12 / level-zero precedent + the
//!   `cssl-host-audio::AudioError` shape :
//!     - `InvalidArgument` — builder-side precondition.
//!     - `VoiceNotFound`   — `stop()` / `set_volume()` of an unknown id.
//!     - `BusNotFound`     — routing target does not exist.
//!     - `SoundNotFound`   — `play()` referenced an absent handle.
//!     - `BankFull`        — sound bank capacity exceeded.
//!     - `MixerFull`       — voice pool exhausted, voice could not be
//!                            allocated (PRIME-DIRECTIVE-honoring : no
//!                            silent-drop, callers learn explicitly).
//!     - `FormatMismatch`  — playback format is incompatible with the
//!                            mixer's output format AND no resampler is
//!                            wired at stage-0.
//!     - `HostAudio`       — wraps a `cssl-host-audio::AudioError` for
//!                            errors that come from the underlying
//!                            output-stream open / start / submit calls.
//!     - `CaptureForbidden`— PRIME-DIRECTIVE structural reject : the mixer
//!                            does NOT support capture-mode, and any
//!                            attempt to ask for a recordable post-effect
//!                            stream returns this variant.

use thiserror::Error;

/// Errors returned by the mixer + DSP layer.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum MixError {
    /// Builder-side argument precondition failed.
    #[error("mixer invalid argument for `{site}`: {reason}")]
    InvalidArgument {
        /// Where the invalid argument was detected.
        site: String,
        /// Why it was rejected.
        reason: String,
    },

    /// VoiceId did not match any active voice.
    #[error("mixer voice {0:?} not found")]
    VoiceNotFound(crate::voice::VoiceId),

    /// BusId did not match any registered bus.
    #[error("mixer bus {0:?} not found")]
    BusNotFound(crate::bus::BusId),

    /// SoundHandle did not match any sound in the bank.
    #[error("sound handle {0:?} not found in bank")]
    SoundNotFound(crate::sound::SoundHandle),

    /// SoundBank cannot accept more sounds.
    #[error("sound-bank full ({capacity} slots used, max={max})")]
    BankFull {
        /// Currently allocated slot count.
        capacity: usize,
        /// Hard cap.
        max: usize,
    },

    /// Mixer voice pool exhausted ; per PRIME-DIRECTIVE the mixer never
    /// silent-drops, so play() returns this when no voice can be
    /// allocated. Callers may choose voice-stealing or back-pressure.
    #[error("mixer voice pool full ({active} active, max={max})")]
    MixerFull {
        /// Active voices.
        active: usize,
        /// Hard cap.
        max: usize,
    },

    /// Playback format does not match output + no resampler is wired.
    #[error("mixer format mismatch (sound {sound_rate} Hz {sound_channels}ch, mixer {mixer_rate} Hz {mixer_channels}ch)")]
    FormatMismatch {
        /// Sound's sample rate.
        sound_rate: u32,
        /// Sound's channel count.
        sound_channels: u16,
        /// Mixer's output sample rate.
        mixer_rate: u32,
        /// Mixer's output channel count.
        mixer_channels: u16,
    },

    /// Wrapped underlying host-audio error from cssl-host-audio.
    #[error("host audio error: {0}")]
    HostAudio(String),

    /// Structural reject — the mixer does NOT expose a capture / record /
    /// loopback surface. Any request that would bypass the PRIME-DIRECTIVE
    /// surveillance gate returns this. There is no flag, no env-var, no
    /// override that flips this path on.
    #[error(
        "audio capture / loopback / record-output is structurally forbidden by PRIME-DIRECTIVE \
         (this mixer is OUTPUT-only)"
    )]
    CaptureForbidden,
}

impl MixError {
    /// Build an `InvalidArgument` variant.
    #[must_use]
    pub fn invalid(site: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidArgument {
            site: site.into(),
            reason: reason.into(),
        }
    }

    /// Build a `BankFull` variant.
    #[must_use]
    pub const fn bank_full(capacity: usize, max: usize) -> Self {
        Self::BankFull { capacity, max }
    }

    /// Build a `MixerFull` variant.
    #[must_use]
    pub const fn mixer_full(active: usize, max: usize) -> Self {
        Self::MixerFull { active, max }
    }

    /// Build a `FormatMismatch` variant.
    #[must_use]
    pub const fn format_mismatch(
        sound_rate: u32,
        sound_channels: u16,
        mixer_rate: u32,
        mixer_channels: u16,
    ) -> Self {
        Self::FormatMismatch {
            sound_rate,
            sound_channels,
            mixer_rate,
            mixer_channels,
        }
    }

    /// Wrap a `cssl-host-audio::AudioError`.
    #[must_use]
    pub fn host_audio(err: cssl_host_audio::AudioError) -> Self {
        Self::HostAudio(err.to_string())
    }

    /// Is this error a "host doesn't support that feature" condition ?
    /// Used by callers who want to surface a clear "feature not yet
    /// implemented" path distinct from a real bug.
    #[must_use]
    pub const fn is_capture_forbidden(&self) -> bool {
        matches!(self, Self::CaptureForbidden)
    }

    /// Is this error a "format incompatibility" condition that a
    /// resampler slice could resolve ?
    #[must_use]
    pub const fn is_format_mismatch(&self) -> bool {
        matches!(self, Self::FormatMismatch { .. })
    }
}

impl From<cssl_host_audio::AudioError> for MixError {
    fn from(err: cssl_host_audio::AudioError) -> Self {
        Self::host_audio(err)
    }
}

/// Crate-wide `Result` alias.
pub type Result<T> = core::result::Result<T, MixError>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::BusId;
    use crate::sound::SoundHandle;
    use crate::voice::VoiceId;

    #[test]
    fn invalid_constructor_carries_context() {
        let e = MixError::invalid("Mixer::play", "empty pcm");
        let s = format!("{e}");
        assert!(s.contains("Mixer::play"));
        assert!(s.contains("empty pcm"));
    }

    #[test]
    fn voice_not_found_renders_id() {
        let e = MixError::VoiceNotFound(VoiceId(42));
        assert!(format!("{e}").contains("42"));
    }

    #[test]
    fn bus_not_found_renders_id() {
        let e = MixError::BusNotFound(BusId(7));
        assert!(format!("{e}").contains('7'));
    }

    #[test]
    fn sound_not_found_renders_id() {
        let e = MixError::SoundNotFound(SoundHandle(13));
        assert!(format!("{e}").contains("13"));
    }

    #[test]
    fn bank_full_renders_capacity() {
        let e = MixError::bank_full(64, 64);
        let s = format!("{e}");
        assert!(s.contains("64"));
    }

    #[test]
    fn mixer_full_renders_active_count() {
        let e = MixError::mixer_full(64, 64);
        let s = format!("{e}");
        assert!(s.contains("64 active"));
    }

    #[test]
    fn format_mismatch_renders_both_formats() {
        let e = MixError::format_mismatch(44_100, 2, 48_000, 2);
        let s = format!("{e}");
        assert!(s.contains("44100"));
        assert!(s.contains("48000"));
    }

    #[test]
    fn capture_forbidden_renders_prime_directive_message() {
        let e = MixError::CaptureForbidden;
        let s = format!("{e}");
        assert!(s.contains("PRIME-DIRECTIVE"));
        assert!(s.contains("OUTPUT-only"));
    }

    #[test]
    fn host_audio_wraps_underlying_error() {
        let underlying = cssl_host_audio::AudioError::loader("missing libasound");
        let wrapped = MixError::from(underlying);
        match wrapped {
            MixError::HostAudio(msg) => assert!(msg.contains("missing libasound")),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn is_capture_forbidden_classification() {
        assert!(MixError::CaptureForbidden.is_capture_forbidden());
        assert!(!MixError::invalid("x", "y").is_capture_forbidden());
    }

    #[test]
    fn is_format_mismatch_classification() {
        assert!(
            MixError::format_mismatch(44_100, 2, 48_000, 2).is_format_mismatch()
        );
        assert!(!MixError::CaptureForbidden.is_format_mismatch());
    }
}
