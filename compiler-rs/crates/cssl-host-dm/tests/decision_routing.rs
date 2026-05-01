// § T11-W7-C-DM tests/decision_routing.rs
// Decision-routing : route_intent → DmDecision mapping correctness across all variants.

use std::sync::Arc;

use cssl_host_dm::{
    AuditEvent, AuditSink, DirectorMaster, DmCapTable, DmDecision, IntentSummary,
    RecordingAuditSink, SceneStateSnapshot, Stage0HeuristicArbiter, DM_CAP_ALL,
    DM_CAP_COMPANION_RELAY,
};

// DM_CAP_ALL re-export check : add a constant locally for clarity if missing.
const _: u32 = DM_CAP_ALL;

struct ArcSink(Arc<RecordingAuditSink>);
impl AuditSink for ArcSink {
    fn emit(&self, e: AuditEvent) {
        self.0.emit(e);
    }
}

fn dm_with(cap_bits: u32) -> (DirectorMaster, Arc<RecordingAuditSink>) {
    let rec = Arc::new(RecordingAuditSink::new());
    let dm = DirectorMaster::new(
        DmCapTable::from_bits(cap_bits),
        Box::new(Stage0HeuristicArbiter::new()),
        Box::new(ArcSink(Arc::clone(&rec))),
    );
    (dm, rec)
}

#[test]
fn empty_intent_batch_silent_pass_decision() {
    let (dm, _) = dm_with(DM_CAP_ALL);
    let dec = dm
        .route_intent(Vec::new(), SceneStateSnapshot::neutral("zone:x"))
        .unwrap();
    assert_eq!(dec, DmDecision::Silent);
    assert_eq!(dm.silent_pass_count(), 1);
}

#[test]
fn unknown_intent_silent_pass_decision() {
    let (dm, _) = dm_with(DM_CAP_ALL);
    let intents = vec![IntentSummary::new("nonsense", 0.99, None)];
    let dec = dm
        .route_intent(intents, SceneStateSnapshot::neutral("zone:x"))
        .unwrap();
    assert_eq!(dec, DmDecision::Silent);
}

#[test]
fn low_confidence_intent_silent_pass_decision() {
    let (dm, _) = dm_with(DM_CAP_ALL);
    let intents = vec![IntentSummary::new(
        "spawn",
        0.05,
        Some(String::from("zone:lo")),
    )];
    let dec = dm
        .route_intent(intents, SceneStateSnapshot::neutral("zone:x"))
        .unwrap();
    assert_eq!(dec, DmDecision::Silent);
}

#[test]
fn talk_intent_emits_companion_prompt() {
    let (dm, rec) = dm_with(DM_CAP_COMPANION_RELAY);
    let intents = vec![IntentSummary::new(
        "talk",
        0.85,
        Some(String::from("companion")),
    )];
    let dec = dm
        .route_intent(intents, SceneStateSnapshot::neutral("zone:hall"))
        .unwrap();
    assert_eq!(dec, DmDecision::EmittedCompanionPrompt);
    assert_eq!(rec.count_kind("dm.companion_prompt"), 1);
}
