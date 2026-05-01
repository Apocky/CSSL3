//! § cssl-host-llm-bridge::tests::types — cap-bit + LlmMode + LlmConfig.

use cssl_host_llm_bridge::{CapBits, LlmConfig, LlmMode};

#[test]
fn cap_bits_none_denies_all_external_modes() {
    let caps = CapBits::none();
    assert!(!caps.has(CapBits::EXTERNAL_API));
    assert!(!caps.has(CapBits::LOCAL_OLLAMA));
    assert!(!caps.has(CapBits::SUBSTRATE_ONLY));
}

#[test]
fn cap_bits_all_grants_every_mode() {
    let caps = CapBits::all();
    assert!(caps.has(CapBits::EXTERNAL_API));
    assert!(caps.has(CapBits::LOCAL_OLLAMA));
    assert!(caps.has(CapBits::SUBSTRATE_ONLY));
    assert_eq!(caps.0, 0b111);
}

#[test]
fn llm_mode_as_str_stable() {
    assert_eq!(LlmMode::ExternalAnthropic.as_str(), "external_anthropic");
    assert_eq!(LlmMode::LocalOllama.as_str(), "local_ollama");
    assert_eq!(LlmMode::SubstrateOnly.as_str(), "substrate_only");
}

#[test]
fn llm_config_defaults() {
    let cfg = LlmConfig::default();
    assert!(matches!(cfg.mode, LlmMode::SubstrateOnly));
    assert!(cfg.anthropic_api_key.is_none());
    assert_eq!(cfg.anthropic_model, "claude-opus-4-7");
    assert_eq!(cfg.ollama_endpoint, "http://localhost:11434");
    assert_eq!(cfg.ollama_model, "qwen2.5-coder:32b");
    assert_eq!(cfg.max_tokens, 4096);
    assert!((cfg.temperature - 0.7).abs() < 1e-6);
    // simulate_delay should default to false so tests stay fast.
    assert!(!cfg.simulate_delay);
}
