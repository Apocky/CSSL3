//! § cssl-host-llm-bridge — three-mode LLM bridge for the Mycelium-Desktop.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Provides a uniform `LlmBridge` trait + `make_bridge` factory dispatching
//!   to one of three implementations :
//!     - **Mode-A** `anthropic` — HTTPS to api.anthropic.com (richest)
//!     - **Mode-B** `ollama`    — HTTP to localhost:11434     (zero-cost)
//!     - **Mode-C** `substrate` — templated stub             (always-on)
//!
//! § PER specs/grand-vision/23_MYCELIUM_DESKTOP.csl § THREE-MODE LLM-BRIDGE
//!   Mode-C is the truly-self-sufficient fallback : ¬ external-dep ·
//!   ¬ local-LLM-required · always-available. The factory enforces the
//!   default-deny posture : every mode is cap-gated at construction.
//!
//! § PRIME-DIRECTIVE
//!   - API keys NEVER serialized (`#[serde(skip)]` on the field).
//!   - All log lines redact via `types::redact_api_key`.
//!   - Cap-bits enforce explicit-consent at the gate.
//!   - Mode-C ¬ surveillance : returns local templates only.
//!   - Audit-event emitted on every successful call (token-count + cost).
//!
//! § DISCIPLINE
//!   - `#![forbid(unsafe_code)]` — zero `unsafe` in this crate.
//!   - Workspace lints inherited.
//!   - No tokio/async — blocking ureq matches the existing host pattern.

#![forbid(unsafe_code)]

pub mod anthropic;
pub mod cost;
pub mod ollama;
pub mod substrate;
pub mod types;

pub use anthropic::AnthropicBridge;
pub use cost::estimate_usd;
pub use ollama::OllamaBridge;
pub use substrate::SubstrateBridge;
pub use types::{
    redact_api_key, CapBits, LlmAuditEvent, LlmConfig, LlmError, LlmEvent, LlmMessage, LlmMode,
    LlmRole,
};

/// Common bridge contract. Implemented by all three modes ; the factory
/// returns `Box<dyn LlmBridge>` so the host's dispatcher routes uniformly.
///
/// § Streaming-vs-blocking
///   `chat` is the simple blocking-and-collect variant ; useful for tests
///   and short prompts where the UI doesn't need typewriter animation.
///   `chat_stream` emits incremental events through the caller's closure
///   and returns the final audit row on completion.
pub trait LlmBridge: Send + Sync {
    /// Implementation-specific identifier (used in audit logs).
    fn name(&self) -> &'static str;
    /// Discriminator for routing logic.
    fn mode(&self) -> LlmMode;
    /// Synchronous chat — collect the full assistant reply.
    fn chat(&self, messages: &[LlmMessage]) -> Result<String, LlmError>;
    /// Streaming chat — emit events through the closure ; return audit row.
    ///
    /// § Object-safety : the closure type is `&mut dyn FnMut` rather than a
    /// generic parameter so the trait remains dyn-compatible (the factory
    /// returns `Box<dyn LlmBridge>`).  Callers that prefer the generic
    /// shape can use the inherent `chat_stream` wrapper offered by every
    /// concrete bridge type, or wrap their closure : `bridge.chat_stream(
    /// messages, &mut |ev| { ... })`.
    fn chat_stream(
        &self,
        messages: &[LlmMessage],
        on_event: &mut dyn FnMut(LlmEvent),
    ) -> Result<LlmAuditEvent, LlmError>;
    /// Cooperatively cancel an in-flight `chat_stream`. The next event the
    /// stream emits will be `LlmEvent::Error("cancelled")` and the call
    /// returns shortly thereafter.
    fn cancel(&self);
}

/// Factory : produce the bridge implementation for the configured mode.
///
/// § Failure-modes
///   - `LlmError::CapDenied(_)`     — required cap-bit absent.
///   - `LlmError::NotConfigured(_)` — required field empty (e.g., API key).
pub fn make_bridge(
    config: &LlmConfig,
    caps: CapBits,
) -> Result<Box<dyn LlmBridge>, LlmError> {
    match config.mode {
        LlmMode::ExternalAnthropic => Ok(Box::new(AnthropicBridge::new(config.clone(), caps)?)),
        LlmMode::LocalOllama => Ok(Box::new(OllamaBridge::new(config.clone(), caps)?)),
        LlmMode::SubstrateOnly => Ok(Box::new(SubstrateBridge::new(config.clone(), caps)?)),
    }
}

/// Wall-clock unix seconds. Shared by every bridge that emits an audit row.
pub(crate) fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
