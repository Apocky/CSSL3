//! § wired_voice — wrapper around `cssl-host-voice`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Re-exports the input-side voice pipeline + STT abstraction so MCP
//!   tools can probe the audit-event count without each call-site reaching
//!   across the path-dep. Mic capture is gated by sovereign caps per
//!   PRIME-DIRECTIVE.
//!
//! § wrapped surface
//!   - [`VoiceSession`] — cap-gated mic-capture + transcribe driver.
//!   - [`SttBackend`] / [`StubSttBackend`] / [`EchoSttBackend`] — STT trait.
//!   - [`AudioRingBuffer`] / [`AudioAuditEvent`] — capture + audit shapes.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; cap-gated by design.

pub use cssl_host_voice::{
    render_jsonl, AudioAuditEvent, AudioAuditKind, AudioAuditStatus, AudioRingBuffer,
    EchoSttBackend, SttBackend, SttErr, SttResult, StubSttBackend, VoiceSession,
};

/// Convenience : count the audit events emitted by an optional VoiceSession.
/// Returns 0 if no session is attached. Used by the `voice.audit_count` MCP
/// tool to surface the cap-decision audit trail.
#[must_use]
pub fn audit_event_count(session: Option<&VoiceSession>) -> usize {
    session.map_or(0, |s| s.audit_events().len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_session_yields_zero_count() {
        assert_eq!(audit_event_count(None), 0);
    }

    #[test]
    fn fresh_session_has_zero_audit_events() {
        let backend = Box::new(StubSttBackend::new("hello"));
        let session = VoiceSession::new(backend, 1, 16_000, 1, 0);
        assert_eq!(audit_event_count(Some(&session)), 0);
    }
}
