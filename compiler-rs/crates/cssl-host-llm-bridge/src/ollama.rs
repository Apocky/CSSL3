//! § cssl-host-llm-bridge::ollama — Mode-B local Ollama bridge.
//!
//! § ROLE
//!   POST to `${endpoint}/api/chat` with the Ollama chat envelope. Streaming
//!   responses are line-delimited JSON (NDJSON) — one JSON object per line.
//!   The parser folds `message.content` deltas into `LlmEvent::TextDelta`
//!   and the terminal `done: true` line into `LlmEvent::Done`.
//!
//! § PRIME-DIRECTIVE
//!   - Cap-bit `LOCAL_OLLAMA` required at `make_bridge`.
//!   - Plain HTTP only (localhost:11434). No TLS feature needed.
//!   - Audit-emit recorded by the bridge driver ; this module only owns the
//!     wire format.
//!
//! § FALLBACK
//!   A connection-refused error (Ollama not running) becomes
//!   `LlmError::Network(...)` so the host can route to Mode-A or Mode-C.

use std::io::{BufRead, BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::types::{
    CapBits, LlmAuditEvent, LlmConfig, LlmError, LlmEvent, LlmMessage, LlmMode,
};
use crate::LlmBridge;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Mode-B bridge handle.
pub struct OllamaBridge {
    cfg: LlmConfig,
    cancel: AtomicBool,
}

impl OllamaBridge {
    /// Construct a Mode-B bridge after verifying the cap-bit.
    pub fn new(cfg: LlmConfig, caps: CapBits) -> Result<Self, LlmError> {
        if !caps.has(CapBits::LOCAL_OLLAMA) {
            return Err(LlmError::CapDenied("LOCAL_OLLAMA"));
        }
        tracing::info!(
            mode = "local_ollama",
            model = cfg.ollama_model.as_str(),
            endpoint = cfg.ollama_endpoint.as_str(),
            "ollama bridge constructed"
        );
        Ok(Self {
            cfg,
            cancel: AtomicBool::new(false),
        })
    }

    fn chat_url(&self) -> String {
        format!("{}/api/chat", self.cfg.ollama_endpoint.trim_end_matches('/'))
    }

    fn build_body(&self, messages: &[LlmMessage], stream: bool) -> Value {
        let chat: Vec<Value> = messages
            .iter()
            .map(|m| {
                json!({
                    "role": m.role.as_str(),
                    "content": m.content,
                })
            })
            .collect();
        json!({
            "model": self.cfg.ollama_model,
            "messages": chat,
            "stream": stream,
            "options": {
                "temperature": self.cfg.temperature,
                "num_predict": self.cfg.max_tokens,
            },
        })
    }

    fn agent() -> ureq::Agent {
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(REQUEST_TIMEOUT)
            .timeout_write(REQUEST_TIMEOUT)
            .build()
    }
}

impl LlmBridge for OllamaBridge {
    fn name(&self) -> &'static str {
        "mycelium-ollama-mode-b"
    }

    fn mode(&self) -> LlmMode {
        LlmMode::LocalOllama
    }

    fn chat(&self, messages: &[LlmMessage]) -> Result<String, LlmError> {
        let url = self.chat_url();
        let body = self.build_body(messages, false);
        let agent = Self::agent();
        let resp = agent
            .post(&url)
            .set("content-type", "application/json")
            .send_string(&serde_json::to_string(&body).map_err(|e| {
                LlmError::InvalidResponse(format!("body serialize: {e}"))
            })?);
        match resp {
            Ok(r) => {
                let s = r
                    .into_string()
                    .map_err(|e| LlmError::Network(format!("body read: {e}")))?;
                parse_ollama_non_streaming(&s)
            }
            Err(ureq::Error::Status(status, r)) => {
                let body = r.into_string().unwrap_or_default();
                Err(LlmError::Api { status, body })
            }
            Err(ureq::Error::Transport(t)) => Err(LlmError::Network(format!("{t}"))),
        }
    }

    fn chat_stream(
        &self,
        messages: &[LlmMessage],
        on_event: &mut dyn FnMut(LlmEvent),
    ) -> Result<LlmAuditEvent, LlmError> {
        self.cancel.store(false, Ordering::SeqCst);
        let url = self.chat_url();
        let body = self.build_body(messages, true);
        let agent = Self::agent();
        let resp = agent
            .post(&url)
            .set("content-type", "application/json")
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
        let mut stop_reason = String::from("stop");
        let cancel = &self.cancel;
        consume_ollama_stream(reader, &mut |dec: OllamaDecoded| -> bool {
            if cancel.load(Ordering::SeqCst) {
                on_event(LlmEvent::Error("cancelled".into()));
                return false;
            }
            match dec {
                OllamaDecoded::TextDelta(s) => on_event(LlmEvent::TextDelta(s)),
                OllamaDecoded::Done {
                    input_t,
                    output_t,
                    reason,
                } => {
                    input_tokens = input_t;
                    output_tokens = output_t;
                    if !reason.is_empty() {
                        stop_reason = reason;
                    }
                }
            }
            true
        })?;

        on_event(LlmEvent::Done {
            input_tokens,
            output_tokens,
            stop_reason,
        });

        Ok(LlmAuditEvent {
            mode: LlmMode::LocalOllama,
            model: self.cfg.ollama_model.clone(),
            input_tokens,
            output_tokens,
            estimated_cost_usd: 0.0,
            timestamp_unix: crate::now_unix(),
        })
    }

    fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

// ─── PARSER ───────────────────────────────────────────────────────────────

/// Non-streaming response shape.
#[derive(Debug, Deserialize, Serialize)]
struct OllamaNonStreamingResponse {
    #[serde(default)]
    message: Option<OllamaMessage>,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize)]
struct OllamaMessage {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

/// Pull the assistant text out of a non-streaming response body.
pub(crate) fn parse_ollama_non_streaming(body: &str) -> Result<String, LlmError> {
    let parsed: OllamaNonStreamingResponse = serde_json::from_str(body)
        .map_err(|e| LlmError::InvalidResponse(format!("non-streaming decode: {e}")))?;
    parsed
        .message
        .and_then(|m| m.content)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| LlmError::InvalidResponse("no message content".into()))
}

/// Decoded NDJSON line variants.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum OllamaDecoded {
    TextDelta(String),
    Done {
        input_t: u32,
        output_t: u32,
        reason: String,
    },
}

/// Parse a single NDJSON line emitted by `/api/chat?stream=true`.
pub(crate) fn parse_ollama_chunk(line: &str) -> Result<Option<OllamaDecoded>, LlmError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let v: Value = serde_json::from_str(trimmed)
        .map_err(|e| LlmError::InvalidResponse(format!("ollama json: {e}")))?;
    let done = v.get("done").and_then(Value::as_bool).unwrap_or(false);
    if done {
        let input_t = v
            .get("prompt_eval_count")
            .and_then(Value::as_u64)
            .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX));
        let output_t = v
            .get("eval_count")
            .and_then(Value::as_u64)
            .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX));
        let reason = v
            .get("done_reason")
            .and_then(Value::as_str)
            .unwrap_or("stop")
            .to_string();
        return Ok(Some(OllamaDecoded::Done {
            input_t,
            output_t,
            reason,
        }));
    }
    if let Some(text) = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
    {
        if !text.is_empty() {
            return Ok(Some(OllamaDecoded::TextDelta(text.to_string())));
        }
    }
    Ok(None)
}

/// Public surface used by the integration-tests in `tests/ollama_mode.rs`.
pub mod testing {
    use super::{parse_ollama_chunk, OllamaDecoded};
    use crate::types::LlmError;

    /// Stable variant tag for tests.
    #[derive(Debug, Clone, PartialEq)]
    pub enum OllamaChunkKind {
        /// Text delta from `message.content`.
        TextDelta(String),
        /// Terminal `done: true` line with token counts.
        Done {
            /// Input tokens.
            input_t: u32,
            /// Output tokens.
            output_t: u32,
            /// Stop reason (often `"stop"`).
            reason: String,
        },
        /// Recognized line with no content payload.
        Empty,
    }

    /// Test-friendly wrapper around `parse_ollama_chunk`.
    pub fn parse_chunk(line: &str) -> Result<OllamaChunkKind, LlmError> {
        match parse_ollama_chunk(line)? {
            None => Ok(OllamaChunkKind::Empty),
            Some(OllamaDecoded::TextDelta(s)) => Ok(OllamaChunkKind::TextDelta(s)),
            Some(OllamaDecoded::Done {
                input_t,
                output_t,
                reason,
            }) => Ok(OllamaChunkKind::Done {
                input_t,
                output_t,
                reason,
            }),
        }
    }
}

/// Drain an NDJSON stream from a reader.
fn consume_ollama_stream<R: Read, F: FnMut(OllamaDecoded) -> bool>(
    reader: R,
    on_decoded: &mut F,
) -> Result<(), LlmError> {
    let mut br = BufReader::new(reader);
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = br
            .read_line(&mut buf)
            .map_err(|e| LlmError::Network(format!("ollama read: {e}")))?;
        if n == 0 {
            return Ok(());
        }
        if let Some(dec) = parse_ollama_chunk(&buf)? {
            if !on_decoded(dec) {
                return Ok(());
            }
        }
    }
}
