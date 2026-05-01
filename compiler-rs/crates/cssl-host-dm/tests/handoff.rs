// § T11-W7-C-DM tests/handoff.rs
// Handoff-event audit-emission ; cross-role-cap-bleed structurally avoided.

use std::sync::Arc;

use cssl_host_dm::{
    AuditEvent, AuditSink, DirectorMaster, DmCapTable, HandoffEvent,
    RecordingAuditSink, Role, Stage0HeuristicArbiter,
};

struct ArcSink(Arc<RecordingAuditSink>);
impl AuditSink for ArcSink {
    fn emit(&self, e: AuditEvent) {
        self.0.emit(e);
    }
}

fn dm_with_recorder() -> (DirectorMaster, Arc<RecordingAuditSink>) {
    let rec = Arc::new(RecordingAuditSink::new());
    let dm = DirectorMaster::new(
        DmCapTable::all_granted(),
        Box::new(Stage0HeuristicArbiter::new()),
        Box::new(ArcSink(Arc::clone(&rec))),
    );
    (dm, rec)
}

#[test]
fn dm_to_gm_handoff_emits_canonical_audit_name() {
    let (dm, rec) = dm_with_recorder();
    let h = HandoffEvent::new(Role::DM, Role::GM, 0xCAFE_BABE, 1234, 7);
    dm.emit_handoff(h);
    assert!(rec.contains_kind("handoff.dm_to_gm"));
}

#[test]
fn dm_to_collaborator_handoff_uses_collab_tag() {
    let (dm, rec) = dm_with_recorder();
    let h = HandoffEvent::new(Role::DM, Role::Collaborator, 1, 100, 1);
    dm.emit_handoff(h);
    assert!(rec.contains_kind("handoff.dm_to_collab"));
}

#[test]
fn handoff_audit_carries_payload_handle_and_trace_id() {
    let (dm, rec) = dm_with_recorder();
    let h = HandoffEvent::new(Role::DM, Role::Coder, 0xDEAD_BEEF, 9_000, 42);
    dm.emit_handoff(h);
    let evs = rec.events();
    let e = evs
        .iter()
        .find(|e| e.kind == "handoff.dm_to_coder")
        .expect("emitted");
    assert_eq!(e.attribs.get("trace_id"), Some(&String::from("42")));
    assert!(e.attribs.contains_key("payload_handle"));
    assert!(e.attribs.contains_key("ts_micros"));
}
