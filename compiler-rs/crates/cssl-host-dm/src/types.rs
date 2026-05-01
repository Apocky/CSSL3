//! Typed value-objects emitted (or consumed) by the DM scaffold.
//!
//! § SPEC : `specs/grand-vision/10_INTELLIGENCE.csl` § ROLE-DM § OUTPUTS
//!
//! § GAP : `IntentSummary` mocks the eventual public type from
//!   `cssl-intent-router` (which today lives inside `loa-host/src/intent_router.rs`
//!   and exposes no stable public crate boundary). The shape — `kind` /
//!   `confidence` / `target` — is structurally compatible with
//!   `loa-host::intent_router::Intent` so the swap is a registry-edit,
//!   not a refactor. § promote-when intent_router exposes a workspace crate.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ───────────────────────────────────────────────────────────────────────
// § IntentSummary  (mocked ; structurally-compatible with loa-host::Intent)
// ───────────────────────────────────────────────────────────────────────

/// Mocked summary of an incoming intent.
///
/// `kind` is the routing-tag (e.g. `"spawn"`, `"talk"`, `"examine"`,
/// `"cocreate"`, `"unknown"`). `confidence` ∈ [0.0, 1.0]. `target` is an
/// optional zone or entity identifier that the arbiter table-rules switch on.
///
/// § GAP : promote-to `cssl-intent-router::Intent` import once that crate
/// stabilises a public `Intent` type at the workspace boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntentSummary {
    /// Routing tag — e.g. `"spawn"`, `"talk"`, `"examine"`, `"cocreate"`, `"unknown"`.
    pub kind: String,
    /// Self-reported classifier confidence ∈ [0.0, 1.0]. Used for the
    /// `intent-confidence-low → DROP-to-Unknown` failure-mode.
    pub confidence: f32,
    /// Optional zone-id / entity-tag the intent is directed at.
    pub target: Option<String>,
}

impl IntentSummary {
    /// The canonical low-confidence "unknown" placeholder.
    #[must_use]
    pub fn unknown() -> Self {
        Self {
            kind: String::from("unknown"),
            confidence: 0.0,
            target: None,
        }
    }

    /// Convenience constructor for tests + table-rules.
    #[must_use]
    pub fn new(kind: impl Into<String>, confidence: f32, target: Option<String>) -> Self {
        Self {
            kind: kind.into(),
            confidence,
            target,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § SceneEditOp
// ───────────────────────────────────────────────────────────────────────

/// Discriminator for the kind of scene edit. Kept enum-shaped so adding
/// a new edit-class is a one-line addition that the arbiter can pattern-
/// match on without breaking serde wire-compat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum SceneEditKind {
    /// Stamp a new condensation seed-cell at `location`.
    SeedStamp,
    /// Adjust an existing zone's radiance / tension.
    AdjustRadiance,
    /// Mark a zone as transitioning to a new manifestation phase.
    PhaseAdvance,
    /// Generic fallback used by stage-1 stubs / tests.
    Other,
}

/// Scene-edit operation : the DM's `→ Stage-S4 seed-stamp` output.
///
/// `attribs` is a `BTreeMap` for serde determinism (BTree iter is
/// ordered ; output bytes are stable across runs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneEditOp {
    /// Discriminator for the edit class.
    pub kind: SceneEditKind,
    /// Spatial / zone tag for the edit (e.g. `"zone:atrium-7"`).
    pub location: String,
    /// String-keyed attribute bag. BTreeMap → deterministic serde output.
    pub attribs: BTreeMap<String, String>,
}

impl SceneEditOp {
    /// Construct a seed-stamp edit at `location` with no attribs.
    #[must_use]
    pub fn seed_stamp(location: impl Into<String>) -> Self {
        Self {
            kind: SceneEditKind::SeedStamp,
            location: location.into(),
            attribs: BTreeMap::new(),
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § SpawnOrder
// ───────────────────────────────────────────────────────────────────────

/// Spawn-order : DM-bias `Intent::SpawnCondensation` re-emission.
///
/// `cap_pre_grant` is the cap-bit-set the receiver needs in order to enact
/// the spawn ; the DM pre-allocates it from its own table at emission time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnOrder {
    /// Tag describing what to spawn (e.g. `"npc"`, `"prop"`, `"manifest"`).
    pub intent_kind: String,
    /// Zone identifier where the spawn lands.
    pub zone_id: String,
    /// Cap-bit-set the receiver needs to enact this spawn.
    pub cap_pre_grant: u32,
}

// ───────────────────────────────────────────────────────────────────────
// § NpcSpawnRequest
// ───────────────────────────────────────────────────────────────────────

/// NPC-spawn-request : NPC-handle + zone + cap-pre-grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NpcSpawnRequest {
    /// Stable identifier for the NPC archetype.
    pub npc_handle: String,
    /// Zone where the NPC should be instantiated.
    pub zone_id: String,
    /// Cap-bit-set the receiver needs to enact this NPC instantiation.
    pub cap_pre_grant: u32,
}

// ───────────────────────────────────────────────────────────────────────
// § CompanionPrompt
// ───────────────────────────────────────────────────────────────────────

/// Companion-prompt : forward a `text-hash` (NOT raw text — egress-min) to
/// the vercel companion-proxy via `NetPostWriteCap`.
///
/// `text_hash` is a `u64` digest of the prose payload ; the actual prose
/// is held on the GM-side and lookup-ed by hash. This keeps the DM's
/// emission egress-minimal per spec § OUTPUTS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanionPrompt {
    /// 64-bit digest of the prompt prose. Lookup-ed by the GM/proxy.
    pub text_hash: u64,
    /// Cap-bit the receiver needs to relay this prompt.
    pub cap_check: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_summary_unknown_is_zero_confidence() {
        let u = IntentSummary::unknown();
        assert_eq!(u.kind, "unknown");
        assert!(u.confidence.abs() < 1e-9);
        assert!(u.target.is_none());
    }

    #[test]
    fn scene_edit_op_seed_stamp_default() {
        let op = SceneEditOp::seed_stamp("zone:atrium-7");
        assert_eq!(op.kind, SceneEditKind::SeedStamp);
        assert_eq!(op.location, "zone:atrium-7");
        assert!(op.attribs.is_empty());
    }

    #[test]
    fn serde_round_trip_scene_edit_op() {
        let mut attribs = BTreeMap::new();
        attribs.insert(String::from("radiance"), String::from("0.42"));
        let op = SceneEditOp {
            kind: SceneEditKind::AdjustRadiance,
            location: String::from("zone:hall-3"),
            attribs,
        };
        let s = serde_json::to_string(&op).expect("serialize");
        let back: SceneEditOp = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(op, back);
    }

    #[test]
    fn serde_round_trip_spawn_order() {
        let so = SpawnOrder {
            intent_kind: String::from("npc"),
            zone_id: String::from("zone:atrium-1"),
            cap_pre_grant: 0b0011,
        };
        let s = serde_json::to_string(&so).expect("serialize");
        let back: SpawnOrder = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(so, back);
    }

    #[test]
    fn serde_round_trip_companion_prompt() {
        let cp = CompanionPrompt {
            text_hash: 0xDEAD_BEEF_CAFE_F00D,
            cap_check: 4,
        };
        let s = serde_json::to_string(&cp).expect("serialize");
        let back: CompanionPrompt = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(cp, back);
    }
}
