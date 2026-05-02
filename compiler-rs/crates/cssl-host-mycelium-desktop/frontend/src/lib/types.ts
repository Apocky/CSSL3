// § TS mirrors of all Rust IPC types from src/commands.rs + src/config.rs.
// § The Rust enums use serde tag = "type", rename_all = "snake_case" so the
//   wire shape is `{ type: "send_message", content: "..." }` etc.
// § PRIME-DIRECTIVE : keep this in lock-step with the Rust source.

/* ─────────────── upstream config types (config.rs) ─────────────── */

export type LlmMode = "external_anthropic" | "local_ollama" | "substrate_only";

export type CapMode = "sovereign_master" | "default" | "paranoid";

export type UiTheme = "dark" | "light" | "high-contrast";

export interface LlmConfig {
  mode: LlmMode;
  // anthropic_api_key is `#[serde(skip)]` upstream — never on the wire.
  anthropic_model: string;
  ollama_endpoint: string;
  ollama_model: string;
  max_tokens: number;
  temperature: number;
  simulate_delay: boolean;
}

export interface AppConfig {
  llm: LlmConfig;
  caps: CapMode;
  sandbox_paths: string[];
  ui_theme: UiTheme;
  auto_audit: boolean;
  revert_window_secs: number;
  knowledge_top_k: number;
  context_token_budget: number;
}

/* ─────────────── IPC summaries (commands.rs) ─────────────── */

export interface ToolCallSummary {
  tool: string;
  status: string;
}

export interface TurnSummary {
  turn_id: number;
  user_input_preview: string;
  reply_preview: string;
  elapsed_ms: number;
}

export interface SubstrateHit {
  doc_name: string;
  score: number;
}

/* ─────────────── IpcCommand (commands.rs) ─────────────── */

export type IpcCommand =
  | { type: "start_session" }
  | { type: "send_message"; content: string }
  | { type: "cancel" }
  | { type: "get_history"; limit: number }
  | { type: "grant_cap"; tool: string; mode: "auto" | "require_approval" }
  | { type: "revoke_cap"; tool: string }
  | { type: "revoke_all_sovereign_caps" }
  | { type: "open_settings" }
  | { type: "query_substrate"; query: string; top_k: number }
  | { type: "update_config"; config: AppConfig }
  | { type: "get_config" }
  | { type: "get_substrate_doc_count" }
  | { type: "save_anthropic_key"; key: string }
  | { type: "load_anthropic_key_masked" }
  | { type: "has_anthropic_key" };

/* ─────────────── IpcResponse (commands.rs) ─────────────── */

export type IpcResponse =
  | { type: "session_started"; session_id: string }
  | {
      type: "message_reply";
      turn_id: number;
      content: string;
      tool_calls: ToolCallSummary[];
      elapsed_ms: number;
    }
  | { type: "cancelled" }
  | { type: "history"; turns: TurnSummary[] }
  | { type: "cap_granted"; tool: string }
  | { type: "cap_revoked"; tool: string }
  | { type: "all_sovereign_revoked" }
  | { type: "settings_opened" }
  | { type: "substrate_matches"; hits: SubstrateHit[] }
  | { type: "config_updated" }
  | { type: "config"; config: AppConfig }
  | { type: "substrate_doc_count"; count: number }
  | { type: "anthropic_key_saved"; masked: string }
  | { type: "anthropic_key_masked"; masked: string | null }
  | { type: "anthropic_key_configured"; present: boolean }
  | { type: "error"; message: string; code: string };

/* ─────────────── tool-name helpers ─────────────── */

export const TOOL_NAMES = [
  "file_read",
  "file_write",
  "file_edit",
  "bash",
  "git_commit",
  "git_push",
  "mcp_call",
  "vercel_deploy",
  "ollama_chat",
  "anthropic_messages",
  "web_search",
  "spec_query",
] as const;

export type ToolName = (typeof TOOL_NAMES)[number];

/* ─────────────── handoff classifications (state.rs) ─────────────── */

export type Handoff = "dm" | "gm" | "collaborator" | "coder" | "generic";
