//! § cssl-host-llm-bridge::tests::anthropic_mode — Mode-A offline tests.
//!
//! No real network calls. SSE parser is unit-tested against fixture strings.

use cssl_host_llm_bridge::anthropic::testing::{parse_sse_event, SseEventKind};
use cssl_host_llm_bridge::{
    estimate_usd, make_bridge, CapBits, LlmConfig, LlmError, LlmMode,
};

fn cfg_with_key(key: Option<&str>) -> LlmConfig {
    LlmConfig {
        mode: LlmMode::ExternalAnthropic,
        anthropic_api_key: key.map(str::to_string),
        ..LlmConfig::default()
    }
}

#[test]
fn make_bridge_anthropic_requires_cap() {
    let cfg = cfg_with_key(Some("sk-ant-fake"));
    let res = make_bridge(&cfg, CapBits::substrate_only());
    match res {
        Err(LlmError::CapDenied(name)) => assert_eq!(name, "EXTERNAL_API"),
        Ok(_) => panic!("expected cap-denied"),
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

#[test]
fn make_bridge_anthropic_requires_api_key() {
    let cfg = cfg_with_key(None);
    let res = make_bridge(&cfg, CapBits(CapBits::EXTERNAL_API));
    match res {
        Err(LlmError::NotConfigured(name)) => assert_eq!(name, "anthropic_api_key"),
        Ok(_) => panic!("expected not-configured"),
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

#[test]
fn chat_returns_not_configured_without_key() {
    // Empty-string key counts as unset.
    let cfg = cfg_with_key(Some(""));
    let res = make_bridge(&cfg, CapBits(CapBits::EXTERNAL_API));
    assert!(matches!(res, Err(LlmError::NotConfigured(_))));
}

#[test]
fn cost_estimate_anthropic_opus() {
    // Opus 4.7 : $15/M input + $75/M output.
    // 1000 input + 500 output = $0.015 + $0.0375 = $0.0525.
    let usd = estimate_usd(LlmMode::ExternalAnthropic, "claude-opus-4-7", 1000, 500);
    assert!((usd - 0.0525).abs() < 1e-6, "got {usd}");
}

#[test]
fn parse_sse_event_text_delta() {
    let data = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
    match parse_sse_event(data).unwrap() {
        SseEventKind::TextDelta(s) => assert_eq!(s, "Hello"),
        other => panic!("expected text-delta; got {other:?}"),
    }
}

#[test]
fn parse_sse_event_done_with_usage() {
    // message_delta payload : has a usage block — should yield Usage variant.
    let data = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":42,"output_tokens":17}}"#;
    match parse_sse_event(data).unwrap() {
        SseEventKind::Usage { input_t, output_t } => {
            assert_eq!(input_t, 42);
            assert_eq!(output_t, 17);
        }
        other => panic!("expected usage; got {other:?}"),
    }
    // message_stop : terminator with stop reason.
    let stop = r#"{"type":"message_stop"}"#;
    match parse_sse_event(stop).unwrap() {
        SseEventKind::Stop(reason) => assert_eq!(reason, "end_turn"),
        other => panic!("expected stop; got {other:?}"),
    }
}
