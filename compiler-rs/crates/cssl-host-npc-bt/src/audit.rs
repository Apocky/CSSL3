// § audit.rs — own AuditSink trait (decoupled from cssl-host-attestation)
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § FAILURE-MODES + § AXIOMS (∀ NPC-action audit-emit)
// § I> kinds : npc.bt_tick · npc.goap_plan · npc.dialog_choice ·
//              npc.economy_trade · npc.scope_violation (SIG0003)
// § I> Box<dyn AuditSink> — no panics on emit ; poison → silent-pass
// § I> own-sink avoids circular-dep with cssl-host-attestation during scaffold
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Structured audit-event ; `kind` matches the canonical event-name set.
///
/// `attribs` is BTreeMap → deterministic serde output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Canonical event-name e.g. "npc.bt_tick" / "npc.scope_violation".
    pub kind: String,
    /// Attribute bag — sorted-key serialize.
    pub attribs: BTreeMap<String, String>,
}

impl AuditEvent {
    /// Bare event with empty attribs.
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

/// Receiver trait for audit-events. Object-safe ; impls **must not panic**.
pub trait AuditSink: Send + Sync {
    /// Record one event. May no-op (cf. NoopAuditSink).
    fn emit(&self, event: AuditEvent);
}

/// Drop-every-event sink — default before the host wires the real aggregator.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopAuditSink;

impl AuditSink for NoopAuditSink {
    fn emit(&self, _event: AuditEvent) {
        // Intentional no-op.
    }
}

/// Sink that buffers events in-memory ; tests assert against the buffer.
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
        self.events.lock().map(|g| g.clone()).unwrap_or_default()
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

/// Canonical event-kind namespace constants.
pub mod kind {
    /// Per BT-tick lifecycle event.
    pub const BT_TICK: &str = "npc.bt_tick";
    /// Per GOAP plan-call event ; carries depth + ms attribs.
    pub const GOAP_PLAN: &str = "npc.goap_plan";
    /// Per cocreative-bias dialogue selection.
    pub const DIALOG_CHOICE: &str = "npc.dialog_choice";
    /// Per market-trade audit (player-injection or NPC↔NPC).
    pub const ECONOMY_TRADE: &str = "npc.economy_trade";
    /// SIG0003 — Sensitive<biometric|gaze|face|body> input rejected.
    pub const SCOPE_VIOLATION: &str = "npc.scope_violation";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_event_builder_works() {
        let e = AuditEvent::bare(kind::BT_TICK).with("npc_id", "42");
        assert_eq!(e.kind, "npc.bt_tick");
        assert_eq!(e.attribs.get("npc_id"), Some(&String::from("42")));
    }

    #[test]
    fn noop_sink_drops_silently() {
        let s = NoopAuditSink;
        s.emit(AuditEvent::bare("any.kind"));
    }

    #[test]
    fn recording_sink_buffers() {
        let r = RecordingAuditSink::new();
        assert!(r.is_empty());
        r.emit(AuditEvent::bare(kind::BT_TICK));
        r.emit(AuditEvent::bare(kind::GOAP_PLAN));
        assert_eq!(r.len(), 2);
        assert!(r.contains_kind(kind::BT_TICK));
        assert_eq!(r.count_kind(kind::GOAP_PLAN), 1);
    }

    #[test]
    fn audit_event_serde_roundtrip() {
        let e = AuditEvent::bare(kind::SCOPE_VIOLATION)
            .with("sig", "SIG0003")
            .with("input", "biometric");
        let j = serde_json::to_string(&e).expect("ser");
        let back: AuditEvent = serde_json::from_str(&j).expect("de");
        assert_eq!(e, back);
    }
}
