//! Audit-chain hook stubs per spec § 9.3 + § 13.3.
//!
//! Real audit-chain (BLAKE3 keyed-mode + Ed25519 batch-verify) lands via
//! `cssl-substrate-prime-directive` D131. This slice supplies the hook
//! traits + canonical tag-strings + a [`NullAuditSink`] for tests.
//!
//! ## Tag stability
//!
//! Per spec § 9.3 audit-tags are part of the wire-protocol surface :
//! tools log under a stable `mcp.tool.<name>` namespace. Tag drift is a
//! breaking change for log-pipelines + alert-rules. The constants below
//! capture the foundation tags ; per-tool tags land with their slice.

use crate::cap::CapKind;
use crate::error::McpResult;
use crate::session::SessionId;

/// Canonical audit-tags for the L5 layer. Per spec § 9.3.
///
/// New tags are append-only ; existing tags are immutable.
pub mod tag {
    /// Server boot.
    pub const SERVER_BOOT: &str = "mcp.server.boot";
    /// Server shutdown (graceful).
    pub const SERVER_SHUTDOWN: &str = "mcp.server.shutdown";
    /// Session opened (post-handshake).
    pub const SESSION_OPENED: &str = "mcp.session.opened";
    /// Session closed.
    pub const SESSION_CLOSED: &str = "mcp.session.closed";
    /// Capability granted to a session.
    pub const CAP_GRANTED: &str = "mcp.cap.granted";
    /// Capability revoked from a session.
    pub const CAP_REVOKED: &str = "mcp.cap.revoked";
    /// Capability check refused.
    pub const CAP_DENIED: &str = "mcp.cap.denied";
    /// Tool was invoked successfully.
    pub const TOOL_INVOKED: &str = "mcp.tool.invoked";
    /// Tool dispatch returned an error.
    pub const TOOL_FAILED: &str = "mcp.tool.failed";
    /// Σ-mask refusal.
    pub const SIGMA_REFUSED: &str = "mcp.tool.sigma_refused";
    /// Biometric refusal.
    pub const BIOMETRIC_REFUSED: &str = "mcp.tool.biometric_refused";
    /// Attestation drift detected.
    pub const ATTESTATION_DRIFT: &str = "mcp.attestation.drift";
}

/// Audit event payload. Real BLAKE3-hashed entries land via D131 ; this
/// struct is the in-process hand-off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    /// Stable tag from the [`tag`] module.
    pub tag: &'static str,
    /// Owning session, if any. `None` for pre-session events
    /// (e.g. `SERVER_BOOT`).
    pub session_id: Option<SessionId>,
    /// Sequence number (monotonic per session).
    pub audit_seq: u64,
    /// Frame number @ event-emit.
    pub frame_n: u64,
    /// Capability kind, when relevant (CAP_GRANTED / CAP_REVOKED /
    /// CAP_DENIED).
    pub cap_kind: Option<CapKind>,
    /// Human-readable detail (audit-friendly ; never PII).
    pub detail: String,
}

impl AuditEvent {
    /// Construct a session-scoped event.
    #[must_use]
    pub fn session(
        tag: &'static str,
        session_id: SessionId,
        audit_seq: u64,
        frame_n: u64,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            tag,
            session_id: Some(session_id),
            audit_seq,
            frame_n,
            cap_kind: None,
            detail: detail.into(),
        }
    }

    /// Construct a pre-session event (server-lifecycle).
    #[must_use]
    pub fn server(tag: &'static str, frame_n: u64, detail: impl Into<String>) -> Self {
        Self {
            tag,
            session_id: None,
            audit_seq: 0,
            frame_n,
            cap_kind: None,
            detail: detail.into(),
        }
    }

    /// Add a [`CapKind`] discriminant (for cap-related events).
    #[must_use]
    pub fn with_cap(mut self, cap_kind: CapKind) -> Self {
        self.cap_kind = Some(cap_kind);
        self
    }
}

/// Audit sink — implementors persist or forward events. Real implementation
/// lands via `cssl_substrate_prime_directive::AuditChain` which BLAKE3-chains
/// + Ed25519-signs each entry.
pub trait AuditSink {
    /// Emit one event. Errors surface as [`McpError::AuditHookError`](crate::error::McpError::AuditHookError).
    fn emit(&self, event: AuditEvent) -> McpResult<()>;
}

/// Null sink — discards events. Useful for tests + the stage-0 server when
/// no real chain is wired. Does NOT signal `AuditHookError` ; the audit
/// gate is enforced upstream by the server.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullAuditSink;

impl AuditSink for NullAuditSink {
    fn emit(&self, _event: AuditEvent) -> McpResult<()> {
        Ok(())
    }
}

/// In-memory recording sink — captures events for test introspection.
/// Uses interior mutability via `RefCell` ; not Send. Tests use this to
/// verify the dispatch path emits the expected audit-tags.
#[derive(Debug, Default)]
pub struct VecAuditSink {
    events: core::cell::RefCell<Vec<AuditEvent>>,
}

impl VecAuditSink {
    /// Construct an empty recording sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the recorded events (clone).
    #[must_use]
    pub fn snapshot(&self) -> Vec<AuditEvent> {
        self.events.borrow().clone()
    }

    /// Returns the number of events recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.borrow().len()
    }

    /// True when no events have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.borrow().is_empty()
    }
}

impl AuditSink for VecAuditSink {
    fn emit(&self, event: AuditEvent) -> McpResult<()> {
        self.events.borrow_mut().push(event);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_sink_accepts_events() {
        let sink = NullAuditSink;
        let ev = AuditEvent::server(tag::SERVER_BOOT, 0, "boot");
        sink.emit(ev).expect("null sink ok");
    }

    #[test]
    fn vec_sink_records() {
        let sink = VecAuditSink::new();
        sink.emit(AuditEvent::server(tag::SERVER_BOOT, 0, "boot"))
            .expect("emit");
        sink.emit(AuditEvent::server(tag::SERVER_SHUTDOWN, 1, "down"))
            .expect("emit");
        assert_eq!(sink.len(), 2);
        let snap = sink.snapshot();
        assert_eq!(snap[0].tag, tag::SERVER_BOOT);
        assert_eq!(snap[1].tag, tag::SERVER_SHUTDOWN);
    }

    #[test]
    fn audit_event_with_cap_records_kind() {
        let ev = AuditEvent::session(tag::CAP_GRANTED, SessionId::new(7), 1, 0, "grant")
            .with_cap(CapKind::DevMode);
        assert_eq!(ev.cap_kind, Some(CapKind::DevMode));
        assert_eq!(ev.tag, tag::CAP_GRANTED);
    }

    #[test]
    fn tags_are_stable_strings() {
        assert_eq!(tag::SERVER_BOOT, "mcp.server.boot");
        assert_eq!(tag::SERVER_SHUTDOWN, "mcp.server.shutdown");
        assert_eq!(tag::SESSION_OPENED, "mcp.session.opened");
        assert_eq!(tag::SESSION_CLOSED, "mcp.session.closed");
        assert_eq!(tag::CAP_GRANTED, "mcp.cap.granted");
        assert_eq!(tag::CAP_REVOKED, "mcp.cap.revoked");
        assert_eq!(tag::CAP_DENIED, "mcp.cap.denied");
        assert_eq!(tag::TOOL_INVOKED, "mcp.tool.invoked");
        assert_eq!(tag::TOOL_FAILED, "mcp.tool.failed");
        assert_eq!(tag::SIGMA_REFUSED, "mcp.tool.sigma_refused");
        assert_eq!(tag::BIOMETRIC_REFUSED, "mcp.tool.biometric_refused");
        assert_eq!(tag::ATTESTATION_DRIFT, "mcp.attestation.drift");
    }

    #[test]
    fn vec_sink_starts_empty() {
        let sink = VecAuditSink::new();
        assert!(sink.is_empty());
        assert_eq!(sink.len(), 0);
    }

    #[test]
    fn server_event_has_no_session() {
        let ev = AuditEvent::server(tag::SERVER_BOOT, 0, "boot");
        assert!(ev.session_id.is_none());
    }

    #[test]
    fn session_event_has_session() {
        let ev = AuditEvent::session(tag::SESSION_OPENED, SessionId::new(42), 1, 0, "x");
        assert_eq!(ev.session_id, Some(SessionId::new(42)));
        assert_eq!(ev.audit_seq, 1);
    }
}
