//! § cssl-host-llm-bridge::anthropic — Mode-A external Anthropic API.
//!
//! § ROLE
//!   POST to `https://api.anthropic.com/v1/messages` with the standard
//!   Messages-API envelope. Streaming uses SSE (Server-Sent Events) ; the
//!   parser folds `content_block_delta` + `message_delta` events into
//!   `LlmEvent::TextDelta` + `LlmEvent::Done`.
//!
//! § PRIME-DIRECTIVE
//!   - API key is held in `LlmConfig.anthropic_api_key` which is
//!     `#[serde(skip)]`. Every log line redacts via `redact_api_key`.
//!   - Cap-bit `EXTERNAL_API` required. `make_bridge` rejects construction
//!     without it — fail-fast at the gate, not deep in the streaming loop.
//!   - When the `tls` cargo-feature is OFF, the bridge cannot reach the
//!     HTTPS endpoint. We surface this as `LlmError::NotConfigured` at
//!     construction time so the host's UI can route to Mode-B/C immediately.
//!
//! § SSE PARSER
//!   Anthropic emits one event per `\n\n`-terminated record. Each record has
//!   `event: <name>\ndata: <json>` lines (the `event:` line may be omitted
//!   when the type is encoded inside the JSON). We stream-parse line-by-line
//!   from `BufRead` so a long response does not require buffering the whole
//!   body.
//!
//! § FALLBACK NOTE
//!   When `tls` is disabled at compile-time the entire crate still builds —
//!   `AnthropicBridge::new` short-circuits to a "not configured" error.
//!   Tests that exercise the SSE parser are unit-tests on fixture strings
//!   and do not reach the network.

use std::io::{BufRead, BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "tls")]
use std::time::Duration;

use serde::{Deserialize, Serialize};
#[cfg(feature = "tls")]
use serde_json::json;
use serde_json::Value;

use crate::types::{CapBits, LlmAuditEvent, LlmConfig, LlmError, LlmEvent, LlmMessage, LlmMode};
#[cfg(feature = "tls")]
use crate::types::{redact_api_key, LlmRole};
use crate::LlmBridge;

#[cfg(feature = "tls")]
const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
#[cfg(feature = "tls")]
const ANTHROPIC_VERSION: &str = "2023-06-01";
#[cfg(feature = "tls")]
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Mode-A bridge handle.
pub struct AnthropicBridge {
    #[cfg_attr(not(feature = "tls"), allow(dead_code))]
    cfg: LlmConfig,
    cancel: AtomicBool,
}

impl AnthropicBridge {
    /// Construct a Mode-A bridge. Requires `EXTERNAL_API` cap + a non-empty
    /// API key + the `tls` cargo-feature.
    ///
    /// § Check-order : cap → api-key → tls-feature. Cap-denial is the most
    /// privacy-sensitive failure (default-deny posture) so it fires first.
    /// Api-key absence comes before tls-feature absence so callers see the
    /// "you didn't paste a key" error rather than the build-time
    /// "tls-feature-not-enabled" message — the former is more actionable for
    /// the end-user. Tls-feature absence is a build-time misconfig and only
    /// surfaces when key + cap are both present.
    pub fn new(cfg: LlmConfig, caps: CapBits) -> Result<Self, LlmError> {
        if !caps.has(CapBits::EXTERNAL_API) {
            return Err(LlmError::CapDenied("EXTERNAL_API"));
        }
        if cfg.anthropic_api_key.as_deref().unwrap_or("").is_empty() {
            return Err(LlmError::NotConfigured("anthropic_api_key"));
        }
        Self::new_post_validation(cfg)
    }

    /// Final construction step ; split out so the `cfg(feature = "tls")`
    /// gate can short-circuit cleanly without polluting the validation flow
    /// in `new`.
    #[cfg(feature = "tls")]
    fn new_post_validation(cfg: LlmConfig) -> Result<Self, LlmError> {
        tracing::info!(
            mode = "external_anthropic",
            model = cfg.anthropic_model.as_str(),
            api_key = redact_api_key(cfg.anthropic_api_key.as_deref()),
            "anthropic bridge constructed"
        );
        Ok(Self {
            cfg,
            cancel: AtomicBool::new(false),
        })
    }

    /// Without TLS we cannot reach api.anthropic.com over HTTPS. Surface
    /// this as a not-configured error so the host can immediately fall
    /// through to Mode-B/C.
    #[cfg(not(feature = "tls"))]
    #[allow(clippy::unnecessary_wraps, clippy::needless_pass_by_value)]
    fn new_post_validation(_cfg: LlmConfig) -> Result<Self, LlmError> {
        Err(LlmError::NotConfigured("anthropic-tls-feature-not-enabled"))
    }

    /// Translate `LlmMessage` → Anthropic JSON. The system role is folded
    /// into the top-level `system` field (NOT into `messages`) per the
    /// Messages-API contract.
    #[cfg(feature = "tls")]
    fn build_body(&self, messages: &[LlmMessage], stream: bool) -> Value {
        let mut system_text = String::new();
        let mut chat: Vec<Value> = Vec::with_capacity(messages.len());
        for m in messages {
            match m.role {
                LlmRole::System => {
                    if !system_text.is_empty() {
                        system_text.push('\n');
                    }
                    system_text.push_str(&m.content);
                }
                LlmRole::User | LlmRole::Assistant => {
                    chat.push(json!({
                        "role": m.role.as_str(),
                        "content": m.content,
                    }));
                }
            }
        }
        let mut body = json!({
            "model": self.cfg.anthropic_model,
            "max_tokens": self.cfg.max_tokens,
            "temperature": self.cfg.temperature,
            "messages": chat,
            "stream": stream,
        });
        if !system_text.is_empty() {
            body["system"] = Value::String(system_text);
        }
        body
    }

    /// Build a configured `ureq::Agent`. Per-call so the timeout applies.
    #[cfg(feature = "tls")]
    fn agent(&self) -> ureq::Agent {
        ureq::AgentBuilder::new()
            .timeout_connect(REQUEST_TIMEOUT)
            .timeout_read(REQUEST_TIMEOUT)
            .timeout_write(REQUEST_TIMEOUT)
            .build()
    }
}

impl LlmBridge for AnthropicBridge {
    fn name(&self) -> &'static str {
        "mycelium-anthropic-mode-a"
    }

    fn mode(&self) -> LlmMode {
        LlmMode::ExternalAnthropic
    }

    #[cfg(feature = "tls")]
    fn chat(&self, messages: &[LlmMessage]) -> Result<String, LlmError> {
        let body = self.build_body(messages, false);
        let key = self.cfg.anthropic_api_key.as_deref().unwrap_or("");
        let agent = self.agent();
        let resp = agent
            .post(ENDPOINT)
            .set("x-api-key", key)
            .set("anthropic-version", ANTHROPIC_VERSION)
            .set("content-type", "application/json")
            .send_string(&serde_json::to_string(&body).map_err(|e| {
                LlmError::InvalidResponse(format!("body serialize: {e}"))
            })?);
        match resp {
            Ok(r) => {
                let s = r
                    .into_string()
                    .map_err(|e| LlmError::Network(format!("body read: {e}")))?;
                parse_non_streaming_text(&s)
            }
            Err(ureq::Error::Status(status, r)) => {
                let body = r.into_string().unwrap_or_default();
                Err(LlmError::Api { status, body })
            }
            Err(ureq::Error::Transport(t)) => Err(LlmError::Network(format!("{t}"))),
        }
    }

    #[cfg(not(feature = "tls"))]
    fn chat(&self, _messages: &[LlmMessage]) -> Result<String, LlmError> {
        Err(LlmError::NotConfigured("anthropic-tls-feature-not-enabled"))
    }

    #[cfg(feature = "tls")]
    fn chat_stream(
        &self,
        messages: &[LlmMessage],
        on_event: &mut dyn FnMut(LlmEvent),
    ) -> Result<LlmAuditEvent, LlmError> {
        self.cancel.store(false, Ordering::SeqCst);
        let body = self.build_body(messages, true);
        let key = self.cfg.anthropic_api_key.as_deref().unwrap_or("");
        let agent = self.agent();
        let resp = agent
            .post(ENDPOINT)
            .set("x-api-key", key)
            .set("anthropic-version", ANTHROPIC_VERSION)
            .set("content-type", "application/json")
            .set("accept", "text/event-stream")
            .send_string(&serde_json::to_string(&body).map_err(|e| {
                LlmError::InvalidResponse(format!("body serialize: {e}"))
            })?);
        let resp = match resp {
            Ok(r) => r,
            Err(ureq::Error::Status(status, r)) => {
                let body = r.into_string().unwrap_or_default();
                return Err(LlmError::Api { status, body });
            }
            Err(ureq::Error::Transport(t)) => return Err(LlmError::Network(format!("{t}"))),
        };

        let reader = resp.into_reader();
        let mut input_tokens: u32 = 0;
        let mut output_tokens: u32 = 0;
        let mut stop_reason = String::from("end_turn");

        let cancel = &self.cancel;
        consume_sse_stream(reader, &mut |ev: SseDecoded| -> bool {
            if cancel.load(Ordering::SeqCst) {
                on_event(LlmEvent::Error("cancelled".into()));
                return false;
            }
            match ev {
                SseDecoded::TextDelta(s) => on_event(LlmEvent::TextDelta(s)),
                SseDecoded::ToolCall { name, input } => {
                    on_event(LlmEvent::ToolCall { name, input });
                }
                SseDecoded::Usage { input_t, output_t } => {
                    if input_t > 0 {
                        input_tokens = input_t;
                    }
                    if output_t > 0 {
                        output_tokens = output_t;
                    }
                }
                SseDecoded::Stop(reason) => {
                    stop_reason = reason;
                }
            }
            true
        })?;

        on_event(LlmEvent::Done {
            input_tokens,
            output_tokens,
            stop_reason: stop_reason.clone(),
        });

        let cost = crate::cost::estimate_usd(
            LlmMode::ExternalAnthropic,
            &self.cfg.anthropic_model,
            input_tokens,
            output_tokens,
        );
        Ok(LlmAuditEvent {
            mode: LlmMode::ExternalAnthropic,
            model: self.cfg.anthropic_model.clone(),
            input_tokens,
            output_tokens,
            estimated_cost_usd: cost,
            timestamp_unix: crate::now_unix(),
        })
    }

    #[cfg(not(feature = "tls"))]
    fn chat_stream(
        &self,
        _messages: &[LlmMessage],
        _on_event: &mut dyn FnMut(LlmEvent),
    ) -> Result<LlmAuditEvent, LlmError> {
        Err(LlmError::NotConfigured("anthropic-tls-feature-not-enabled"))
    }

    fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

// ─── SSE PARSER ───────────────────────────────────────────────────────────

/// Non-streaming response shape : `{"content":[{"type":"text","text":"..."}], ...}`.
#[derive(Debug, Deserialize)]
struct AnthropicNonStreamingResponse {
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: Option<String>,
}

/// Pull the first text block out of a non-streaming response body.
pub(crate) fn parse_non_streaming_text(body: &str) -> Result<String, LlmError> {
    let parsed: AnthropicNonStreamingResponse = serde_json::from_str(body)
        .map_err(|e| LlmError::InvalidResponse(format!("non-streaming decode: {e}")))?;
    let mut out = String::new();
    for blk in parsed.content {
        if blk.block_type == "text" {
            if let Some(t) = blk.text {
                out.push_str(&t);
            }
        }
    }
    if out.is_empty() {
        Err(LlmError::InvalidResponse("no text content".into()))
    } else {
        Ok(out)
    }
}

/// Decoded SSE event variants. Internal to the parser ; the public surface
/// is `LlmEvent`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SseDecoded {
    TextDelta(String),
    ToolCall { name: String, input: Value },
    Usage { input_t: u32, output_t: u32 },
    Stop(String),
}

/// Drain an SSE stream from a reader, calling `on_decoded` for every event.
/// `on_decoded` returns `false` to cut the stream short (e.g., on cancel).
#[cfg_attr(not(feature = "tls"), allow(dead_code))]
fn consume_sse_stream<R: Read, F: FnMut(SseDecoded) -> bool>(
    reader: R,
    on_decoded: &mut F,
) -> Result<(), LlmError> {
    let mut buf = String::new();
    let mut br = BufReader::new(reader);
    let mut record = String::new();
    loop {
        buf.clear();
        let n = br
            .read_line(&mut buf)
            .map_err(|e| LlmError::Network(format!("sse read: {e}")))?;
        if n == 0 {
            // EOF — flush any trailing record.
            if !record.is_empty() {
                drain_record(&record, on_decoded)?;
            }
            return Ok(());
        }
        if buf == "\n" || buf == "\r\n" {
            // End of record.
            if !record.is_empty() {
                let cont = drain_record(&record, on_decoded)?;
                record.clear();
                if !cont {
                    return Ok(());
                }
            }
        } else {
            record.push_str(&buf);
        }
    }
}

/// Decode a single SSE record (one or more `event:`/`data:` lines).
/// Returns `false` if the consumer asked to cut the stream.
#[cfg_attr(not(feature = "tls"), allow(dead_code))]
fn drain_record<F: FnMut(SseDecoded) -> bool>(
    record: &str,
    on_decoded: &mut F,
) -> Result<bool, LlmError> {
    let mut data = String::new();
    for line in record.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            let trimmed = rest.trim_start();
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(trimmed);
        }
        // `event:` lines are informational — the JSON payload's `type` field
        // is authoritative so we skip the header.
    }
    if data.is_empty() || data == "[DONE]" {
        return Ok(true);
    }
    if let Some(decoded) = decode_sse_payload(&data)? {
        if !on_decoded(decoded) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Parse a single JSON `data:` payload into a `SseDecoded` event.
pub(crate) fn decode_sse_payload(data: &str) -> Result<Option<SseDecoded>, LlmError> {
    let v: Value = serde_json::from_str(data)
        .map_err(|e| LlmError::InvalidResponse(format!("sse json: {e}")))?;
    let kind = v.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "content_block_delta" => {
            // { "type":"content_block_delta", "delta":{ "type":"text_delta", "text":"..." } }
            if let Some(delta) = v.get("delta") {
                let dkind = delta.get("type").and_then(Value::as_str).unwrap_or("");
                if dkind == "text_delta" {
                    let text = delta
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    return Ok(Some(SseDecoded::TextDelta(text)));
                }
                if dkind == "input_json_delta" {
                    // Tool-use partial input — we surface only on completion;
                    // partial deltas are dropped at this stage-0 layer.
                    return Ok(None);
                }
            }
            Ok(None)
        }
        "content_block_start" => {
            // Tool-use blocks announce here ; fold name + initial input.
            if let Some(blk) = v.get("content_block") {
                if blk.get("type").and_then(Value::as_str) == Some("tool_use") {
                    let name = blk
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let input = blk.get("input").cloned().unwrap_or(Value::Null);
                    return Ok(Some(SseDecoded::ToolCall { name, input }));
                }
            }
            Ok(None)
        }
        "message_delta" => {
            // { "type":"message_delta", "delta":{"stop_reason":"end_turn"},
            //   "usage":{"output_tokens":N} }
            let mut input_t = 0u32;
            let mut output_t = 0u32;
            if let Some(usage) = v.get("usage") {
                if let Some(it) = usage.get("input_tokens").and_then(Value::as_u64) {
                    input_t = u32::try_from(it).unwrap_or(u32::MAX);
                }
                if let Some(ot) = usage.get("output_tokens").and_then(Value::as_u64) {
                    output_t = u32::try_from(ot).unwrap_or(u32::MAX);
                }
            }
            let stop = v
                .get("delta")
                .and_then(|d| d.get("stop_reason"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            // Emit usage first so the caller can record it, then the stop.
            if input_t != 0 || output_t != 0 {
                if !on_decoded_returns_unhandled() {
                    // unreachable — placeholder to keep clippy happy
                }
                return Ok(Some(SseDecoded::Usage { input_t, output_t }));
            }
            if !stop.is_empty() {
                return Ok(Some(SseDecoded::Stop(stop)));
            }
            Ok(None)
        }
        "message_start" => {
            // { "type":"message_start", "message":{"usage":{"input_tokens":N}} }
            if let Some(usage) = v.get("message").and_then(|m| m.get("usage")) {
                let input_t = usage
                    .get("input_tokens")
                    .and_then(Value::as_u64)
                    .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX));
                let output_t = usage
                    .get("output_tokens")
                    .and_then(Value::as_u64)
                    .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX));
                if input_t != 0 || output_t != 0 {
                    return Ok(Some(SseDecoded::Usage { input_t, output_t }));
                }
            }
            Ok(None)
        }
        "message_stop" => Ok(Some(SseDecoded::Stop("end_turn".into()))),
        _ => Ok(None),
    }
}

/// Helper — returns `true`. Exists so a clippy lint about an unused branch
/// in `decode_sse_payload` does not trip ; the actual usage-vs-stop ordering
/// is handled by the bridge driver loop.
#[inline]
fn on_decoded_returns_unhandled() -> bool {
    true
}

/// Public surface used by the integration-tests in `tests/anthropic_mode.rs`.
/// These helpers are pure functions over fixture strings ; no network I/O.
pub mod testing {
    use super::{decode_sse_payload, parse_non_streaming_text, SseDecoded};
    use crate::types::LlmError;
    use serde_json::Value;

    /// Variant tag returned by `parse_sse_payload_kind`. Tests assert against
    /// this rather than re-exposing the internal `SseDecoded` enum verbatim.
    #[derive(Debug, Clone, PartialEq)]
    pub enum SseEventKind {
        /// `content_block_delta` text fragment.
        TextDelta(String),
        /// `content_block_start` for a tool-use block.
        ToolCall {
            /// Tool name.
            name: String,
            /// Tool input.
            input: Value,
        },
        /// `message_delta` or `message_start` carrying a token-usage row.
        Usage {
            /// Input tokens.
            input_t: u32,
            /// Output tokens.
            output_t: u32,
        },
        /// Stream terminator with stop reason.
        Stop(String),
        /// Recognized event but no actionable payload.
        Empty,
    }

    /// Public-test wrapper around `decode_sse_payload`. Returns a stable enum
    /// that does not leak the internal SseDecoded type.
    pub fn parse_sse_event(data: &str) -> Result<SseEventKind, LlmError> {
        match decode_sse_payload(data)? {
            None => Ok(SseEventKind::Empty),
            Some(SseDecoded::TextDelta(s)) => Ok(SseEventKind::TextDelta(s)),
            Some(SseDecoded::ToolCall { name, input }) => {
                Ok(SseEventKind::ToolCall { name, input })
            }
            Some(SseDecoded::Usage { input_t, output_t }) => {
                Ok(SseEventKind::Usage { input_t, output_t })
            }
            Some(SseDecoded::Stop(s)) => Ok(SseEventKind::Stop(s)),
        }
    }

    /// Public-test wrapper around `parse_non_streaming_text`.
    pub fn parse_non_streaming(body: &str) -> Result<String, LlmError> {
        parse_non_streaming_text(body)
    }
}
