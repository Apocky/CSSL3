// audit.rs — audit-emit trait + in-memory log (mock cssl-host-attestation)
// ══════════════════════════════════════════════════════════════════
// § every state-transition + every hard-cap rejection emits an AuditEvent
// § directive-axis = "ImplementationTransparency" (per PRIME_DIRECTIVE.md § 4)
// § real-system : forwards to cssl-host-attestation which BLAKE3-chains + Ed25519-signs
// § this slice : in-memory Vec for tests + a trait so a concrete cssl-host-attestation
//                impl can drop in later without API churn
// ══════════════════════════════════════════════════════════════════

use crate::edit::{CoderEditId, EditState};
use crate::hard_cap::HardCapDecision;
use std::cell::RefCell;

/// Audit event variants. All include a wall-clock millis timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
    /// State-transition recorded.
    StateTransition {
        /// Edit id.
        id: CoderEditId,
        /// Previous state.
        from: EditState,
        /// New state.
        to: EditState,
        /// Wall-clock millis.
        at_ms: u64,
    },
    /// Hard-cap rejection (edit refused at submit-time).
    HardCapRejected {
        /// Path that triggered the rejection.
        target_file: String,
        /// Specific decision.
        decision: HardCapDecision,
        /// Wall-clock millis.
        at_ms: u64,
    },
}

impl AuditEvent {
    /// Construct a state-transition event.
    pub fn state_transition(id: CoderEditId, from: EditState, to: EditState, at_ms: u64) -> Self {
        Self::StateTransition { id, from, to, at_ms }
    }

    /// Construct a hard-cap rejection event.
    pub fn hard_cap_rejected(target_file: String, decision: HardCapDecision, at_ms: u64) -> Self {
        Self::HardCapRejected {
            target_file,
            decision,
            at_ms,
        }
    }

    /// Directive-axis (per PRIME_DIRECTIVE.md § 4 TRANSPARENCY).
    pub const fn directive_axis(&self) -> &'static str {
        "ImplementationTransparency"
    }
}

/// Audit-log trait. Real impls forward to `cssl-host-attestation`'s
/// BLAKE3-chained + Ed25519-signed log.
pub trait AuditLog: std::fmt::Debug {
    /// Emit one audit event.
    fn emit(&self, event: AuditEvent);
    /// Snapshot (for tests / observability).
    fn snapshot(&self) -> Vec<AuditEvent>;
}

/// In-memory mock used in tests + as a default until `cssl-host-attestation` lands.
#[derive(Debug, Default)]
pub struct InMemoryAuditLog {
    events: RefCell<Vec<AuditEvent>>,
}

impl InMemoryAuditLog {
    /// Create an empty log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of events emitted so far.
    pub fn len(&self) -> usize {
        self.events.borrow().len()
    }

    /// Returns true if no events have been emitted.
    pub fn is_empty(&self) -> bool {
        self.events.borrow().is_empty()
    }
}

impl AuditLog for InMemoryAuditLog {
    fn emit(&self, event: AuditEvent) {
        self.events.borrow_mut().push(event);
    }
    fn snapshot(&self) -> Vec<AuditEvent> {
        self.events.borrow().clone()
    }
}
