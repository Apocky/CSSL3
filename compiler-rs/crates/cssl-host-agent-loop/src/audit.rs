//! § audit — port + event types for sovereign-bypass-RECORDED audit-emit.
//!
//! § Axes :
//!   - `ImplementationTransparency` — every dispatch route emits this so the
//!     host can render "what just happened" without filesystem polling.
//!   - `Transparency`               — generic transparency emissions for
//!     non-implementation events (e.g. context-fetch).
//!   - `Sovereignty`                — emitted BEFORE every mutation.
//!   - `Cocreative`                 — emitted on Collaborator-handoff turns.
//!   - `CapBypass`                  — emitted whenever a `Sovereign*` cap
//!     bypass is RECORDED.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// Audit-port — the loop emits events ; the host's audit-sink persists.
///
/// Implementations MUST be `Send + Sync` because the loop holds the port
/// behind an `Arc<dyn AuditPort>` for cheap cloning across turns.
pub trait AuditPort: Send + Sync {
    /// Emit a single audit event. Implementations are expected to be
    /// non-blocking (queue-and-return) so the loop's hot-path is fast.
    fn emit(&self, event: AuditEvent);
}

/// A single audit-row. The `payload` is `serde_json::Value` so different
/// axes can carry different shapes without adding enum-variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Turn this event belongs to.
    pub turn_id: u64,
    /// Phase string (`TurnPhase::as_str`) at the moment of emission.
    pub phase: &'static str,
    /// Axis discriminator — drives the audit-sink's bucket selection.
    pub axis: AuditAxis,
    /// Per-axis payload (BTreeMap-deterministic via serde_json defaults).
    pub payload: serde_json::Value,
    /// Wall-clock unix-seconds at emission.
    pub timestamp_unix: u64,
}

/// Axis discriminator for `AuditEvent`. Stable string labels are exposed
/// via `as_str` for the audit-sink's text logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAxis {
    /// Default-deny / dispatch-routing transparency.
    ImplementationTransparency,
    /// General-purpose transparency emissions.
    Transparency,
    /// Sovereignty-axis — mutations of host state.
    Sovereignty,
    /// Co-author-axis — Collaborator-handoff events.
    Cocreative,
    /// Cap-bypass — RECORDED by `record_sovereign_bypass`.
    CapBypass,
}

impl AuditAxis {
    /// Stable string label for the audit-sink's text logs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ImplementationTransparency => "implementation_transparency",
            Self::Transparency => "transparency",
            Self::Sovereignty => "sovereignty",
            Self::Cocreative => "cocreative",
            Self::CapBypass => "cap_bypass",
        }
    }
}

/// In-memory audit-port that collects every event into a `Vec` for
/// inspection in tests. Internally guarded by a `Mutex` for `Send + Sync`.
#[derive(Debug, Default)]
pub struct VecAuditPort {
    events: Mutex<Vec<AuditEvent>>,
}

impl VecAuditPort {
    /// Construct an empty port.
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    /// Snapshot every emitted event into a fresh `Vec`. Lock is released
    /// before return ; safe to call concurrently with `emit`.
    pub fn snapshot(&self) -> Vec<AuditEvent> {
        self.events.lock().expect("audit port mutex poisoned").clone()
    }

    /// Number of events currently retained.
    pub fn len(&self) -> usize {
        self.events.lock().expect("audit port mutex poisoned").len()
    }

    /// True iff zero events have been emitted.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl AuditPort for VecAuditPort {
    fn emit(&self, event: AuditEvent) {
        self.events
            .lock()
            .expect("audit port mutex poisoned")
            .push(event);
    }
}

/// No-op audit-port — drops every event. Used by hosts that explicitly
/// opt-out of audit (e.g. private-mode benchmarks). Default for
/// `AgentLoop` constructions that don't pass a port.
#[derive(Debug, Default)]
pub struct NullAuditPort;

impl AuditPort for NullAuditPort {
    fn emit(&self, _event: AuditEvent) {
        // intentionally empty
    }
}
