//! CSSLv3 stage0 — audio host backend.
//!
//! § SPEC : `specs/14_BACKEND.csl § AUDIO HOST BACKENDS` +
//!          `specs/22_TELEMETRY.csl § AUDIO-OPS` +
//!          `specs/04_EFFECTS.csl § BUILT-IN EFFECTS § Realtime<p>`.
//!
//! § STRATEGY (T11-D81, S7-F3)
//!   Stage-0 audio host abstraction with three cfg-gated platform impls :
//!     - **WASAPI** on Windows via `windows-rs 0.58`
//!       (IAudioClient + IAudioRenderClient ; shared-mode mix-format
//!       negotiation + exclusive-mode opt-in).
//!     - **ALSA + PulseAudio** on Linux via `libloading` dynamic-load
//!       (libpulse.so.0 preferred ; libasound.so.2 fallback ;
//!       gate-skip when neither present).
//!     - **CoreAudio** on macOS via `libloading` dynamic-load
//!       (AudioToolbox.framework + AudioUnit default-output).
//!
//! § CANONICAL FORMAT
//!   `f32` interleaved across channels. The platform layer converts to
//!   the device-native format (typically i16 / i24 / f32) at the
//!   boundary. Sample-rate mismatches that cannot be negotiated produce
//!   `AudioError::SampleRateMismatch` — linear-resampling at the
//!   boundary is deferred to a stdlib DSP slice.
//!
//! § PUSH MODE (stage-0)
//!   Caller submits `&[f32]` interleaved buffers via
//!   [`stream::AudioStream::submit_frames`]. The platform layer feeds
//!   the device through its native push surface (WASAPI's
//!   `GetBuffer` / `ReleaseBuffer`, ALSA's `snd_pcm_writei`,
//!   CoreAudio's render-callback closure populated from a ring).
//!   Pull-mode (callback registered with the platform) is deferred to
//!   a follow-up slice.
//!
//! § CAPTURE MODE — DEFERRED PER PRIME-DIRECTIVE
//!   Microphone capture is **not implemented at stage-0**. Per
//!   `PRIME_DIRECTIVE.md § PROHIBITIONS § surveillance`, silent
//!   microphone activation is a BUG class. When capture lands, it MUST
//!   be gated behind :
//!     - `{Sensitive<"microphone">}` effect on the surface,
//!     - a visible UI affordance + revocable consent contract,
//!     - audit-policy entry in the R18 telemetry-ring.
//!   Audio-loopback to system-output-recording is **forbidden** at the
//!   API level — no `record-what-is-playing` surface exists.
//!
//! § THREAD AFFINITY
//!   On Windows, WASAPI requires that the calling thread has called
//!   `CoInitializeEx(COINIT_MULTITHREADED)` before any IAudioClient
//!   call. The `platform::wasapi::CoInitGuard` RAII helper enforces
//!   this on stream open + tears down on drop. Callers who already
//!   manage COM externally pass `coinit_managed = true` to skip the
//!   guard.
//!
//! § UNDERRUN POLICY
//!   When the device drains the platform buffer faster than `submit_frames`
//!   can refill, an [`AudioEvent::Underrun`] is recorded. The default
//!   policy is **never silent-drop** : the underrun counter increments
//!   and the event surfaces in the AudioStream's `drain_events` queue.
//!   Future telemetry-ring integration (T11-D52 / R18) will propagate
//!   underruns to the global ring.

#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::needless_pass_by_value)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

pub mod error;
pub mod format;
pub mod platform;
pub mod ring;
pub mod stream;

pub use error::{AudioError, Result};
pub use format::{AudioFormat, ChannelLayout, SampleRate};
pub use ring::{RingBuffer, RingBufferConfig};
pub use stream::{AudioEvent, AudioStream, AudioStreamConfig, ShareMode};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
