//! § state-machine tests — phase order + abort + classification.

use cssl_host_agent_loop::{
    ApprovalState, Handoff, ToolCall, ToolName, TurnPhase, TurnState,
};

#[path = "common/mod.rs"]
mod common;

#[test]
fn turn_phase_transitions_in_order() {
    let mut s = TurnState::new(1, "hello", 0);
    assert_eq!(s.phase, TurnPhase::ReceiveInput);
    s.advance();
    assert_eq!(s.phase, TurnPhase::Classify);
    s.advance();
    assert_eq!(s.phase, TurnPhase::FetchContext);
    s.advance();
    assert_eq!(s.phase, TurnPhase::LlmCall);
    s.advance();
    assert_eq!(s.phase, TurnPhase::ToolUse);
    s.advance();
    assert_eq!(s.phase, TurnPhase::AuditEmit);
    s.advance();
    assert_eq!(s.phase, TurnPhase::Reply);
    s.advance();
    assert_eq!(s.phase, TurnPhase::Done);
    // Done is sticky.
    s.advance();
    assert_eq!(s.phase, TurnPhase::Done);
    assert!(s.phase.is_terminal());
}

#[test]
fn aborted_phase_carries_reason() {
    let mut s = TurnState::new(7, "ohno", 0);
    s.abort("user cancelled");
    assert_eq!(s.phase, TurnPhase::Aborted("user cancelled".into()));
    assert!(s.phase.is_terminal());
    // Aborted is sticky too.
    s.advance();
    assert_eq!(s.phase, TurnPhase::Aborted("user cancelled".into()));
}

#[test]
fn tool_call_default_pending() {
    let tc = ToolCall::new("call_1", ToolName::FileRead, serde_json::json!({}));
    assert_eq!(tc.id, "call_1");
    assert_eq!(tc.tool, ToolName::FileRead);
    assert_eq!(tc.approved, ApprovalState::Pending);
    assert!(tc.output.is_none());
}

#[test]
fn handoff_classify_coder_keywords() {
    let (loop_, _audit) = common::make_loop_sovereign();
    assert_eq!(loop_.classify("please fix the bug"), Handoff::Coder);
    assert_eq!(loop_.classify("compile the kernel"), Handoff::Coder);
    assert_eq!(loop_.classify("refactor this module"), Handoff::Coder);
    assert_eq!(loop_.classify("build the wasm target"), Handoff::Coder);
}

#[test]
fn handoff_classify_gm_keywords() {
    let (loop_, _audit) = common::make_loop_sovereign();
    assert_eq!(loop_.classify("describe the cavern"), Handoff::Gm);
    assert_eq!(loop_.classify("narrate the scene"), Handoff::Gm);
    assert_eq!(loop_.classify("paint the world"), Handoff::Gm);
}

#[test]
fn handoff_classify_generic_default() {
    let (loop_, _audit) = common::make_loop_sovereign();
    assert_eq!(loop_.classify("hi"), Handoff::Generic);
    assert_eq!(loop_.classify("what time is it"), Handoff::Generic);
    assert_eq!(loop_.classify(""), Handoff::Generic);
}
