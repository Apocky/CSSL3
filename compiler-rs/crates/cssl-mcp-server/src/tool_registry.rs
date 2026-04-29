//! Tool trait + ToolRegistry — register-by-name dispatch.
//!
//! Per spec § 9 + § 7 the engine surfaces 41 tools across 9 categories.
//! Jθ-1 (this slice) lays the foundation : the [`Tool`] trait, the
//! [`ToolRegistry`], and the dispatch path with cap-checking. **No actual
//! tools are registered here** — Jθ-2..Jθ-8 own the per-category
//! registration.
//!
//! ## Why not async-trait at stage-0
//!
//! Jθ-2..Jθ-8 will need async tool-handlers (e.g. `pause`/`resume` block on
//! the engine main-thread synchronization). Stage-0 keeps the trait sync
//! ; the async refactor lands in Jθ-1.1 alongside tokio. This keeps the
//! skeleton compilable without an async-runtime dep.
//!
//! ## Privacy gate
//!
//! [`ToolDescriptor::needed_caps`] is the runtime gate. The compile-time
//! biometric-refusal macro `register_tool!` lands in Jθ-8 ; this slice
//! supplies the runtime gate which Jθ-8 will harden with the macro.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::cap::CapKind;
use crate::error::{McpError, McpResult};
use crate::session::Session;

/// Sync handler signature : params-in, JSON-out, error-out.
///
/// `ctx_session` is borrowed mutably so handlers can bump audit-seq via
/// [`Session::touch`]. The handler is responsible for emitting any
/// audit-events via the [`AuditSink`](crate::audit::AuditSink) it received
/// at registration-time (Jθ-2..Jθ-8 wire this up per-tool).
pub type ToolHandler = Box<dyn Fn(&Value, &mut Session) -> McpResult<Value> + Send + Sync>;

/// Descriptor for a registered tool. Captures the metadata needed by
/// `tools/list` + the gate-check needed by `tools/call`.
pub struct ToolDescriptor {
    /// Stable canonical name, e.g. `"engine_state"`. Convention :
    /// `snake_case` ; one tool-name per registry.
    pub name: &'static str,
    /// Caps required to invoke the tool. Empty = none beyond the
    /// implicit DevMode that the session must already hold.
    pub needed_caps: &'static [CapKind],
    /// Audit-tag emitted by the dispatch path. e.g. `"mcp.tool.engine_state"`.
    pub audit_tag: &'static str,
    /// JSON-schema-like description of params (free-form Value at stage-0 ;
    /// Jθ-1.1 may upgrade to a typed schema-derive).
    pub params_schema: Value,
    /// JSON-schema-like description of result.
    pub result_schema: Value,
    /// Handler body.
    pub handler: ToolHandler,
}

impl core::fmt::Debug for ToolDescriptor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ToolDescriptor")
            .field("name", &self.name)
            .field("needed_caps", &self.needed_caps)
            .field("audit_tag", &self.audit_tag)
            .field("params_schema", &self.params_schema)
            .field("result_schema", &self.result_schema)
            .field("handler", &"<fn>")
            .finish()
    }
}

/// Compile-time-friendly tool description trait. Slices Jθ-2..Jθ-8 implement
/// this trait per-tool so the registry can introspect them at registration
/// without runtime allocation.
pub trait Tool {
    /// Stable canonical tool-name.
    const NAME: &'static str;
    /// Caps required to invoke.
    const NEEDED_CAPS: &'static [CapKind];
    /// Audit-tag string.
    const AUDIT_TAG: &'static str;

    /// Describe parameters (free-form JSON at stage-0).
    fn params_schema() -> Value {
        serde_json::json!({})
    }

    /// Describe result.
    fn result_schema() -> Value {
        serde_json::json!({})
    }

    /// Invoke the tool. The default-implementation returns
    /// [`McpError::ToolNotRegistered`] ; real tools override.
    fn invoke(_params: &Value, _session: &mut Session) -> McpResult<Value> {
        Err(McpError::ToolNotRegistered(Self::NAME.to_string()))
    }
}

/// Tool registry — holds the per-name descriptors + handles dispatch.
///
/// `BTreeMap` for deterministic iteration order in `tools/list` ; per spec
/// § 3 the wire-output must be stable for byte-deterministic audit.
#[derive(Default)]
pub struct ToolRegistry {
    by_name: BTreeMap<&'static str, ToolDescriptor>,
}

impl core::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tool_count", &self.by_name.len())
            .field(
                "tool_names",
                &self.by_name.keys().copied().collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl ToolRegistry {
    /// Construct an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_name: BTreeMap::new(),
        }
    }

    /// Register a tool by descriptor. Returns `Err` if the name was already
    /// registered ; collisions are programmer-error.
    pub fn register(&mut self, descriptor: ToolDescriptor) -> McpResult<()> {
        if self.by_name.contains_key(descriptor.name) {
            return Err(McpError::InvalidRequest(format!(
                "tool '{}' already registered",
                descriptor.name
            )));
        }
        self.by_name.insert(descriptor.name, descriptor);
        Ok(())
    }

    /// Convenience : register a [`Tool`]-implementor by binding its
    /// `invoke` fn into a handler closure.
    pub fn register_typed<T: Tool + 'static>(&mut self) -> McpResult<()> {
        let descriptor = ToolDescriptor {
            name: T::NAME,
            needed_caps: T::NEEDED_CAPS,
            audit_tag: T::AUDIT_TAG,
            params_schema: T::params_schema(),
            result_schema: T::result_schema(),
            handler: Box::new(|params, session| T::invoke(params, session)),
        };
        self.register(descriptor)
    }

    /// Returns true iff a tool with the given name is registered.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    /// Number of registered tools.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// True when the registry has no tools registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Iterate descriptors in stable order (BTreeMap → name-sorted).
    pub fn iter(&self) -> impl Iterator<Item = &ToolDescriptor> {
        self.by_name.values()
    }

    /// Filtered tool-list per session caps. Tools whose `needed_caps` are
    /// not satisfied by the session are omitted.
    pub fn list_for_session(&self, session: &Session) -> Vec<&'static str> {
        self.by_name
            .iter()
            .filter(|(_, d)| session.caps.covers_all(d.needed_caps))
            .map(|(n, _)| *n)
            .collect()
    }

    /// Dispatch a `tools/call`. Performs :
    ///   1. session-active check (initialized)
    ///   2. tool-registered check
    ///   3. cap-coverage check
    ///   4. handler invocation
    ///   5. session.touch (bumps audit-seq)
    ///
    /// The handler is responsible for any tool-specific audit-event ; the
    /// dispatcher does NOT emit `mcp.tool.invoked` here (Jθ-8 wires that).
    pub fn dispatch(
        &self,
        tool_name: &str,
        params: &Value,
        session: &mut Session,
    ) -> McpResult<Value> {
        if !session.is_initialized() {
            return Err(McpError::SessionNotInitialized);
        }
        let descriptor = self
            .by_name
            .get(tool_name)
            .ok_or_else(|| McpError::ToolNotRegistered(tool_name.to_string()))?;
        session.require_caps(descriptor.needed_caps)?;
        let result = (descriptor.handler)(params, session)?;
        // Note : we DO NOT touch on cap-denied / not-registered ; only on
        // successful invocation. This keeps audit-seq aligned with
        // semantically-meaningful events.
        session.touch(session.last_activity_frame);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cap::{Cap, DevMode};
    use crate::session::{Principal, SessionId};
    use serde_json::json;

    // ─── Test fixtures ─────────────────────────────────────────────────
    struct PingTool;
    impl Tool for PingTool {
        const NAME: &'static str = "ping";
        const NEEDED_CAPS: &'static [CapKind] = &[CapKind::DevMode];
        const AUDIT_TAG: &'static str = "mcp.tool.ping";
        fn invoke(_params: &Value, _session: &mut Session) -> McpResult<Value> {
            Ok(json!("pong"))
        }
    }

    struct EchoTool;
    impl Tool for EchoTool {
        const NAME: &'static str = "echo";
        const NEEDED_CAPS: &'static [CapKind] = &[CapKind::DevMode];
        const AUDIT_TAG: &'static str = "mcp.tool.echo";
        fn invoke(params: &Value, _session: &mut Session) -> McpResult<Value> {
            Ok(params.clone())
        }
    }

    struct BiometricFixture;
    impl Tool for BiometricFixture {
        const NAME: &'static str = "biometric_fixture";
        const NEEDED_CAPS: &'static [CapKind] = &[CapKind::DevMode, CapKind::BiometricInspect];
        const AUDIT_TAG: &'static str = "mcp.tool.biometric_fixture";
        fn invoke(_params: &Value, _session: &mut Session) -> McpResult<Value> {
            Ok(json!({"refused":"in-real-life"}))
        }
    }

    fn devmode_session() -> Session {
        let mut s = Session::new(SessionId::new(1), Principal::DevModeChild, 0);
        s.caps
            .grant_dev_mode(Cap::<DevMode>::for_test(), 1)
            .expect("grant");
        s.initialize().expect("init");
        s
    }

    // ─── Registry tests ────────────────────────────────────────────────
    #[test]
    fn registry_starts_empty() {
        let r = ToolRegistry::new();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
    }

    #[test]
    fn registry_register_typed() {
        let mut r = ToolRegistry::new();
        r.register_typed::<PingTool>().expect("register");
        assert_eq!(r.len(), 1);
        assert!(r.contains("ping"));
    }

    #[test]
    fn registry_double_register_refused() {
        let mut r = ToolRegistry::new();
        r.register_typed::<PingTool>().expect("first");
        let err = r.register_typed::<PingTool>().unwrap_err();
        assert!(matches!(err, McpError::InvalidRequest(_)));
    }

    #[test]
    fn registry_iter_is_sorted() {
        let mut r = ToolRegistry::new();
        r.register_typed::<PingTool>().expect("ping");
        r.register_typed::<EchoTool>().expect("echo");
        let names: Vec<_> = r.iter().map(|d| d.name).collect();
        // BTreeMap iterates in sorted order
        assert_eq!(names, vec!["echo", "ping"]);
    }

    #[test]
    fn registry_dispatch_succeeds() {
        let mut r = ToolRegistry::new();
        r.register_typed::<PingTool>().expect("register");
        let mut s = devmode_session();
        let v = r.dispatch("ping", &json!({}), &mut s).expect("dispatch");
        assert_eq!(v, json!("pong"));
    }

    #[test]
    fn registry_dispatch_unregistered_errors() {
        let r = ToolRegistry::new();
        let mut s = devmode_session();
        let err = r
            .dispatch("does-not-exist", &json!({}), &mut s)
            .unwrap_err();
        assert!(matches!(err, McpError::ToolNotRegistered(_)));
    }

    #[test]
    fn registry_dispatch_uninitialized_errors() {
        let mut r = ToolRegistry::new();
        r.register_typed::<PingTool>().expect("register");
        let mut s = Session::new(SessionId::new(1), Principal::DevModeChild, 0);
        s.caps
            .grant_dev_mode(Cap::<DevMode>::for_test(), 1)
            .expect("grant");
        // s NOT initialized
        let err = r.dispatch("ping", &json!({}), &mut s).unwrap_err();
        assert!(matches!(err, McpError::SessionNotInitialized));
    }

    #[test]
    fn registry_dispatch_cap_denied() {
        let mut r = ToolRegistry::new();
        r.register_typed::<BiometricFixture>().expect("register");
        let mut s = devmode_session();
        // s has DevMode but NOT BiometricInspect
        let err = r
            .dispatch("biometric_fixture", &json!({}), &mut s)
            .unwrap_err();
        match err {
            McpError::CapDenied { needed } => {
                assert_eq!(needed, CapKind::BiometricInspect);
            }
            _ => panic!("expected CapDenied for biometric"),
        }
    }

    #[test]
    fn registry_list_for_session_filters_by_caps() {
        let mut r = ToolRegistry::new();
        r.register_typed::<PingTool>().expect("ping");
        r.register_typed::<BiometricFixture>().expect("biometric");
        let s = devmode_session();
        let listed = r.list_for_session(&s);
        // ping requires DevMode (granted) ; biometric_fixture requires
        // BiometricInspect too (NOT granted) → filtered out
        assert!(listed.contains(&"ping"));
        assert!(!listed.contains(&"biometric_fixture"));
    }

    #[test]
    fn registry_dispatch_passes_params() {
        let mut r = ToolRegistry::new();
        r.register_typed::<EchoTool>().expect("register");
        let mut s = devmode_session();
        let inp = json!({"hello":"world"});
        let v = r.dispatch("echo", &inp, &mut s).expect("dispatch");
        assert_eq!(v, inp);
    }

    #[test]
    fn registry_dispatch_bumps_audit_seq() {
        let mut r = ToolRegistry::new();
        r.register_typed::<PingTool>().expect("register");
        let mut s = devmode_session();
        let pre = s.audit_seq;
        r.dispatch("ping", &json!({}), &mut s).expect("dispatch");
        assert!(s.audit_seq > pre);
    }
}
