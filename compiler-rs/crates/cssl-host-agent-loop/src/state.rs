//! § state — turn state-machine + classification + tool-call records.
//!
//! § Phase order (linear ; aborts may short-circuit) :
//!   ReceiveInput → Classify → FetchContext → LlmCall → ToolUse →
//!   AuditEmit → Reply → Done
//!
//! § Determinism : every transition is observable via the audit-port ;
//!   `Aborted(reason)` carries the human-readable cause string for the
//!   host UI / forensic log.

use cssl_host_llm_bridge::LlmMessage;
use serde::{Deserialize, Serialize};

/// Turn-phase discriminator. Linear progression with a terminal `Done` and
/// an `Aborted(reason)` short-circuit reachable from any non-terminal
/// phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnPhase {
    /// Input has just been received from the host UI.
    ReceiveInput,
    /// Heuristic / model-driven handoff classification.
    Classify,
    /// Pull top-K substrate-knowledge docs for context.
    FetchContext,
    /// Call the LLM bridge with the assembled message list.
    LlmCall,
    /// Execute zero-or-more tool-calls returned by the LLM.
    ToolUse,
    /// Emit the audit-row for this turn.
    AuditEmit,
    /// Render the final reply for the host UI.
    Reply,
    /// Terminal — turn completed normally.
    Done,
    /// Terminal — turn was aborted, reason embedded.
    Aborted(String),
}

impl TurnPhase {
    /// Stable string label used in audit events.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReceiveInput => "receive_input",
            Self::Classify => "classify",
            Self::FetchContext => "fetch_context",
            Self::LlmCall => "llm_call",
            Self::ToolUse => "tool_use",
            Self::AuditEmit => "audit_emit",
            Self::Reply => "reply",
            Self::Done => "done",
            Self::Aborted(_) => "aborted",
        }
    }

    /// True iff the phase is terminal (`Done` or `Aborted`).
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Done | Self::Aborted(_))
    }
}

/// Classification of the user's intent — drives tool-availability and
/// prompt-template selection per spec/grand-vision/23 § AGENT-LOOP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Handoff {
    /// Narrative orchestrator — high-level plan / system / design.
    Dm,
    /// Narrator — describe / scene / world.
    Gm,
    /// Co-author / polish / collaborate.
    Collaborator,
    /// Code mutation — compile / build / refactor / fix / bug.
    Coder,
    /// Default for non-routed prompts.
    Generic,
}

impl Handoff {
    /// Stable string label used in audit events + prompt template selection.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dm => "dm",
            Self::Gm => "gm",
            Self::Collaborator => "collaborator",
            Self::Coder => "coder",
            Self::Generic => "generic",
        }
    }
}

/// Approval state for a single tool-call. The lifecycle is :
///   Pending → (Auto | ApprovedSovereign | Denied | Rejected)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    /// Pre-decision ; awaiting cap-check or user approval.
    Pending,
    /// Auto-approved by cap-policy (e.g. read tools under default user).
    Auto,
    /// Sovereign-master bypass — RECORDED via `SovereignBypassRecord`.
    ApprovedSovereign,
    /// Cap-policy denied the tool outright.
    Denied,
    /// User explicitly rejected at the approval prompt.
    Rejected,
}

/// Symbolic name of every tool the loop can dispatch. Cap-policy decides
/// per-tool ; the dispatcher matches on this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolName {
    /// Read a file via `FilePort`.
    FileRead,
    /// Write a file via `FilePort`.
    FileWrite,
    /// Edit (find/replace) a file via `FilePort`.
    FileEdit,
    /// Run a shell command via `BashPort`.
    Bash,
    /// Create a git commit via `GitPort`.
    GitCommit,
    /// Push to a git remote via `GitPort`.
    GitPush,
    /// Dispatch a generic MCP tool via `McpPort`.
    McpCall,
    /// Deploy via `VercelPort`.
    VercelDeploy,
    /// Mode-B chat ; routed through the `LlmBridge`.
    OllamaChat,
    /// Mode-A chat ; routed through the `LlmBridge`.
    AnthropicMessages,
    /// Outbound web search via `WebSearchPort`.
    WebSearch,
    /// Substrate-knowledge corpus query (no I/O ; build-embedded).
    SpecQuery,
}

impl ToolName {
    /// Stable string label used in audit events + JSON serialization.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FileRead => "file_read",
            Self::FileWrite => "file_write",
            Self::FileEdit => "file_edit",
            Self::Bash => "bash",
            Self::GitCommit => "git_commit",
            Self::GitPush => "git_push",
            Self::McpCall => "mcp_call",
            Self::VercelDeploy => "vercel_deploy",
            Self::OllamaChat => "ollama_chat",
            Self::AnthropicMessages => "anthropic_messages",
            Self::WebSearch => "web_search",
            Self::SpecQuery => "spec_query",
        }
    }

    /// True iff the tool is structurally read-only (never mutates host
    /// state). Used by `ToolCaps::default_user` to auto-approve reads.
    #[must_use]
    pub const fn is_read_only(self) -> bool {
        matches!(
            self,
            Self::FileRead | Self::SpecQuery | Self::WebSearch | Self::OllamaChat
                | Self::AnthropicMessages
        )
    }

    /// True iff the tool deploys to the public network (Vercel-deploy is
    /// the only one as of stage-0). Used by `ToolCaps` for tighter gating.
    #[must_use]
    pub const fn is_deploy(self) -> bool {
        matches!(self, Self::VercelDeploy)
    }

    /// True iff the tool publishes to a git remote.
    #[must_use]
    pub const fn is_git_publish(self) -> bool {
        matches!(self, Self::GitPush)
    }
}

/// A single tool-call record retained on the `TurnState` for audit + replay.
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// Stable opaque identifier — the LLM may emit a string, the host may
    /// generate a turn-scoped uuid ; we preserve verbatim.
    pub id: String,
    /// Which tool the LLM requested.
    pub tool: ToolName,
    /// Tool input (JSON shape is per-tool).
    pub input: serde_json::Value,
    /// Tool output, populated post-dispatch ; `None` until executed.
    pub output: Option<serde_json::Value>,
    /// Approval-lifecycle state.
    pub approved: ApprovalState,
}

impl ToolCall {
    /// Construct a fresh `Pending` tool-call.
    pub fn new(id: impl Into<String>, tool: ToolName, input: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            tool,
            input,
            output: None,
            approved: ApprovalState::Pending,
        }
    }
}

/// Full turn-state record. Persisted by the host as the audit-row substrate.
#[derive(Debug, Clone)]
pub struct TurnState {
    /// Monotonic turn-id assigned by `AgentLoop`.
    pub turn_id: u64,
    /// Current phase ; advances through `TurnPhase::as_str` order.
    pub phase: TurnPhase,
    /// Verbatim user input.
    pub user_input: String,
    /// Classification once the heuristic runs.
    pub classification: Option<Handoff>,
    /// Top-K relevant docs fetched for context (name + score).
    pub fetched_docs: Vec<(String, f32)>,
    /// Assembled LLM message list for the call.
    pub llm_messages: Vec<LlmMessage>,
    /// Tool-call records executed during this turn.
    pub tool_calls: Vec<ToolCall>,
    /// Final reply rendered for the UI.
    pub final_reply: Option<String>,
    /// Wall-clock unix-seconds at turn-start.
    pub started_unix: u64,
}

impl TurnState {
    /// Construct a fresh turn-state in the `ReceiveInput` phase.
    pub fn new(turn_id: u64, user_input: impl Into<String>, started_unix: u64) -> Self {
        Self {
            turn_id,
            phase: TurnPhase::ReceiveInput,
            user_input: user_input.into(),
            classification: None,
            fetched_docs: Vec::new(),
            llm_messages: Vec::new(),
            tool_calls: Vec::new(),
            final_reply: None,
            started_unix,
        }
    }

    /// Advance the phase. The legal transition table is :
    ///   ReceiveInput → Classify → FetchContext → LlmCall → ToolUse →
    ///   AuditEmit → Reply → Done
    /// Any non-matching pair leaves the state unchanged ; callers should
    /// treat that as a programmer-error and audit it.
    pub fn advance(&mut self) {
        self.phase = match &self.phase {
            TurnPhase::ReceiveInput => TurnPhase::Classify,
            TurnPhase::Classify => TurnPhase::FetchContext,
            TurnPhase::FetchContext => TurnPhase::LlmCall,
            TurnPhase::LlmCall => TurnPhase::ToolUse,
            TurnPhase::ToolUse => TurnPhase::AuditEmit,
            TurnPhase::AuditEmit => TurnPhase::Reply,
            TurnPhase::Reply | TurnPhase::Done => TurnPhase::Done,
            TurnPhase::Aborted(r) => TurnPhase::Aborted(r.clone()),
        };
    }

    /// Mark the turn aborted with a human-readable reason.
    pub fn abort(&mut self, reason: impl Into<String>) {
        self.phase = TurnPhase::Aborted(reason.into());
    }
}
