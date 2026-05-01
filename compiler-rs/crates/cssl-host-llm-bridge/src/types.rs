//! § cssl-host-llm-bridge::types — shared types across all three modes.
//!
//! § Contents : `LlmMode`, `LlmRole`, `LlmMessage`, `LlmEvent`, `CapBits`,
//! `LlmConfig`, `LlmAuditEvent`, `LlmError`.
//!
//! § PRIME-DIRECTIVE
//!   - `LlmConfig` does NOT serialize the API key. The field is `#[serde(skip)]`
//!     so any accidental config-dump cannot leak the secret.
//!   - `redact_api_key` collapses any non-empty key to `"sk-ant-***"` for
//!     log/diagnostic output.
//!   - `CapBits` enforces default-deny : `CapBits::none()` rejects every mode
//!     that requires external resources ; only `SUBSTRATE_ONLY` is on-by-default
//!     via `CapBits::substrate_only()`.

use serde::{Deserialize, Serialize};

/// Discriminator across the three bridge modes.
///
/// § per specs/grand-vision/23_MYCELIUM_DESKTOP.csl § THREE-MODE LLM-BRIDGE :
///   - `ExternalAnthropic` : richest · costs-tokens · requires API-key + cap-bit
///   - `LocalOllama`       : zero-cost · privacy-max · requires running Ollama
///   - `SubstrateOnly`     : ¬ LLM · templated · always-available · ¬ external-dep
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmMode {
    /// Mode-A — Anthropic Messages API over HTTPS.
    ExternalAnthropic,
    /// Mode-B — local Ollama HTTP server.
    LocalOllama,
    /// Mode-C — templated stub. The truly-self-sufficient fallback.
    SubstrateOnly,
}

impl LlmMode {
    /// Stable string label used in audit events and log lines.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExternalAnthropic => "external_anthropic",
            Self::LocalOllama => "local_ollama",
            Self::SubstrateOnly => "substrate_only",
        }
    }
}

/// Conversational role for a single message in the chat history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmRole {
    /// System prompt — folded into the top-level `system` field for Mode-A.
    System,
    /// User turn — the human's input.
    User,
    /// Assistant turn — the model's prior reply.
    Assistant,
}

impl LlmRole {
    /// Stable string label used in JSON serialization for Mode-A and Mode-B.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

/// A single message in the chat history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmMessage {
    /// Speaker role.
    pub role: LlmRole,
    /// Plain-text content. Future revisions may add multimodal blocks ; the
    /// stage-0 surface is text-only.
    pub content: String,
}

impl LlmMessage {
    /// Convenience constructor.
    pub fn new(role: LlmRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
}

/// Streaming-channel event emitted by `LlmBridge::chat_stream`.
#[derive(Debug, Clone, PartialEq)]
pub enum LlmEvent {
    /// Incremental text token(s) appended to the assistant turn.
    TextDelta(String),
    /// Tool-call request emitted by the model. Mode-C never emits this ;
    /// Mode-A may (when the upstream model returns `tool_use`) ; Mode-B
    /// may surface this when Ollama function-calling support lands.
    ToolCall {
        /// Tool name as declared by the caller.
        name: String,
        /// Tool input JSON value.
        input: serde_json::Value,
    },
    /// Terminal event — the stream has fully completed.
    Done {
        /// Input token count reported by the provider (or estimated for Mode-C).
        input_tokens: u32,
        /// Output token count reported by the provider.
        output_tokens: u32,
        /// Stop reason : "end_turn" / "max_tokens" / "stop_sequence" / etc.
        stop_reason: String,
    },
    /// Mid-stream error — the bridge could not continue. The caller should
    /// treat the stream as terminated.
    Error(String),
}

/// Capability bitset. Each bridge mode requires a specific bit ; absence of
/// the bit at construction time fails fast with `LlmError::CapDenied`.
///
/// § PRIME-DIRECTIVE — default-deny : `CapBits::none()` denies every mode
/// that touches an external resource. Mode-C is always-on via the
/// `SUBSTRATE_ONLY` bit which `CapBits::substrate_only()` sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CapBits(pub u8);

impl CapBits {
    /// Mode-A — Anthropic API. Touches the public internet ; gated.
    pub const EXTERNAL_API: u8 = 0b001;
    /// Mode-B — local Ollama. Localhost-only but still cap-gated for parity.
    pub const LOCAL_OLLAMA: u8 = 0b010;
    /// Mode-C — substrate-templated. Always-on by default.
    pub const SUBSTRATE_ONLY: u8 = 0b100;

    /// Empty cap-set : every external mode is denied. Mode-C still requires
    /// the `SUBSTRATE_ONLY` bit (always-on by policy but we keep the gate
    /// explicit so the audit trail records consent).
    #[must_use]
    pub const fn none() -> Self {
        Self(0)
    }

    /// Cap-set with `SUBSTRATE_ONLY` enabled — the always-on default.
    #[must_use]
    pub const fn substrate_only() -> Self {
        Self(Self::SUBSTRATE_ONLY)
    }

    /// Cap-set with every bit raised. Use only when the host has performed
    /// the consent ceremony for ALL three modes.
    #[must_use]
    pub const fn all() -> Self {
        Self(Self::EXTERNAL_API | Self::LOCAL_OLLAMA | Self::SUBSTRATE_ONLY)
    }

    /// True iff the bit is set.
    #[must_use]
    pub const fn has(self, bit: u8) -> bool {
        (self.0 & bit) != 0
    }
}

/// Bridge configuration. Loaded by the host from `~/.mycelium/config.json`
/// (or equivalent) ; the API key is held in OS-keychain in production.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Selected mode.
    pub mode: LlmMode,
    /// Anthropic API key. **Never serialized** — `#[serde(skip)]` ensures
    /// that any config-dump round-trip cannot leak the secret. Loaded
    /// separately from the keychain at runtime.
    #[serde(skip)]
    pub anthropic_api_key: Option<String>,
    /// Anthropic model identifier. Default : `claude-opus-4-7`.
    pub anthropic_model: String,
    /// Ollama HTTP endpoint. Default : `http://localhost:11434`.
    pub ollama_endpoint: String,
    /// Ollama model identifier. Default : `qwen2.5-coder:32b`.
    pub ollama_model: String,
    /// Maximum tokens to generate per response.
    pub max_tokens: u32,
    /// Sampling temperature in `[0.0, 1.0]`.
    pub temperature: f32,
    /// When `false`, Mode-C streams without any sleep between chunks. Tests
    /// set this to `false` so the suite finishes in milliseconds.
    pub simulate_delay: bool,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            mode: LlmMode::SubstrateOnly,
            anthropic_api_key: None,
            anthropic_model: "claude-opus-4-7".into(),
            ollama_endpoint: "http://localhost:11434".into(),
            ollama_model: "qwen2.5-coder:32b".into(),
            max_tokens: 4096,
            temperature: 0.7,
            simulate_delay: false,
        }
    }
}

/// Audit row emitted at the end of every `chat_stream` call. The host's
/// audit-sink persists these to the structured log + the cost dashboard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmAuditEvent {
    /// Which bridge serviced this call.
    pub mode: LlmMode,
    /// Model identifier used (provider-specific string).
    pub model: String,
    /// Input tokens consumed.
    pub input_tokens: u32,
    /// Output tokens produced.
    pub output_tokens: u32,
    /// USD cost estimate for the call.
    pub estimated_cost_usd: f64,
    /// Unix timestamp at completion.
    pub timestamp_unix: u64,
}

/// Structured error type for the bridge surface.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// A required capability bit was missing.
    #[error("capability denied: {0}")]
    CapDenied(&'static str),
    /// Network-layer failure (DNS · connect · TLS · timeout).
    #[error("network error: {0}")]
    Network(String),
    /// HTTP layer returned a non-2xx status.
    #[error("api error {status}: {body}")]
    Api {
        /// HTTP status code.
        status: u16,
        /// Response body (truncated to a reasonable length by the impl).
        body: String,
    },
    /// Provider returned a payload the parser did not recognize.
    #[error("invalid response: {0}")]
    InvalidResponse(String),
    /// Required configuration was missing (e.g., API key for Mode-A).
    #[error("not configured: {0}")]
    NotConfigured(&'static str),
}

/// Redact an API key for log output. Returns `"sk-ant-***"` for any non-empty
/// key, `"<unset>"` otherwise. Used by every `tracing::info!` call that needs
/// to record which key was used without leaking the secret.
#[must_use]
pub fn redact_api_key(key: Option<&str>) -> &'static str {
    match key {
        Some(k) if !k.is_empty() => "sk-ant-***",
        _ => "<unset>",
    }
}
