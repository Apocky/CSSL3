// § audit.rs · AuditEmitter trait + stderr/vec impls
// ══════════════════════════════════════════════════════════════════════════════
// § I> AuditEmitter abstracts cssl-host-attestation (W6+ sibling) audit-bus.
//   ¬ direct-dep · trait-shape designed to match ; integration-time wiring.
// § I> StderrAuditEmitter : dev-fallback writes serde-JSON to stderr
// § I> VecAuditEmitter : test-fixture captures events for assertion
// ══════════════════════════════════════════════════════════════════════════════
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::disagreement::DisagreementFlag;

/// Audit-event variants emitted by the validator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind")]
pub enum AuditEvent {
    /// A consensus disagreement was detected and flagged.
    DisagreementFlagged(DisagreementFlag),
    /// Tampering (signature failure) was detected.
    TamperDetected {
        event_id: [u8; 32],
        emitter_pubkey: [u8; 32],
        reason: String,
    },
    /// An untrusted-ts event was rejected (no/stale ServerTick).
    UntrustedTsRejected { event_id: [u8; 32], ts: u64 },
    /// A tick monotonic-violation was observed (release-only).
    MonotonicViolation { prev: u64, next: u64 },
}

/// Generic audit-emitter abstraction.
///
/// Implementations forward audit-events to an attestation-bus, log-pipe, or
/// in-memory test buffer. MUST NOT silently drop on disagreement.
pub trait AuditEmitter {
    /// Emit one audit-event ; never silently drops.
    fn emit(&self, event: AuditEvent);
}

/// Stderr-fallback emitter — used when no `cssl-host-attestation` is wired.
#[derive(Debug, Default)]
pub struct StderrAuditEmitter;

impl AuditEmitter for StderrAuditEmitter {
    fn emit(&self, event: AuditEvent) {
        // Debug-format keeps lib-side dep-free ; richer JSON-emit lives in
        // downstream wiring (see cssl-host-attestation integration in W6+).
        eprintln!("[coherence-proof::audit] {event:?}");
    }
}

/// In-memory emitter — used by tests + harnesses to assert emission.
#[derive(Debug, Default)]
pub struct VecAuditEmitter {
    events: Mutex<Vec<AuditEvent>>,
}

impl VecAuditEmitter {
    /// Construct an empty emitter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the captured events (clones).
    pub fn snapshot(&self) -> Vec<AuditEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Number of captured events.
    pub fn len(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    /// True iff no events captured.
    pub fn is_empty(&self) -> bool {
        self.events.lock().unwrap().is_empty()
    }
}

impl AuditEmitter for VecAuditEmitter {
    fn emit(&self, event: AuditEvent) {
        self.events.lock().unwrap().push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disagreement::DisagreementReason;

    #[test]
    fn vec_emitter_captures_events() {
        let emitter = VecAuditEmitter::new();
        assert!(emitter.is_empty());
        emitter.emit(AuditEvent::TamperDetected {
            event_id: [1u8; 32],
            emitter_pubkey: [2u8; 32],
            reason: "bad-sig".into(),
        });
        assert_eq!(emitter.len(), 1);
    }

    #[test]
    fn audit_event_serde_round_trip_disagreement() {
        let flag = DisagreementFlag {
            event_id: [9u8; 32],
            expected_root: [1u8; 32],
            actual_root: [2u8; 32],
            flagger_pubkey: [3u8; 32],
            reason: DisagreementReason::MerkleMismatch,
        };
        let ev = AuditEvent::DisagreementFlagged(flag);
        let json = serde_json::to_string(&ev).unwrap();
        let de: AuditEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, de);
    }
}
