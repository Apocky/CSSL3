//! Transport layer — sync I/O at stage-0 ; async deferred to Jθ-1.1.
//!
//! Per spec § 5 the MCP server supports three transports :
//!   - stdio (default, line-delimited JSON @ stage-0 ; LSP `Content-Length`
//!     framing in Jθ-1.1)
//!   - unix-socket (multi-client local inspector ; trait-only @ stage-0)
//!   - websocket (loopback default + `Cap<RemoteDev>` for non-loopback ;
//!     trait-only @ stage-0)
//!
//! Stage-0 deliberately avoids tokio. The [`Transport`] trait uses
//! `std::io::{BufRead, Write}` which keeps the dep-graph minimal and the
//! crate compilable in environments where tokio is unavailable. The async
//! variant lands in Jθ-1.1.

use std::io::{self, BufRead, BufReader, Read, Write};

use crate::error::McpError;

/// Transport surface — read line-delimited JSON-RPC payloads + write
/// line-delimited JSON-RPC payloads.
///
/// Errors are mapped to [`McpError::TransportError`] so callers can route
/// them through the same audit + cap-check flow as protocol-level errors.
pub trait Transport {
    /// Read one frame (up-to-and-including the terminating newline) from
    /// the transport. Returns `Ok(None)` on clean EOF, `Ok(Some(line))`
    /// for a payload, `Err(_)` for I/O failure.
    fn read_frame(&mut self) -> Result<Option<String>, McpError>;

    /// Write one frame (a single JSON-RPC payload) followed by a terminating
    /// newline. Implementations MUST flush before returning so the receiver
    /// observes the frame promptly.
    fn write_frame(&mut self, frame: &str) -> Result<(), McpError>;

    /// Stable label for audit-events. e.g. `"stdio"`, `"unix-socket"`,
    /// `"websocket"`.
    fn label(&self) -> &'static str;
}

// ─── StdioTransport (functional @ stage-0) ────────────────────────────────────

/// Stdio transport — line-delimited JSON over stdin / stdout.
///
/// At stage-0 we use line-delimiting (one JSON object per line) which is
/// trivial to debug + fully sufficient for the skeleton's test-suite.
/// Production framing (`Content-Length: <n>\r\n\r\n<json>`) lands in
/// Jθ-1.1 alongside the tokio-async refactor.
pub struct StdioTransport<R: Read, W: Write> {
    reader: BufReader<R>,
    writer: W,
}

impl<R: Read, W: Write> StdioTransport<R, W> {
    /// Construct a new stdio transport from a [`Read`] + [`Write`] pair.
    /// Tests substitute in-memory pipes ; production uses
    /// [`StdioTransport::from_stdio`] which binds to the process stdin/stdout.
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
        }
    }
}

impl StdioTransport<io::Stdin, io::Stdout> {
    /// Bind to the process stdin / stdout.
    #[must_use]
    pub fn from_stdio() -> Self {
        Self {
            reader: BufReader::new(io::stdin()),
            writer: io::stdout(),
        }
    }
}

impl<R: Read, W: Write> Transport for StdioTransport<R, W> {
    fn read_frame(&mut self) -> Result<Option<String>, McpError> {
        let mut line = String::new();
        let n = self
            .reader
            .read_line(&mut line)
            .map_err(|e| McpError::TransportError(format!("stdio read: {e}")))?;
        if n == 0 {
            return Ok(None);
        }
        // Strip the trailing newline (and CR, if present).
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        Ok(Some(line))
    }

    fn write_frame(&mut self, frame: &str) -> Result<(), McpError> {
        self.writer
            .write_all(frame.as_bytes())
            .and_then(|()| self.writer.write_all(b"\n"))
            .and_then(|()| self.writer.flush())
            .map_err(|e| McpError::TransportError(format!("stdio write: {e}")))
    }

    fn label(&self) -> &'static str {
        "stdio"
    }
}

// ─── UnixSocketTransport (trait-stub) ─────────────────────────────────────────

/// Unix-domain-socket transport. **Not implemented at stage-0.** The trait
/// stub is present so Jθ-2..Jθ-8 can write `Box<dyn Transport>`-typed code
/// that compiles uniformly across platforms ; the real impl lands in
/// Jθ-1.1 once tokio is in the crate.
pub struct UnixSocketTransport {
    /// Audit-friendly path label. The actual socket is bound by the
    /// stage-1 implementation.
    pub path_label: String,
}

impl UnixSocketTransport {
    /// Construct a trait-stub. Always errors at `read_frame` / `write_frame` ;
    /// real impl lands in Jθ-1.1.
    #[must_use]
    pub fn stub(path_label: impl Into<String>) -> Self {
        Self {
            path_label: path_label.into(),
        }
    }
}

impl Transport for UnixSocketTransport {
    fn read_frame(&mut self) -> Result<Option<String>, McpError> {
        Err(McpError::TransportError(
            "UnixSocketTransport stub : implementation deferred to Jθ-1.1".to_string(),
        ))
    }

    fn write_frame(&mut self, _frame: &str) -> Result<(), McpError> {
        Err(McpError::TransportError(
            "UnixSocketTransport stub : implementation deferred to Jθ-1.1".to_string(),
        ))
    }

    fn label(&self) -> &'static str {
        "unix-socket"
    }
}

// ─── WebSocketTransport (trait-stub ; loopback-default discipline encoded) ───

/// Bind-address policy for [`WebSocketTransport`]. Per spec § 5.3 :
/// loopback (127.0.0.1 / ::1) is allowed without [`Cap<RemoteDev>`](crate::cap::RemoteDev) ;
/// any other bind requires the cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsBindPolicy {
    /// Loopback-only (127.0.0.1 / ::1). Default + safest.
    LoopbackOnly,
    /// Non-loopback bind ; ABS-REQUIRES `Cap<RemoteDev>` witness.
    NonLoopback {
        /// Audit-friendly bind label, e.g. `"0.0.0.0:8080"`.
        bind_label: String,
    },
}

/// WebSocket transport. **Not implemented at stage-0.** Trait-only stub.
/// The bind-policy + cap-check is encoded so callers can verify the gate
/// at construction-time even before the full impl lands.
pub struct WebSocketTransport {
    /// Bind policy chosen at construction.
    pub policy: WsBindPolicy,
}

impl WebSocketTransport {
    /// Construct a loopback-only stub.
    #[must_use]
    pub fn loopback_stub() -> Self {
        Self {
            policy: WsBindPolicy::LoopbackOnly,
        }
    }

    /// Construct a non-loopback stub, witnessed by `Cap<RemoteDev>`. The
    /// witness is consumed at construction time per the spec § 5.3
    /// refusal-table.
    #[must_use]
    pub fn non_loopback_with_cap(
        bind_label: impl Into<String>,
        _cap: crate::cap::Cap<crate::cap::RemoteDev>,
    ) -> Self {
        Self {
            policy: WsBindPolicy::NonLoopback {
                bind_label: bind_label.into(),
            },
        }
    }

    /// Convenience : check if the policy is loopback-only.
    #[must_use]
    pub const fn is_loopback_only(&self) -> bool {
        matches!(self.policy, WsBindPolicy::LoopbackOnly)
    }
}

impl Transport for WebSocketTransport {
    fn read_frame(&mut self) -> Result<Option<String>, McpError> {
        Err(McpError::TransportError(
            "WebSocketTransport stub : implementation deferred to Jθ-1.1".to_string(),
        ))
    }

    fn write_frame(&mut self, _frame: &str) -> Result<(), McpError> {
        Err(McpError::TransportError(
            "WebSocketTransport stub : implementation deferred to Jθ-1.1".to_string(),
        ))
    }

    fn label(&self) -> &'static str {
        "websocket"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdio_round_trip_in_memory() {
        let input = b"hello\nworld\n".to_vec();
        let mut output = Vec::<u8>::new();
        {
            let mut t = StdioTransport::new(input.as_slice(), &mut output);
            let line1 = t.read_frame().expect("read1").expect("some1");
            assert_eq!(line1, "hello");
            let line2 = t.read_frame().expect("read2").expect("some2");
            assert_eq!(line2, "world");
            let line3 = t.read_frame().expect("read3");
            assert!(line3.is_none(), "expected EOF");
            t.write_frame(r#"{"foo":1}"#).expect("write");
        }
        assert_eq!(output, b"{\"foo\":1}\n");
    }

    #[test]
    fn stdio_strips_crlf() {
        let input = b"with-cr\r\n".to_vec();
        let mut output = Vec::<u8>::new();
        let mut t = StdioTransport::new(input.as_slice(), &mut output);
        let line = t.read_frame().expect("read").expect("some");
        assert_eq!(line, "with-cr");
    }

    #[test]
    fn stdio_label_is_stdio() {
        let input: &[u8] = b"";
        let mut output = Vec::<u8>::new();
        let t = StdioTransport::new(input, &mut output);
        assert_eq!(t.label(), "stdio");
    }

    #[test]
    fn stdio_eof_returns_none() {
        let input: &[u8] = b"";
        let mut output = Vec::<u8>::new();
        let mut t = StdioTransport::new(input, &mut output);
        let r = t.read_frame().expect("ok");
        assert!(r.is_none());
    }

    #[test]
    fn unix_socket_stub_errors() {
        let mut t = UnixSocketTransport::stub("/tmp/foo.sock");
        assert!(t.read_frame().is_err());
        assert!(t.write_frame("x").is_err());
        assert_eq!(t.label(), "unix-socket");
        assert_eq!(t.path_label, "/tmp/foo.sock");
    }

    #[test]
    fn websocket_stub_loopback_default() {
        let t = WebSocketTransport::loopback_stub();
        assert!(t.is_loopback_only());
        assert_eq!(t.label(), "websocket");
    }

    #[test]
    fn websocket_non_loopback_consumes_cap() {
        let cap = crate::cap::Cap::<crate::cap::RemoteDev>::for_test();
        let t = WebSocketTransport::non_loopback_with_cap("0.0.0.0:8080", cap);
        assert!(!t.is_loopback_only());
        match &t.policy {
            WsBindPolicy::NonLoopback { bind_label } => {
                assert_eq!(bind_label, "0.0.0.0:8080");
            }
            WsBindPolicy::LoopbackOnly => panic!("expected non-loopback policy"),
        }
    }

    #[test]
    fn websocket_stub_read_write_errors() {
        let mut t = WebSocketTransport::loopback_stub();
        assert!(t.read_frame().is_err());
        assert!(t.write_frame("x").is_err());
    }
}
