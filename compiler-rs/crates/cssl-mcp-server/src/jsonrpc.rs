//! JSON-RPC 2.0 envelope per spec § 3.
//!
//! Conforms to the JSON-RPC 2.0 specification (<https://www.jsonrpc.org/specification>).
//! The MCP standard (2025-03-26) layers on top of JSON-RPC 2.0 ; this module
//! only models the wire-envelope. Method dispatch + cap-checks live in
//! [`server`](crate::server) + [`tool_registry`](crate::tool_registry).
//!
//! ## Determinism
//!
//! Per spec § 3 we emit JSON via `serde_json` with a stable key-order.
//! `serde_json::Value` already preserves insertion-order via `serde_json`'s
//! `preserve_order` feature ; for byte-determinism in audit-recording we
//! emit canonical key-order at the struct-field level (declaration order in
//! `#[derive(Serialize)]`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::McpError;

/// JSON-RPC version literal. Always serialized as `"2.0"` ; envelopes
/// declaring a different version are rejected with [`McpError::InvalidRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonRpcVersion;

impl JsonRpcVersion {
    /// The literal version string as it appears on the wire.
    pub const LITERAL: &'static str = "2.0";
}

/// JSON-RPC 2.0 request envelope.
///
/// `id` MAY be `null`, a string, or a number per the spec ; we model it as
/// [`serde_json::Value`] to preserve fidelity on round-trip. Notifications
/// (no `id`) are represented by [`Notification`] instead.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Request {
    /// MUST be `"2.0"`.
    pub jsonrpc: String,
    /// Method name. Convention : `dotted.case.identifier` (e.g.
    /// `tools/list`, `tools/call`, `initialize`).
    pub method: String,
    /// Method parameters. Per spec § 3, defaults to `{}` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    /// Request id. MUST be present + non-null for requests (vs notifications).
    pub id: Value,
}

/// JSON-RPC 2.0 notification — like [`Request`] but without an `id`.
/// The server MUST NOT respond to a notification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Notification {
    /// MUST be `"2.0"`.
    pub jsonrpc: String,
    /// Method name.
    pub method: String,
    /// Method parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 response envelope. Either `result` OR `error` is set ;
/// per spec the two are mutually exclusive.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Response {
    /// MUST be `"2.0"`.
    pub jsonrpc: String,
    /// Echoed from the corresponding [`Request`].
    pub id: Value,
    /// Result or error body.
    #[serde(flatten)]
    pub body: ResponseBody,
}

/// The mutually-exclusive `result` / `error` body of a [`Response`].
///
/// `#[serde(untagged)]` resolves to whichever variant matches the JSON
/// shape ; valid responses MUST contain exactly one of these fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ResponseBody {
    /// Success body.
    Success {
        /// Tool / method result. May be any JSON value.
        result: Value,
    },
    /// Error body.
    Failure {
        /// JSON-RPC error-object.
        error: ErrorObject,
    },
}

/// JSON-RPC 2.0 error-object per spec § 5.1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorObject {
    /// Error code. See [`McpError::as_jsonrpc_code`].
    pub code: i32,
    /// Human-readable message. See [`McpError::as_jsonrpc_message`].
    pub message: String,
    /// Optional structured payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Request {
    /// Construct a new request envelope.
    #[must_use]
    pub fn new(id: impl Into<Value>, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: JsonRpcVersion::LITERAL.to_string(),
            method: method.into(),
            params,
            id: id.into(),
        }
    }

    /// Parse a JSON-RPC request from a wire string. Validates the
    /// `"jsonrpc": "2.0"` field per spec.
    pub fn parse(wire: &str) -> Result<Self, McpError> {
        let req: Self = serde_json::from_str(wire)
            .map_err(|e| McpError::ParseError(format!("invalid JSON: {e}")))?;
        if req.jsonrpc != JsonRpcVersion::LITERAL {
            return Err(McpError::InvalidRequest(format!(
                "jsonrpc field must be \"2.0\", got {:?}",
                req.jsonrpc
            )));
        }
        if req.method.is_empty() {
            return Err(McpError::InvalidRequest(
                "method field is empty".to_string(),
            ));
        }
        Ok(req)
    }

    /// Serialize the request to its wire form.
    pub fn emit(&self) -> Result<String, McpError> {
        serde_json::to_string(self)
            .map_err(|e| McpError::InternalError(format!("emit failed: {e}")))
    }
}

impl Response {
    /// Construct a success response.
    #[must_use]
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: JsonRpcVersion::LITERAL.to_string(),
            id,
            body: ResponseBody::Success { result },
        }
    }

    /// Construct a failure response from an [`McpError`]. Honours the
    /// stable code+message mapping.
    #[must_use]
    pub fn failure(id: Value, err: &McpError) -> Self {
        Self {
            jsonrpc: JsonRpcVersion::LITERAL.to_string(),
            id,
            body: ResponseBody::Failure {
                error: ErrorObject {
                    code: err.as_jsonrpc_code(),
                    message: err.as_jsonrpc_message(),
                    data: None,
                },
            },
        }
    }

    /// Serialize to wire form.
    pub fn emit(&self) -> Result<String, McpError> {
        serde_json::to_string(self)
            .map_err(|e| McpError::InternalError(format!("emit failed: {e}")))
    }

    /// Parse from wire form. Used primarily by tests + diagnostics.
    pub fn parse(wire: &str) -> Result<Self, McpError> {
        let resp: Self = serde_json::from_str(wire)
            .map_err(|e| McpError::ParseError(format!("invalid JSON: {e}")))?;
        if resp.jsonrpc != JsonRpcVersion::LITERAL {
            return Err(McpError::InvalidRequest(format!(
                "jsonrpc field must be \"2.0\", got {:?}",
                resp.jsonrpc
            )));
        }
        Ok(resp)
    }
}

impl Notification {
    /// Construct a new notification.
    #[must_use]
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: JsonRpcVersion::LITERAL.to_string(),
            method: method.into(),
            params,
        }
    }

    /// Serialize to wire form.
    pub fn emit(&self) -> Result<String, McpError> {
        serde_json::to_string(self)
            .map_err(|e| McpError::InternalError(format!("emit failed: {e}")))
    }

    /// Parse from wire form.
    pub fn parse(wire: &str) -> Result<Self, McpError> {
        let n: Self = serde_json::from_str(wire)
            .map_err(|e| McpError::ParseError(format!("invalid JSON: {e}")))?;
        if n.jsonrpc != JsonRpcVersion::LITERAL {
            return Err(McpError::InvalidRequest(format!(
                "jsonrpc field must be \"2.0\", got {:?}",
                n.jsonrpc
            )));
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_round_trip() {
        let req = Request::new(json!(1), "tools/list", Some(json!({})));
        let wire = req.emit().expect("emit");
        let back = Request::parse(&wire).expect("parse");
        assert_eq!(back.method, "tools/list");
        assert_eq!(back.id, json!(1));
        assert_eq!(back.jsonrpc, "2.0");
    }

    #[test]
    fn request_parse_rejects_bad_version() {
        let wire = r#"{"jsonrpc":"1.0","method":"x","id":1}"#;
        let err = Request::parse(wire).unwrap_err();
        assert!(matches!(err, McpError::InvalidRequest(_)));
    }

    #[test]
    fn request_parse_rejects_empty_method() {
        let wire = r#"{"jsonrpc":"2.0","method":"","id":1}"#;
        let err = Request::parse(wire).unwrap_err();
        assert!(matches!(err, McpError::InvalidRequest(_)));
    }

    #[test]
    fn request_parse_rejects_invalid_json() {
        let wire = "not json";
        let err = Request::parse(wire).unwrap_err();
        assert!(matches!(err, McpError::ParseError(_)));
    }

    #[test]
    fn response_success_round_trip() {
        let resp = Response::success(json!(42), json!({"frame_n": 100}));
        let wire = resp.emit().expect("emit");
        let back = Response::parse(&wire).expect("parse");
        assert_eq!(back.id, json!(42));
        match back.body {
            ResponseBody::Success { result } => {
                assert_eq!(result, json!({"frame_n": 100}));
            }
            ResponseBody::Failure { .. } => panic!("expected success body"),
        }
    }

    #[test]
    fn response_failure_round_trip() {
        let err = McpError::MethodNotFound("foo".to_string());
        let resp = Response::failure(json!(7), &err);
        let wire = resp.emit().expect("emit");
        let back = Response::parse(&wire).expect("parse");
        match back.body {
            ResponseBody::Failure { error } => {
                assert_eq!(error.code, -32_601);
                assert_eq!(error.message, "Method not found");
            }
            ResponseBody::Success { .. } => panic!("expected failure body"),
        }
    }

    #[test]
    fn notification_round_trip() {
        let n = Notification::new("notifications/log", Some(json!({"level": "info"})));
        let wire = n.emit().expect("emit");
        let back = Notification::parse(&wire).expect("parse");
        assert_eq!(back.method, "notifications/log");
    }

    #[test]
    fn notification_omits_id() {
        let n = Notification::new("ping", None);
        let wire = n.emit().expect("emit");
        assert!(!wire.contains("\"id\""));
    }

    #[test]
    fn version_literal_is_2_0() {
        assert_eq!(JsonRpcVersion::LITERAL, "2.0");
    }

    #[test]
    fn error_object_serializes_data_when_present() {
        let err_obj = ErrorObject {
            code: -32_000,
            message: "x".to_string(),
            data: Some(json!({"hint": "foo"})),
        };
        let wire = serde_json::to_string(&err_obj).unwrap();
        assert!(wire.contains("\"data\""));
    }

    #[test]
    fn error_object_omits_data_when_absent() {
        let err_obj = ErrorObject {
            code: -32_000,
            message: "x".to_string(),
            data: None,
        };
        let wire = serde_json::to_string(&err_obj).unwrap();
        assert!(!wire.contains("\"data\""));
    }

    #[test]
    fn request_with_null_id_round_trips() {
        let req = Request::new(json!(null), "x", None);
        let wire = req.emit().expect("emit");
        let back = Request::parse(&wire).expect("parse");
        assert_eq!(back.id, json!(null));
    }

    #[test]
    fn request_with_string_id_round_trips() {
        let req = Request::new(json!("req-abc"), "x", None);
        let wire = req.emit().expect("emit");
        let back = Request::parse(&wire).expect("parse");
        assert_eq!(back.id, json!("req-abc"));
    }
}
