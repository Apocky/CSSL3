//! § config — `AppConfig` + JSON persistence.
//!
//! § Stage-0 surface
//!   `AppConfig` holds the LLM-bridge config, the cap-mode discriminator,
//!   sandbox-paths, theme, audit-toggle, revert-window, knowledge top-K,
//!   and the context-token budget.
//!
//! § PRIME-DIRECTIVE
//!   - `LlmConfig::anthropic_api_key` is `#[serde(skip)]` upstream — the
//!     key is loaded separately at runtime + never round-trips through
//!     this file's JSON path.
//!   - `revert_window_secs` is bounded `[0, 600]` ; values outside that
//!     range fail config-load.
//!   - `knowledge_top_k` and `context_token_budget` have non-zero
//!     invariants ; load returns `InvalidValue` otherwise.

use std::path::Path;

use cssl_host_agent_loop::{CapMode, LlmConfig};
use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

/// Top-level Mycelium application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// LLM bridge configuration (mode, model, endpoints, sampling).
    pub llm: LlmConfig,
    /// Top-level cap-mode discriminator. The runtime `ToolCaps` is
    /// derived from this at app-construction.
    pub caps: CapMode,
    /// Project-root paths the agent may read/write within. Stage-0
    /// scaffolding stores these as bare strings ; richer wave wires
    /// `std::path::PathBuf` validation.
    pub sandbox_paths: Vec<String>,
    /// UI theme selection. Defaults to `Dark` (the apocky.com aesthetic).
    pub ui_theme: UiTheme,
    /// When `true`, every action emits an `AuditEvent` to the bound
    /// audit-port. When `false`, only Sovereignty-axis events do.
    pub auto_audit: bool,
    /// Revert-window in seconds for mutations. Default `30` ; per
    /// spec § SOVEREIGN-CAP-REVOKE.
    pub revert_window_secs: u32,
    /// Top-K relevant docs to fetch from substrate-knowledge per turn.
    /// Default `5`.
    pub knowledge_top_k: usize,
    /// Coarse total-token budget for the assembled message-list. Default
    /// `50_000`.
    pub context_token_budget: usize,
}

/// UI theme discriminator. Default `Dark` matches the apocky.com aesthetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UiTheme {
    /// Dark default — radial-gradient #0a0a0f → #15151f per spec § THEME.
    Dark,
    /// Light theme — ¬ default ; opt-in via Settings.
    Light,
    /// High-contrast accessibility theme.
    HighContrast,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            caps: CapMode::Default,
            sandbox_paths: Vec::new(),
            ui_theme: UiTheme::Dark,
            auto_audit: true,
            revert_window_secs: 30,
            knowledge_top_k: 5,
            context_token_budget: 50_000,
        }
    }
}

impl AppConfig {
    /// Validate structural invariants. Called by `load_from_path` after
    /// JSON-parse and by `MyceliumApp::new` before construction.
    ///
    /// § Rules
    ///   - `revert_window_secs` ≤ 600 (10 minutes upper-bound).
    ///   - `knowledge_top_k` > 0.
    ///   - `context_token_budget` > 0.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.revert_window_secs > 600 {
            return Err(ConfigError::InvalidValue("revert_window_secs"));
        }
        if self.knowledge_top_k == 0 {
            return Err(ConfigError::InvalidValue("knowledge_top_k"));
        }
        if self.context_token_budget == 0 {
            return Err(ConfigError::InvalidValue("context_token_budget"));
        }
        Ok(())
    }
}

/// Load + validate an `AppConfig` from a JSON file at `path`.
///
/// § Rejection cases
///   - File missing → `ConfigError::Io`.
///   - JSON parse failure → `ConfigError::Json`.
///   - Structural invariant failure → `ConfigError::InvalidValue`.
pub fn load_from_path(path: &Path) -> Result<AppConfig, ConfigError> {
    let body = std::fs::read_to_string(path)?;
    let cfg: AppConfig = serde_json::from_str(&body)?;
    cfg.validate()?;
    Ok(cfg)
}

/// Save the validated `AppConfig` to `path` as pretty-printed JSON.
///
/// § Note : `LlmConfig::anthropic_api_key` is `#[serde(skip)]` upstream so
/// the JSON file never contains the secret. The host stores keys in the
/// OS keychain (wave-D wires the keychain-port).
pub fn save_to_path(config: &AppConfig, path: &Path) -> Result<(), ConfigError> {
    config.validate()?;
    let body = serde_json::to_string_pretty(config)?;
    std::fs::write(path, body)?;
    Ok(())
}
