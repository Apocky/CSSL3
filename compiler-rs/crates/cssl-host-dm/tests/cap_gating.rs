// § T11-W7-C-DM tests/cap_gating.rs
// Cap-gating coverage : every emit-* method enforces its cap-bit ;
// revoke-mid-session drops cleanly + emits dm.cap_revoked audit-event.

use std::collections::BTreeMap;
use std::sync::Arc;

use cssl_host_dm::{
    AuditEvent, AuditSink, CompanionPrompt, DirectorMaster, DmCapTable, DmErr,
    NpcSpawnRequest, RecordingAuditSink, SceneEditKind, SceneEditOp,
    Stage0HeuristicArbiter, SpawnOrder, DM_CAP_COMPANION_RELAY, DM_CAP_SCENE_EDIT,
    DM_CAP_SPAWN_NPC,
};

struct ArcSink(Arc<RecordingAuditSink>);
impl AuditSink for ArcSink {
    fn emit(&self, e: AuditEvent) {
        self.0.emit(e);
    }
}

fn dm_with_caps(cap_bits: u32) -> (DirectorMaster, Arc<RecordingAuditSink>) {
    let rec = Arc::new(RecordingAuditSink::new());
    let dm = DirectorMaster::new(
        DmCapTable::from_bits(cap_bits),
        Box::new(Stage0HeuristicArbiter::new()),
        Box::new(ArcSink(Arc::clone(&rec))),
    );
    (dm, rec)
}

#[test]
fn scene_edit_blocked_when_cap_missing() {
    let (dm, rec) = dm_with_caps(0);
    let op = SceneEditOp {
        kind: SceneEditKind::SeedStamp,
        location: String::from("zone:atrium-1"),
        attribs: BTreeMap::new(),
    };
    let r = dm.emit_scene_edit(op);
    assert_eq!(
        r,
        Err(DmErr::CapRevoked {
            needed: DM_CAP_SCENE_EDIT
        })
    );
    assert!(rec.contains_kind("dm.cap_revoked"));
    assert_eq!(rec.count_kind("dm.scene_edit"), 0);
}

#[test]
fn scene_edit_allowed_when_cap_held() {
    let (dm, rec) = dm_with_caps(DM_CAP_SCENE_EDIT);
    let op = SceneEditOp::seed_stamp("zone:hall-3");
    assert!(dm.emit_scene_edit(op).is_ok());
    assert_eq!(rec.count_kind("dm.scene_edit"), 1);
    assert_eq!(rec.count_kind("dm.cap_revoked"), 0);
}

#[test]
fn spawn_order_blocked_when_cap_missing() {
    let (dm, rec) = dm_with_caps(DM_CAP_SCENE_EDIT); // wrong cap held
    let so = SpawnOrder {
        intent_kind: String::from("spawn"),
        zone_id: String::from("zone:x"),
        cap_pre_grant: 0,
    };
    let r = dm.emit_spawn_order(so);
    assert_eq!(
        r,
        Err(DmErr::CapRevoked {
            needed: DM_CAP_SPAWN_NPC
        })
    );
    assert!(rec.contains_kind("dm.cap_revoked"));
}

#[test]
fn companion_prompt_blocked_when_cap_missing() {
    let (dm, rec) = dm_with_caps(DM_CAP_SCENE_EDIT | DM_CAP_SPAWN_NPC);
    let cp = CompanionPrompt {
        text_hash: 0xabc,
        cap_check: DM_CAP_COMPANION_RELAY,
    };
    let r = dm.emit_companion_prompt(cp);
    assert_eq!(
        r,
        Err(DmErr::CapRevoked {
            needed: DM_CAP_COMPANION_RELAY
        })
    );
    assert!(rec.contains_kind("dm.cap_revoked"));
}

#[test]
fn npc_spawn_blocked_when_cap_missing_and_audit_records_op_attrib() {
    let (dm, rec) = dm_with_caps(0);
    let req = NpcSpawnRequest {
        npc_handle: String::from("npc:guard"),
        zone_id: String::from("zone:gate"),
        cap_pre_grant: DM_CAP_SPAWN_NPC,
    };
    let r = dm.emit_npc_spawn(req);
    assert!(matches!(r, Err(DmErr::CapRevoked { .. })));
    let events = rec.events();
    let revoked = events
        .iter()
        .find(|e| e.kind == "dm.cap_revoked")
        .expect("cap_revoked emitted");
    assert_eq!(
        revoked.attribs.get("op"),
        Some(&String::from("dm.npc_spawn"))
    );
}
