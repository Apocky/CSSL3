//! Audit log : every cap-mutation + visitor-action is recorded.
//!
//! This is a **stub for cssl-host-attestation FFI** — once that crate is
//! published as a workspace-dep, the [`AuditLog::flush`] method will fan
//! events out to it. For now the log is held in-memory and round-tripped
//! through serde for snapshot tests.

use crate::ids::{Pubkey, Timestamp};
use serde::{Deserialize, Serialize};

/// Discriminator for what an [`AuditEvent`] represents.
///
/// Variants are append-only — never reorder + never delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AuditKind {
    /// Home was constructed.
    Created,
    /// Owner changed the access-mode.
    ModeChanged,
    /// Owner granted a cap-bit set.
    CapGranted,
    /// Owner revoked a cap-bit set.
    CapRevoked,
    /// Owner changed archetype.
    ArchetypeChanged,
    /// A visitor entered the Home.
    VisitorEntered,
    /// A visitor was disconnected (mode revocation or owner action).
    VisitorEjected,
    /// A decoration was placed.
    DecorationPlaced,
    /// A decoration was removed.
    DecorationRemoved,
    /// A trophy was pinned.
    TrophyPinned,
    /// A trophy was unpinned.
    TrophyUnpinned,
    /// A companion was added.
    CompanionAdded,
    /// A companion was dismissed.
    CompanionDismissed,
    /// A portal was registered.
    PortalRegistered,
    /// A portal was disabled.
    PortalDisabled,
    /// A forge-recipe was queued.
    ForgeQueued,
    /// A forge-queue item was cancelled.
    ForgeCancelled,
    /// A memorial-ascription was posted.
    MemorialPosted,
    /// Mycelial-terminal opt-in toggled.
    MycelialOptToggled,
}

/// One audit-log entry. Serialized form is the canonical FFI shape.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AuditEvent {
    /// What kind of mutation this was.
    pub kind: AuditKind,
    /// Caller-supplied monotonic timestamp.
    pub at: Timestamp,
    /// Pubkey that triggered the event (owner or visitor).
    pub actor: Pubkey,
    /// Free-form short note for human-readable transparency.
    pub note: String,
}

/// Append-only event log held inside a [`crate::Home`].
///
/// Iteration is insertion-ordered (a `Vec` ; events are timestamped so this
/// is sufficient — sorting by `at` would lose ties at the same tick).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditLog {
    /// Events in insertion order.
    pub events: Vec<AuditEvent>,
}

impl AuditLog {
    /// New empty log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append one event.
    pub fn push(&mut self, ev: AuditEvent) {
        self.events.push(ev);
    }

    /// Number of recorded events.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the log is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Iterate events in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &AuditEvent> {
        self.events.iter()
    }

    /// Filter events of a specific kind, in insertion order.
    pub fn of_kind(&self, kind: AuditKind) -> impl Iterator<Item = &AuditEvent> {
        self.events.iter().filter(move |e| e.kind == kind)
    }
}
