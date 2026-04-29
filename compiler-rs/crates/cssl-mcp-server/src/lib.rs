//! cssl-mcp-server — JSON-RPC 2.0 MCP server skeleton for CSSLv3 L5 layer.
//!
//! § T11-D229 (Wave-Jθ-1) : foundational crate for the LLM-runtime-attach
//! surface defined in `_drafts/phase_j/08_l5_mcp_llm_spec.md`. This is the
//! **skeleton-only** slice ; subsequent slices Jθ-2..Jθ-8 register specific
//! tool-categories (state-inspect / telemetry / health / time-control /
//! hot-reload / test-status / privacy-cap-audit-IFC).
//!
//! ## Crate layout
//!
//! ```text
//! cssl_mcp_server::jsonrpc       — JSON-RPC 2.0 envelope (Request / Response /
//!                                   ErrorObject + parse + emit helpers)
//! cssl_mcp_server::transport     — Transport trait + StdioTransport impl
//!                                   (line-delimited JSON @ stage-0) +
//!                                   UnixSocket / WebSocket trait stubs
//! cssl_mcp_server::session       — Session struct + cap-token-bound +
//!                                   handshake protocol returning Cap-checked
//!                                   session
//! cssl_mcp_server::cap           — Cap<DevMode> / Cap<RemoteDev> /
//!                                   Cap<BiometricInspect> sealed-newtype stubs
//! cssl_mcp_server::tool_registry — Tool trait + ToolRegistry + register-by-name
//!                                   dispatch (no actual tools registered ;
//!                                   Jθ-2..8 own that)
//! cssl_mcp_server::server        — McpServer struct + accept-loop +
//!                                   transport-multiplexing
//! cssl_mcp_server::audit         — audit-chain hook stubs (real BLAKE3 +
//!                                   Ed25519 wires-in via D131)
//! cssl_mcp_server::error         — McpError sum-type
//! ```
//!
//! ## Privacy invariants (load-bearing)
//!
//! Per `PRIME_DIRECTIVE.md § 1` (anti-surveillance) + § 0 (consent = OS) :
//!
//! - `Cap<BiometricInspect>` is **default-DENIED** : the only constructor is
//!   `for_test()` which is non-Copy + non-Clone, so test fixtures cannot
//!   accidentally grant it broadly. Real grants land via D131 cap-issuance
//!   once that crate stabilizes.
//! - `Cap<RemoteDev>` is **default-DENIED** : non-loopback transports refuse
//!   without an explicit witness.
//! - `Cap<DevMode>` is **default-OFF** : `McpServer::new` consumes the cap on
//!   construction ; release-builds can compile-out the constructor with a
//!   `dev-mode` feature gate (deferred to Jθ-1.1 — too much workspace churn
//!   to gate at this slice).
//! - The `Tool` trait advertises `NEEDED_CAPS` so `ToolRegistry::dispatch`
//!   can reject calls whose session lacks the witnesses. Jθ-8 adds the
//!   compile-time biometric-refusal at `register_tool!` macro level.
//!
//! ## ATTESTATION
//! Per `PRIME_DIRECTIVE.md § 11` : there was no hurt nor harm in the making
//! of this, to anyone, anything, or anybody. Biometric tools are NOT
//! registered in this slice (Jθ-8 owns that gate).

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]

pub mod audit;
pub mod cap;
pub mod error;
pub mod jsonrpc;
pub mod server;
pub mod session;
pub mod tool_registry;
pub mod transport;

pub use audit::{AuditEvent, AuditSink, NullAuditSink};
pub use cap::{BiometricInspect, Cap, CapKind, CapMarker, CapWitness, DevMode, RemoteDev};
pub use error::{McpError, McpResult};
pub use jsonrpc::{ErrorObject, JsonRpcVersion, Notification, Request, Response, ResponseBody};
pub use server::McpServer;
pub use session::{Principal, Session, SessionCapSet, SessionId};
pub use tool_registry::{Tool, ToolDescriptor, ToolHandler, ToolRegistry};
pub use transport::{StdioTransport, Transport};

/// Stable attestation string. Crate consumers can match on this constant for
/// drift-detection per spec § 20.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

/// MCP protocol version pinned at this slice. Spec § 2 anchors to
/// `MCP-2025-03-26`. If the upstream protocol bumps, this constant + a
/// DECISIONS amendment are required.
pub const MCP_PROTOCOL_VERSION: &str = "MCP-2025-03-26";

/// Crate-level scaffold marker. Used in the integration test that asserts the
/// skeleton compiles with the documented public surface.
pub const STAGE0_SCAFFOLD: &str = "cssl-mcp-server::T11-D229::W-Jθ-1::skeleton";

#[cfg(test)]
mod sanity_tests {
    use super::*;

    #[test]
    fn attestation_constant_present() {
        assert!(ATTESTATION.contains("no hurt nor harm"));
    }

    #[test]
    fn protocol_version_pinned() {
        assert_eq!(MCP_PROTOCOL_VERSION, "MCP-2025-03-26");
    }

    #[test]
    fn scaffold_marker_non_empty() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
        assert!(STAGE0_SCAFFOLD.contains("D229"));
    }
}
