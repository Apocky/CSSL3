//! Speech-to-text backend trait + ABI types + test-helper backends.
//!
//! § PHILOSOPHY
//!   This module is intentionally model-agnostic. The crate ships **no**
//!   real STT — concrete backends (Whisper local, Whisper remote-proxy,
//!   Vosk, future ones) implement [`SttBackend`] in their own crate.
//!   Two test helpers ([`StubSttBackend`], [`EchoSttBackend`]) live here
//!   so unit tests, golden replays, and offline development never need
//!   network access or a model file.
//!
//! § ERROR DISCIPLINE
//!   Failures are returned as [`SttErr`] variants ; no panics, no
//!   unwraps, no silent dropping of caller audio. Network and
//!   model-load failures carry their own stringly-typed message slots
//!   so concrete backends can surface vendor-specific diagnostics.

/// Result of a successful transcribe call.
#[derive(Debug, Clone, PartialEq)]
pub struct SttResult {
    /// Recognized text. UTF-8 only (caller responsibility to upstream).
    pub text: String,
    /// Best-effort confidence in `[0.0, 1.0]`. Backends without a real
    /// confidence model should return `1.0` for stub-canned outputs.
    pub confidence: f32,
    /// Wall-clock latency the backend reports (ms).
    pub latency_ms: u32,
    /// IETF BCP-47 language tag e.g. "en", "ja", "ko".
    pub language: String,
}

/// STT failure modes.
#[derive(Debug, Clone, PartialEq)]
pub enum SttErr {
    /// Caller submitted a zero-length audio slice.
    EmptyAudio,
    /// Backend cannot serve at this moment (deactivated, paused, etc).
    BackendUnavailable,
    /// Loading the model failed ; carries a vendor-specific message.
    ModelLoadFailed(String),
    /// Network call failed (only for remote backends).
    NetworkErr(String),
    /// Audio shorter than backend's minimum window.
    TooShort,
    /// Audio longer than backend's maximum window.
    TooLong,
}

impl core::fmt::Display for SttErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyAudio => write!(f, "stt: empty audio buffer"),
            Self::BackendUnavailable => write!(f, "stt: backend unavailable"),
            Self::ModelLoadFailed(m) => write!(f, "stt: model-load failed: {m}"),
            Self::NetworkErr(m) => write!(f, "stt: network error: {m}"),
            Self::TooShort => write!(f, "stt: audio too short"),
            Self::TooLong => write!(f, "stt: audio too long"),
        }
    }
}

impl std::error::Error for SttErr {}

/// Pluggable STT backend.
///
/// Implementations MUST be thread-safe (`Send + Sync`) so a single
/// backend instance can serve multiple voice sessions. Implementations
/// MUST NOT panic ; failures are reported via [`SttErr`].
///
/// `name()` returns a stable identifier the host uses to decide cap
/// routing : a name beginning with `"remote-"` is gated by
/// `AUD_CAP_SEND_REMOTE_STT` ; any other name is gated by
/// `AUD_CAP_LOCAL_STT`.
pub trait SttBackend: Send + Sync {
    /// Stable identifier (e.g. "stub", "echo", "whisper-local",
    /// "remote-vercel-whisper").
    fn name(&self) -> &str;

    /// Transcribe `samples` (interleaved f32 PCM) at `sample_rate_hz`.
    fn transcribe(&self, samples: &[f32], sample_rate_hz: u32) -> Result<SttResult, SttErr>;
}

/// Test helper : returns a fixed canned transcript regardless of audio
/// content. Useful for unit tests and golden replays.
#[derive(Debug, Clone)]
pub struct StubSttBackend {
    canned_text: String,
}

impl StubSttBackend {
    /// Construct with the canned text the backend will always return.
    #[must_use]
    pub fn new(canned_text: impl Into<String>) -> Self {
        Self {
            canned_text: canned_text.into(),
        }
    }
}

impl SttBackend for StubSttBackend {
    fn name(&self) -> &'static str {
        "stub"
    }

    fn transcribe(&self, samples: &[f32], _sample_rate_hz: u32) -> Result<SttResult, SttErr> {
        if samples.is_empty() {
            return Err(SttErr::EmptyAudio);
        }
        Ok(SttResult {
            text: self.canned_text.clone(),
            confidence: 1.0,
            latency_ms: 0,
            language: "en".into(),
        })
    }
}

/// Test helper : returns a deterministic format string describing the
/// audio it received. Useful for verifying a session passed the right
/// data through to its backend.
#[derive(Debug, Clone, Default)]
pub struct EchoSttBackend;

impl EchoSttBackend {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl SttBackend for EchoSttBackend {
    fn name(&self) -> &'static str {
        "echo"
    }

    fn transcribe(&self, samples: &[f32], sample_rate_hz: u32) -> Result<SttResult, SttErr> {
        if samples.is_empty() {
            return Err(SttErr::EmptyAudio);
        }
        // Reject pathologically-tiny inputs so the "too short" path is
        // exercised in tests without depending on real frame thresholds.
        if samples.len() < 8 {
            return Err(SttErr::TooShort);
        }
        Ok(SttResult {
            text: format!("(audio: {} samples, {} Hz)", samples.len(), sample_rate_hz),
            confidence: 1.0,
            latency_ms: 0,
            language: "en".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_returns_canned() {
        let b = StubSttBackend::new("hello world");
        let r = b
            .transcribe(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8], 16_000)
            .expect("stub ok");
        assert_eq!(r.text, "hello world");
        assert_eq!(b.name(), "stub");
        assert!((r.confidence - 1.0).abs() < 1e-6);
    }

    #[test]
    fn echo_returns_format() {
        let b = EchoSttBackend::new();
        let samples: Vec<f32> = vec![0.0; 32];
        let r = b.transcribe(&samples, 16_000).expect("echo ok");
        assert_eq!(r.text, "(audio: 32 samples, 16000 Hz)");
        assert_eq!(b.name(), "echo");
    }

    #[test]
    fn empty_audio_rejected() {
        let stub = StubSttBackend::new("X");
        let echo = EchoSttBackend::new();
        assert_eq!(stub.transcribe(&[], 16_000), Err(SttErr::EmptyAudio));
        assert_eq!(echo.transcribe(&[], 16_000), Err(SttErr::EmptyAudio));
    }

    #[test]
    fn too_short_rejected() {
        let echo = EchoSttBackend::new();
        // 7 < 8 minimum
        let r = echo.transcribe(&[0.1; 7], 16_000);
        assert_eq!(r, Err(SttErr::TooShort));
    }

    #[test]
    fn err_display_formats() {
        // bonus : exercise Display for coverage of the error-shape.
        assert!(SttErr::EmptyAudio.to_string().contains("empty"));
        assert!(SttErr::ModelLoadFailed("x".into())
            .to_string()
            .contains("model-load"));
        assert!(SttErr::NetworkErr("oops".into())
            .to_string()
            .contains("network"));
    }
}
