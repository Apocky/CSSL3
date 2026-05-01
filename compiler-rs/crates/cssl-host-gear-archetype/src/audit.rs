//! § Audit-sink trait — own-it-here per spec, avoid circular dep on
//! `cssl-host-attestation` during wave-7 scaffolding ; integration-time
//! aggregator wires the broader audit-chain.
//!
//! § AUDIT-EVENT-NAMES (per GDD § FAILURE-MODES + § UPGRADE-PATH) :
//!   gear.dropped              — drop-table emission (rarity, mat, slot)
//!   gear.transmuted           — successful tier-shift
//!   gear.transmute_rejected   — forbidden / invalid transmute attempt
//!   gear.bonded               — Legendary+ bond-event
//!   gear.bond_rejected        — bond ineligible / already-bonded
//!   gear.rerolled             — affix-slot reroll
//!   gear.reroll_warn_bonded   — reroll on bonded item (warn-only)
//!   gear.leveled              — XP → item-level bump

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;

// ───────────────────────────────────────────────────────────────────────
// § AuditEvent
// ───────────────────────────────────────────────────────────────────────

/// Structured audit-event. `attribs` is `BTreeMap` for deterministic serde.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Canonical event-name per spec § AUDIT-EVENT-NAMES.
    pub kind: String,
    /// Attribute bag. Stable serde-key-order.
    pub attribs: BTreeMap<String, String>,
}

impl AuditEvent {
    /// Construct with no attribs.
    #[must_use]
    pub fn bare(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            attribs: BTreeMap::new(),
        }
    }

    /// Builder-style attrib set.
    #[must_use]
    pub fn with(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.attribs.insert(k.into(), v.into());
        self
    }
}

// ───────────────────────────────────────────────────────────────────────
// § AuditSink trait
// ───────────────────────────────────────────────────────────────────────

/// Object-safe sink. Implementations must NOT panic on any input.
pub trait AuditSink: Send + Sync {
    /// Record one event. May no-op (cf. `NoopAuditSink`).
    fn emit(&self, event: AuditEvent);
}

// ───────────────────────────────────────────────────────────────────────
// § NoopAuditSink
// ───────────────────────────────────────────────────────────────────────

/// Drop-every-event sink. Useful before host wires the real aggregator.
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

/// Buffer every event in-memory. Tests assert against the buffer.
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

    /// Snapshot of events emitted so far.
    #[must_use]
    pub fn events(&self) -> Vec<AuditEvent> {
        self.events.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Number of events.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// True iff buffer empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Iff at least one event has the given kind.
    #[must_use]
    pub fn contains_kind(&self, kind: &str) -> bool {
        self.events
            .lock()
            .map(|g| g.iter().any(|e| e.kind == kind))
            .unwrap_or(false)
    }

    /// Count events with given kind.
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
        // poison → silent-pass per GDD failure-mode philosophy
    }
}
