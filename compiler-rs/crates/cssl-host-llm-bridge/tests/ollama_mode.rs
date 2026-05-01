//! § cssl-host-llm-bridge::tests::ollama_mode — Mode-B offline tests.
//!
//! NDJSON parser exercised on fixture strings ; the unreachable-localhost test
//! uses 127.0.0.1:1 to fail fast at connect time without any real Ollama.

use cssl_host_llm_bridge::ollama::testing::{parse_chunk, OllamaChunkKind};
use cssl_host_llm_bridge::{
    estimate_usd, make_bridge, CapBits, LlmConfig, LlmError, LlmMessage, LlmMode, LlmRole,
};

fn cfg() -> LlmConfig {
    LlmConfig {
        mode: LlmMode::LocalOllama,
        ..LlmConfig::default()
    }
}

#[test]
fn make_bridge_ollama_requires_cap() {
    let res = make_bridge(&cfg(), CapBits::substrate_only());
    match res {
        Err(LlmError::CapDenied(name)) => assert_eq!(name, "LOCAL_OLLAMA"),
        Ok(_) => panic!("expected cap-denied"),
        Err(e) => panic!("unexpected error: {e:?}"),
    }
}

#[test]
fn chat_returns_network_error_when_unreachable() {
    // 127.0.0.1:1 is RFC-reserved for tcpmux ; nothing listens by default,
    // so connect() refuses immediately. Keeps the test deterministic + fast.
    let cfg = LlmConfig {
        mode: LlmMode::LocalOllama,
        ollama_endpoint: "http://127.0.0.1:1".into(),
        ..LlmConfig::default()
    };
    let bridge = make_bridge(&cfg, CapBits(CapBits::LOCAL_OLLAMA)).unwrap();
    let res = bridge.chat(&[LlmMessage::new(LlmRole::User, "hello")]);
    match res {
        Err(LlmError::Network(_)) => { /* ok */ }
        Err(e) => panic!("expected Network err; got {e:?}"),
        Ok(s) => panic!("expected error; got reply={s:?}"),
    }
}

#[test]
fn parse_ollama_chunk_text() {
    let line = r#"{"model":"qwen2.5","created_at":"x","message":{"role":"assistant","content":"Hello"},"done":false}"#;
    match parse_chunk(line).unwrap() {
        OllamaChunkKind::TextDelta(s) => assert_eq!(s, "Hello"),
        other => panic!("expected text-delta; got {other:?}"),
    }
}

#[test]
fn parse_ollama_chunk_done() {
    let line = r#"{"model":"qwen2.5","created_at":"x","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":12,"eval_count":34}"#;
    match parse_chunk(line).unwrap() {
        OllamaChunkKind::Done {
            input_t,
            output_t,
            reason,
        } => {
            assert_eq!(input_t, 12);
            assert_eq!(output_t, 34);
            assert_eq!(reason, "stop");
        }
        other => panic!("expected done; got {other:?}"),
    }
}

#[test]
fn cost_estimate_ollama_zero() {
    let usd = estimate_usd(LlmMode::LocalOllama, "qwen2.5-coder:32b", 10_000, 5_000);
    assert!(usd.abs() < 1e-9, "got {usd}");
}

#[test]
fn make_bridge_ollama_default_endpoint() {
    // Default endpoint is localhost:11434 — bridge constructs even though
    // nothing listens (we don't try to connect at make_bridge time).
    let bridge = make_bridge(&cfg(), CapBits(CapBits::LOCAL_OLLAMA)).unwrap();
    assert_eq!(bridge.mode(), LlmMode::LocalOllama);
    assert!(bridge.name().contains("ollama"));
}
