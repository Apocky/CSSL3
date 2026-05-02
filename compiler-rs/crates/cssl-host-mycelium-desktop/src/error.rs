//! § error — application-level error types.
//!
//! § Two distinct error families :
//!   - `AppError` — surfaced from `MyceliumApp` lifecycle methods + IPC
//!     dispatch. Wraps the upstream loop / bridge / config errors.
//!   - `ConfigError` — surfaced from `config::load_from_path` /
//!     `config::save_to_path`. Distinct so callers that only touch config
//!     don't have to depend on the loop crate's error surface.

use cssl_host_agent_loop::{LlmError, LoopError};

use crate::secrets::SecretsError;

/// Top-level application error type. Combines loop / bridge / config / cap
/// cases into a single `?`-friendly surface.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Wrapped agent-loop error (LLM / tool / abort / budget).
    #[error("loop: {0}")]
    Loop(#[from] LoopError),
    /// Wrapped LLM-bridge error (rare ; the loop normally wraps these).
    #[error("llm: {0}")]
    Llm(#[from] LlmError),
    /// Configuration was rejected at construction or runtime.
    #[error("config: {0}")]
    Config(#[from] ConfigError),
    /// A cap-grant / revoke could not be applied — typically because the
    /// requested tool is unknown or the mode-string failed to parse.
    #[error("cap policy: {0}")]
    CapPolicy(String),
    /// A turn was running and could not be cancelled cleanly.
    #[error("session: {0}")]
    Session(String),
    /// A command-name was unknown or its payload was malformed.
    #[error("command: {0}")]
    Command(String),
    /// Secrets-port failure (key persist / load / validate).
    #[error("secrets: {0}")]
    Secrets(#[from] SecretsError),
}

impl AppError {
    /// Stable string code for the IPC `Error` variant — lets the frontend
    /// branch on machine-readable error class rather than parsing message
    /// text.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Loop(_) => "loop",
            Self::Llm(_) => "llm",
            Self::Config(_) => "config",
            Self::CapPolicy(_) => "cap_policy",
            Self::Session(_) => "session",
            Self::Command(_) => "command",
            Self::Secrets(_) => "secrets",
        }
    }
}

/// Configuration-layer error. Distinct from `AppError` so config-only
/// callers don't depend on the loop crate's error surface.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// I/O-layer failure (path-not-found · permission · disk-full · …).
    #[error("io: {0}")]
    Io(String),
    /// JSON parse / serialize failed.
    #[error("json: {0}")]
    Json(String),
    /// A field's value violated a structural invariant (e.g. negative
    /// `revert_window_secs`).
    #[error("invalid value: {0}")]
    InvalidValue(&'static str),
}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<serde_json::Error> for ConfigError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e.to_string())
    }
}
