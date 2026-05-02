//! § telemetry — sovereignty-respecting telemetry sink.
//!
//! Events emitted by `HotfixClient` :
//!   • `Checked`     — manifest fetched + verified.
//!   • `Downloaded`  — bundle bytes fetched + verified.
//!   • `Applied`     — apply succeeded.
//!   • `RolledBack`  — apply failed → reverted.
//!   • `Skipped`     — Σ-mask deny / pinned / already-current.
//!   • `Revoked`     — locally-installed version was on revocation list.
//!   • `ApplyFailed` — failure with reason ; sink may forward to apocky.com.
//!
//! Per the Σ-mask sovereignty axiom the SINK is responsible for honouring
//! consent : the client always emits, but a sink wired to the
//! per-user-consent layer drops events for which the user has not opted
//! in to telemetry.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// § One telemetry event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HotfixEvent {
    Checked {
        ts_ns: u64,
    },
    Downloaded {
        channel: String,
        version: String,
        size_bytes: u64,
        ts_ns: u64,
    },
    Applied {
        channel: String,
        version: String,
        ts_ns: u64,
    },
    RolledBack {
        channel: String,
        from_version: String,
        to_version: String,
        reason: String,
        ts_ns: u64,
    },
    Skipped {
        channel: String,
        reason: String,
        ts_ns: u64,
    },
    Revoked {
        channel: String,
        version: String,
        ts_ns: u64,
    },
    ApplyFailed {
        channel: String,
        version: String,
        error: String,
        ts_ns: u64,
    },
}

/// § Sink trait. Implementations may forward to log, file, audit-row, or
/// HTTP. The default in tests captures events into a vec.
pub trait TelemetrySink: Send + Sync {
    fn emit(&self, ev: HotfixEvent);
}

/// § Mock sink that records all events for inspection.
#[derive(Default)]
pub struct MockTelemetrySink {
    pub events: Mutex<Vec<HotfixEvent>>,
}

impl MockTelemetrySink {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    #[must_use]
    pub fn count(&self) -> usize {
        self.events.lock().unwrap().len()
    }
    pub fn snapshot(&self) -> Vec<HotfixEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl TelemetrySink for MockTelemetrySink {
    fn emit(&self, ev: HotfixEvent) {
        self.events.lock().unwrap().push(ev);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_sink_records_events() {
        let s = MockTelemetrySink::new();
        s.emit(HotfixEvent::Checked { ts_ns: 1 });
        s.emit(HotfixEvent::Skipped {
            channel: "cssl.bundle".to_string(),
            reason: "off".to_string(),
            ts_ns: 2,
        });
        assert_eq!(s.count(), 2);
        let snap = s.snapshot();
        assert!(matches!(snap[0], HotfixEvent::Checked { .. }));
    }

    #[test]
    fn event_serde_roundtrip() {
        let e = HotfixEvent::Applied {
            channel: "security.patch".to_string(),
            version: "1.0.1".to_string(),
            ts_ns: 42,
        };
        let s = serde_json::to_string(&e).unwrap();
        let back: HotfixEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(e, back);
    }
}
