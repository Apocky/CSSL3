//! cssl-host-dm — DM (Director-Master) orchestrator scaffold.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § T11-W7-C-DM
//! § SPEC : `specs/grand-vision/10_INTELLIGENCE.csl` § ROLE-DM
//! § SIBLING : `specs/grand-vision/11_KAN_RIDE.csl` § SP-4 dm::scene_arbiter
//!
//! § ROLE
//!   The DM is the *scene-arbiter* + *intent-routing-master*. It receives
//!   typed `IntentSummary` events (mocked here ; eventually
//!   `cssl-intent-router::Intent`) plus a `SceneStateSnapshot` and decides
//!   which downstream effect (scene-edit / spawn-order / npc-spawn /
//!   companion-prompt) — if any — to emit. Every emission is **cap-gated**
//!   against [`cap_ladder::DmCapTable`] and **audit-emitted** through an
//!   [`audit_sink::AuditSink`].
//!
//! § AXIOMS (per spec § AXIOMS)
//!   • narrow-orchestrator-roster : DM ⊕ GM ⊕ Collab ⊕ Coder ; ¬ generic-AGI
//!   • ¬ self-improvement-recursion : KAN-splines BAKED @ comptime
//!   • ¬ open-ended-goal-pursuit : responds-to player-action only
//!   • ¬ cross-role-bleed : DM cannot exercise `GM_CAP_VOICE_EMIT` directly ;
//!     inter-role transitions = explicit `HandoffEvent`s
//!   • sovereign-cap-bound · audit-emit · refusable-by-player
//!   • Sensitive<gaze|biometric|face|body> structurally banned-from this
//!     crate's feature-set (no types defined for these)
//!
//! § STAGES
//!   • stage-0 : `arbiter::Stage0HeuristicArbiter` — rule-table lookup ;
//!     deterministic ; replay-bit-equal given snapshot.
//!   • stage-1 : `arbiter::Stage1KanStubArbiter` — interface-only swap-point ;
//!     delegates to inner `Box<dyn SceneArbiter>` with stage-0 fallback.
//!     Real KAN integration lands @ `cssl-substrate-kan` wave-7+.
//!
//! § FAILURE-MODES (per spec § FAILURE-MODES)
//!   • cap-revoked-mid       → DROP+user-feedback ; defer-to-GM-narration
//!   • intent-confidence-low → DROP-to-Unknown ; route-DM-fallback @ S3
//!   • no-action-fallback    → SILENT-PASS counter-incr
//!   • sovereign-mismatch    → [`DmErr::SovereignMismatch`] (variant ; no runtime-attempt)
//!
//! § SCOPE
//!   Wave-7 mission is the SCAFFOLD ; full causal-seed DAG mutation +
//!   intent-router wire are deferred. This crate exposes an orchestrator
//!   surface that compiles + tests stand-alone.

#![forbid(unsafe_code)]

pub mod arbiter;
pub mod audit_sink;
pub mod cap_ladder;
pub mod dm;
pub mod handoff;
pub mod scene_state;
pub mod types;

pub use arbiter::{
    SceneArbiter, ScenePick, Stage0HeuristicArbiter, Stage1KanStubArbiter,
};
pub use audit_sink::{AuditEvent, AuditSink, NoopAuditSink, RecordingAuditSink};
pub use cap_ladder::{
    DmCapTable, DM_CAP_ALL, DM_CAP_COMPANION_RELAY, DM_CAP_SCENE_EDIT,
    DM_CAP_SPAWN_NPC,
};
pub use dm::{DirectorMaster, DmDecision, DmErr};
pub use handoff::{HandoffEvent, Role};
pub use scene_state::SceneStateSnapshot;
pub use types::{
    CompanionPrompt, IntentSummary, NpcSpawnRequest, SceneEditOp,
    SceneEditKind, SpawnOrder,
};

/// Crate-level PRIME-DIRECTIVE attestation banner (mirrors sibling crates).
///
/// § I> consent=OS · violation=bug · no-override-exists
/// § I> DM responds-only to player-action ; ¬ self-trigger ; ¬ surveillance
pub const PRIME_DIRECTIVE_BANNER: &str =
    "consent=OS • violation=bug • no-override-exists";

/// Crate version (matches Cargo.toml).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod root_tests {
    use super::*;

    #[test]
    fn prime_directive_banner_nonempty() {
        assert!(!PRIME_DIRECTIVE_BANNER.is_empty());
        assert!(PRIME_DIRECTIVE_BANNER.contains("consent=OS"));
    }

    #[test]
    fn version_present() {
        assert!(!VERSION.is_empty());
        assert!(VERSION.contains('.'));
    }
}
