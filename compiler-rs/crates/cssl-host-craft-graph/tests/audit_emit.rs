// § tests : audit-sink emit + recording (100% audit-coverage target)
// ══════════════════════════════════════════════════════════════════
use cssl_host_craft_graph::audit::{
    emit, AuditEvent, AuditKind, AuditSink, NoopAuditSink, RecordingAuditSink,
};

#[test]
fn t_recording_sink_collects_events() {
    let mut sink = RecordingAuditSink::new();
    emit(&mut sink, AuditKind::CraftCompleted, "{\"id\":7}");
    emit(&mut sink, AuditKind::CraftFailed, "{\"reason\":\"skill-too-low\"}");
    emit(&mut sink, AuditKind::CraftDeconstruct, "{\"item\":42}");

    assert_eq!(sink.count(), 3);
    assert_eq!(sink.count_kind(AuditKind::CraftCompleted), 1);
    assert_eq!(sink.count_kind(AuditKind::CraftDeconstruct), 1);
}

#[test]
fn t_noop_sink_drops_silently() {
    let mut sink = NoopAuditSink;
    sink.emit(AuditEvent {
        kind: AuditKind::CraftTransmute,
        payload: "ignored".into(),
    });
    // No panic, no observable state ; trait contract is fire-and-forget.
}

#[test]
fn t_audit_kind_strings_match_gdd() {
    // GDD § AXIOMS : audit kinds craft.completed / craft.failed / craft.deconstruct
    // / craft.transmute / craft.brew.
    assert_eq!(AuditKind::CraftCompleted.as_str(), "craft.completed");
    assert_eq!(AuditKind::CraftFailed.as_str(), "craft.failed");
    assert_eq!(AuditKind::CraftDeconstruct.as_str(), "craft.deconstruct");
    assert_eq!(AuditKind::CraftTransmute.as_str(), "craft.transmute");
    assert_eq!(AuditKind::CraftBrew.as_str(), "craft.brew");
}
