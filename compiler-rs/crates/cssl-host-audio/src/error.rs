//! Central error type for the audio host backend.
//!
//! § DESIGN
//!   `AudioError` mirrors the d3d12 / level-zero precedent (cssl-host-d3d12
//!   `D3d12Error`) :
//!     - `LoaderMissing` — backend cfg-gated out OR loader DLL absent
//!       (gate-skip territory ; not a real bug on hosts without audio).
//!     - `Hresult` — Windows-specific HRESULT failure with context.
//!     - `Errno` — Unix-style errno failure with context.
//!     - `OsStatus` — macOS-style OSStatus failure with context.
//!     - `NotSupported` — capability negotiation failed (sample rate,
//!       channel count, format).
//!     - `InvalidArgument` — builder-side precondition.
//!     - `DeviceNotFound` — no default-output device on the host.
//!     - `BufferUnderrun` — recorded but rarely surfaced as a hard error
//!       (the AudioEvent::Underrun stream is the canonical channel).
//!     - `SampleRateMismatch` — requested rate differs from device + no
//!       resampler at stage-0.
//!     - `CaptureNotImplemented` — capture-mode is deferred per
//!       PRIME-DIRECTIVE ; surfaces on any capture-API call.

use thiserror::Error;

/// Errors returned by the audio host backend.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AudioError {
    /// Underlying audio loader is missing or this is a non-supported target.
    #[error("audio loader unavailable — {detail}")]
    LoaderMissing {
        /// Free-form description of which loader is unavailable.
        detail: String,
    },

    /// Stub probe used (no real FFI active).
    #[error("audio FFI not wired (stub probe in use)")]
    FfiNotWired,

    /// A WASAPI / Windows audio call returned a failing HRESULT.
    #[error("audio call `{context}` failed (HRESULT 0x{hresult:08x}): {message}")]
    Hresult {
        /// Free-form description of which call failed.
        context: String,
        /// Raw HRESULT integer.
        hresult: i32,
        /// Human-readable message.
        message: String,
    },

    /// An ALSA / Unix audio call returned a failing errno.
    #[error("audio call `{context}` failed (errno {errno}): {message}")]
    Errno {
        /// Free-form description of which call failed.
        context: String,
        /// POSIX errno value.
        errno: i32,
        /// Human-readable message.
        message: String,
    },

    /// A CoreAudio / macOS call returned a failing OSStatus.
    #[error("audio call `{context}` failed (OSStatus {status}): {message}")]
    OsStatus {
        /// Free-form description of which call failed.
        context: String,
        /// Raw OSStatus integer.
        status: i32,
        /// Human-readable message.
        message: String,
    },

    /// Device or backend does not support the requested feature / format.
    #[error("audio feature unsupported: {feature}")]
    NotSupported {
        /// Which feature / format / capability is missing.
        feature: String,
    },

    /// Builder-side argument precondition failed.
    #[error("audio invalid argument for `{site}`: {reason}")]
    InvalidArgument {
        /// Where the invalid argument was detected.
        site: String,
        /// Why it was rejected.
        reason: String,
    },

    /// No default audio output device on the host.
    #[error("no audio output device found ({reason})")]
    DeviceNotFound {
        /// Why no device was found.
        reason: String,
    },

    /// Buffer underrun recorded as a hard error (rare ; usually surfaces via
    /// [`crate::stream::AudioEvent::Underrun`] instead).
    #[error("audio buffer underrun ({frames_lost} frames lost)")]
    BufferUnderrun {
        /// How many frames the device drained while we were behind.
        frames_lost: u64,
    },

    /// Requested sample-rate could not be negotiated with the device + no
    /// resampler is wired at stage-0.
    #[error("audio sample-rate mismatch (requested {requested} Hz, device {device} Hz)")]
    SampleRateMismatch {
        /// What the caller asked for.
        requested: u32,
        /// What the device exposes.
        device: u32,
    },

    /// Capture-mode (microphone input) is deferred per PRIME-DIRECTIVE.
    /// See module docs on [`crate::lib`](crate) for the consent / UI-affordance
    /// requirements that gate capture-mode.
    #[error(
        "audio capture (microphone) is not implemented at stage-0 — \
         deferred per PRIME-DIRECTIVE consent + UI-affordance gate"
    )]
    CaptureNotImplemented,
}

impl AudioError {
    /// Build a `LoaderMissing` variant.
    #[must_use]
    pub fn loader(detail: impl Into<String>) -> Self {
        Self::LoaderMissing {
            detail: detail.into(),
        }
    }

    /// Build a `Hresult` variant. Convenience for FFI sites.
    #[must_use]
    pub fn hresult(context: impl Into<String>, hresult: i32, message: impl Into<String>) -> Self {
        Self::Hresult {
            context: context.into(),
            hresult,
            message: message.into(),
        }
    }

    /// Build an `Errno` variant.
    #[must_use]
    pub fn errno(context: impl Into<String>, errno: i32, message: impl Into<String>) -> Self {
        Self::Errno {
            context: context.into(),
            errno,
            message: message.into(),
        }
    }

    /// Build an `OsStatus` variant.
    #[must_use]
    pub fn os_status(context: impl Into<String>, status: i32, message: impl Into<String>) -> Self {
        Self::OsStatus {
            context: context.into(),
            status,
            message: message.into(),
        }
    }

    /// Build a `NotSupported` variant.
    #[must_use]
    pub fn unsupported(feature: impl Into<String>) -> Self {
        Self::NotSupported {
            feature: feature.into(),
        }
    }

    /// Build an `InvalidArgument` variant.
    #[must_use]
    pub fn invalid(site: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidArgument {
            site: site.into(),
            reason: reason.into(),
        }
    }

    /// Build a `DeviceNotFound` variant.
    #[must_use]
    pub fn no_device(reason: impl Into<String>) -> Self {
        Self::DeviceNotFound {
            reason: reason.into(),
        }
    }

    /// Build a `SampleRateMismatch` variant.
    #[must_use]
    pub const fn sample_rate_mismatch(requested: u32, device: u32) -> Self {
        Self::SampleRateMismatch { requested, device }
    }

    /// Is this error a "host doesn't have audio" condition (skip-test territory)
    /// rather than a real bug ?
    #[must_use]
    pub const fn is_loader_missing(&self) -> bool {
        matches!(
            self,
            Self::LoaderMissing { .. } | Self::FfiNotWired | Self::DeviceNotFound { .. }
        )
    }

    /// Is this error a capture-deferred condition (PRIME-DIRECTIVE gate) ?
    /// Used by callers who want to surface a clear "feature not yet implemented"
    /// distinct from a real platform failure.
    #[must_use]
    pub const fn is_capture_deferred(&self) -> bool {
        matches!(self, Self::CaptureNotImplemented)
    }
}

/// Crate-wide `Result` alias.
pub type Result<T> = core::result::Result<T, AudioError>;

#[cfg(test)]
mod tests {
    use super::AudioError;

    #[test]
    fn loader_constructor_carries_detail() {
        let e = AudioError::loader("libasound.so.2 not on LD_LIBRARY_PATH");
        match e {
            AudioError::LoaderMissing { detail } => assert!(detail.contains("libasound")),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn hresult_constructor_renders_human_message() {
        // 0x88890008 = AUDCLNT_E_UNSUPPORTED_FORMAT
        let raw_hresult = i32::from_ne_bytes(0x88890008_u32.to_ne_bytes());
        let e = AudioError::hresult(
            "IAudioClient::Initialize",
            raw_hresult,
            "format unsupported",
        );
        let s = format!("{e}");
        assert!(s.contains("IAudioClient::Initialize"));
        assert!(s.contains("0x88890008"));
        assert!(s.contains("format unsupported"));
    }

    #[test]
    fn errno_constructor_renders_human_message() {
        let e = AudioError::errno("snd_pcm_open", 2, "No such file or directory");
        let s = format!("{e}");
        assert!(s.contains("snd_pcm_open"));
        assert!(s.contains("errno 2"));
    }

    #[test]
    fn os_status_constructor_renders_human_message() {
        let e = AudioError::os_status("AudioComponentInstanceNew", -10846, "no component found");
        let s = format!("{e}");
        assert!(s.contains("AudioComponentInstanceNew"));
        assert!(s.contains("OSStatus -10846"));
    }

    #[test]
    fn unsupported_constructor() {
        let e = AudioError::unsupported("8-channel f32 mix");
        assert!(format!("{e}").contains("8-channel"));
    }

    #[test]
    fn invalid_constructor() {
        let e = AudioError::invalid("AudioFormat", "channels=0");
        let s = format!("{e}");
        assert!(s.contains("AudioFormat"));
        assert!(s.contains("channels=0"));
    }

    #[test]
    fn no_device_constructor() {
        let e = AudioError::no_device("default-output not enumerated");
        assert!(format!("{e}").contains("default-output"));
    }

    #[test]
    fn sample_rate_mismatch_carries_both_rates() {
        let e = AudioError::sample_rate_mismatch(48_000, 44_100);
        let s = format!("{e}");
        assert!(s.contains("48000"));
        assert!(s.contains("44100"));
    }

    #[test]
    fn capture_not_implemented_renders_consent_message() {
        let e = AudioError::CaptureNotImplemented;
        let s = format!("{e}");
        assert!(s.contains("PRIME-DIRECTIVE"));
        assert!(s.contains("consent"));
    }

    #[test]
    fn buffer_underrun_carries_frames_lost() {
        let e = AudioError::BufferUnderrun { frames_lost: 256 };
        assert!(format!("{e}").contains("256 frames"));
    }

    #[test]
    fn is_loader_missing_classification() {
        assert!(AudioError::loader("x").is_loader_missing());
        assert!(AudioError::FfiNotWired.is_loader_missing());
        assert!(AudioError::no_device("x").is_loader_missing());
        assert!(!AudioError::unsupported("x").is_loader_missing());
        assert!(!AudioError::invalid("a", "b").is_loader_missing());
    }

    #[test]
    fn is_capture_deferred_classification() {
        assert!(AudioError::CaptureNotImplemented.is_capture_deferred());
        assert!(!AudioError::loader("x").is_capture_deferred());
        assert!(!AudioError::unsupported("x").is_capture_deferred());
    }
}
