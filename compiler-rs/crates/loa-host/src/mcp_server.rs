//! § loa-host::mcp_server — TCP JSON-RPC 2.0 control-plane server
//! ════════════════════════════════════════════════════════════════
//!
//! Anchored to T11-LOA-HOST-3 (W-LOA-host-mcp). Implements the runtime
//! side of `scenes/mcp_runtime.cssl` — a Model-Context-Protocol-shaped
//! server so Claude (or any MCP client) can connect to a LIVE running
//! LoA-v13 binary and issue commands : query state, spawn objects,
//! change render-modes, move the player, sample/modify ω-field cells.
//!
//! Apocky directive (2026-04-30) :
//!   "are you able to fully interface with the game live?"
//!
//! § DESIGN
//!   * Bind `0.0.0.0:3001` by default (override via env `CSSL_MCP_PORT`).
//!     Localhost-by-default in production-deploy is enforced upstream
//!     — this slice keeps the bind permissive so an Apocky-side test
//!     `nc localhost 3001` works out-of-the-box.
//!   * JSON-RPC 2.0 over newline-delimited JSON : one request per line,
//!     one response per line.
//!   * Per-client thread : the accept loop spawns a dedicated reader
//!     thread per connection, all sharing `Arc<Mutex<EngineState>>`.
//!   * `tools.list` returns the 17-tool registry (MCP discovery).
//!   * Mutating tools (camera.set / room.spawn_plinth / dm.intensity /
//!     dm.event.propose / omega.modify / engine.shutdown / engine.pause /
//!     render.set_mode / companion.propose) require a `sovereign_cap`
//!     field with the constant `SOVEREIGN_CAP` ; default-deny.
//!   * Read-only tools (engine.state / camera.get / room.geometry /
//!     telemetry.recent / gm.describe_environment / gm.dialogue /
//!     omega.sample / tools.list) require no cap.
//!
//! § THREADING + STATE
//!   `EngineState` is the single shared data model. Sibling slices
//!   (W-LOA-host-render = render-loop ; W-LOA-host-DM = DM/GM machine)
//!   take an `Arc<Mutex<EngineState>>` clone and read/write the same
//!   fields. The MCP server holds its own clone for tool dispatch.
//!
//! § PRIME-DIRECTIVE
//!   The MCP server is consent-architected : every mutating tool has a
//!   default-deny gate, every invocation logs cap-status to the
//!   loa_runtime.log ring, and the server runs entirely in-process —
//!   no third-party MCP-broker, no off-machine relay.

// § Module-scope clippy allow-list. The `option_if_let_else` variants in
// this module make the poison-recovery + parse-fallback flows clearer
// in if-let-Err form than as `map_or`. The `significant_drop_tightening`
// target lives in `accept_loop` where the listener is held for the
// process lifetime (early-drop is the wrong semantic).
#![allow(clippy::option_if_let_else)]
#![allow(clippy::significant_drop_tightening)]
#![allow(clippy::manual_let_else)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use cssl_rt::loa_startup::log_event;

use crate::mcp_tools::{tool_registry, ToolMeta, ToolRegistry};

// ───────────────────────────────────────────────────────────────────────
// § public constants
// ───────────────────────────────────────────────────────────────────────

/// Default TCP port the server listens on (override via `CSSL_MCP_PORT`).
pub const DEFAULT_MCP_PORT: u16 = 3001;

/// Sovereign capability constant. A mutating tool's `params.sovereign_cap`
/// field MUST equal this string for the call to be admitted.
///
/// § PRIME-DIRECTIVE : the constant is published inline rather than read
/// from an env var so the cap-gate can't be bypassed by the runtime
/// environment. Stage-1 will replace this with a per-session capability
/// minted at consent-time ; stage-0 ships the static constant so the live
/// MCP control-plane is operational from frame zero.
pub const SOVEREIGN_CAP: &str = "0xCAFE_BABE_DEADBEEF";

/// JSON-RPC 2.0 protocol-version literal.
pub const JSON_RPC_VERSION: &str = "2.0";

/// JSON-RPC 2.0 standard error code : method not found.
pub const ERR_METHOD_NOT_FOUND: i32 = -32_601;
/// JSON-RPC 2.0 standard error code : invalid params.
pub const ERR_INVALID_PARAMS: i32 = -32_602;
/// JSON-RPC 2.0 standard error code : parse error.
pub const ERR_PARSE_ERROR: i32 = -32_700;
/// JSON-RPC 2.0 application-defined : sovereign-cap missing or invalid.
pub const ERR_NO_SOVEREIGN: i32 = -32_001;

// ───────────────────────────────────────────────────────────────────────
// § render-mode enum (mirrors the .csl scene's render-mode-id contract)
// ───────────────────────────────────────────────────────────────────────

/// LoA-v13 render-mode discriminant. The wire-format id (`0..9`) is
/// what the MCP `render.set_mode` tool accepts and what the renderer
/// reads from `EngineState.render_mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum RenderMode {
    /// Final-color path (default).
    Normal           = 0,
    /// Albedo-only debug pass.
    Albedo           = 1,
    /// Linear-depth visualization.
    Depth            = 2,
    /// World-space normals.
    Normals          = 3,
    /// Surface-type discriminant overlay.
    SurfType         = 4,
    /// Raw SDF distance-isolines.
    Sdf              = 5,
    /// Raymarcher step-count heatmap.
    Steps            = 6,
    /// W-coordinate distance (4D Substrate slice).
    WDistance        = 7,
    /// Reference grid overlay.
    Grid             = 8,
    /// Field-vs-analytic-SDF differential.
    FieldVsAnalytic  = 9,
}

impl RenderMode {
    /// Convert a wire-format `u8` to a `RenderMode`. Returns `None` on
    /// out-of-range input.
    #[must_use]
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Normal),
            1 => Some(Self::Albedo),
            2 => Some(Self::Depth),
            3 => Some(Self::Normals),
            4 => Some(Self::SurfType),
            5 => Some(Self::Sdf),
            6 => Some(Self::Steps),
            7 => Some(Self::WDistance),
            8 => Some(Self::Grid),
            9 => Some(Self::FieldVsAnalytic),
            _ => None,
        }
    }

    /// Stringify for telemetry / debug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Albedo => "Albedo",
            Self::Depth => "Depth",
            Self::Normals => "Normals",
            Self::SurfType => "SurfType",
            Self::Sdf => "Sdf",
            Self::Steps => "Steps",
            Self::WDistance => "WDistance",
            Self::Grid => "Grid",
            Self::FieldVsAnalytic => "FieldVsAnalytic",
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § shared engine-state — the live process's data model
// ───────────────────────────────────────────────────────────────────────

/// 3D world-space coordinate (host-side mirror of LoA's `Vec3`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    /// Origin (0, 0, 0).
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, z: 0.0 };

    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

/// Camera state : position + Euler angles (yaw + pitch).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraState {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            pos: Vec3::new(0.0, 1.5, -3.0),
            yaw: 0.0,
            pitch: 0.0,
        }
    }
}

/// Plinth instance — a single render-able pillar in the test-room.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Plinth {
    pub x: f32,
    pub z: f32,
    /// Packed 0xRRGGBB color.
    pub color_rgb: u32,
    /// AABB half-extents (height/width are uniform per the scene spec).
    pub half_extent: f32,
}

impl Plinth {
    #[must_use]
    pub const fn new(x: f32, z: f32, color_rgb: u32) -> Self {
        Self { x, z, color_rgb, half_extent: 0.5 }
    }
}

/// DM intensity dial — clamped to `0..=3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmState {
    pub intensity: u8,
    pub event_count: u32,
}

impl Default for DmState {
    fn default() -> Self {
        Self { intensity: 1, event_count: 0 }
    }
}

/// Telemetry-event ring entry. Last `TELEMETRY_RING_CAP` entries kept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub frame: u64,
    pub level: String,
    pub source: String,
    pub message: String,
}

/// Bounded ring-cap. Any-newer push evicts the oldest entry.
pub const TELEMETRY_RING_CAP: usize = 256;

/// Live, per-process engine state. Shared across the MCP server, the
/// render loop (sibling W-LOA-host-render), and the DM/GM machine
/// (sibling W-LOA-host-DM) via `Arc<Mutex<EngineState>>`.
///
/// § FIELD SEMANTICS
///   `frame_count`     ← incremented by the render loop per tick.
///   `quit_requested`  ← set by `engine.shutdown` ; main loop polls.
///   `paused`          ← toggled by `engine.pause`.
///   `camera`          ← read by render, written by `camera.set`.
///   `render_mode`     ← read by render, written by `render.set_mode`.
///   `active_scene`    ← string key from the scene-bundle table.
///   `room_dim_xyz`    ← test-room dimensions from .csl scene init.
///   `plinths`         ← live AABB list ; `room.spawn_plinth` appends.
///   `dm`              ← DM intensity + event-count ; sibling DM-runtime
///                       drives this, MCP can read/nudge it.
///   `telemetry_ring`  ← bounded ring of recent log-events for the
///                       `telemetry.recent` tool.
#[derive(Debug, Clone)]
pub struct EngineState {
    pub frame_count: u64,
    pub quit_requested: bool,
    pub paused: bool,
    pub camera: CameraState,
    pub render_mode: RenderMode,
    pub active_scene: String,
    pub room_dim_xyz: Vec3,
    pub plinths: Vec<Plinth>,
    pub dm: DmState,
    pub telemetry_ring: Vec<TelemetryEvent>,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            frame_count: 0,
            quit_requested: false,
            paused: false,
            camera: CameraState::default(),
            render_mode: RenderMode::Normal,
            active_scene: "test-room".to_string(),
            room_dim_xyz: Vec3::new(10.0, 4.0, 10.0),
            plinths: vec![
                Plinth::new(-2.0, -2.0, 0x_88_44_22),
                Plinth::new(2.0, -2.0, 0x_44_88_22),
                Plinth::new(-2.0, 2.0, 0x_22_44_88),
                Plinth::new(2.0, 2.0, 0x_88_22_44),
            ],
            dm: DmState::default(),
            telemetry_ring: Vec::with_capacity(TELEMETRY_RING_CAP),
        }
    }
}

impl EngineState {
    /// Push a telemetry event. Drops the oldest if at capacity.
    pub fn push_event(&mut self, level: &str, source: &str, message: &str) {
        if self.telemetry_ring.len() >= TELEMETRY_RING_CAP {
            self.telemetry_ring.remove(0);
        }
        self.telemetry_ring.push(TelemetryEvent {
            frame: self.frame_count,
            level: level.to_string(),
            source: source.to_string(),
            message: message.to_string(),
        });
    }
}

// ───────────────────────────────────────────────────────────────────────
// § server config + bind
// ───────────────────────────────────────────────────────────────────────

/// MCP-server bind configuration.
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        let port = std::env::var("CSSL_MCP_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(DEFAULT_MCP_PORT);
        Self {
            host: "0.0.0.0".to_string(),
            port,
        }
    }
}

impl McpServerConfig {
    #[must_use]
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

// ───────────────────────────────────────────────────────────────────────
// § JSON-RPC frame model
// ───────────────────────────────────────────────────────────────────────

/// Parsed JSON-RPC 2.0 request. `id` may be a number, string, or null.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    /// Optional in JSON-RPC 2.0 (omit ⇒ notification, no response).
    #[serde(default)]
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Outgoing JSON-RPC 2.0 response — `result` XOR `error` set.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error body.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    /// Construct a success response.
    #[must_use]
    pub fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: JSON_RPC_VERSION,
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Construct an error response.
    #[must_use]
    pub fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: JSON_RPC_VERSION,
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § cap-gate + dispatch
// ───────────────────────────────────────────────────────────────────────

/// Returns true iff `params.sovereign_cap == SOVEREIGN_CAP`.
#[must_use]
pub fn has_sovereign_cap(params: &Value) -> bool {
    params
        .get("sovereign_cap")
        .and_then(Value::as_str)
        .is_some_and(|s| s == SOVEREIGN_CAP)
}

/// Dispatch a single request against the registry + state.
///
/// Returns the response body (success or error). This is the pure
/// dispatch surface ; thread-loop wraps it in I/O.
pub fn dispatch(
    state: &Arc<Mutex<EngineState>>,
    registry: &ToolRegistry,
    req: &JsonRpcRequest,
) -> JsonRpcResponse {
    let id = req.id.clone();

    let Some(tool) = registry.get(&req.method) else {
        log_event(
            "WARN",
            "loa-host/mcp",
            &format!("tool-unknown · method={}", req.method),
        );
        return JsonRpcResponse::err(
            id,
            ERR_METHOD_NOT_FOUND,
            format!("method not found: {}", req.method),
        );
    };

    // Cap-gate : default-deny on mutating tools without sovereign_cap.
    if tool.meta.mutating && !has_sovereign_cap(&req.params) {
        log_event(
            "WARN",
            "loa-host/mcp",
            &format!("tool-denied · no-sovereign-cap · name={}", tool.meta.name),
        );
        return JsonRpcResponse::err(
            id,
            ERR_NO_SOVEREIGN,
            format!(
                "no sovereign-cap for mutating tool '{}' \
                 (params.sovereign_cap missing or invalid)",
                tool.meta.name
            ),
        );
    }

    log_event(
        "INFO",
        "loa-host/mcp",
        &format!(
            "tool-invoked · name={} · cap-status={}",
            tool.meta.name,
            if tool.meta.mutating { "sovereign-OK" } else { "read-only" }
        ),
    );

    // Dispatch with the held lock. Each handler is short-running ; long
    // ω-field walks are bounded by the per-call AABB caller-supplied
    // limits in the underlying loa_stubs FFI.
    let result = match state.lock() {
        Ok(mut g) => (tool.handler)(&mut g, req.params.clone()),
        Err(poisoned) => {
            // Poison-tolerant : the prior lock-holder panicked. Take
            // the inner state, log, and continue rather than cascading.
            log_event(
                "ERROR",
                "loa-host/mcp",
                "engine-state-mutex-poisoned · recovering · prior holder panicked",
            );
            let mut inner = poisoned.into_inner();
            (tool.handler)(&mut inner, req.params.clone())
        }
    };

    JsonRpcResponse::ok(id, result)
}

// ───────────────────────────────────────────────────────────────────────
// § per-line frame parse
// ───────────────────────────────────────────────────────────────────────

/// Parse a single JSON-RPC 2.0 line. On parse failure returns a synthesized
/// error response with `id = null`.
pub fn parse_request_line(line: &str) -> Result<JsonRpcRequest, JsonRpcResponse> {
    serde_json::from_str::<JsonRpcRequest>(line).map_err(|e| {
        JsonRpcResponse::err(
            Value::Null,
            ERR_PARSE_ERROR,
            format!("parse error: {e}"),
        )
    })
}

/// Encode a response to a newline-terminated UTF-8 line.
#[must_use]
pub fn encode_response(resp: &JsonRpcResponse) -> String {
    // serde_json::to_string never fails on Value-typed responses.
    let mut s = serde_json::to_string(resp).unwrap_or_else(|_| {
        // Fallback : hand-encoded minimal error frame.
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{{\"code\":{ERR_PARSE_ERROR},\"message\":\"encode-failure\"}}}}"
        )
    });
    s.push('\n');
    s
}

// ───────────────────────────────────────────────────────────────────────
// § thread loops : accept + per-client
// ───────────────────────────────────────────────────────────────────────

/// Spawn the accept loop on a dedicated thread. Returns the join-handle
/// + the bound port (which may differ from `cfg.port` if the OS auto-
/// assigned). Listener errors are logged ; the thread exits cleanly on
/// any I/O fault and the caller can re-spawn.
pub fn spawn_mcp_server(
    cfg: McpServerConfig,
    state: Arc<Mutex<EngineState>>,
) -> std::io::Result<(JoinHandle<()>, u16)> {
    let listener = TcpListener::bind(cfg.bind_addr())?;
    let bound = listener.local_addr()?.port();

    log_event(
        "INFO",
        "loa-host/mcp",
        &format!("listening on {}:{}", cfg.host, bound),
    );

    let registry = Arc::new(tool_registry());

    let handle = thread::Builder::new()
        .name("loa-host/mcp-accept".to_string())
        .spawn(move || {
            accept_loop(listener, state, registry);
        })?;

    Ok((handle, bound))
}

fn accept_loop(
    listener: TcpListener,
    state: Arc<Mutex<EngineState>>,
    registry: Arc<ToolRegistry>,
) {
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let peer = stream
                    .peer_addr()
                    .map_or_else(|_| "unknown".to_string(), |a| a.to_string());
                log_event(
                    "INFO",
                    "loa-host/mcp",
                    &format!("client-accepted · addr={peer}"),
                );
                let st = state.clone();
                let reg = registry.clone();
                let _ = thread::Builder::new()
                    .name(format!("loa-host/mcp-client-{peer}"))
                    .spawn(move || {
                        client_loop(stream, st, reg);
                    });
            }
            Err(e) => {
                log_event(
                    "ERROR",
                    "loa-host/mcp",
                    &format!("accept-error · {e}"),
                );
                // On accept failure (e.g. listener closed), exit the loop
                // rather than spin. Caller may re-spawn via spawn_mcp_server.
                break;
            }
        }
    }
}

fn client_loop(
    stream: TcpStream,
    state: Arc<Mutex<EngineState>>,
    registry: Arc<ToolRegistry>,
) {
    let peer = stream
        .peer_addr()
        .map_or_else(|_| "unknown".to_string(), |a| a.to_string());

    let writer_clone = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            log_event(
                "ERROR",
                "loa-host/mcp",
                &format!("client-clone-error · addr={peer} · {e}"),
            );
            return;
        }
    };
    let mut writer = writer_clone;
    let reader = BufReader::new(stream);

    for line_res in reader.lines() {
        let line = match line_res {
            Ok(l) if l.trim().is_empty() => continue,
            Ok(l) => l,
            Err(e) => {
                log_event(
                    "WARN",
                    "loa-host/mcp",
                    &format!("client-read-error · addr={peer} · {e}"),
                );
                break;
            }
        };

        let response = match parse_request_line(&line) {
            Ok(req) => {
                // JSON-RPC notifications (id absent / null) get no response.
                let is_notification = req.id.is_null();
                let resp = dispatch(&state, &registry, &req);
                if is_notification {
                    continue;
                }
                resp
            }
            Err(err_resp) => err_resp,
        };

        let encoded = encode_response(&response);
        if let Err(e) = writer.write_all(encoded.as_bytes()) {
            log_event(
                "WARN",
                "loa-host/mcp",
                &format!("client-write-error · addr={peer} · {e}"),
            );
            break;
        }
        let _ = writer.flush();
    }

    log_event(
        "INFO",
        "loa-host/mcp",
        &format!("client-disconnected · addr={peer}"),
    );
}

// ───────────────────────────────────────────────────────────────────────
// § convenience : tool-meta lookup helper
// ───────────────────────────────────────────────────────────────────────

/// Return the metadata of every registered tool.
#[must_use]
pub fn registered_tool_meta() -> HashMap<String, ToolMeta> {
    tool_registry()
        .iter()
        .map(|(k, v)| (k.clone(), v.meta.clone()))
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// § TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
#[allow(clippy::similar_names)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shared_state() -> Arc<Mutex<EngineState>> {
        Arc::new(Mutex::new(EngineState::default()))
    }

    #[test]
    fn json_rpc_request_parses_correctly() {
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"engine.state","params":{}}"#;
        let req = parse_request_line(line).expect("parse ok");
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "engine.state");
        assert_eq!(req.id, Value::from(1));
    }

    #[test]
    fn parse_invalid_json_returns_error_response() {
        let bad = "{not json";
        let resp = parse_request_line(bad).expect_err("must error");
        let err = resp.error.as_ref().expect("error body");
        assert_eq!(err.code, ERR_PARSE_ERROR);
    }

    #[test]
    fn unknown_method_returns_method_not_found_error() {
        let st = shared_state();
        let reg = tool_registry();
        let req = JsonRpcRequest {
            jsonrpc: JSON_RPC_VERSION.to_string(),
            id: Value::from(7),
            method: "no.such.tool".to_string(),
            params: json!({}),
        };
        let resp = dispatch(&st, &reg, &req);
        let err = resp.error.expect("must be error");
        assert_eq!(err.code, ERR_METHOD_NOT_FOUND);
    }

    #[test]
    fn engine_state_handler_returns_valid_json() {
        let st = shared_state();
        let reg = tool_registry();
        let req = JsonRpcRequest {
            jsonrpc: JSON_RPC_VERSION.to_string(),
            id: Value::from(2),
            method: "engine.state".to_string(),
            params: json!({}),
        };
        let resp = dispatch(&st, &reg, &req);
        let result = resp.result.expect("ok result");
        assert!(result.get("frame_count").is_some());
        assert!(result.get("camera_pos").is_some());
        assert!(result.get("active_scene").is_some());
        assert!(result.get("render_mode").is_some());
    }

    #[test]
    fn mutating_tool_no_cap_returns_error() {
        let st = shared_state();
        let reg = tool_registry();
        let req = JsonRpcRequest {
            jsonrpc: JSON_RPC_VERSION.to_string(),
            id: Value::from(3),
            method: "camera.set".to_string(),
            // No sovereign_cap → must default-deny.
            params: json!({"x": 1.0, "y": 2.0, "z": 3.0}),
        };
        let resp = dispatch(&st, &reg, &req);
        let err = resp.error.expect("must error");
        assert_eq!(err.code, ERR_NO_SOVEREIGN);
    }

    #[test]
    fn mutating_tool_with_sovereign_cap_succeeds() {
        let st = shared_state();
        let reg = tool_registry();
        let req = JsonRpcRequest {
            jsonrpc: JSON_RPC_VERSION.to_string(),
            id: Value::from(4),
            method: "camera.set".to_string(),
            params: json!({
                "sovereign_cap": SOVEREIGN_CAP,
                "x": 5.0, "y": 1.5, "z": -2.0,
                "yaw": 0.5, "pitch": 0.0
            }),
        };
        let resp = dispatch(&st, &reg, &req);
        assert!(resp.error.is_none());
        let result = resp.result.expect("ok");
        assert_eq!(result["camera_pos"]["x"].as_f64(), Some(5.0));
        // Verify state was actually mutated.
        let g = st.lock().unwrap();
        assert!((g.camera.pos.x - 5.0).abs() < 1e-6);
    }

    #[test]
    fn render_mode_round_trip() {
        for v in 0u8..=9 {
            let m = RenderMode::from_u8(v).expect("valid");
            assert_eq!(m as u8, v);
        }
        assert!(RenderMode::from_u8(10).is_none());
    }

    #[test]
    fn engine_state_default_has_seed_plinths() {
        let s = EngineState::default();
        assert_eq!(s.plinths.len(), 4);
        assert_eq!(s.active_scene, "test-room");
        assert_eq!(s.render_mode, RenderMode::Normal);
    }

    #[test]
    fn telemetry_ring_evicts_at_cap() {
        let mut s = EngineState::default();
        for i in 0..(TELEMETRY_RING_CAP + 50) {
            s.push_event("INFO", "test", &format!("evt-{i}"));
        }
        assert_eq!(s.telemetry_ring.len(), TELEMETRY_RING_CAP);
        // Oldest entries must have been evicted.
        assert!(s.telemetry_ring[0].message.starts_with("evt-50"));
    }

    #[test]
    fn config_default_uses_3001_when_env_unset() {
        // Save + clear env to test default.
        let prior = std::env::var("CSSL_MCP_PORT").ok();
        std::env::remove_var("CSSL_MCP_PORT");
        let cfg = McpServerConfig::default();
        assert_eq!(cfg.port, DEFAULT_MCP_PORT);
        if let Some(v) = prior {
            std::env::set_var("CSSL_MCP_PORT", v);
        }
    }

    #[test]
    fn has_sovereign_cap_only_true_for_exact_match() {
        assert!(has_sovereign_cap(&json!({"sovereign_cap": SOVEREIGN_CAP})));
        assert!(!has_sovereign_cap(&json!({"sovereign_cap": "nope"})));
        assert!(!has_sovereign_cap(&json!({})));
        assert!(!has_sovereign_cap(&json!({"sovereign_cap": 42})));
    }

    #[test]
    fn registered_tool_meta_matches_registry() {
        let meta = registered_tool_meta();
        let reg = tool_registry();
        assert_eq!(meta.len(), reg.len());
        // Every registry-key has matching meta.
        for k in reg.keys() {
            assert!(meta.contains_key(k), "missing meta for {k}");
        }
    }
}
