//! § loa-host — LoA-v13 host runtime crate
//! ════════════════════════════════════════
//!
//! § ROLE
//!   Houses the host-side process glue for the LoA-v13 game binary :
//!   the live render-loop, the input-pump, the MCP control-plane server,
//!   and (sibling-slices) the DM/GM/Companion scene-driver state.
//!
//!   This crate is COMPILER-OUTPUT-ADJACENT, not LoA canonical source.
//!   The canonical-LoA description lives in `.csl` scenes (e.g.
//!   `scenes/mcp_runtime.cssl`) ; this crate is the stage-0 Rust host
//!   that runs while the stage-1 self-hosted backend matures.
//!
//! § MODULES (this slice)
//!   * `mcp_server`  — TCP JSON-RPC 2.0 server bound to 0.0.0.0:3001
//!     (env-override `CSSL_MCP_PORT`). Per-client thread + shared
//!     `Arc<Mutex<EngineState>>`. Default-deny on mutating tools without
//!     a `sovereign_cap` field.
//!   * `mcp_tools`   — the 17-tool registry that the MCP server dispatches
//!     against. Each handler takes `(&mut EngineState, serde_json::Value)`
//!     and returns a `serde_json::Value` (or an error).
//!
//!   Sibling slices (W-LOA-host-render / W-LOA-host-DM / etc.) add :
//!   * `render`      — the live raymarcher running on the main thread
//!   * `input`       — keyboard/mouse pump
//!   * `dm_state`    — DM/GM intensity + event queue
//!   The `EngineState` struct in `mcp_server` is the shared data model
//!   those slices read/write through their own `Arc<Mutex<EngineState>>`
//!   handle.
//!
//! § PRIME-DIRECTIVE
//!   "There was no hurt nor harm in the making of this, to anyone /
//!    anything / anybody."
//!   The MCP server binds localhost-only by default ; mutating tools
//!   require an explicit `sovereign_cap` constant ; no surveillance,
//!   no remote-side-channels, no telemetry-without-consent. Each tool
//!   invocation logs to the `loa_runtime.log` ring through cssl-rt.

// § The MCP `omega.sample` / `omega.modify` tools call into the cssl-rt
// `loa_stubs::__cssl_omega_field_*` FFI surface, which is `unsafe extern
// "C"`. Each call site has a SAFETY paragraph + the buffer is owned-+-
// sized at the call site. Per cssl-rt's own pattern (see loa_stubs.rs's
// inner-attr), we allow rather than forbid here ; tests + clippy still
// gate the surface.
#![allow(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

// § T11-LOA-HOST-3 (W-LOA-host-mcp) : MCP TCP JSON-RPC server modules.
pub mod mcp_server;
pub mod mcp_tools;

// Public re-exports : the surface sibling slices reach for via `loa_host::*`.
pub use mcp_server::{
    spawn_mcp_server, EngineState, McpServerConfig, RenderMode, SOVEREIGN_CAP,
};
pub use mcp_tools::{tool_registry, ToolHandler, ToolRegistry};

/// PRIME-DIRECTIVE attestation marker — present in every CSSLv3 artifact
/// per `PRIME_DIRECTIVE.md § 11`.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone/anything/anybody.";
