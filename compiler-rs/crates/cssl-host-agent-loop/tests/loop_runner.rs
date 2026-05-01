//! § loop_runner tests — end-to-end turn execution + audit + budget.

use cssl_host_agent_loop::{LlmRole, TurnPhase, TurnState};

#[path = "common/mod.rs"]
mod common;

#[test]
fn run_turn_substrate_mode_completes() {
    let (mut loop_, _audit) = common::make_loop_sovereign();
    let state = loop_.run_turn("hello mycelium").expect("run_turn");
    assert_eq!(state.phase, TurnPhase::Done);
    assert_eq!(state.turn_id, 1);
    assert!(state.final_reply.is_some());
    let reply = state.final_reply.as_ref().unwrap();
    assert!(!reply.is_empty());
}

#[test]
fn run_turn_emits_audit_events() {
    let (mut loop_, audit) = common::make_loop_sovereign();
    let _ = loop_.run_turn("describe the world").expect("run_turn");
    let events = audit.snapshot();
    assert!(!events.is_empty(), "must emit at least one event");
    // We expect at least: input_received, classified, context_fetched,
    // llm_reply, tool_use_phase, turn_complete (six emissions).
    assert!(events.len() >= 6, "expected >=6 events, got {}", events.len());
    // Final event should be the sovereignty turn_complete row.
    let last = events.last().unwrap();
    assert_eq!(last.axis.as_str(), "sovereignty");
    assert_eq!(last.payload["event"], "turn_complete");
}

#[test]
fn run_turn_fetches_context_top_k() {
    let (mut loop_, _audit) = common::make_loop_sovereign();
    loop_.knowledge_top_k = 7;
    let state = loop_.run_turn("substrate query").expect("run_turn");
    // Number of fetched docs is bounded by top_k.
    assert!(state.fetched_docs.len() <= 7);
}

#[test]
fn build_messages_includes_system_canon() {
    let (loop_, _audit) = common::make_loop_sovereign();
    let mut state = TurnState::new(1, "build me a thing", 0);
    state.fetched_docs = Vec::new();
    let msgs = loop_.build_messages(&state);
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, LlmRole::System);
    assert!(msgs[0].content.contains("[PRIME-DIRECTIVE + CANON]"));
    assert_eq!(msgs[1].role, LlmRole::User);
    assert_eq!(msgs[1].content, "build me a thing");
}

#[test]
fn build_messages_respects_token_budget() {
    let (mut loop_, _audit) = common::make_loop_sovereign();
    // Very tight budget — under-budget assembly should still produce
    // valid messages without panicking.
    loop_.context_token_budget = 16;
    let mut state = TurnState::new(2, "narrate", 0);
    state.fetched_docs = vec![("nonexistent.md".into(), 1.0)];
    let msgs = loop_.build_messages(&state);
    assert_eq!(msgs.len(), 2);
    // System message is bounded ; we're not asserting an exact size since
    // the canon docs are build-environment dependent. We just assert the
    // function did not panic and the structure is intact.
    assert_eq!(msgs[0].role, LlmRole::System);
}

#[test]
fn abort_sets_aborted_phase() {
    let mut state = TurnState::new(3, "x", 0);
    state.abort("interrupted");
    assert_eq!(state.phase, TurnPhase::Aborted("interrupted".into()));
    // AgentLoop::abort emits an audit-event.
    let (mut loop_, audit) = common::make_loop_sovereign();
    loop_.abort("user pressed escape");
    let events = audit.snapshot();
    let last = events.last().expect("must emit abort event");
    assert_eq!(last.payload["event"], "abort");
    assert_eq!(last.payload["reason"], "user pressed escape");
}

#[test]
fn multiple_turns_increment_id() {
    let (mut loop_, _audit) = common::make_loop_sovereign();
    let s1 = loop_.run_turn("first").unwrap();
    let s2 = loop_.run_turn("second").unwrap();
    let s3 = loop_.run_turn("third").unwrap();
    assert_eq!(s1.turn_id, 1);
    assert_eq!(s2.turn_id, 2);
    assert_eq!(s3.turn_id, 3);
}

#[test]
fn loop_error_llm_failure_propagates() {
    // A failing-bridge stub : returns LlmError::NotConfigured immediately.
    use cssl_host_agent_loop::{
        AuditPort, LlmAuditEvent, LlmBridge, LlmError, LlmEvent, LlmMessage, LlmMode,
        ToolCaps, ToolHandlers, VecAuditPort, AgentLoop,
    };
    use std::sync::Arc;

    struct Failing;
    impl LlmBridge for Failing {
        fn name(&self) -> &'static str {
            "failing"
        }
        fn mode(&self) -> LlmMode {
            LlmMode::SubstrateOnly
        }
        fn chat(&self, _messages: &[LlmMessage]) -> Result<String, LlmError> {
            Err(LlmError::NotConfigured("test"))
        }
        fn chat_stream(
            &self,
            _messages: &[LlmMessage],
            _on_event: &mut dyn FnMut(LlmEvent),
        ) -> Result<LlmAuditEvent, LlmError> {
            Err(LlmError::NotConfigured("test"))
        }
        fn cancel(&self) {}
    }

    let port: Arc<dyn AuditPort> = Arc::new(VecAuditPort::new());
    let mut lp = AgentLoop::new(
        Box::new(Failing),
        ToolCaps::sovereign_master(),
        ToolHandlers::null(),
        port,
    );
    let r = lp.run_turn("hello");
    assert!(r.is_err(), "expected LLM failure to propagate");
}
