//! Inter-role handoff event-types.
//!
//! § SPEC : `specs/grand-vision/10_INTELLIGENCE.csl` § INTER-ROLE-PROTOCOL
//!
//! Cross-role-cap-bleed is structurally banned ; inter-role transitions are
//! ONLY ever expressed as [`HandoffEvent`]s. The DM cannot exercise
//! `GM_CAP_VOICE_EMIT` directly — it emits a `handoff.dm_to_gm` event and
//! the GM-instance acts under its own cap-ladder.
//!
//! § SCOPE
//!   This module defines the value-types only. Buffering / replay / policy
//!   logic lives in `cssl-host-handoff-protocol` (sibling crate). Wave-7's
//!   DM scaffold emits these events through the [`crate::AuditSink`] surface
//!   and lets the host wire them onward.

use serde::{Deserialize, Serialize};

/// Narrow-orchestrator role enumeration.
///
/// Order is alphabetical-by-capability so deterministic enum-discriminant
/// ids stay stable when a future role lands (insert in alphabetical slot).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    /// Runtime-mutator : Intent → .csl source-fragment AST-diff. STAGE-0
    /// requires explicit player-confirm for every edit ; automated edits
    /// are deferred per spec (high blast-radius).
    Coder,
    /// Co-author : player-suggestion-integrator + cocreative-bias updater.
    Collaborator,
    /// Orchestrator : scene-arbiter + intent-routing-master. THIS CRATE.
    DM,
    /// Narrator : text-emitter + pacing-controller.
    GM,
}

impl Role {
    /// Stable string-tag (matches the spec § AUDIT-EVENT-NAMES naming).
    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            Self::DM => "dm",
            Self::GM => "gm",
            Self::Collaborator => "collab",
            Self::Coder => "coder",
        }
    }
}

/// Structured handoff-event between two roles.
///
/// `payload_handle` is an opaque-token (per spec : "payload-handle = opaque-
/// token ¬ raw-egress") — the receiver looks up the actual payload by the
/// `u128` handle, the DM never inlines raw content into the audit record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffEvent {
    /// Source role.
    pub from_role: Role,
    /// Target role.
    pub to_role: Role,
    /// Opaque payload-handle (NOT raw payload bytes).
    pub payload_handle: u128,
    /// Microsecond-resolution timestamp at emission.
    pub ts_micros: u64,
    /// Trace-correlation id linking related events into one chain.
    pub trace_id: u64,
}

impl HandoffEvent {
    /// Construct a new handoff event.
    #[must_use]
    pub fn new(
        from_role: Role,
        to_role: Role,
        payload_handle: u128,
        ts_micros: u64,
        trace_id: u64,
    ) -> Self {
        Self {
            from_role,
            to_role,
            payload_handle,
            ts_micros,
            trace_id,
        }
    }

    /// Canonical audit-event name for this handoff
    /// (`"handoff.<from>_to_<to>"`).
    #[must_use]
    pub fn audit_name(&self) -> String {
        format!("handoff.{}_to_{}", self.from_role.tag(), self.to_role.tag())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_tags_match_spec() {
        assert_eq!(Role::DM.tag(), "dm");
        assert_eq!(Role::GM.tag(), "gm");
        assert_eq!(Role::Collaborator.tag(), "collab");
        assert_eq!(Role::Coder.tag(), "coder");
    }

    #[test]
    fn handoff_audit_name_format() {
        let h = HandoffEvent::new(Role::DM, Role::GM, 0xCAFE, 100, 7);
        assert_eq!(h.audit_name(), "handoff.dm_to_gm");
    }

    #[test]
    fn handoff_serde_round_trip() {
        let h = HandoffEvent::new(Role::Collaborator, Role::Coder, 0xDEAD_BEEF, 9_000, 42);
        let j = serde_json::to_string(&h).expect("serialize");
        let back: HandoffEvent = serde_json::from_str(&j).expect("deserialize");
        assert_eq!(h, back);
    }
}
