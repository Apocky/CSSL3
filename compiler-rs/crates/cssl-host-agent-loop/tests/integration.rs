//! § integration tests — end-to-end smoke under Mode-C.

use cssl_host_agent_loop::{Handoff, TurnPhase};

#[path = "common/mod.rs"]
mod common;

#[test]
fn end_to_end_substrate_only_mode_smoke() {
    let (mut loop_, audit) = common::make_loop_sovereign();
    let state = loop_
        .run_turn("orchestrate the system design plan")
        .expect("turn ok");
    assert_eq!(state.phase, TurnPhase::Done);
    assert_eq!(state.classification, Some(Handoff::Dm));
    assert!(state.final_reply.is_some());

    // Audit should contain at least one Sovereignty + one
    // ImplementationTransparency + one Transparency event.
    let events = audit.snapshot();
    let has_sov = events.iter().any(|e| e.axis.as_str() == "sovereignty");
    let has_impl = events
        .iter()
        .any(|e| e.axis.as_str() == "implementation_transparency");
    let has_tx = events.iter().any(|e| e.axis.as_str() == "transparency");
    assert!(has_sov, "expected sovereignty event");
    assert!(has_impl, "expected implementation_transparency event");
    assert!(has_tx, "expected transparency event");
}

#[test]
fn end_to_end_with_tool_use_routing_smoke() {
    let (mut loop_, audit) = common::make_loop_default();
    // Coder-classified prompt — exercises the heuristic.
    let state = loop_.run_turn("please fix this bug").expect("turn ok");
    assert_eq!(state.classification, Some(Handoff::Coder));
    assert_eq!(state.phase, TurnPhase::Done);

    // Audit should mention the classification.
    let events = audit.snapshot();
    let classified = events.iter().find(|e| {
        e.payload
            .get("event")
            .and_then(|v| v.as_str())
            == Some("classified")
    });
    assert!(classified.is_some(), "must emit classified event");
    assert_eq!(
        classified.unwrap().payload["handoff"],
        "coder",
        "handoff label must serialize"
    );
}
