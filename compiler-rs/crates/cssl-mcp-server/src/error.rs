//! `McpError` sum-type — stable error-codes per spec § 13.6.
//!
//! Per `_drafts/phase_j/08_l5_mcp_llm_spec.md` § 13.6 the MCP server has a
//! 16-code stable surface. JSON-RPC 2.0 reserves `-32768..-32000` for
//! protocol-level errors ; we layer `McpError` codes on top using the
//! application-error range `-32000..-32099` (well-known JSON-RPC convention).
//!
//! The variants here are deliberately complete : Jθ-2..Jθ-8 will reuse this
//! enum for their tool-implementations rather than each slice introducing a
//! local error-type.

use thiserror::Error;

/// Result alias for crate-level operations.
pub type McpResult<T> = Result<T, McpError>;

/// MCP server error sum-type. Maps to JSON-RPC 2.0 error-objects via
/// [`McpError::as_jsonrpc_code`] + [`McpError::as_jsonrpc_message`].
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum McpError {
    // ─── JSON-RPC 2.0 protocol-level (mirror standard codes) ──────────────
    /// `-32700` — invalid JSON received by the server.
    #[error("parse error: {0}")]
    ParseError(String),
    /// `-32600` — the JSON sent is not a valid Request object.
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    /// `-32601` — the method does not exist or is not available to this session.
    #[error("method not found: {0}")]
    MethodNotFound(String),
    /// `-32602` — invalid method parameters.
    #[error("invalid params: {0}")]
    InvalidParams(String),
    /// `-32603` — internal JSON-RPC error.
    #[error("internal error: {0}")]
    InternalError(String),

    // ─── MCP application-level (-32000..-32099) ────────────────────────────
    /// `-32000` — the requested capability witness is missing from the session.
    #[error("cap denied: {needed:?} (session lacks witness)")]
    CapDenied {
        /// Capability that was required but absent.
        needed: crate::cap::CapKind,
    },
    /// `-32001` — `Cap<RemoteDev>` is required to bind a non-loopback transport.
    #[error("Cap<RemoteDev> required for non-loopback transport")]
    RemoteDevRequired,
    /// `-32002` — biometric tool invocation refused at registration / dispatch.
    #[error("biometric refusal: {0}")]
    BiometricRefused(&'static str),
    /// `-32003` — Σ-mask sovereign cell touched without grant.
    #[error("sigma-refused: {0}")]
    SigmaRefused(&'static str),
    /// `-32004` — session not initialized (pre-handshake tool call).
    #[error("session not initialized")]
    SessionNotInitialized,
    /// `-32005` — session already initialized (double-initialize).
    #[error("session already initialized")]
    SessionAlreadyInitialized,
    /// `-32006` — tool not registered with the [`ToolRegistry`](crate::tool_registry::ToolRegistry).
    #[error("tool not registered: {0}")]
    ToolNotRegistered(String),
    /// `-32007` — transport-level I/O error.
    #[error("transport error: {0}")]
    TransportError(String),
    /// `-32008` — kill-switch fired ; server is shutting down.
    #[error("kill-switch fired: {0}")]
    KillSwitchFired(&'static str),
    /// `-32009` — attestation drift detected (constant mutated since build).
    #[error("attestation drift detected")]
    AttestationDrift,
    /// `-32010` — audit-chain hook returned an error.
    #[error("audit hook error: {0}")]
    AuditHookError(String),
}

impl McpError {
    /// Maps the variant to its JSON-RPC error-code per spec § 13.6.
    #[must_use]
    pub const fn as_jsonrpc_code(&self) -> i32 {
        match self {
            Self::ParseError(_) => -32_700,
            Self::InvalidRequest(_) => -32_600,
            Self::MethodNotFound(_) => -32_601,
            Self::InvalidParams(_) => -32_602,
            Self::InternalError(_) => -32_603,
            Self::CapDenied { .. } => -32_000,
            Self::RemoteDevRequired => -32_001,
            Self::BiometricRefused(_) => -32_002,
            Self::SigmaRefused(_) => -32_003,
            Self::SessionNotInitialized => -32_004,
            Self::SessionAlreadyInitialized => -32_005,
            Self::ToolNotRegistered(_) => -32_006,
            Self::TransportError(_) => -32_007,
            Self::KillSwitchFired(_) => -32_008,
            Self::AttestationDrift => -32_009,
            Self::AuditHookError(_) => -32_010,
        }
    }

    /// Human-readable error message for the JSON-RPC `error.message` field.
    /// Exact strings must remain stable across versions for client-side
    /// pattern-matching.
    #[must_use]
    pub fn as_jsonrpc_message(&self) -> String {
        match self {
            Self::ParseError(_) => "Parse error".to_string(),
            Self::InvalidRequest(_) => "Invalid Request".to_string(),
            Self::MethodNotFound(_) => "Method not found".to_string(),
            Self::InvalidParams(_) => "Invalid params".to_string(),
            Self::InternalError(_) => "Internal error".to_string(),
            Self::CapDenied { .. } => "Capability denied".to_string(),
            Self::RemoteDevRequired => "Cap<RemoteDev> required".to_string(),
            Self::BiometricRefused(_) => "Biometric refused".to_string(),
            Self::SigmaRefused(_) => "Sigma refused".to_string(),
            Self::SessionNotInitialized => "Session not initialized".to_string(),
            Self::SessionAlreadyInitialized => "Session already initialized".to_string(),
            Self::ToolNotRegistered(_) => "Tool not registered".to_string(),
            Self::TransportError(_) => "Transport error".to_string(),
            Self::KillSwitchFired(_) => "Kill-switch fired".to_string(),
            Self::AttestationDrift => "Attestation drift".to_string(),
            Self::AuditHookError(_) => "Audit hook error".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cap::CapKind;

    #[test]
    fn jsonrpc_codes_stable() {
        assert_eq!(McpError::ParseError("x".into()).as_jsonrpc_code(), -32_700);
        assert_eq!(
            McpError::InvalidRequest("x".into()).as_jsonrpc_code(),
            -32_600
        );
        assert_eq!(
            McpError::MethodNotFound("x".into()).as_jsonrpc_code(),
            -32_601
        );
        assert_eq!(
            McpError::InvalidParams("x".into()).as_jsonrpc_code(),
            -32_602
        );
        assert_eq!(
            McpError::InternalError("x".into()).as_jsonrpc_code(),
            -32_603
        );
        assert_eq!(
            McpError::CapDenied {
                needed: CapKind::DevMode
            }
            .as_jsonrpc_code(),
            -32_000
        );
        assert_eq!(McpError::RemoteDevRequired.as_jsonrpc_code(), -32_001);
        assert_eq!(McpError::BiometricRefused("x").as_jsonrpc_code(), -32_002);
        assert_eq!(McpError::SigmaRefused("x").as_jsonrpc_code(), -32_003);
        assert_eq!(McpError::SessionNotInitialized.as_jsonrpc_code(), -32_004);
        assert_eq!(
            McpError::SessionAlreadyInitialized.as_jsonrpc_code(),
            -32_005
        );
        assert_eq!(
            McpError::ToolNotRegistered("x".into()).as_jsonrpc_code(),
            -32_006
        );
        assert_eq!(
            McpError::TransportError("x".into()).as_jsonrpc_code(),
            -32_007
        );
        assert_eq!(McpError::KillSwitchFired("x").as_jsonrpc_code(), -32_008);
        assert_eq!(McpError::AttestationDrift.as_jsonrpc_code(), -32_009);
        assert_eq!(
            McpError::AuditHookError("x".into()).as_jsonrpc_code(),
            -32_010
        );
    }

    #[test]
    fn jsonrpc_messages_stable() {
        assert_eq!(
            McpError::ParseError("x".into()).as_jsonrpc_message(),
            "Parse error"
        );
        assert_eq!(
            McpError::MethodNotFound("x".into()).as_jsonrpc_message(),
            "Method not found"
        );
        assert_eq!(
            McpError::CapDenied {
                needed: CapKind::DevMode
            }
            .as_jsonrpc_message(),
            "Capability denied"
        );
    }

    #[test]
    fn display_includes_payload() {
        let err = McpError::ToolNotRegistered("foo".to_string());
        let display = format!("{err}");
        assert!(display.contains("foo"));
    }

    #[test]
    fn errors_are_clone_and_eq() {
        let a = McpError::SessionNotInitialized;
        // SAFETY-EQUIVALENT : we exercise Clone + PartialEq deliberately ;
        // clippy::redundant_clone suppressed locally for the assertion.
        #[allow(clippy::redundant_clone)]
        let b = a.clone();
        assert_eq!(a, b);
    }
}
