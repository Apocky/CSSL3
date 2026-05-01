//! § audit_sink.rs — minimal audit-event sink trait.
//!
//! Mirrors the `cssl-host-attestation` audit pattern (event-kind +
//! timestamp + status string) without depending on it (cycle-risk
//! avoidance @ W7-D ; the integration slice can wrap a
//! `cssl-host-attestation` adapter around `RecordingAuditSink` later).

use serde::{Deserialize, Serialize};

/// One audit event emitted by the GM.
///
/// The `kind` strings follow `gm.<verb>` naming (e.g. `gm.text_emit`,
/// `gm.pacing_mark`, `gm.tone_tune`, `gm.cap_denied`). `status` is
/// `"ok"`, `"deny"`, or `"degrade"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub kind: String,
    pub status: String,
    pub ts_micros: u64,
    pub note: String,
}

/// Trait every audit-sink implements.
///
/// `record` MUST NOT panic and MUST NOT block. The default in-tree
/// implementation [`NullAuditSink`] no-ops ; [`RecordingAuditSink`]
/// stores events in a `Vec` for tests + replay.
pub trait AuditSink: Send + Sync {
    /// Append one event.
    fn record(&self, event: AuditEvent);
}

/// No-op sink — discards every event.
#[derive(Debug, Default)]
pub struct NullAuditSink;

impl AuditSink for NullAuditSink {
    fn record(&self, _event: AuditEvent) {
        // no-op
    }
}

/// In-memory sink — keeps every event for inspection.
///
/// The internal `Vec<AuditEvent>` is wrapped in a `Mutex` so the sink
/// is `Send + Sync` without `unsafe`. A simple `parking_lot`-free
/// `std::sync::Mutex` is enough for stage-0.
#[derive(Debug, Default)]
pub struct RecordingAuditSink {
    events: std::sync::Mutex<Vec<AuditEvent>>,
}

impl RecordingAuditSink {
    /// Construct an empty recording sink.
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Snapshot current event-list (clones).
    #[must_use]
    pub fn snapshot(&self) -> Vec<AuditEvent> {
        self.events.lock().map_or_else(|_| Vec::new(), |g| g.clone())
    }

    /// Count events of a given `kind` in this sink.
    #[must_use]
    pub fn count_kind(&self, kind: &str) -> usize {
        self.events
            .lock()
            .map_or(0, |g| g.iter().filter(|e| e.kind == kind).count())
    }
}

impl AuditSink for RecordingAuditSink {
    fn record(&self, event: AuditEvent) {
        if let Ok(mut g) = self.events.lock() {
            g.push(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_sink_swallows_events() {
        let sink = NullAuditSink;
        sink.record(AuditEvent {
            kind: String::from("gm.text_emit"),
            status: String::from("ok"),
            ts_micros: 1,
            note: String::new(),
        });
        // no panic + no observable state — pass.
    }

    #[test]
    fn recording_sink_collects_events() {
        let sink = RecordingAuditSink::new();
        sink.record(AuditEvent {
            kind: String::from("gm.text_emit"),
            status: String::from("ok"),
            ts_micros: 1,
            note: String::new(),
        });
        sink.record(AuditEvent {
            kind: String::from("gm.pacing_mark"),
            status: String::from("ok"),
            ts_micros: 2,
            note: String::new(),
        });
        let snap = sink.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(sink.count_kind("gm.text_emit"), 1);
        assert_eq!(sink.count_kind("gm.pacing_mark"), 1);
        assert_eq!(sink.count_kind("gm.nonexistent"), 0);
    }
}
