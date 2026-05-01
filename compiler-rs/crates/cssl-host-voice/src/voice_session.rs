//! Voice-session orchestrator : couples a [`crate::AudioRingBuffer`] with
//! a [`crate::SttBackend`] under cap-bit gating, accumulating a
//! [`TranscriptEntry`] queue.
//!
//! § CAP BITS
//!   - `AUD_CAP_MIC_ACCESS` (1) — required to push samples.
//!   - `AUD_CAP_SEND_REMOTE_STT` (2) — required when backend.name()
//!     starts with `"remote-"`.
//!   - `AUD_CAP_LOCAL_STT` (4) — required when backend.name() does NOT
//!     start with `"remote-"`.
//!   - `AUD_CAP_SOVEREIGN` (1<<31) — bypass-bit ; only set by the
//!     sovereign-cap layer when the user has consented at the substrate
//!     level. When set, deny-checks are still RECORDED in the audit
//!     trail (so the user retains visibility) but do not return
//!     `CapDenied`.
//!
//! § ABI-ONLY
//!   This module never touches the OS audio stack. Callers (the host
//!   crate, in wave-5) feed [`VoiceSession::push_audio`] with sample
//!   batches obtained via whatever capture API they choose — the
//!   cap-check + ring + audit happens here.

use crate::audit::{AudioAuditEvent, AudioAuditKind, AudioAuditStatus};
use crate::capture::AudioRingBuffer;
use crate::stt::{SttBackend, SttErr};

/// Permission bit : push raw mic-samples into the ring.
pub const AUD_CAP_MIC_ACCESS: u32 = 1;
/// Permission bit : forward audio to a remote (network) STT backend.
pub const AUD_CAP_SEND_REMOTE_STT: u32 = 2;
/// Permission bit : run STT against a local-process backend.
pub const AUD_CAP_LOCAL_STT: u32 = 4;
/// Sovereign-cap bypass bit (see module docs).
pub const AUD_CAP_SOVEREIGN: u32 = 1 << 31;

/// One transcript event from a successful transcribe call.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptEntry {
    /// ISO-8601 / RFC-3339 UTC timestamp string.
    pub ts_iso: String,
    /// Recognized text.
    pub text: String,
    /// Backend-reported confidence in `[0.0, 1.0]`.
    pub confidence: f32,
    /// Backend-reported wall-clock latency (ms).
    pub latency_ms: u32,
}

/// Voice-session error variants.
#[derive(Debug, Clone, PartialEq)]
pub enum VoiceErr {
    /// Caller lacked the required cap-bit. Carries the missing-bit
    /// constant for diagnostic surfaces.
    CapDenied(u32),
    /// STT backend rejected the audio.
    Stt(SttErr),
}

impl core::fmt::Display for VoiceErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::CapDenied(bit) => write!(f, "voice: cap-denied (bit=0x{bit:x})"),
            Self::Stt(e) => write!(f, "voice: stt: {e}"),
        }
    }
}

impl std::error::Error for VoiceErr {}

impl From<SttErr> for VoiceErr {
    fn from(e: SttErr) -> Self {
        Self::Stt(e)
    }
}

/// Coupled ring + STT-backend + audit-ledger session.
pub struct VoiceSession {
    backend: Box<dyn SttBackend>,
    ring: AudioRingBuffer,
    caps: u32,
    /// Whether the session was constructed with the sovereign-bypass bit.
    /// Tracked separately so audit always reports the underlying decision
    /// even when bypass converts deny → allow.
    sovereign_cap_used: bool,
    transcripts: Vec<TranscriptEntry>,
    audit: Vec<AudioAuditEvent>,
}

impl VoiceSession {
    /// Construct a session with `caps` bitmask. Backend ownership moves
    /// in ; the ring is built from the seconds/sample-rate/channels
    /// arguments.
    #[must_use]
    pub fn new(
        backend: Box<dyn SttBackend>,
        ring_seconds: u32,
        sample_rate_hz: u32,
        channels: u8,
        caps: u32,
    ) -> Self {
        let ring = AudioRingBuffer::new(ring_seconds, sample_rate_hz, channels);
        let sovereign_cap_used = (caps & AUD_CAP_SOVEREIGN) != 0;
        Self {
            backend,
            ring,
            caps,
            sovereign_cap_used,
            transcripts: Vec::new(),
            audit: Vec::new(),
        }
    }

    /// Push raw audio frame. Cap-checks `AUD_CAP_MIC_ACCESS`.
    pub fn push_audio(&mut self, samples: &[f32]) -> Result<(), VoiceErr> {
        let allowed = (self.caps & AUD_CAP_MIC_ACCESS) != 0;
        let bypass = self.sovereign_cap_used;
        if !allowed && !bypass {
            self.audit.push(AudioAuditEvent {
                ts_iso: timestamp_now(),
                kind: AudioAuditKind::CapDenied,
                samples: samples.len() as u64,
                sample_rate_hz: self.ring.sample_rate_hz(),
                sovereign_cap: bypass,
                backend: self.backend.name().into(),
                latency_ms: 0,
                status: AudioAuditStatus::Denied,
            });
            return Err(VoiceErr::CapDenied(AUD_CAP_MIC_ACCESS));
        }
        self.audit.push(AudioAuditEvent {
            ts_iso: timestamp_now(),
            kind: AudioAuditKind::CaptureBegin,
            samples: samples.len() as u64,
            sample_rate_hz: self.ring.sample_rate_hz(),
            sovereign_cap: bypass && !allowed,
            backend: self.backend.name().into(),
            latency_ms: 0,
            status: AudioAuditStatus::Ok,
        });
        self.ring.push_samples(samples);
        self.audit.push(AudioAuditEvent {
            ts_iso: timestamp_now(),
            kind: AudioAuditKind::CaptureEnd,
            samples: samples.len() as u64,
            sample_rate_hz: self.ring.sample_rate_hz(),
            sovereign_cap: bypass && !allowed,
            backend: self.backend.name().into(),
            latency_ms: 0,
            status: AudioAuditStatus::Ok,
        });
        Ok(())
    }

    /// Transcribe the most-recent `seconds` from the ring. Cap-checks
    /// `AUD_CAP_LOCAL_STT` or `AUD_CAP_SEND_REMOTE_STT` per the
    /// backend.name() prefix. Appends a [`TranscriptEntry`] on success
    /// and returns a reference to it.
    pub fn transcribe_recent(&mut self, seconds: f32) -> Result<&TranscriptEntry, VoiceErr> {
        let is_remote = self.backend.name().starts_with("remote-");
        let needed_cap = if is_remote {
            AUD_CAP_SEND_REMOTE_STT
        } else {
            AUD_CAP_LOCAL_STT
        };
        let allowed = (self.caps & needed_cap) != 0;
        let bypass = self.sovereign_cap_used;
        let backend_name = self.backend.name().to_owned();
        let sample_rate = self.ring.sample_rate_hz();
        if !allowed && !bypass {
            self.audit.push(AudioAuditEvent {
                ts_iso: timestamp_now(),
                kind: AudioAuditKind::CapDenied,
                samples: 0,
                sample_rate_hz: sample_rate,
                sovereign_cap: false,
                backend: backend_name,
                latency_ms: 0,
                status: AudioAuditStatus::Denied,
            });
            return Err(VoiceErr::CapDenied(needed_cap));
        }
        let want = ((seconds.max(0.0) * (sample_rate as f32))
            * (self.ring.channel_count() as f32)) as usize;
        let buf = self.ring.most_recent(want);
        self.audit.push(AudioAuditEvent {
            ts_iso: timestamp_now(),
            kind: AudioAuditKind::TranscribeBegin,
            samples: buf.len() as u64,
            sample_rate_hz: sample_rate,
            sovereign_cap: bypass && !allowed,
            backend: backend_name.clone(),
            latency_ms: 0,
            status: AudioAuditStatus::Ok,
        });
        match self.backend.transcribe(&buf, sample_rate) {
            Ok(res) => {
                let entry = TranscriptEntry {
                    ts_iso: timestamp_now(),
                    text: res.text,
                    confidence: res.confidence,
                    latency_ms: res.latency_ms,
                };
                self.transcripts.push(entry);
                self.audit.push(AudioAuditEvent {
                    ts_iso: timestamp_now(),
                    kind: AudioAuditKind::TranscribeEnd,
                    samples: buf.len() as u64,
                    sample_rate_hz: sample_rate,
                    sovereign_cap: bypass && !allowed,
                    backend: backend_name,
                    latency_ms: res.latency_ms,
                    status: AudioAuditStatus::Ok,
                });
                // Last appended is what we just pushed.
                let last = self
                    .transcripts
                    .last()
                    .expect("just pushed an entry above");
                Ok(last)
            }
            Err(e) => {
                self.audit.push(AudioAuditEvent {
                    ts_iso: timestamp_now(),
                    kind: AudioAuditKind::TranscribeEnd,
                    samples: buf.len() as u64,
                    sample_rate_hz: sample_rate,
                    sovereign_cap: bypass && !allowed,
                    backend: backend_name,
                    latency_ms: 0,
                    status: AudioAuditStatus::Failed(e.to_string()),
                });
                Err(VoiceErr::Stt(e))
            }
        }
    }

    /// Borrow the accumulated transcripts (in submission order).
    #[must_use]
    pub fn transcripts(&self) -> &[TranscriptEntry] {
        &self.transcripts
    }

    /// Borrow the audit-event log (in submission order).
    #[must_use]
    pub fn audit_events(&self) -> &[AudioAuditEvent] {
        &self.audit
    }

    /// Borrow the underlying ring (read-only).
    #[must_use]
    pub fn ring(&self) -> &AudioRingBuffer {
        &self.ring
    }

    /// True iff the session was constructed with the sovereign-cap bit.
    #[must_use]
    pub fn sovereign_cap_used(&self) -> bool {
        self.sovereign_cap_used
    }

    /// Granted cap bitmask.
    #[must_use]
    pub fn caps(&self) -> u32 {
        self.caps
    }
}

/// Best-effort RFC-3339 UTC timestamp without pulling chrono.
/// Returns an empty string if the system clock is set before the
/// epoch (which would be a serious environmental bug, not a panic
/// surface for this crate).
fn timestamp_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map_or_else(
        |_| String::new(),
        |d| {
            // Render as seconds-since-epoch with millis ; this is a
            // canonical, monotone string that downstream audit
            // pipelines can parse without timezone disambiguation.
            // Format : "epoch:<seconds>.<millis>"
            format!("epoch:{}.{:03}", d.as_secs(), d.subsec_millis())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::{EchoSttBackend, StubSttBackend};

    fn mk_session(caps: u32) -> VoiceSession {
        VoiceSession::new(
            Box::new(StubSttBackend::new("hello")),
            1,
            16_000,
            1,
            caps,
        )
    }

    #[test]
    fn caps_deny_mic() {
        let mut s = mk_session(AUD_CAP_LOCAL_STT); // no MIC bit
        let frame: Vec<f32> = vec![0.1; 32];
        let r = s.push_audio(&frame);
        assert_eq!(r, Err(VoiceErr::CapDenied(AUD_CAP_MIC_ACCESS)));
        // Audit captured the denial.
        let denials = s
            .audit_events()
            .iter()
            .filter(|e| matches!(e.status, AudioAuditStatus::Denied))
            .count();
        assert_eq!(denials, 1);
    }

    #[test]
    fn caps_allow_mic_permits_push() {
        let mut s = mk_session(AUD_CAP_MIC_ACCESS);
        let frame: Vec<f32> = vec![0.1; 32];
        s.push_audio(&frame).expect("push permitted");
        // Begin + End audit events recorded.
        let begin = s
            .audit_events()
            .iter()
            .filter(|e| matches!(e.kind, AudioAuditKind::CaptureBegin))
            .count();
        let end = s
            .audit_events()
            .iter()
            .filter(|e| matches!(e.kind, AudioAuditKind::CaptureEnd))
            .count();
        assert_eq!(begin, 1);
        assert_eq!(end, 1);
    }

    #[test]
    fn transcribe_with_stub() {
        let mut s = mk_session(AUD_CAP_MIC_ACCESS | AUD_CAP_LOCAL_STT);
        let frame: Vec<f32> = vec![0.1; 32];
        s.push_audio(&frame).unwrap();
        let entry = s.transcribe_recent(0.001).expect("transcribe ok");
        assert_eq!(entry.text, "hello");
        assert!((entry.confidence - 1.0).abs() < 1e-6);
        assert_eq!(s.transcripts().len(), 1);
    }

    #[test]
    fn transcribe_empty_rejected() {
        // No audio pushed → buffer length 0 → backend rejects.
        let mut s = mk_session(AUD_CAP_LOCAL_STT);
        let r = s.transcribe_recent(0.5);
        // EmptyAudio is the expected stt rejection.
        assert_eq!(r, Err(VoiceErr::Stt(SttErr::EmptyAudio)));
    }

    #[test]
    fn transcripts_accumulate() {
        let mut s = mk_session(AUD_CAP_MIC_ACCESS | AUD_CAP_LOCAL_STT);
        let frame: Vec<f32> = vec![0.1; 32];
        s.push_audio(&frame).unwrap();
        s.transcribe_recent(0.001).unwrap();
        s.push_audio(&frame).unwrap();
        s.transcribe_recent(0.001).unwrap();
        assert_eq!(s.transcripts().len(), 2);
    }

    #[test]
    fn stt_error_propagates() {
        // Echo backend rejects too-short ; supply too-short ring.
        let mut s = VoiceSession::new(
            Box::new(EchoSttBackend::new()),
            1,
            16_000,
            1,
            AUD_CAP_MIC_ACCESS | AUD_CAP_LOCAL_STT,
        );
        let frame: Vec<f32> = vec![0.1; 4]; // < 8 minimum
        s.push_audio(&frame).unwrap();
        let r = s.transcribe_recent(0.001);
        assert_eq!(r, Err(VoiceErr::Stt(SttErr::TooShort)));
        // Failure is recorded in audit.
        let failures = s
            .audit_events()
            .iter()
            .filter(|e| matches!(e.status, AudioAuditStatus::Failed(_)))
            .count();
        assert_eq!(failures, 1);
    }

    #[test]
    fn sovereign_cap_bypasses_deny() {
        // Caps deliberately omit MIC + LOCAL_STT — sovereign bypass should
        // still allow the operation.
        let mut s = mk_session(AUD_CAP_SOVEREIGN);
        let frame: Vec<f32> = vec![0.1; 32];
        s.push_audio(&frame).expect("sovereign bypass push");
        let entry = s.transcribe_recent(0.001).expect("sovereign bypass stt");
        assert_eq!(entry.text, "hello");
        assert!(s.sovereign_cap_used());
        // Audit must still record that bypass occurred (sovereign_cap true
        // on at least one CaptureBegin event).
        let bypassed = s
            .audit_events()
            .iter()
            .any(|e| e.sovereign_cap);
        assert!(bypassed);
    }

    #[test]
    fn remote_backend_requires_remote_cap() {
        // A backend whose name starts with "remote-" must be gated by
        // AUD_CAP_SEND_REMOTE_STT, not AUD_CAP_LOCAL_STT.
        struct RemoteStub;
        impl SttBackend for RemoteStub {
            fn name(&self) -> &'static str {
                "remote-vercel-whisper-stub"
            }
            fn transcribe(
                &self,
                _samples: &[f32],
                _sample_rate_hz: u32,
            ) -> Result<crate::stt::SttResult, SttErr> {
                Ok(crate::stt::SttResult {
                    text: "remote".into(),
                    confidence: 0.9,
                    latency_ms: 42,
                    language: "en".into(),
                })
            }
        }
        let mut s = VoiceSession::new(
            Box::new(RemoteStub),
            1,
            16_000,
            1,
            AUD_CAP_MIC_ACCESS | AUD_CAP_LOCAL_STT, // wrong bit !
        );
        let frame: Vec<f32> = vec![0.1; 32];
        s.push_audio(&frame).unwrap();
        let r = s.transcribe_recent(0.001);
        assert_eq!(r, Err(VoiceErr::CapDenied(AUD_CAP_SEND_REMOTE_STT)));

        // Now grant the right cap.
        let mut s2 = VoiceSession::new(
            Box::new(RemoteStub),
            1,
            16_000,
            1,
            AUD_CAP_MIC_ACCESS | AUD_CAP_SEND_REMOTE_STT,
        );
        s2.push_audio(&frame).unwrap();
        let entry = s2.transcribe_recent(0.001).expect("remote cap granted");
        assert_eq!(entry.text, "remote");
        assert_eq!(entry.latency_ms, 42);
    }
}
