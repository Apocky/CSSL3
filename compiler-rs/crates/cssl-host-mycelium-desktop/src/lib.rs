//! § cssl-host-mycelium-desktop — Mycelium · the autonomous-local-agent app.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Wave-C1 of T11-W10. The terminal crate that wires together :
//!     - cssl-host-substrate-knowledge (wave-A : build-embedded canon)
//!     - cssl-host-llm-bridge          (wave-A : 3-mode dispatch)
//!     - cssl-host-agent-loop          (wave-B : turn-state-machine)
//!   into a single application (`MyceliumApp`) plus a uniform IPC surface
//!   (`IpcCommand` / `IpcResponse`) that the Tauri-2.x frontend invokes.
//!
//! § PER specs/grand-vision/23_MYCELIUM_DESKTOP.csl
//!   § ARCHITECTURE / § UI / § AGENT-LOOP / § SECURITY · SOVEREIGNTY ·
//!   ATTESTATION / § STAGE-0-FALLBACK
//!
//! § PRIME-DIRECTIVE
//!   - `#![forbid(unsafe_code)]`.
//!   - Default-deny posture is preserved end-to-end : `MyceliumApp::new`
//!     constructs a `ToolCaps::default_user`-shaped policy unless the
//!     consumer explicitly opts into `sovereign_master`.
//!   - Sovereign-revoke (Ctrl+Shift+Alt+S in the UI) routes through
//!     `MyceliumApp::revoke_all_sovereign_caps` which downgrades the policy
//!     to `Paranoid` + emits the audit-bypass record.
//!   - Tauri runtime is feature-gated. The default workspace `cargo build`
//!     does NOT pull the 200-crate Tauri dep ; only `--features tauri-shell`
//!     surfaces the bin entry-point (currently a clear-error-stub-main —
//!     see `src/bin/tauri_shell.rs` + `frontend/README.md`).
//!
//! § DISCIPLINE
//!   - Workspace lints inherited.
//!   - BTreeMap-deterministic-serde via `serde_json::to_value` defaults.
//!   - No tokio/async — blocking ports match the existing host pattern.

#![forbid(unsafe_code)]

pub mod app;
pub mod chat_sync_wire;
pub mod commands;
pub mod config;
pub mod error;
pub mod session;

pub use app::{GrantMode, MyceliumApp, TurnResult};
pub use chat_sync_wire::ChatSyncWire;
pub use commands::{
    handle_command, IpcCommand, IpcResponse, SubstrateHit, ToolCallSummary, TurnSummary,
};
pub use config::{load_from_path, save_to_path, AppConfig, UiTheme};
pub use error::{AppError, ConfigError};
pub use session::{Session, SessionSnapshot, StoredTurn};

// Re-export upstream types that surface in our public API.
pub use cssl_host_agent_loop::{
    AuditAxis, AuditEvent, AuditPort, CapBits, CapMode, Handoff, LlmConfig, LlmMode, ToolCaps,
    ToolName, VecAuditPort,
};

/// Wall-clock unix seconds. Falls back to `0` on the (impossible) clock-skew
/// pre-1970 case so the function is total. Used by session timestamping.
#[must_use]
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Crate-version string ; surfaced in audit rows + UI for forensic traceability.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
