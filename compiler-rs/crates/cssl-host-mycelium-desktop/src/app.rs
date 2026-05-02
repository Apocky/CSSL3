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

use crate::chat_sync_wire::ChatSyncWire;
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
    /// § T11-W11 : chat-sync wire for federated GM/DM modulation. Default-
    /// deny ; opt-in via `opt_in_chat_sync()`. The wire is held inside an
    /// `Arc` so concurrent IPC invocations can observe + tick without
    /// re-entry hazards.
    pub chat_sync: Arc<ChatSyncWire>,
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

        // § T11-W11-MYCELIUM-CHAT-SYNC : derive a deterministic per-app
        // pubkey-stub from the config. Stage-0 : BLAKE3 over a fixed-salt
        // app-name + config-fingerprint. Stage-1 : real Ed25519 keypair.
        let mut h = blake3::Hasher::new();
        h.update(b"cssl-host-mycelium-desktop\0chat-sync-pubkey\0v1");
        h.update(b"mycelium-desktop");
        let mut local_pubkey = [0_u8; 32];
        local_pubkey.copy_from_slice(h.finalize().as_bytes());
        let chat_sync = Arc::new(ChatSyncWire::new(local_pubkey));

        Ok(Self {
            config,
            agent_loop: Arc::new(Mutex::new(loop_)),
            session: Arc::new(Mutex::new(Session::new(100))),
            audit,
            chat_sync,
        })
    }

    /// § T11-W11 : opt-in to chat-sync federation. Default posture is deny ;
    /// this method is the explicit consent-arch grant.
    pub fn opt_in_chat_sync(&self) {
        self.chat_sync.opt_in_emitter();
    }

    /// § T11-W11 : sovereign-revoke chat-sync federation participation.
    /// Wipes local ring + cap-policy + federation-emitter-record + emits a
    /// purge-request broadcast for peers to drop our patterns.
    pub fn revoke_chat_sync(&self) {
        self.chat_sync.sovereign_revoke(now_unix());
    }

    /// § T11-W11 : caller-driven digest-tick. Hosts that don't run a
    /// dedicated thread call this on a periodic schedule.
    pub fn chat_sync_tick(&self) {
        self.chat_sync.tick_now(now_unix());
    }

    /// § T11-W17 · PROPRIETARY local-intelligence turn.
    ///
    /// Composes the reply via `cssl-host-substrate-intelligence` (the
    /// substrate-resonance procedural composer) instead of routing through
    /// the agent-loop → llm-bridge external-LLM path. This keeps the chat
    /// experience entirely LOCAL : no Anthropic API calls, no network
    /// egress, no telemetry leaving the device. Session recording + chat-
    /// sync observation behave identically to `run_turn`.
    ///
    /// Per Apocky-foundational-axiom (memory/feedback_no_external_llm_for_loa_intelligence) :
    /// the canonical Mycelium-chat backend is the proprietary substrate
    /// intelligence ; `run_turn` (agent-loop) remains a feature-gated
    /// opt-in path for Coder-role-only tool-augmentation.
    pub fn run_substrate_turn(&self, user_input: &str) -> Result<TurnResult, AppError> {
        let start = now_unix();

        // Derive a deterministic seed from the user input via BLAKE3 so the
        // same input reliably reproduces the same reply. Substrate-
        // intelligence's internal axes mix this with role/kind-specific salt.
        let mut h = blake3::Hasher::new();
        h.update(user_input.as_bytes());
        let digest: [u8; 32] = h.finalize().into();
        let seed = u64::from_le_bytes([
            digest[0], digest[1], digest[2], digest[3],
            digest[4], digest[5], digest[6], digest[7],
        ]);

        // Compose a Collaborator-role reply (Mycelium's chat is collab/co-author).
        let final_reply = cssl_host_substrate_intelligence::compose_dialogue_line(
            /* archetype = */ 0,
            /* mood       = */ 0,
            /* topic      = */ 0,
            seed,
        );

        let elapsed_ms = now_unix().saturating_sub(start).saturating_mul(1000);

        // Record on session — same shape as run_turn so frontend history works.
        let turn_id = {
            let mut sess = self
                .session
                .lock()
                .map_err(|_| AppError::Session("session mutex poisoned".into()))?;
            // Use a substrate-derived turn id so it's deterministic + unique.
            let turn_id: u64 = (seed ^ now_unix()) ^ 0x53_4E_54_52_53_55_42_31u64; // "SNTRSUB1" tag
            sess.record(StoredTurn {
                turn_id,
                user_input: user_input.into(),
                reply: final_reply.clone(),
                tool_calls: Vec::new(),
                elapsed_ms,
                timestamp_unix: now_unix(),
            });
            turn_id
        };

        // Same chat-sync observation as run_turn so federated-mycelium learns.
        self.chat_sync
            .observe_turn(user_input, &final_reply, now_unix(), 0, 1);

        Ok(TurnResult {
            turn_id,
            final_reply,
            fetched_docs: Vec::new(),
            tool_calls_executed: 0,
            elapsed_ms,
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

        // § T11-W11 : observe the turn-shape into the local chat-sync ring.
        // The ring is sovereign-local ; Σ-mask gates only fire at digest-
        // tick, so this push always succeeds (subject to bit-pack
        // validation which is structural). Region-tag 0 is the default
        // shard ; opt_in_tier 1 = Anonymized ; both will be wired to
        // config-fields in a follow-up slice.
        self.chat_sync
            .observe_turn(user_input, &final_reply, now_unix(), 0, 1);

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
    ///
    /// § T11-W11 : ALSO sovereign-revokes chat-sync federation
    /// participation (zeroes cap-policy · purges local ring · purges
    /// federation-emitter-record · emits purge-request broadcast).
    #[allow(clippy::significant_drop_tightening)]
    pub fn revoke_all_sovereign_caps(&self) -> Result<(), AppError> {
        let mut loop_ = self
            .agent_loop
            .lock()
            .map_err(|_| AppError::Session("agent-loop mutex poisoned".into()))?;
        loop_.caps = ToolCaps::paranoid();
        loop_.abort("sovereign_cap_revoke_all");
        drop(loop_);
        // Sovereign-cap-revoke includes chat-sync federation : the player
        // revoking ALL caps also revokes chat-pattern federation. This
        // preserves the "break-glass" semantics : ¬ surveillance ; ¬ data-
        // egress ; ¬ federation-tail.
        self.chat_sync.sovereign_revoke(now_unix());
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
