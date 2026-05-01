//! Audit-sink trait : the DM emits structured `AuditEvent`s on every
//! cap-gated action so the per-session attestation can be reconstructed.
//!
//! § SPEC : `specs/grand-vision/10_INTELLIGENCE.csl` § AUDIT-EVENT-NAMES
//!
//! The DM owns its own minimal `AuditSink` trait (rather than depending on
//! `cssl-host-attestation` directly) to avoid a circular-dep hazard during
//! wave-7 scaffolding ; the host wires this into the broader audit-aggregator
//! at integration-time.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;

// ───────────────────────────────────────────────────────────────────────
// § AuditEvent
// ───────────────────────────────────────────────────────────────────────

/// Structured audit-event. `kind` matches the spec § AUDIT-EVENT-NAMES set
/// (e.g. `"dm.scene_edit"`, `"dm.spawn_order"`, `"handoff.dm_to_gm"`).
///
/// `attribs` is a `BTreeMap` for deterministic serde output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Canonical event-name (e.g. `"dm.scene_edit"`).
    pub kind: String,
    /// Attribute bag — deterministic serde-stable.
    pub attribs: BTreeMap<String, String>,
}

impl AuditEvent {
    /// Construct an event with zero attribs.
    #[must_use]
    pub fn bare(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            attribs: BTreeMap::new(),
        }
    }

    /// Builder-style attrib insertion.
    #[must_use]
    pub fn with(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.attribs.insert(k.into(), v.into());
        self
    }
}

// ───────────────────────────────────────────────────────────────────────
// § AuditSink trait
// ───────────────────────────────────────────────────────────────────────

/// Trait implemented by every receiver of DM audit-events.
///
/// Object-safe — the DM stores `Box<dyn AuditSink>`. Implementations
/// **must not panic** on any input ; failure-modes are silent-pass.
pub trait AuditSink: Send + Sync {
    /// Record a single event. May no-op (cf. `NoopAuditSink`).
    fn emit(&self, event: AuditEvent);
}

// ───────────────────────────────────────────────────────────────────────
// § NoopAuditSink
// ───────────────────────────────────────────────────────────────────────

/// Drop-every-event sink. Useful in tests + as the default before the host
/// wires the real attestation aggregator.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopAuditSink;

impl AuditSink for NoopAuditSink {
    fn emit(&self, _event: AuditEvent) {
        // Intentional no-op.
    }
}

// ───────────────────────────────────────────────────────────────────────
// § RecordingAuditSink
// ───────────────────────────────────────────────────────────────────────

/// Sink that buffers every emitted event in-memory ; tests assert against
/// the buffer.
///
/// Internally `Mutex<Vec<AuditEvent>>` so the sink is `Sync` while still
/// owning a growable buffer.
#[derive(Debug, Default)]
pub struct RecordingAuditSink {
    events: Mutex<Vec<AuditEvent>>,
}

impl RecordingAuditSink {
    /// Construct an empty recorder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    /// Snapshot of every event emitted so far.
    #[must_use]
    pub fn events(&self) -> Vec<AuditEvent> {
        self.events
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Number of events emitted so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// True iff no events have been emitted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// True iff at least one recorded event has the given `kind`.
    #[must_use]
    pub fn contains_kind(&self, kind: &str) -> bool {
        self.events
            .lock()
            .map(|g| g.iter().any(|e| e.kind == kind))
            .unwrap_or(false)
    }

    /// Count of events whose `kind == kind`.
    #[must_use]
    pub fn count_kind(&self, kind: &str) -> usize {
        self.events
            .lock()
            .map(|g| g.iter().filter(|e| e.kind == kind).count())
            .unwrap_or(0)
    }
}

impl AuditSink for RecordingAuditSink {
    fn emit(&self, event: AuditEvent) {
        if let Ok(mut g) = self.events.lock() {
            g.push(event);
        }
        // poison → silent-pass per spec failure-mode philosophy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_event_builder() {
        let e = AuditEvent::bare("dm.scene_edit").with("zone", "atrium-7");
        assert_eq!(e.kind, "dm.scene_edit");
        assert_eq!(e.attribs.get("zone"), Some(&String::from("atrium-7")));
    }

    #[test]
    fn noop_sink_drops_silently() {
        let s = NoopAuditSink;
        s.emit(AuditEvent::bare("any.kind"));
        // No assertion possible — just check it compiles + doesn't panic.
    }

    #[test]
    fn recording_sink_buffers() {
        let r = RecordingAuditSink::new();
        assert!(r.is_empty());
        r.emit(AuditEvent::bare("dm.scene_edit"));
        r.emit(AuditEvent::bare("dm.spawn_order"));
        assert_eq!(r.len(), 2);
        assert!(r.contains_kind("dm.scene_edit"));
        assert_eq!(r.count_kind("dm.spawn_order"), 1);
        assert_eq!(r.count_kind("nonexistent.kind"), 0);
    }

    #[test]
    fn audit_event_serde_round_trip() {
        let e = AuditEvent::bare("handoff.dm_to_gm")
            .with("trace_id", "42")
            .with("payload", "abc");
        let j = serde_json::to_string(&e).expect("serialize");
        let back: AuditEvent = serde_json::from_str(&j).expect("deserialize");
        assert_eq!(e, back);
    }
}
