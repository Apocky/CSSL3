//! § cssl-host-agent-loop — Mycelium-Desktop turn-based agent-loop.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Wave-B of T11-W10. Provides the state-machine that drives a single
//!   user-turn end-to-end :
//!     ReceiveInput → Classify → FetchContext → LlmCall → ToolUse →
//!     AuditEmit → Reply → Done
//!   wired around the wave-A primitives (cssl-host-substrate-knowledge for
//!   build-embedded canon retrieval ; cssl-host-llm-bridge for the three-mode
//!   LLM dispatch).
//!
//! § PER specs/grand-vision/23_MYCELIUM_DESKTOP.csl § AGENT-LOOP
//!   - 11 tool dispatchers, all cap-gated.
//!   - Default-deny posture ; every mutation is preceded by a `cap_check`
//!     and followed by an `audit.emit(AuditAxis::Sovereignty, _)`.
//!   - Sovereign-bypass is RECORDED, not silent — `record_sovereign_bypass`
//!     produces an audit row that the host's audit-sink persists.
//!
//! § ARCHITECTURE
//!   Side-effect-isolated via port traits :
//!     - `FilePort`   — read / write / edit
//!     - `BashPort`   — shell execution
//!     - `GitPort`    — commit / push
//!     - `McpPort`    — generic MCP tool dispatch
//!     - `VercelPort` — `apocky.com` deploy
//!     - `WebSearchPort` — outward search
//!   Tests inject `Mem*Port` / `Null*Port` ; wave-C wires real impls.
//!
//! § PRIME-DIRECTIVE
//!   - `#![forbid(unsafe_code)]`.
//!   - No file / network access at lib level — every I/O goes through a port.
//!   - Spec-query routes through `cssl-host-substrate-knowledge` only ;
//!     no fallthrough to filesystem.
//!   - `record_sovereign_bypass` is the ONLY path that lets a sovereign
//!     master override a default-deny — and it's RECORDED structurally.
//!
//! § DISCIPLINE
//!   - Workspace lints inherited.
//!   - BTreeMap-deterministic-serde via `serde_json::to_value` defaults.
//!   - Token-budget enforcement : `build_messages` truncates over-budget.

#![forbid(unsafe_code)]

pub mod audit;
pub mod caps;
pub mod loop_runner;
pub mod state;
pub mod tools;

pub use audit::{AuditAxis, AuditEvent, AuditPort, NullAuditPort, VecAuditPort};
pub use caps::{CapDecision, CapMode, SovereignBypassRecord, ToolCaps};
pub use loop_runner::{AgentLoop, LoopError};
pub use state::{ApprovalState, Handoff, ToolCall, ToolName, TurnPhase, TurnState};
pub use tools::{
    dispatch, BashOutput, BashPort, FilePort, GitPort, McpPort, MemFilePort, NullBashPort,
    NullGitPort, NullMcpPort, NullVercelPort, NullWebSearchPort, ToolError, ToolHandlers,
    VercelPort, WebSearchHit, WebSearchPort,
};

// Re-export types from cssl-host-llm-bridge that surface in our public API,
// so consumers can use a single import path.
pub use cssl_host_llm_bridge::{
    make_bridge, CapBits, LlmAuditEvent, LlmBridge, LlmConfig, LlmError, LlmEvent, LlmMessage,
    LlmMode, LlmRole, SubstrateBridge,
};

/// Wall-clock unix seconds. Falls back to `0` on the (impossible) clock-skew
/// pre-1970 case so the function is total. Used by audit-event timestamping.
#[must_use]
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Crate-version string ; surfaced in audit rows for forensic traceability.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
