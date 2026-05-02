//! § commands — IPC command/response surface + dispatcher.
//!
//! § Single dispatch entry-point
//!   `handle_command(app, cmd) -> IpcResponse` is the entirety of the IPC
//!   surface. The Tauri-2.x feature-gated bin wraps this single function
//!   in a `tauri::command!` macro ; the rest of the time the IPC contract
//!   is identical between feature-on and feature-off builds, so tests
//!   can drive it directly without Tauri.
//!
//! § PRIME-DIRECTIVE
//!   - Every command path is cap-gated at the `MyceliumApp` layer.
//!   - Errors are flattened into a single `IpcResponse::Error` variant
//!     with a stable `code` discriminator + the underlying message.
//!   - No command returns until the underlying `MyceliumApp` method has
//!     either succeeded or surfaced an error.

use serde::{Deserialize, Serialize};

use crate::app::{parse_tool_name, GrantMode, MyceliumApp};
use crate::config::AppConfig;
use crate::error::AppError;

/// IPC command surface — one variant per UI action.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcCommand {
    /// Initialize a new chat session.
    StartSession,
    /// Send a user message to the agent-loop.
    SendMessage {
        /// Message body.
        content: String,
    },
    /// Cooperatively cancel the in-flight turn.
    Cancel,
    /// Fetch the most-recent N turns from the session-buffer.
    GetHistory {
        /// Maximum turns to return.
        limit: usize,
    },
    /// Grant a tool with the given approval mode.
    GrantCap {
        /// Tool-name string (`file_read`, `bash`, …).
        tool: String,
        /// Grant mode (`auto` / `require_approval`).
        mode: String,
    },
    /// Revoke the named tool.
    RevokeCap {
        /// Tool-name string.
        tool: String,
    },
    /// Break-glass — revoke all caps + downgrade to `Paranoid`.
    RevokeAllSovereignCaps,
    /// Open the Settings pane.
    OpenSettings,
    /// Top-K substrate-knowledge query.
    QuerySubstrate {
        /// Query string.
        query: String,
        /// Top-K to return.
        top_k: usize,
    },
    /// Replace the running config.
    UpdateConfig {
        /// New config to install.
        config: AppConfig,
    },
    /// Fetch the running config.
    GetConfig,
    /// Total embedded substrate-doc count.
    GetSubstrateDocCount,
}

/// IPC response surface — one variant per `IpcCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcResponse {
    /// Session started.
    SessionStarted {
        /// Opaque session-id assigned by `Session::new`.
        session_id: String,
    },
    /// Reply for a user message.
    MessageReply {
        /// Monotonic turn-id.
        turn_id: u64,
        /// Final reply body.
        content: String,
        /// Summaries of dispatched tool-calls.
        tool_calls: Vec<ToolCallSummary>,
        /// Wall-clock duration in milliseconds.
        elapsed_ms: u64,
    },
    /// Cancel acknowledged.
    Cancelled,
    /// History snapshot.
    History {
        /// Last-N turn summaries.
        turns: Vec<TurnSummary>,
    },
    /// Cap-grant acknowledged.
    CapGranted {
        /// Tool-name granted.
        tool: String,
    },
    /// Cap-revoke acknowledged.
    CapRevoked {
        /// Tool-name revoked.
        tool: String,
    },
    /// Break-glass acknowledged.
    AllSovereignRevoked,
    /// Settings pane open acknowledged.
    SettingsOpened,
    /// Substrate-knowledge query result.
    SubstrateMatches {
        /// Top-K hits.
        hits: Vec<SubstrateHit>,
    },
    /// Config updated.
    ConfigUpdated,
    /// Current config snapshot.
    Config {
        /// Snapshot of the running `AppConfig`.
        config: AppConfig,
    },
    /// Total embedded doc count.
    SubstrateDocCount {
        /// Count.
        count: usize,
    },
    /// Error variant — flat shape so the frontend can branch on `code`.
    Error {
        /// Human-readable message.
        message: String,
        /// Stable error class string.
        code: String,
    },
}

/// Compact summary of a tool-call for the IPC surface — the full call
/// record is held by the agent-loop's audit-port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallSummary {
    /// Tool-name string.
    pub tool: String,
    /// Approval / dispatch status string.
    pub status: String,
}

/// Compact turn summary for the history pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnSummary {
    /// Monotonic turn-id.
    pub turn_id: u64,
    /// First N chars of the user input.
    pub user_input_preview: String,
    /// First N chars of the assistant reply.
    pub reply_preview: String,
    /// Wall-clock duration in milliseconds.
    pub elapsed_ms: u64,
}

/// One substrate-knowledge query hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubstrateHit {
    /// Doc-name (e.g. `"specs/grand-vision/23_MYCELIUM_DESKTOP.csl"`).
    pub doc_name: String,
    /// Jaccard score in `[0.0, 1.0]`.
    pub score: f32,
}

/// Maximum preview-string length retained on `TurnSummary`.
pub const PREVIEW_LEN: usize = 200;

/// Top-level dispatcher. The Tauri-2.x bin wraps this single function in a
/// `tauri::command!` ; tests drive it directly.
pub fn handle_command(app: &mut MyceliumApp, cmd: IpcCommand) -> IpcResponse {
    match cmd {
        IpcCommand::StartSession => IpcResponse::SessionStarted {
            session_id: app.get_session().id,
        },
        IpcCommand::SendMessage { content } => {
            // § T11-W17 · canonical chat-path = PROPRIETARY local intelligence
            //   (substrate_intelligence.csl, stage-0-shim cssl-host-substrate-
            //    intelligence). NO Anthropic API. NO LLM-bridge. NO network.
            //   Per Apocky-foundational-axiom (memory/feedback_no_external_
            //    llm_for_loa_intelligence).
            match app.run_substrate_turn(&content) {
                Ok(turn) => IpcResponse::MessageReply {
                    turn_id: turn.turn_id,
                    content: turn.final_reply,
                    tool_calls: Vec::new(),
                    elapsed_ms: turn.elapsed_ms,
                },
                Err(e) => to_error(&e),
            }
        }
        IpcCommand::Cancel => match app.cancel_current_turn() {
            Ok(()) => IpcResponse::Cancelled,
            Err(e) => to_error(&e),
        },
        IpcCommand::GetHistory { limit } => {
            let snap = app.get_session();
            let total = snap.turns.len();
            let start = total.saturating_sub(limit);
            let turns: Vec<TurnSummary> = snap.turns[start..]
                .iter()
                .map(|t| TurnSummary {
                    turn_id: t.turn_id,
                    user_input_preview: preview(&t.user_input),
                    reply_preview: preview(&t.reply),
                    elapsed_ms: t.elapsed_ms,
                })
                .collect();
            IpcResponse::History { turns }
        }
        IpcCommand::GrantCap { tool, mode } => {
            let parsed_tool = match parse_tool_name(&tool) {
                Ok(t) => t,
                Err(e) => return to_error(&e),
            };
            let parsed_mode = match GrantMode::parse(&mode) {
                Ok(m) => m,
                Err(e) => return to_error(&e),
            };
            match app.grant_cap(parsed_tool, parsed_mode) {
                Ok(()) => IpcResponse::CapGranted { tool },
                Err(e) => to_error(&e),
            }
        }
        IpcCommand::RevokeCap { tool } => {
            let parsed_tool = match parse_tool_name(&tool) {
                Ok(t) => t,
                Err(e) => return to_error(&e),
            };
            match app.revoke_cap(parsed_tool) {
                Ok(()) => IpcResponse::CapRevoked { tool },
                Err(e) => to_error(&e),
            }
        }
        IpcCommand::RevokeAllSovereignCaps => match app.revoke_all_sovereign_caps() {
            Ok(()) => IpcResponse::AllSovereignRevoked,
            Err(e) => to_error(&e),
        },
        IpcCommand::OpenSettings => IpcResponse::SettingsOpened,
        IpcCommand::QuerySubstrate { query, top_k } => {
            let hits: Vec<SubstrateHit> =
                cssl_host_substrate_knowledge::query_relevant(&query, top_k)
                    .into_iter()
                    .map(|(name, score)| SubstrateHit {
                        doc_name: name.to_string(),
                        score,
                    })
                    .collect();
            IpcResponse::SubstrateMatches { hits }
        }
        IpcCommand::UpdateConfig { config } => match app.update_config(config) {
            Ok(()) => IpcResponse::ConfigUpdated,
            Err(e) => to_error(&e),
        },
        IpcCommand::GetConfig => IpcResponse::Config {
            config: app.config.clone(),
        },
        IpcCommand::GetSubstrateDocCount => IpcResponse::SubstrateDocCount {
            count: cssl_host_substrate_knowledge::doc_count(),
        },
    }
}

/// Truncate a string to `PREVIEW_LEN` chars at a UTF-8 boundary.
fn preview(s: &str) -> String {
    if s.len() <= PREVIEW_LEN {
        return s.to_string();
    }
    let mut cut = PREVIEW_LEN;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s[..cut].to_string()
}

/// Translate `AppError` into the wire `IpcResponse::Error` variant.
fn to_error(e: &AppError) -> IpcResponse {
    IpcResponse::Error {
        message: e.to_string(),
        code: e.code().to_string(),
    }
}
