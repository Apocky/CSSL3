//! `DirectorMaster` — the DM orchestrator value-type.
//!
//! § SPEC : `specs/grand-vision/10_INTELLIGENCE.csl` § ROLE-DM
//!
//! § ROLE
//!   Holds a [`DmCapTable`] + a [`SceneArbiter`] + an [`AuditSink`] ; routes
//!   incoming intent-batches to a `ScenePick` via the arbiter, then emits
//!   the corresponding cap-gated effect (`SceneEditOp` / `SpawnOrder` /
//!   `NpcSpawnRequest` / `CompanionPrompt`) through the audit-sink.
//!
//! § FAILURE-MODE WIRING
//!   - cap-revoked-mid : every emit-* method checks `cap_table.has(...)` ;
//!     on miss, emits `dm.cap_revoked` audit-event + returns `DmErr::CapRevoked`.
//!   - intent-confidence-low : the arbiter pre-filters via `CONFIDENCE_LOW` ;
//!     `ScenePick::Silent` is the result + `route_intent` returns
//!     `DmDecision::Silent`.
//!   - sovereign-mismatch : exposed as `DmErr::SovereignMismatch` ; no
//!     runtime-attempt-on-mismatch.

use std::sync::Mutex;

use crate::arbiter::{SceneArbiter, ScenePick};
use crate::audit_sink::{AuditEvent, AuditSink, NoopAuditSink};
use crate::cap_ladder::{
    DmCapTable, DM_CAP_COMPANION_RELAY, DM_CAP_SCENE_EDIT, DM_CAP_SPAWN_NPC,
};
use crate::scene_state::SceneStateSnapshot;
use crate::types::{
    CompanionPrompt, IntentSummary, NpcSpawnRequest, SceneEditOp, SpawnOrder,
};

// ───────────────────────────────────────────────────────────────────────
// § DmDecision — what `route_intent` resolved to.
// ───────────────────────────────────────────────────────────────────────

/// Outcome of one DM `route_intent` round.
///
/// `Emitted*` variants mean the matching emit-* method ran AND its cap-check
/// passed. `Silent` is the no-action-fallback per spec § FAILURE-MODES.
/// `Dropped` carries a tag describing why the decision was dropped (e.g.
/// `"cap_revoked"`, `"low_confidence"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DmDecision {
    /// Scene-edit was emitted.
    EmittedSceneEdit,
    /// Spawn-order was emitted.
    EmittedSpawnOrder,
    /// NPC-spawn-request was emitted.
    EmittedNpcSpawn,
    /// Companion-prompt was emitted.
    EmittedCompanionPrompt,
    /// No action taken — silent-pass.
    Silent,
    /// Decision was dropped with a reason tag (cap_revoked / low_confidence).
    Dropped { reason: String },
}

// ───────────────────────────────────────────────────────────────────────
// § DmErr
// ───────────────────────────────────────────────────────────────────────

/// Errors returned by `DirectorMaster::*` emit methods.
///
/// `SovereignMismatch` is the spec's `SIG0003` runtime-surface ; it is
/// returned without attempting the operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DmErr {
    /// Cap-bit required for this emission is not held by the DM.
    /// Carries the missing cap-bit-set for caller diagnostic.
    CapRevoked { needed: u32 },
    /// Sovereign-mismatch — receiver's sovereign-handle disagrees with DM's
    /// recorded sovereign-grant. Spec § FAILURE-MODES § sovereign-mismatch
    /// (SIG0003 compile-time ; no runtime-attempt).
    SovereignMismatch,
}

impl std::fmt::Display for DmErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CapRevoked { needed } => write!(f, "DM cap-revoked : needed bit-set 0b{needed:b}"),
            Self::SovereignMismatch => write!(f, "DM sovereign-mismatch : SIG0003"),
        }
    }
}

impl std::error::Error for DmErr {}

// ───────────────────────────────────────────────────────────────────────
// § DirectorMaster
// ───────────────────────────────────────────────────────────────────────

/// The DM orchestrator value-type.
///
/// `cap_table` is wrapped in a `Mutex` so an external sovereign-revoke event
/// can mutate the table mid-session without `&mut self` plumbing through every
/// caller. `arbiter` + `audit_sink` are `Box<dyn ...>` so the DM is composable
/// over either stage-0 or stage-1 backends.
///
/// `silent_pass_count` is the spec's `no-action-fallback → counter-incr`
/// surface — the host can read it for telemetry without disturbing internal
/// state.
pub struct DirectorMaster {
    /// Cap-bit-set held by this DM instance. `Mutex` guards revocation.
    cap_table: Mutex<DmCapTable>,
    /// Decision-policy backend (stage-0 heuristic OR stage-1 KAN-stub).
    arbiter: Box<dyn SceneArbiter>,
    /// Audit-event sink (default = `NoopAuditSink`).
    audit_sink: Box<dyn AuditSink>,
    /// Spec § FAILURE-MODES § no-action-fallback counter.
    silent_pass_count: Mutex<u64>,
}

impl DirectorMaster {
    /// Construct a DM with the given cap-table + arbiter + audit-sink.
    #[must_use]
    pub fn new(
        cap_table: DmCapTable,
        arbiter: Box<dyn SceneArbiter>,
        audit_sink: Box<dyn AuditSink>,
    ) -> Self {
        Self {
            cap_table: Mutex::new(cap_table),
            arbiter,
            audit_sink,
            silent_pass_count: Mutex::new(0),
        }
    }

    /// Convenience : DM with an explicit cap-table + arbiter, no audit
    /// (NoopAuditSink). Useful for harness-tests + benches.
    #[must_use]
    pub fn with_noop_audit(cap_table: DmCapTable, arbiter: Box<dyn SceneArbiter>) -> Self {
        Self::new(cap_table, arbiter, Box::new(NoopAuditSink))
    }

    /// Read-only view of the current cap-table.
    #[must_use]
    pub fn cap_table_snapshot(&self) -> DmCapTable {
        self.cap_table
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// Sovereign-revoke a cap-bit-set. Emits `dm.cap_revoked` audit-event.
    pub fn revoke_cap(&self, cap_bits: u32) {
        if let Ok(mut g) = self.cap_table.lock() {
            g.revoke(cap_bits);
        }
        self.audit_sink.emit(
            AuditEvent::bare("dm.cap_revoked")
                .with("cap_bits", format!("0b{cap_bits:b}")),
        );
    }

    /// Sovereign-grant a cap-bit-set. Emits `dm.cap_granted` audit-event.
    pub fn grant_cap(&self, cap_bits: u32) {
        if let Ok(mut g) = self.cap_table.lock() {
            g.grant(cap_bits);
        }
        self.audit_sink.emit(
            AuditEvent::bare("dm.cap_granted")
                .with("cap_bits", format!("0b{cap_bits:b}")),
        );
    }

    /// Total no-action-fallback events recorded across the session.
    #[must_use]
    pub fn silent_pass_count(&self) -> u64 {
        self.silent_pass_count
            .lock()
            .map(|g| *g)
            .unwrap_or(0)
    }

    /// Backend identifier (e.g. `"stage0-heuristic"`).
    #[must_use]
    pub fn arbiter_name(&self) -> &'static str {
        self.arbiter.name()
    }

    // ───────────────────────────────────────────────────────────────────
    // § Arbitration + routing
    // ───────────────────────────────────────────────────────────────────

    /// Route a batch of intents through the arbiter + emit the matching
    /// effect (cap-gated). Returns the [`DmDecision`] describing what
    /// happened. Errors are surfaced as [`DmErr`].
    pub fn route_intent(
        &self,
        intents: Vec<IntentSummary>,
        scene: SceneStateSnapshot,
    ) -> Result<DmDecision, DmErr> {
        let pick = self.arbiter.arbitrate(&intents, &scene);
        match pick {
            ScenePick::Silent => {
                self.bump_silent_pass();
                Ok(DmDecision::Silent)
            }
            ScenePick::SceneEdit { location } => {
                let op = SceneEditOp::seed_stamp(location);
                match self.emit_scene_edit(op) {
                    Ok(()) => Ok(DmDecision::EmittedSceneEdit),
                    Err(DmErr::CapRevoked { .. }) => {
                        Ok(DmDecision::Dropped {
                            reason: String::from("cap_revoked"),
                        })
                    }
                    Err(e) => Err(e),
                }
            }
            ScenePick::SpawnCondensation { zone_id } => {
                let order = SpawnOrder {
                    intent_kind: String::from("spawn"),
                    zone_id,
                    cap_pre_grant: DM_CAP_SPAWN_NPC,
                };
                match self.emit_spawn_order(order) {
                    Ok(()) => Ok(DmDecision::EmittedSpawnOrder),
                    Err(DmErr::CapRevoked { .. }) => {
                        Ok(DmDecision::Dropped {
                            reason: String::from("cap_revoked"),
                        })
                    }
                    Err(e) => Err(e),
                }
            }
            ScenePick::SpawnNpc { npc_handle, zone_id } => {
                let req = NpcSpawnRequest {
                    npc_handle,
                    zone_id,
                    cap_pre_grant: DM_CAP_SPAWN_NPC,
                };
                match self.emit_npc_spawn(req) {
                    Ok(()) => Ok(DmDecision::EmittedNpcSpawn),
                    Err(DmErr::CapRevoked { .. }) => {
                        Ok(DmDecision::Dropped {
                            reason: String::from("cap_revoked"),
                        })
                    }
                    Err(e) => Err(e),
                }
            }
            ScenePick::CompanionPrompt { text_hash } => {
                let prompt = CompanionPrompt {
                    text_hash,
                    cap_check: DM_CAP_COMPANION_RELAY,
                };
                match self.emit_companion_prompt(prompt) {
                    Ok(()) => Ok(DmDecision::EmittedCompanionPrompt),
                    Err(DmErr::CapRevoked { .. }) => {
                        Ok(DmDecision::Dropped {
                            reason: String::from("cap_revoked"),
                        })
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }

    fn bump_silent_pass(&self) {
        if let Ok(mut g) = self.silent_pass_count.lock() {
            *g = g.saturating_add(1);
        }
        self.audit_sink.emit(AuditEvent::bare("dm.silent_pass"));
    }

    // ───────────────────────────────────────────────────────────────────
    // § Cap-gated emission methods
    // ───────────────────────────────────────────────────────────────────

    /// Emit a scene-edit. Cap-gated `DM_CAP_SCENE_EDIT`. Audit
    /// `dm.scene_edit` on success ; `dm.cap_revoked` on miss.
    pub fn emit_scene_edit(&self, op: SceneEditOp) -> Result<(), DmErr> {
        if !self.has_cap(DM_CAP_SCENE_EDIT) {
            self.audit_sink.emit(
                AuditEvent::bare("dm.cap_revoked")
                    .with("op", "dm.scene_edit")
                    .with("needed", "DM_CAP_SCENE_EDIT"),
            );
            return Err(DmErr::CapRevoked {
                needed: DM_CAP_SCENE_EDIT,
            });
        }
        self.audit_sink.emit(
            AuditEvent::bare("dm.scene_edit")
                .with("location", op.location.clone())
                .with("kind", format!("{:?}", op.kind)),
        );
        Ok(())
    }

    /// Emit a spawn-order. Cap-gated `DM_CAP_SPAWN_NPC`. Audit
    /// `dm.spawn_order` on success ; `dm.cap_revoked` on miss.
    pub fn emit_spawn_order(&self, order: SpawnOrder) -> Result<(), DmErr> {
        if !self.has_cap(DM_CAP_SPAWN_NPC) {
            self.audit_sink.emit(
                AuditEvent::bare("dm.cap_revoked")
                    .with("op", "dm.spawn_order")
                    .with("needed", "DM_CAP_SPAWN_NPC"),
            );
            return Err(DmErr::CapRevoked {
                needed: DM_CAP_SPAWN_NPC,
            });
        }
        self.audit_sink.emit(
            AuditEvent::bare("dm.spawn_order")
                .with("zone", order.zone_id.clone())
                .with("intent_kind", order.intent_kind),
        );
        Ok(())
    }

    /// Emit an NPC-spawn-request. Cap-gated `DM_CAP_SPAWN_NPC`. Audit
    /// `dm.npc_spawn` on success ; `dm.cap_revoked` on miss.
    pub fn emit_npc_spawn(&self, req: NpcSpawnRequest) -> Result<(), DmErr> {
        if !self.has_cap(DM_CAP_SPAWN_NPC) {
            self.audit_sink.emit(
                AuditEvent::bare("dm.cap_revoked")
                    .with("op", "dm.npc_spawn")
                    .with("needed", "DM_CAP_SPAWN_NPC"),
            );
            return Err(DmErr::CapRevoked {
                needed: DM_CAP_SPAWN_NPC,
            });
        }
        self.audit_sink.emit(
            AuditEvent::bare("dm.npc_spawn")
                .with("npc", req.npc_handle.clone())
                .with("zone", req.zone_id),
        );
        Ok(())
    }

    /// Emit a companion-prompt. Cap-gated `DM_CAP_COMPANION_RELAY`. Audit
    /// `dm.companion_prompt` on success ; `dm.cap_revoked` on miss.
    pub fn emit_companion_prompt(&self, prompt: CompanionPrompt) -> Result<(), DmErr> {
        if !self.has_cap(DM_CAP_COMPANION_RELAY) {
            self.audit_sink.emit(
                AuditEvent::bare("dm.cap_revoked")
                    .with("op", "dm.companion_prompt")
                    .with("needed", "DM_CAP_COMPANION_RELAY"),
            );
            return Err(DmErr::CapRevoked {
                needed: DM_CAP_COMPANION_RELAY,
            });
        }
        self.audit_sink.emit(
            AuditEvent::bare("dm.companion_prompt")
                .with("text_hash", format!("{:#018x}", prompt.text_hash)),
        );
        Ok(())
    }

    /// Emit a handoff-event. NO cap-gate (handoff IS the mechanism that
    /// avoids cross-role-cap-bleed). Audit `handoff.<from>_to_<to>`.
    pub fn emit_handoff(&self, event: crate::handoff::HandoffEvent) {
        let name = event.audit_name();
        self.audit_sink.emit(
            AuditEvent::bare(name)
                .with("trace_id", event.trace_id.to_string())
                .with("payload_handle", format!("{:#034x}", event.payload_handle))
                .with("ts_micros", event.ts_micros.to_string()),
        );
    }

    fn has_cap(&self, cap_bit: u32) -> bool {
        self.cap_table
            .lock()
            .map(|g| g.has(cap_bit))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arbiter::Stage0HeuristicArbiter;
    use crate::audit_sink::RecordingAuditSink;
    use std::sync::Arc;

    fn dm_with_recorder(cap_bits: u32) -> (DirectorMaster, Arc<RecordingAuditSink>) {
        // Recorder needs to outlive the DM for assertion ; wrap in Arc + a
        // tiny shim that forwards to the inner Arc.
        struct ArcSink(Arc<RecordingAuditSink>);
        impl AuditSink for ArcSink {
            fn emit(&self, e: AuditEvent) {
                self.0.emit(e);
            }
        }
        let rec = Arc::new(RecordingAuditSink::new());
        let dm = DirectorMaster::new(
            DmCapTable::from_bits(cap_bits),
            Box::new(Stage0HeuristicArbiter::new()),
            Box::new(ArcSink(Arc::clone(&rec))),
        );
        (dm, rec)
    }

    #[test]
    fn silent_pass_increments_counter() {
        let (dm, _rec) = dm_with_recorder(0);
        assert_eq!(dm.silent_pass_count(), 0);
        let _ = dm.route_intent(
            Vec::new(),
            SceneStateSnapshot::neutral("zone:test"),
        );
        assert_eq!(dm.silent_pass_count(), 1);
    }

    #[test]
    fn cap_grant_revoke_round_trip() {
        let (dm, _rec) = dm_with_recorder(0);
        assert!(!dm.cap_table_snapshot().has(DM_CAP_SCENE_EDIT));
        dm.grant_cap(DM_CAP_SCENE_EDIT);
        assert!(dm.cap_table_snapshot().has(DM_CAP_SCENE_EDIT));
        dm.revoke_cap(DM_CAP_SCENE_EDIT);
        assert!(!dm.cap_table_snapshot().has(DM_CAP_SCENE_EDIT));
    }
}
