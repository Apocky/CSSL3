// § T11-W7-C-DM tests/spawn_order.rs
// SpawnOrder + NpcSpawnRequest emission semantics.

use std::sync::Arc;

use cssl_host_dm::{
    AuditEvent, AuditSink, DirectorMaster, DmCapTable, DmDecision, IntentSummary,
    NpcSpawnRequest, RecordingAuditSink, SceneStateSnapshot, SpawnOrder,
    Stage0HeuristicArbiter, DM_CAP_SPAWN_NPC,
};

struct ArcSink(Arc<RecordingAuditSink>);
impl AuditSink for ArcSink {
    fn emit(&self, e: AuditEvent) {
        self.0.emit(e);
    }
}

fn dm_with_recorder(cap_bits: u32) -> (DirectorMaster, Arc<RecordingAuditSink>) {
    let rec = Arc::new(RecordingAuditSink::new());
    let dm = DirectorMaster::new(
        DmCapTable::from_bits(cap_bits),
        Box::new(Stage0HeuristicArbiter::new()),
        Box::new(ArcSink(Arc::clone(&rec))),
    );
    (dm, rec)
}

#[test]
fn spawn_order_emits_audit_with_zone() {
    let (dm, rec) = dm_with_recorder(DM_CAP_SPAWN_NPC);
    let so = SpawnOrder {
        intent_kind: String::from("spawn"),
        zone_id: String::from("zone:nave"),
        cap_pre_grant: DM_CAP_SPAWN_NPC,
    };
    dm.emit_spawn_order(so).unwrap();
    let evs = rec.events();
    let e = evs
        .iter()
        .find(|e| e.kind == "dm.spawn_order")
        .expect("emitted");
    assert_eq!(e.attribs.get("zone"), Some(&String::from("zone:nave")));
}

#[test]
fn npc_spawn_emits_audit_with_npc_and_zone() {
    let (dm, rec) = dm_with_recorder(DM_CAP_SPAWN_NPC);
    let req = NpcSpawnRequest {
        npc_handle: String::from("npc:librarian"),
        zone_id: String::from("zone:archive"),
        cap_pre_grant: DM_CAP_SPAWN_NPC,
    };
    dm.emit_npc_spawn(req).unwrap();
    let evs = rec.events();
    let e = evs
        .iter()
        .find(|e| e.kind == "dm.npc_spawn")
        .expect("emitted");
    assert_eq!(e.attribs.get("npc"), Some(&String::from("npc:librarian")));
    assert_eq!(e.attribs.get("zone"), Some(&String::from("zone:archive")));
}

#[test]
fn route_intent_spawn_kind_emits_spawn_order() {
    let (dm, rec) = dm_with_recorder(DM_CAP_SPAWN_NPC);
    let intents = vec![IntentSummary::new(
        "spawn",
        0.95,
        Some(String::from("zone:hi")),
    )];
    let dec = dm
        .route_intent(intents, SceneStateSnapshot::neutral("zone:x"))
        .unwrap();
    assert_eq!(dec, DmDecision::EmittedSpawnOrder);
    assert_eq!(rec.count_kind("dm.spawn_order"), 1);
}

#[test]
fn route_intent_spawn_npc_kind_emits_npc_spawn() {
    let (dm, rec) = dm_with_recorder(DM_CAP_SPAWN_NPC);
    let intents = vec![IntentSummary::new(
        "spawn_npc",
        0.95,
        Some(String::from("npc:guide")),
    )];
    let dec = dm
        .route_intent(intents, SceneStateSnapshot::neutral("zone:plaza"))
        .unwrap();
    assert_eq!(dec, DmDecision::EmittedNpcSpawn);
    assert_eq!(rec.count_kind("dm.npc_spawn"), 1);
}
