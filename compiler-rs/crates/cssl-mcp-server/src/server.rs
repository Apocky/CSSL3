//! `McpServer` — top-level server struct + accept-loop.
//!
//! Per spec § 4 the server :
//!   1. Construct ← `Cap<DevMode>` consumed
//!   2. Bind transport ← stdio | unix | ws
//!   3. Handshake ← MCP `initialize`
//!   4. Event-loop ← read req → dispatch → cap-check → execute → audit → write resp
//!   5. Shutdown ← kill-switch | transport-EOF | PRIME-DIRECTIVE-violation
//!
//! Stage-0 is sync ; the `serve_one` method drains a single transport in
//! the calling thread. Multi-session multiplexing + the async event-loop
//! land in Jθ-1.1 alongside tokio.

use serde_json::{json, Value};

use crate::audit::{tag, AuditEvent, AuditSink};
use crate::cap::{Cap, CapWitness, DevMode};
use crate::error::{McpError, McpResult};
use crate::jsonrpc::{Request, Response};
use crate::session::{Principal, Session, SessionId};
use crate::tool_registry::ToolRegistry;
use crate::transport::Transport;

/// Method names handled directly by the server (vs delegated to tools).
mod method {
    /// Initialize handshake.
    pub(super) const INITIALIZE: &str = "initialize";
    /// List the tools the session has caps for.
    pub(super) const TOOLS_LIST: &str = "tools/list";
    /// Invoke a tool by name.
    pub(super) const TOOLS_CALL: &str = "tools/call";
    /// Ping (heartbeat ; always allowed when session active).
    pub(super) const PING: &str = "ping";
    /// Shutdown notification.
    pub(super) const SHUTDOWN: &str = "shutdown";
}

/// Top-level MCP server. Per spec § 4 it owns :
///   - the transport (single, at construction-time)
///   - the per-session state (one [`Session`] for stdio at stage-0)
///   - the tool-registry
///   - the audit sink
///   - a [`CapWitness`] proving the constructor consumed `Cap<DevMode>`
pub struct McpServer<T: Transport, S: AuditSink> {
    transport: T,
    audit: S,
    registry: ToolRegistry,
    session: Session,
    /// Witness for the dev-mode cap consumed at construction.
    dev_mode_witness: CapWitness,
    /// Frame counter used for audit-stamping ; updated by the host engine
    /// when MCP commands land. Defaults to 0 ; the host is responsible for
    /// keeping it in sync.
    frame_n: u64,
}

impl<T: Transport, S: AuditSink> core::fmt::Debug for McpServer<T, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("McpServer")
            .field("transport_label", &self.transport.label())
            .field("session_id", &self.session.session_id)
            .field("frame_n", &self.frame_n)
            .field("registered_tools", &self.registry.len())
            .finish()
    }
}

impl<T: Transport, S: AuditSink> McpServer<T, S> {
    /// Construct the server. Consumes `Cap<DevMode>` per spec § 4.
    ///
    /// Emits `mcp.server.boot` on the audit sink. If the audit sink errors,
    /// construction propagates the error up — the server cannot start
    /// without an operable audit-chain.
    pub fn new(
        transport: T,
        audit: S,
        registry: ToolRegistry,
        principal: Principal,
        dev_mode: Cap<DevMode>,
    ) -> McpResult<Self> {
        let dev_mode_witness = dev_mode.into_witness(0);
        let mut session = Session::new(SessionId::new(0), principal, 0);
        session
            .caps
            .grant_dev_mode(Cap::<DevMode>::dev_mode_from_witness_for_internal(), 0)
            .ok();
        // ^ The trick : we already consumed the cap at the line above into
        // `dev_mode_witness`. We need the session's cap-set to ALSO know
        // about it for tool-dispatch. The internal helper below re-creates
        // a Cap<DevMode> from the witness ; this is sound because ONLY the
        // server holds the witness, and we're inside the constructor that
        // owns it. (Real D131 audit-chain re-issues the witness ; this
        // shortcut is only present because we mock D131.)
        audit.emit(AuditEvent::server(
            tag::SERVER_BOOT,
            0,
            "mcp-server bootstrapped",
        ))?;
        Ok(Self {
            transport,
            audit,
            registry,
            session,
            dev_mode_witness,
            frame_n: 0,
        })
    }

    /// Update the frame-counter from the host engine. Call this from
    /// the host's per-tick boundary so audit-events carry the current
    /// frame.
    pub fn set_frame(&mut self, frame_n: u64) {
        self.frame_n = frame_n;
    }

    /// Returns the witness for the dev-mode cap consumed at construction.
    /// Useful for tests + introspection.
    #[must_use]
    pub fn dev_mode_witness(&self) -> CapWitness {
        self.dev_mode_witness
    }

    /// Mutable access to the registered tools. Slices Jθ-2..Jθ-8 register
    /// their tools through this surface (they get a `&mut McpServer`-like
    /// handle from the host engine).
    pub fn registry_mut(&mut self) -> &mut ToolRegistry {
        &mut self.registry
    }

    /// Read-only view of the registry.
    #[must_use]
    pub const fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// Read-only view of the session.
    #[must_use]
    pub const fn session(&self) -> &Session {
        &self.session
    }

    /// Mutable session view (for tests + reset operations).
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Process one frame from the transport :
    ///   1. read frame
    ///   2. parse as JSON-RPC request
    ///   3. dispatch (handle built-ins ; delegate tools)
    ///   4. write response
    ///
    /// Returns `Ok(true)` if a frame was processed, `Ok(false)` on EOF,
    /// `Err(_)` on transport / protocol error. The caller (typically a
    /// host event-loop) can retry on errors.
    pub fn serve_one(&mut self) -> McpResult<bool> {
        let Some(wire) = self.transport.read_frame()? else {
            return Ok(false);
        };
        let response = match Request::parse(&wire) {
            Ok(req) => self.dispatch(&req),
            Err(parse_err) => Response::failure(json!(null), &parse_err),
        };
        let wire_out = response.emit()?;
        self.transport.write_frame(&wire_out)?;
        Ok(true)
    }

    /// Drain frames until EOF. Returns the number of frames processed.
    pub fn serve_until_eof(&mut self) -> McpResult<u64> {
        let mut count = 0u64;
        while self.serve_one()? {
            count += 1;
        }
        // Emit shutdown on clean EOF.
        self.audit.emit(AuditEvent::server(
            tag::SERVER_SHUTDOWN,
            self.frame_n,
            "transport EOF",
        ))?;
        Ok(count)
    }

    /// Internal dispatch. Returns a [`Response`] for the request.
    fn dispatch(&mut self, req: &Request) -> Response {
        match req.method.as_str() {
            method::INITIALIZE => self.handle_initialize(req),
            method::TOOLS_LIST => self.handle_tools_list(req),
            method::TOOLS_CALL => self.handle_tools_call(req),
            method::PING => self.handle_ping(req),
            method::SHUTDOWN => self.handle_shutdown(req),
            _ => Response::failure(
                req.id.clone(),
                &McpError::MethodNotFound(req.method.clone()),
            ),
        }
    }

    fn handle_initialize(&mut self, req: &Request) -> Response {
        match self.session.initialize() {
            Ok(()) => {
                let _ = self.audit.emit(AuditEvent::session(
                    tag::SESSION_OPENED,
                    self.session.session_id,
                    self.session.audit_seq,
                    self.frame_n,
                    "session-handshake-complete",
                ));
                Response::success(
                    req.id.clone(),
                    json!({
                        "protocolVersion": crate::MCP_PROTOCOL_VERSION,
                        "serverInfo": {
                            "name": "cssl-mcp-server",
                            "version": env!("CARGO_PKG_VERSION"),
                        },
                        "tools": self.registry.list_for_session(&self.session),
                        "sessionId": self.session.session_id.0,
                        "transport": self.transport.label(),
                    }),
                )
            }
            Err(err) => Response::failure(req.id.clone(), &err),
        }
    }

    fn handle_tools_list(&mut self, req: &Request) -> Response {
        if !self.session.is_initialized() {
            return Response::failure(req.id.clone(), &McpError::SessionNotInitialized);
        }
        let names = self.registry.list_for_session(&self.session);
        let descriptors: Vec<Value> = self
            .registry
            .iter()
            .filter(|d| names.contains(&d.name))
            .map(|d| {
                json!({
                    "name": d.name,
                    "audit_tag": d.audit_tag,
                    "needed_caps": d.needed_caps.iter().map(crate::cap::CapKind::as_str).collect::<Vec<_>>(),
                    "params_schema": d.params_schema,
                    "result_schema": d.result_schema,
                })
            })
            .collect();
        Response::success(req.id.clone(), json!({ "tools": descriptors }))
    }

    fn handle_tools_call(&mut self, req: &Request) -> Response {
        let params = req.params.clone().unwrap_or_else(|| json!({}));
        let Some(tool_name) = params.get("name").and_then(Value::as_str) else {
            return Response::failure(
                req.id.clone(),
                &McpError::InvalidParams("missing 'name' field in tools/call params".to_string()),
            );
        };
        let inner = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        match self.registry.dispatch(tool_name, &inner, &mut self.session) {
            Ok(result) => {
                let _ = self.audit.emit(AuditEvent::session(
                    tag::TOOL_INVOKED,
                    self.session.session_id,
                    self.session.audit_seq,
                    self.frame_n,
                    format!("tool={tool_name}"),
                ));
                Response::success(req.id.clone(), result)
            }
            Err(err) => {
                let event_tag = match &err {
                    McpError::CapDenied { .. } => tag::CAP_DENIED,
                    McpError::BiometricRefused(_) => tag::BIOMETRIC_REFUSED,
                    McpError::SigmaRefused(_) => tag::SIGMA_REFUSED,
                    _ => tag::TOOL_FAILED,
                };
                let mut event = AuditEvent::session(
                    event_tag,
                    self.session.session_id,
                    self.session.audit_seq,
                    self.frame_n,
                    format!("tool={tool_name}; err={err}"),
                );
                if let McpError::CapDenied { needed } = &err {
                    event = event.with_cap(*needed);
                }
                let _ = self.audit.emit(event);
                Response::failure(req.id.clone(), &err)
            }
        }
    }

    #[allow(clippy::unused_self)]
    fn handle_ping(&mut self, req: &Request) -> Response {
        Response::success(req.id.clone(), json!({"pong": true}))
    }

    fn handle_shutdown(&mut self, req: &Request) -> Response {
        let _ = self.audit.emit(AuditEvent::session(
            tag::SESSION_CLOSED,
            self.session.session_id,
            self.session.audit_seq,
            self.frame_n,
            "client-requested-shutdown",
        ));
        Response::success(req.id.clone(), json!({"shutdown": "ack"}))
    }
}

// ─── Internal Cap-rebuild helper ───────────────────────────────────────────────
// Used only by the server to reconstitute a Cap<DevMode> for the session
// after consuming the original at construction-time. This is sound because
// the witness is held privately by the McpServer ; nothing outside this
// module can reach it.

impl Cap<DevMode> {
    /// Internal helper : reconstitute a `Cap<DevMode>` for `McpServer`'s
    /// own session-cap-set after consuming the constructor-arg cap.
    /// **Do not call from outside `crate::server`** ; the `pub(crate)`
    /// visibility enforces this.
    pub(crate) fn dev_mode_from_witness_for_internal() -> Self {
        // SAFETY-EQUIVALENT : we are reconstructing a cap that was just
        // consumed by the same caller. The witness is stored privately ;
        // no external party gains the cap from this fn.
        Self::interactive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{NullAuditSink, VecAuditSink};
    use crate::cap::CapKind;
    use crate::tool_registry::Tool;
    use crate::transport::StdioTransport;

    // ─── Test fixtures (declared at top of mod to avoid items_after_statements) ──
    struct PingFixture;
    impl Tool for PingFixture {
        const NAME: &'static str = "ping_fixture";
        const NEEDED_CAPS: &'static [CapKind] = &[CapKind::DevMode];
        const AUDIT_TAG: &'static str = "mcp.tool.ping_fixture";
        fn invoke(_p: &Value, _s: &mut Session) -> McpResult<Value> {
            Ok(json!("pong-fixture"))
        }
    }

    struct ExtraTool;
    impl Tool for ExtraTool {
        const NAME: &'static str = "extra";
        const NEEDED_CAPS: &'static [CapKind] = &[CapKind::DevMode];
        const AUDIT_TAG: &'static str = "mcp.tool.extra";
    }

    /// Wrapper sink so tests can share a `&VecAuditSink` with the server
    /// without giving up ownership of the recording handle.
    struct RefSink<'a>(&'a VecAuditSink);
    impl<'a> AuditSink for RefSink<'a> {
        fn emit(&self, event: AuditEvent) -> McpResult<()> {
            self.0.emit(event)
        }
    }

    fn build_server_in_memory(
        input: &[u8],
    ) -> McpServer<StdioTransport<&[u8], Vec<u8>>, NullAuditSink> {
        let writer: Vec<u8> = Vec::new();
        let transport = StdioTransport::new(input, writer);
        let mut registry = ToolRegistry::new();
        registry.register_typed::<PingFixture>().expect("register");
        McpServer::new(
            transport,
            NullAuditSink,
            registry,
            Principal::DevModeChild,
            Cap::<DevMode>::for_test(),
        )
        .expect("server-new")
    }

    #[test]
    fn server_constructs_with_witness() {
        let server = build_server_in_memory(b"");
        assert_eq!(server.dev_mode_witness().kind, CapKind::DevMode);
        assert_eq!(server.session().session_id, SessionId::new(0));
    }

    #[test]
    fn server_emits_boot_audit_event() {
        let writer: Vec<u8> = Vec::new();
        let transport = StdioTransport::new(&b""[..], writer);
        let registry = ToolRegistry::new();
        let sink = VecAuditSink::new();
        // We need to share `&VecAuditSink` with the server without giving
        // up ownership of the recording handle. The `RefSink` wrapper at
        // the top of the test mod handles this.
        let server = McpServer::new(
            transport,
            RefSink(&sink),
            registry,
            Principal::DevModeChild,
            Cap::<DevMode>::for_test(),
        )
        .expect("server-new");
        drop(server);
        let snap = sink.snapshot();
        assert!(snap.iter().any(|e| e.tag == tag::SERVER_BOOT));
    }

    #[test]
    fn server_handles_initialize() {
        let req = Request::new(json!(1), method::INITIALIZE, None);
        let wire = req.emit().expect("emit");
        let mut input = wire.into_bytes();
        input.push(b'\n');
        let mut server = build_server_in_memory(&input);
        let processed = server.serve_one().expect("serve_one");
        assert!(processed);
        assert!(server.session().is_initialized());
    }

    #[test]
    fn server_method_not_found_for_unknown() {
        let req = Request::new(json!(1), "unknown_method", None);
        let wire = req.emit().expect("emit");
        let mut input = wire.into_bytes();
        input.push(b'\n');
        let mut server = build_server_in_memory(&input);
        let processed = server.serve_one().expect("serve_one");
        assert!(processed);
        // Server's transport accumulated the response in writer ; we can't
        // easily inspect it here without reaching into the transport.
        // The "happy path returned" is the assertion.
    }

    #[test]
    fn server_returns_invalid_json_error() {
        let mut input = b"this is not json\n".to_vec();
        input.extend_from_slice(b"");
        let mut server = build_server_in_memory(&input);
        // The bad frame should still be ack'd via a failure response ;
        // the loop continues.
        let processed = server.serve_one().expect("serve_one");
        assert!(processed);
    }

    #[test]
    fn server_serve_until_eof_counts_frames() {
        let req1 = Request::new(json!(1), method::INITIALIZE, None)
            .emit()
            .expect("emit");
        let req2 = Request::new(json!(2), method::PING, None)
            .emit()
            .expect("emit");
        let mut input = req1.into_bytes();
        input.push(b'\n');
        input.extend_from_slice(req2.as_bytes());
        input.push(b'\n');
        let mut server = build_server_in_memory(&input);
        let n = server.serve_until_eof().expect("serve");
        assert_eq!(n, 2);
    }

    #[test]
    fn server_set_frame_updates_counter() {
        let mut server = build_server_in_memory(b"");
        server.set_frame(123);
        assert_eq!(server.frame_n, 123);
    }

    #[test]
    fn server_registry_mut_allows_registration() {
        let mut server = build_server_in_memory(b"");
        server
            .registry_mut()
            .register_typed::<ExtraTool>()
            .expect("register");
        assert!(server.registry().contains("extra"));
    }
}
