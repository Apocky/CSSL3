//! CSSLv3 stage0 — voice-input ABI surface.
//!
//! § SCOPE
//!   Input-side voice pipeline foundation : cap-gated mic-capture ring +
//!   pluggable STT backend trait + transcript queue + audit-event ledger.
//!
//! § DISJOINT FROM `cssl-host-audio`
//!   The sibling crate `cssl-host-audio` is OUTPUT-side (WASAPI / ALSA /
//!   CoreAudio playback push). This crate is INPUT-side ABI-only — no
//!   real audio-IO crate (no `cpal`, no `whisper-rs`, no `windows-rs`
//!   capture dep). The driving rationale is PRIME_DIRECTIVE-alignment :
//!   microphone capture is a §PROHIBITION class (surveillance) absent
//!   explicit caps, visible UI affordance, and audit-trail. This crate
//!   defines the protocol layer where those checks live ; concrete
//!   audio-IO + STT-model integrations attach via the [`stt::SttBackend`]
//!   trait without modifying this crate.
//!
//! § CAP MODEL
//!   Three permission bits gate the surface :
//!     - `voice_session::AUD_CAP_MIC_ACCESS` — push raw samples into ring.
//!     - `voice_session::AUD_CAP_LOCAL_STT` — submit ring contents to a
//!       local-process STT backend.
//!     - `voice_session::AUD_CAP_SEND_REMOTE_STT` — submit ring contents
//!       to a network-resident STT backend (e.g. Vercel proxy →
//!       Whisper.cpp service).
//!   `VoiceSession::new` records the granted set. Each operation
//!   re-checks the relevant bit ; missing-bit returns
//!   [`voice_session::VoiceErr::CapDenied`] without panicking.
//!
//! § DETERMINISM
//!   The ring buffer is fixed-capacity (caller-picked at construction).
//!   Push semantics are deterministic ring-overwrite — no allocator
//!   churn, no async runtime, no implicit threading. Test-helper
//!   STT backends ([`stt::StubSttBackend`], [`stt::EchoSttBackend`])
//!   produce reproducible outputs for golden-replay tests.
//!
//! § AUDIT
//!   Every capture-gate / transcribe-gate / cap-deny event is recorded
//!   via [`audit::AudioAuditEvent`] which serializes to JSONL via
//!   [`audit::render_jsonl`]. The host wires these into the global
//!   R18 telemetry-ring at integration time (deferred to wave-5).
//!
//! § FORBIDDEN BEHAVIOURS
//!   - silent mic activation                  → CapDenied(MIC_ACCESS)
//!   - audio-loopback / record-output         → no API surface exists
//!   - STT submission without backend-cap     → CapDenied(LOCAL_STT/REMOTE_STT)
//!   - panic on bad input                     → all errors via Result
//!   - unbounded growth                       → ring is fixed-capacity

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
// § PEDANTIC ALLOWS — match precedent set by cssl-host-histograms +
// cssl-host-procgen-rooms : seconds/sample-rate accounting uses f32 ;
// strict equality on test-asserts where 0.0 is a definitive sentinel.
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::float_cmp)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod audit;
pub mod capture;
pub mod stt;
pub mod voice_session;

pub use audit::{render_jsonl, AudioAuditEvent, AudioAuditKind, AudioAuditStatus};
pub use capture::AudioRingBuffer;
pub use stt::{EchoSttBackend, SttBackend, SttErr, SttResult, StubSttBackend};
pub use voice_session::{
    TranscriptEntry, VoiceErr, VoiceSession, AUD_CAP_LOCAL_STT, AUD_CAP_MIC_ACCESS,
    AUD_CAP_SEND_REMOTE_STT,
};

/// Crate-version constant for scaffold verification + audit-event tagging.
pub const STAGE0_VOICE_ABI: &str = env!("CARGO_PKG_VERSION");
