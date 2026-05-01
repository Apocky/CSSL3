// § audit : own AuditSink trait + RecordingAuditSink for tests
// ══════════════════════════════════════════════════════════════════
//! Local audit-sink abstraction (NO external substrate-deps per crate constraints).
//!
//! Per `GDDs/CRAFT_DECONSTRUCT_ALCHEMY.csl § AXIOMS` :
//!   t∞: ∀ craft ⇒ Audit<>-emit (anti-cheat anchor + lineage-trace)
//!
//! Events :
//! - `craft.completed`   — successful recipe-evaluation
//! - `craft.failed`      — recipe-evaluation failure (skill, materials, etc.)
//! - `craft.deconstruct` — deconstruct executed
//! - `craft.transmute`   — transmute attempted (success or fail)
//! - `craft.brew`        — alchemy brew (success or fail)
//!
//! The downstream `cssl-host-attestation` crate will consume these events ; we
//! keep the trait local-and-narrow to avoid cross-crate substrate coupling.

use serde::{Deserialize, Serialize};

/// § AuditEvent : tagged event with payload string (JSON-encoded by emitter).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub kind: AuditKind,
    pub payload: String,
}

/// § AuditKind : enum of supported event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuditKind {
    CraftCompleted,
    CraftFailed,
    CraftDeconstruct,
    CraftTransmute,
    CraftBrew,
}

impl AuditKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AuditKind::CraftCompleted => "craft.completed",
            AuditKind::CraftFailed => "craft.failed",
            AuditKind::CraftDeconstruct => "craft.deconstruct",
            AuditKind::CraftTransmute => "craft.transmute",
            AuditKind::CraftBrew => "craft.brew",
        }
    }
}

/// § AuditSink : write-only event consumer. Implementations may persist,
/// forward to attestation, or discard. The contract is fire-and-forget.
pub trait AuditSink {
    fn emit(&mut self, event: AuditEvent);
}

/// § NoopAuditSink : production default if attestation not wired. Drops events.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopAuditSink;

impl AuditSink for NoopAuditSink {
    fn emit(&mut self, _event: AuditEvent) {}
}

/// § RecordingAuditSink : test-helper that retains all events in-memory.
#[derive(Debug, Clone, Default)]
pub struct RecordingAuditSink {
    pub events: Vec<AuditEvent>,
}

impl RecordingAuditSink {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn count_kind(&self, kind: AuditKind) -> usize {
        self.events.iter().filter(|e| e.kind == kind).count()
    }
}

impl AuditSink for RecordingAuditSink {
    fn emit(&mut self, event: AuditEvent) {
        self.events.push(event);
    }
}

/// § emit_helper : wrap a kind+payload into an event and dispatch.
pub fn emit<S: AuditSink>(sink: &mut S, kind: AuditKind, payload: impl Into<String>) {
    sink.emit(AuditEvent {
        kind,
        payload: payload.into(),
    });
}
