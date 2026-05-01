// § T11-W7-C-DM tests/scene_edit_audit.rs
// Audit-event emission on every scene-edit ; revoke-mid emits dm.cap_revoked.

use std::sync::Arc;

use cssl_host_dm::{
    AuditEvent, AuditSink, DirectorMaster, DmCapTable, DmDecision, IntentSummary,
    RecordingAuditSink, SceneEditOp, SceneStateSnapshot, Stage0HeuristicArbiter,
    DM_CAP_SCENE_EDIT,
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
fn audit_fires_on_emit_scene_edit() {
    let (dm, rec) = dm_with_recorder(DM_CAP_SCENE_EDIT);
    dm.emit_scene_edit(SceneEditOp::seed_stamp("zone:test"))
        .expect("cap held");
    let events = rec.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "dm.scene_edit");
    assert_eq!(events[0].attribs.get("location"), Some(&String::from("zone:test")));
}

#[test]
fn revoke_mid_session_emits_cap_revoked_and_drops() {
    let (dm, rec) = dm_with_recorder(DM_CAP_SCENE_EDIT);
    // First emission succeeds.
    dm.emit_scene_edit(SceneEditOp::seed_stamp("zone:1")).unwrap();
    assert_eq!(rec.count_kind("dm.scene_edit"), 1);
    // Sovereign revokes mid-session.
    dm.revoke_cap(DM_CAP_SCENE_EDIT);
    assert!(rec.contains_kind("dm.cap_revoked")); // emitted by revoke_cap itself
    // Second emission must drop + emit cap_revoked again.
    let r = dm.emit_scene_edit(SceneEditOp::seed_stamp("zone:2"));
    assert!(r.is_err());
    // dm.scene_edit count unchanged (still 1).
    assert_eq!(rec.count_kind("dm.scene_edit"), 1);
    assert!(rec.count_kind("dm.cap_revoked") >= 2);
}

#[test]
fn route_intent_emits_scene_edit_audit() {
    let (dm, rec) = dm_with_recorder(DM_CAP_SCENE_EDIT);
    let intents = vec![IntentSummary::new(
        "examine",
        0.9,
        Some(String::from("zone:atrium")),
    )];
    let scene = SceneStateSnapshot::neutral("zone:default");
    let dec = dm.route_intent(intents, scene).unwrap();
    assert_eq!(dec, DmDecision::EmittedSceneEdit);
    assert_eq!(rec.count_kind("dm.scene_edit"), 1);
}

#[test]
fn route_intent_drops_when_cap_revoked_mid() {
    let (dm, rec) = dm_with_recorder(DM_CAP_SCENE_EDIT);
    // Revoke before route.
    dm.revoke_cap(DM_CAP_SCENE_EDIT);
    let intents = vec![IntentSummary::new(
        "examine",
        0.9,
        Some(String::from("zone:atrium")),
    )];
    let dec = dm
        .route_intent(intents, SceneStateSnapshot::neutral("zone:x"))
        .unwrap();
    assert_eq!(
        dec,
        DmDecision::Dropped {
            reason: String::from("cap_revoked")
        }
    );
    assert_eq!(rec.count_kind("dm.scene_edit"), 0);
    assert!(rec.contains_kind("dm.cap_revoked"));
}
