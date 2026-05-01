//! § caps — capability discipline for the agent-loop's 11 tools.
//!
//! § Modes :
//!   - `SovereignMaster` — all tools, auto-approve all (RECORDS bypass)
//!   - `Default`         — read tools auto, write+git+deploy require approval
//!   - `Paranoid`        — read-only ; writes / git / deploy DENIED
//!
//! § PRIME-DIRECTIVE
//!   Default-deny is the structural posture. `SovereignMaster` does NOT
//!   silently bypass the gate — it RECORDS via `SovereignBypassRecord`
//!   which the audit-port persists with `AuditAxis::CapBypass`.

use crate::state::ToolName;
use crate::now_unix;
use serde::{Deserialize, Serialize};

/// Top-level cap-mode discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapMode {
    /// Sovereign-master — all tools, auto-approve all (with bypass-record).
    SovereignMaster,
    /// Default user — reads auto, mutating tools require approval.
    Default,
    /// Paranoid — read-only ; deny everything mutating.
    Paranoid,
}

/// The ternary cap-decision returned by `ToolCaps::check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapDecision {
    /// Tool runs without further confirmation.
    Allow,
    /// Tool requires explicit user approval before running.
    RequireApproval,
    /// Tool is denied outright.
    Deny,
}

/// Cap-policy for a session. The `allow` list bounds which tools may run
/// at all ; `auto_approve` is the subset that runs without prompting.
#[derive(Debug, Clone)]
pub struct ToolCaps {
    /// Top-level mode (informational ; gates roll up from explicit lists).
    pub mode: CapMode,
    /// Tools allowed to run.
    pub allow: Vec<ToolName>,
    /// Subset of `allow` that runs without an approval prompt.
    pub auto_approve: Vec<ToolName>,
}

impl ToolCaps {
    /// All-tools-on, all-auto-approved sovereign-master mode. Bypass
    /// remains RECORDED at dispatch time via `record_sovereign_bypass`.
    #[must_use]
    pub fn sovereign_master() -> Self {
        let all = Self::all_tools();
        Self {
            mode: CapMode::SovereignMaster,
            allow: all.clone(),
            auto_approve: all,
        }
    }

    /// Default user : reads + spec_query + web_search + LLM-chat are
    /// auto-approved ; writes / git / deploy / bash require approval.
    #[must_use]
    pub fn default_user() -> Self {
        let all = Self::all_tools();
        let auto: Vec<ToolName> = all
            .iter()
            .copied()
            .filter(|t| t.is_read_only())
            .collect();
        Self {
            mode: CapMode::Default,
            allow: all,
            auto_approve: auto,
        }
    }

    /// Paranoid : read-only. Writes, git, deploy, bash, MCP are DENIED.
    #[must_use]
    pub fn paranoid() -> Self {
        let allow: Vec<ToolName> = Self::all_tools()
            .into_iter()
            .filter(|t| t.is_read_only())
            .collect();
        Self {
            mode: CapMode::Paranoid,
            auto_approve: allow.clone(),
            allow,
        }
    }

    /// Decide whether the given tool may run.
    ///
    /// The sequence :
    ///   1. If not in `allow` → `Deny`.
    ///   2. Else if in `auto_approve` → `Allow`.
    ///   3. Else → `RequireApproval`.
    #[must_use]
    pub fn check(&self, tool: ToolName) -> CapDecision {
        if !self.allow.contains(&tool) {
            return CapDecision::Deny;
        }
        if self.auto_approve.contains(&tool) {
            return CapDecision::Allow;
        }
        CapDecision::RequireApproval
    }

    /// Record a sovereign-master bypass — emits a structured record the
    /// caller threads to the audit-port. This is the ONLY path that lets
    /// a default-deny become a default-allow, and the record is the
    /// PRIME-DIRECTIVE-mandated structural footprint.
    #[must_use]
    pub fn record_sovereign_bypass(&self, tool: ToolName) -> SovereignBypassRecord {
        SovereignBypassRecord {
            tool,
            recorded_unix: now_unix(),
            reason: match self.mode {
                CapMode::SovereignMaster => "sovereign_master_default_allow",
                CapMode::Default => "manual_user_override",
                CapMode::Paranoid => "explicit_paranoid_override",
            },
        }
    }

    /// Construct the canonical full-tool list. Order matches the
    /// `ToolName` declaration so audit-row diffs are deterministic.
    #[must_use]
    pub fn all_tools() -> Vec<ToolName> {
        vec![
            ToolName::FileRead,
            ToolName::FileWrite,
            ToolName::FileEdit,
            ToolName::Bash,
            ToolName::GitCommit,
            ToolName::GitPush,
            ToolName::McpCall,
            ToolName::VercelDeploy,
            ToolName::OllamaChat,
            ToolName::AnthropicMessages,
            ToolName::WebSearch,
            ToolName::SpecQuery,
        ]
    }
}

/// Audit-row payload for a sovereign-master bypass. The audit-port emits
/// this with `AuditAxis::CapBypass` ; the host's audit-sink persists the
/// row to the structured log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SovereignBypassRecord {
    /// Which tool was bypassed.
    pub tool: ToolName,
    /// Unix timestamp at which the bypass was recorded.
    pub recorded_unix: u64,
    /// Static reason string (one of three discriminators).
    pub reason: &'static str,
}
