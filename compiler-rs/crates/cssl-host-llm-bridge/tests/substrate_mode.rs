//! § cssl-host-llm-bridge::tests::substrate_mode — Mode-C templated bridge.

use cssl_host_llm_bridge::{
    make_bridge, CapBits, LlmConfig, LlmEvent, LlmMessage, LlmMode, LlmRole,
};

fn cfg() -> LlmConfig {
    LlmConfig {
        mode: LlmMode::SubstrateOnly,
        simulate_delay: false,
        ..LlmConfig::default()
    }
}

fn user(text: &str) -> LlmMessage {
    LlmMessage::new(LlmRole::User, text)
}

#[test]
fn make_bridge_substrate_works() {
    let bridge = make_bridge(&cfg(), CapBits::substrate_only()).expect("substrate bridge");
    assert_eq!(bridge.mode(), LlmMode::SubstrateOnly);
    assert!(bridge.name().contains("substrate"));
}

#[test]
fn chat_returns_templated_for_unknown() {
    let bridge = make_bridge(&cfg(), CapBits::substrate_only()).unwrap();
    let reply = bridge.chat(&[user("hello there")]).unwrap();
    assert!(
        reply.starts_with("[Mycelium · Mode-C · stage-0-templated]"),
        "got: {reply}"
    );
}

#[test]
fn chat_routes_code_keyword() {
    let bridge = make_bridge(&cfg(), CapBits::substrate_only()).unwrap();
    let reply = bridge.chat(&[user("write some code please")]).unwrap();
    assert!(reply.contains("cannot generate code"), "got: {reply}");
    // also routes the "rust" keyword
    let r2 = bridge.chat(&[user("help me with Rust traits")]).unwrap();
    assert!(r2.contains("cannot generate code"), "got: {r2}");
}

#[test]
fn chat_routes_spec_keyword() {
    let bridge = make_bridge(&cfg(), CapBits::substrate_only()).unwrap();
    let reply = bridge.chat(&[user("look up this spec")]).unwrap();
    assert!(reply.contains("spec_query tool"), "got: {reply}");
}

#[test]
fn chat_routes_git_keyword() {
    let bridge = make_bridge(&cfg(), CapBits::substrate_only()).unwrap();
    let reply = bridge.chat(&[user("run git status")]).unwrap();
    assert!(reply.contains("git commands"), "got: {reply}");
}

#[test]
fn chat_stream_emits_word_deltas() {
    let bridge = make_bridge(&cfg(), CapBits::substrate_only()).unwrap();
    let mut deltas: Vec<String> = Vec::new();
    let mut got_done = false;
    let _audit = bridge
        .chat_stream(&[user("hello there friend")], &mut |ev| match ev {
            LlmEvent::TextDelta(s) => deltas.push(s),
            LlmEvent::Done { .. } => got_done = true,
            _ => {}
        })
        .unwrap();
    assert!(got_done);
    // Reassembled, the deltas form the templated reply.
    let reassembled: String = deltas.concat();
    assert!(reassembled.starts_with("[Mycelium"));
    // Multiple deltas → at least 5 words split out.
    assert!(deltas.len() >= 5, "got {} deltas", deltas.len());
}

#[test]
fn chat_stream_emits_done_event_with_token_count() {
    let bridge = make_bridge(&cfg(), CapBits::substrate_only()).unwrap();
    let mut input_tokens = 0u32;
    let mut output_tokens = 0u32;
    let mut stop_reason = String::new();
    let audit = bridge
        .chat_stream(&[user("a b c d e")], &mut |ev| {
            if let LlmEvent::Done {
                input_tokens: i,
                output_tokens: o,
                stop_reason: s,
            } = ev
            {
                input_tokens = i;
                output_tokens = o;
                stop_reason = s;
            }
        })
        .unwrap();
    assert!(input_tokens >= 5, "got input_tokens={input_tokens}");
    assert!(output_tokens > 0);
    assert_eq!(stop_reason, "end_turn");
    assert_eq!(audit.mode, LlmMode::SubstrateOnly);
    // Mode-C is always free.
    assert!(audit.estimated_cost_usd.abs() < 1e-9);
}

#[test]
fn cap_check_substrate_always_on() {
    // Mode-C still requires the SUBSTRATE_ONLY bit (default-deny audit gate).
    let bridge = make_bridge(&cfg(), CapBits::none());
    assert!(bridge.is_err(), "Mode-C must reject empty cap-set");
    // But with the always-on default cap-set it succeeds.
    let bridge = make_bridge(&cfg(), CapBits::substrate_only());
    assert!(bridge.is_ok());
}
