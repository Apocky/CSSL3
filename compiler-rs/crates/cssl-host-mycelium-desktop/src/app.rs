//! § app — `MyceliumApp` lifecycle + cap-grant orchestration.
//!
//! § Responsibilities
//!   - Construct the agent-loop from an `AppConfig` (bridge factory +
//!     cap-policy + null tool-handlers + collecting audit-port).
//!   - Drive a single turn end-to-end + record the result on the session.
//!   - Translate cap grant/revoke requests against the live `ToolCaps`.
//!   - Implement the sovereign-cap-revoke-all hot-key (Ctrl+Shift+Alt+S)
//!     by downgrading the policy to `Paranoid`.
//!
//! § PRIME-DIRECTIVE
//!   - All state mutating methods cap-check first.
//!   - `revoke_all_sovereign_caps` is the global break-glass — it always
//!     succeeds + always resets to `Paranoid`.
//!   - The audit-port is `Arc`-owned by the app so callers can snapshot
//!     events without disturbing in-flight turns.

use std::sync::{Arc, Mutex};

use cssl_host_agent_loop::{
    make_bridge, AgentLoop, CapBits, LlmMode, ToolCaps, ToolHandlers, ToolName, VecAuditPort,
};
use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::error::AppError;
use crate::now_unix;
use crate::session::{Session, SessionSnapshot, StoredTurn};

/// Top-level Mycelium application. Owns the agent-loop, session, audit
/// sink, and config. Constructed once per-process at app-bootstrap.
pub struct MyceliumApp {
    /// Active configuration. Serialized to disk on `update_config`.
    pub config: AppConfig,
    /// Wave-B agent-loop wrapped in a `Mutex` so the IPC layer can drive
    /// it from multiple frontend invocations without re-entry hazards.
    pub agent_loop: Arc<Mutex<AgentLoop>>,
    /// In-memory turn-history buffer.
    pub session: Arc<Mutex<Session>>,
    /// Collecting audit-port. Cloned `Arc` reference is also held by the
    /// agent-loop's `audit` field so emissions land in the same sink.
    pub audit: Arc<VecAuditPort>,
}

/// Result of a single turn — the IPC layer fans this into a
/// `MessageReply` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnResult {
    /// Monotonic turn-id assigned by the agent-loop.
    pub turn_id: u64,
    /// Final assistant reply.
    pub final_reply: String,
    /// Names of substrate-knowledge docs fetched for context this turn.
    pub fetched_docs: Vec<String>,
    /// Number of tool-calls actually dispatched (stage-0 always 0 ;
    /// wave-C2 wires real tool-loop).
    pub tool_calls_executed: usize,
    /// Wall-clock duration of the turn in milliseconds.
    pub elapsed_ms: u64,
}

/// Discriminator for cap-grants : auto vs require-approval. Maps directly
/// onto the `ToolCaps::auto_approve` membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantMode {
    /// Tool runs without an approval prompt.
    Auto,
    /// Tool runs after explicit approval.
    RequireApproval,
}

impl MyceliumApp {
    /// Construct a fresh app from validated config. Builds the LLM bridge,
    /// derives the cap-policy, wires null tool-handlers + a collecting
    /// audit-port, and primes a 100-turn session-buffer.
    pub fn new(config: AppConfig) -> Result<Self, AppError> {
        config.validate()?;

        // Cap-bits are deterministically derived from the cap-mode.
        // Mode-A requires EXTERNAL_API ; Mode-B requires LOCAL_OLLAMA ;
        // Mode-C is always-on. Since the cap-policy gates the bridge at
        // a higher layer, we conservatively grant `all()` here for
        // sovereign-master, `substrate_only` otherwise — wave-C2 wires a
        // tighter mapping once the consent-ceremony UI lands.
        let bridge_caps = match (config.caps, config.llm.mode) {
            (cssl_host_agent_loop::CapMode::SovereignMaster, _) => CapBits::all(),
            (_, LlmMode::ExternalAnthropic) => CapBits(CapBits::EXTERNAL_API),
            (_, LlmMode::LocalOllama) => CapBits(CapBits::LOCAL_OLLAMA),
            (_, LlmMode::SubstrateOnly) => CapBits::substrate_only(),
        };

        let bridge = make_bridge(&config.llm, bridge_caps)?;
        let caps = match config.caps {
            cssl_host_agent_loop::CapMode::SovereignMaster => ToolCaps::sovereign_master(),
            cssl_host_agent_loop::CapMode::Default => ToolCaps::default_user(),
            cssl_host_agent_loop::CapMode::Paranoid => ToolCaps::paranoid(),
        };
        let tools = ToolHandlers::null();
        let audit = Arc::new(VecAuditPort::new());
        let mut loop_ = AgentLoop::new(bridge, caps, tools, audit.clone());
        loop_.knowledge_top_k = config.knowledge_top_k;
        loop_.context_token_budget = config.context_token_budget;

        Ok(Self {
            config,
            agent_loop: Arc::new(Mutex::new(loop_)),
            session: Arc::new(Mutex::new(Session::new(100))),
            audit,
        })
    }

    /// Drive a single turn end-to-end + record on the session.
    pub fn run_turn(&self, user_input: &str) -> Result<TurnResult, AppError> {
        let start = now_unix();
        let state = {
            let mut loop_ = self
                .agent_loop
                .lock()
                .map_err(|_| AppError::Session("agent-loop mutex poisoned".into()))?;
            loop_.run_turn(user_input)?
        };
        let elapsed_ms = now_unix().saturating_sub(start).saturating_mul(1000);
        let final_reply = state.final_reply.clone().unwrap_or_default();
        let fetched_docs: Vec<String> =
            state.fetched_docs.iter().map(|(n, _)| n.clone()).collect();
        let tool_calls_executed = state.tool_calls.len();
        let tool_call_names: Vec<String> = state
            .tool_calls
            .iter()
            .map(|c| c.tool.as_str().to_string())
            .collect();

        // Record on session.
        {
            let mut sess = self
                .session
                .lock()
                .map_err(|_| AppError::Session("session mutex poisoned".into()))?;
            sess.record(StoredTurn {
                turn_id: state.turn_id,
                user_input: user_input.into(),
                reply: final_reply.clone(),
                tool_calls: tool_call_names,
                elapsed_ms,
                timestamp_unix: now_unix(),
            });
        }

        Ok(TurnResult {
            turn_id: state.turn_id,
            final_reply,
            fetched_docs,
            tool_calls_executed,
            elapsed_ms,
        })
    }

    /// Cooperatively cancel the in-flight turn. No-op when idle. Surfaces
    /// the abort via the bridge's `cancel` and a `Sovereignty`-axis audit
    /// event.
    // We hold the mutex through the abort call so the abort + future
    // mutations are serialized ; clippy's significant-drop-tightening lint
    // wants an explicit early drop, but the lock-scope IS the unit of
    // atomicity here. Allow at function-level.
    #[allow(clippy::significant_drop_tightening)]
    pub fn cancel_current_turn(&self) -> Result<(), AppError> {
        let mut loop_ = self
            .agent_loop
            .lock()
            .map_err(|_| AppError::Session("agent-loop mutex poisoned".into()))?;
        loop_.abort("user_cancelled");
        Ok(())
    }

    /// Take a clone-snapshot of the current session. On (impossible)
    /// poison, returns an empty snapshot rather than panicking — UI
    /// shouldn't crash on session-buffer corruption.
    pub fn get_session(&self) -> SessionSnapshot {
        self.session
            .lock()
            .map_or_else(|_| Session::new(0).snapshot(), |s| s.snapshot())
    }

    /// Grant the named tool with the given approval mode. Adds to `allow`
    /// (idempotent) ; if `mode = Auto`, also adds to `auto_approve`.
    // The lock-scope is intentionally the whole grant-policy mutation so
    // concurrent grants/revokes don't interleave. See cancel_current_turn
    // for rationale.
    #[allow(clippy::significant_drop_tightening)]
    pub fn grant_cap(&self, tool: ToolName, mode: GrantMode) -> Result<(), AppError> {
        let mut loop_ = self
            .agent_loop
            .lock()
            .map_err(|_| AppError::Session("agent-loop mutex poisoned".into()))?;
        if !loop_.caps.allow.contains(&tool) {
            loop_.caps.allow.push(tool);
        }
        match mode {
            GrantMode::Auto => {
                if !loop_.caps.auto_approve.contains(&tool) {
                    loop_.caps.auto_approve.push(tool);
                }
            }
            GrantMode::RequireApproval => {
                loop_.caps.auto_approve.retain(|t| *t != tool);
            }
        }
        Ok(())
    }

    /// Revoke the named tool from both `allow` and `auto_approve`. Idempotent.
    #[allow(clippy::significant_drop_tightening)]
    pub fn revoke_cap(&self, tool: ToolName) -> Result<(), AppError> {
        let mut loop_ = self
            .agent_loop
            .lock()
            .map_err(|_| AppError::Session("agent-loop mutex poisoned".into()))?;
        loop_.caps.allow.retain(|t| *t != tool);
        loop_.caps.auto_approve.retain(|t| *t != tool);
        Ok(())
    }

    /// The Ctrl+Shift+Alt+S break-glass : reset cap-policy to `Paranoid`,
    /// halt-all-pending tool-calls (via `bridge.cancel`), and emit a
    /// `Sovereignty`-axis audit row.
    #[allow(clippy::significant_drop_tightening)]
    pub fn revoke_all_sovereign_caps(&self) -> Result<(), AppError> {
        let mut loop_ = self
            .agent_loop
            .lock()
            .map_err(|_| AppError::Session("agent-loop mutex poisoned".into()))?;
        loop_.caps = ToolCaps::paranoid();
        loop_.abort("sovereign_cap_revoke_all");
        Ok(())
    }

    /// Apply a new `AppConfig` to the running app. Stage-0 only the
    /// non-bridge fields are hot-swappable ; switching LLM mode requires
    /// constructing a fresh `MyceliumApp`. Caller is responsible for that
    /// path.
    #[allow(clippy::significant_drop_tightening)]
    pub fn update_config(&mut self, config: AppConfig) -> Result<(), AppError> {
        config.validate()?;
        // Hot-swap policy + budget knobs in-place.
        {
            let mut loop_ = self
                .agent_loop
                .lock()
                .map_err(|_| AppError::Session("agent-loop mutex poisoned".into()))?;
            loop_.caps = match config.caps {
                cssl_host_agent_loop::CapMode::SovereignMaster => ToolCaps::sovereign_master(),
                cssl_host_agent_loop::CapMode::Default => ToolCaps::default_user(),
                cssl_host_agent_loop::CapMode::Paranoid => ToolCaps::paranoid(),
            };
            loop_.knowledge_top_k = config.knowledge_top_k;
            loop_.context_token_budget = config.context_token_budget;
        }
        self.config = config;
        Ok(())
    }
}

impl GrantMode {
    /// Parse from the IPC string-form (`"auto"` / `"require_approval"`).
    pub fn parse(s: &str) -> Result<Self, AppError> {
        match s {
            "auto" => Ok(Self::Auto),
            "require_approval" => Ok(Self::RequireApproval),
            other => Err(AppError::CapPolicy(format!("unknown grant mode: {other}"))),
        }
    }
}

/// Parse the IPC tool-name string. Distinct from `ToolName::as_str` so the
/// dispatch is single-source-of-truth here.
pub fn parse_tool_name(s: &str) -> Result<ToolName, AppError> {
    match s {
        "file_read" => Ok(ToolName::FileRead),
        "file_write" => Ok(ToolName::FileWrite),
        "file_edit" => Ok(ToolName::FileEdit),
        "bash" => Ok(ToolName::Bash),
        "git_commit" => Ok(ToolName::GitCommit),
        "git_push" => Ok(ToolName::GitPush),
        "mcp_call" => Ok(ToolName::McpCall),
        "vercel_deploy" => Ok(ToolName::VercelDeploy),
        "ollama_chat" => Ok(ToolName::OllamaChat),
        "anthropic_messages" => Ok(ToolName::AnthropicMessages),
        "web_search" => Ok(ToolName::WebSearch),
        "spec_query" => Ok(ToolName::SpecQuery),
        other => Err(AppError::CapPolicy(format!("unknown tool: {other}"))),
    }
}
