//! The NDJSON turn stream: `apocky.apocrypha-chat-stream.v1`.
//!
//! `/api/apocrypha/chat` writes one JSON object per line while the reply is
//! being composed — `delta` lines carrying text as it is written, then exactly
//! one terminal `completed` or `error`. This module owns the reading of that
//! stream and nothing else, so its rules can be tested without a socket.
//!
//! The rule worth stating plainly: the text shown while streaming and the text
//! in the verified terminal response must be the same text. A stream that
//! renders one thing and certifies another is refused rather than displayed —
//! otherwise the live view becomes a place to say something the receipt does
//! not cover.

use crate::protocol::{self, ProtocolError, Result};

pub const SCHEMA: &str = "apocky.apocrypha-chat-stream.v1";
pub const MAX_DELTA_BYTES: usize = 32 * 1024;
pub const MAX_STREAM_BYTES: usize = 128 * 1024;
/// A line longer than this is refused before it is parsed.
pub const MAX_LINE_BYTES: usize = MAX_DELTA_BYTES + 8 * 1024;

fn reject(message: &str) -> ProtocolError {
    ProtocolError::Rejected(message.to_string())
}

#[derive(Debug, Default)]
pub struct TurnStream {
    accumulated: String,
    terminal: Option<serde_json::Value>,
}

impl TurnStream {
    pub fn new() -> Self {
        Self::default()
    }

    /// What has been written so far.
    pub fn text(&self) -> &str {
        &self.accumulated
    }

    pub fn is_finished(&self) -> bool {
        self.terminal.is_some()
    }

    /// Accepts one line; returns the delta it carried, when it carried one.
    pub fn accept(&mut self, raw: &str) -> Result<Option<String>> {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.is_empty() {
            return Ok(None);
        }
        if line.len() > MAX_LINE_BYTES {
            return Err(reject("The reply stream sent an oversized line."));
        }
        if self.terminal.is_some() {
            return Err(reject("The reply stream continued after it had finished."));
        }
        let event: serde_json::Value =
            serde_json::from_str(line).map_err(|_| reject("The reply stream sent an invalid line."))?;
        if !event.is_object() || event.get("schema_version").and_then(|v| v.as_str()) != Some(SCHEMA) {
            return Err(reject("The reply stream used an unsupported format."));
        }
        match event.get("type").and_then(|v| v.as_str()) {
            Some("delta") => {
                let delta = event
                    .get("text")
                    .and_then(|v| v.as_str())
                    .filter(|value| !value.is_empty() && value.len() <= MAX_DELTA_BYTES)
                    .ok_or_else(|| reject("The reply stream sent an invalid fragment."))?;
                if self.accumulated.len() + delta.len() > MAX_STREAM_BYTES {
                    return Err(reject("The reply grew past the size this app will display."));
                }
                self.accumulated.push_str(delta);
                Ok(Some(delta.to_string()))
            }
            Some("completed") => {
                let result = event
                    .get("result")
                    .filter(|value| value.is_object())
                    .ok_or_else(|| reject("The reply stream finished without its verified response."))?;
                let certified = result.get("text").and_then(|v| v.as_str()).unwrap_or_default();
                if !self.accumulated.is_empty() && self.accumulated.trim() != certified {
                    return Err(reject(
                        "The reply shown while it was being written does not match the verified reply.",
                    ));
                }
                self.terminal = Some(result.clone());
                Ok(None)
            }
            Some("error") => Err(reject(
                event
                    .get("error")
                    .and_then(|v| v.as_str())
                    .filter(|value| !value.is_empty() && value.len() <= 512)
                    .unwrap_or("The reply could not be completed."),
            )),
            _ => Err(reject("The reply stream sent an unknown event.")),
        }
    }

    /// Closes the stream and returns the verified reply.
    pub fn finish(self, session: &str, request: &str) -> Result<String> {
        let terminal = self
            .terminal
            .ok_or_else(|| reject("The reply stream ended before the reply was finished."))?;
        protocol::chat_result(&terminal, session, request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(text: &str) -> String {
        serde_json::json!({ "schema_version": SCHEMA, "type": "delta", "text": text }).to_string()
    }

    fn completed(session: &str, request: &str, text: &str) -> String {
        serde_json::json!({
            "schema_version": SCHEMA,
            "type": "completed",
            "result": {
                "outcome": "completed",
                "session_id": session,
                "request_id": request,
                "text": text,
                "model_id": "apocrypha-runtime",
                "response_digest": "b".repeat(64),
            },
        })
        .to_string()
    }

    #[test]
    fn a_reply_arrives_in_fragments_and_ends_verified() {
        let session = protocol::new_id();
        let request = protocol::new_id();
        let mut stream = TurnStream::new();
        assert_eq!(stream.accept(&delta("Hello")).unwrap().as_deref(), Some("Hello"));
        assert_eq!(stream.accept(&delta(", world")).unwrap().as_deref(), Some(", world"));
        assert_eq!(stream.text(), "Hello, world");
        assert!(!stream.is_finished());
        assert_eq!(stream.accept(&completed(&session, &request, "Hello, world")).unwrap(), None);
        assert!(stream.is_finished());
        assert_eq!(stream.finish(&session, &request).unwrap(), "Hello, world");
    }

    #[test]
    fn trailing_whitespace_in_the_stream_is_forgiven_but_the_words_are_not() {
        let session = protocol::new_id();
        let request = protocol::new_id();
        let mut stream = TurnStream::new();
        stream.accept(&delta("Answer\n")).unwrap();
        assert!(stream.accept(&completed(&session, &request, "Answer")).is_ok());

        let mut different = TurnStream::new();
        different.accept(&delta("Answer")).unwrap();
        let error = different
            .accept(&completed(&session, &request, "A different answer"))
            .unwrap_err();
        assert!(error.to_string().contains("does not match the verified reply"));
    }

    #[test]
    fn a_stream_that_never_finishes_is_not_a_reply() {
        let mut stream = TurnStream::new();
        stream.accept(&delta("half a th")).unwrap();
        let error = stream.finish(&protocol::new_id(), &protocol::new_id()).unwrap_err();
        assert!(error.to_string().contains("ended before the reply was finished"));
    }

    #[test]
    fn nothing_may_follow_the_terminal_event() {
        let session = protocol::new_id();
        let request = protocol::new_id();
        let mut stream = TurnStream::new();
        stream.accept(&completed(&session, &request, "done")).unwrap();
        assert!(stream.accept(&delta("more")).is_err());
    }

    #[test]
    fn an_error_event_becomes_the_message_the_person_sees() {
        let mut stream = TurnStream::new();
        let event = serde_json::json!({
            "schema_version": SCHEMA, "type": "error", "error": "runtime_unavailable",
        })
        .to_string();
        assert_eq!(stream.accept(&event).unwrap_err().to_string(), "runtime_unavailable");
    }

    #[test]
    fn malformed_lines_are_refused_rather_than_rendered() {
        let session = protocol::new_id();
        let request = protocol::new_id();
        for bad in [
            "not json".to_string(),
            "[]".to_string(),
            serde_json::json!({ "type": "delta", "text": "x" }).to_string(),
            serde_json::json!({ "schema_version": "other.v1", "type": "delta", "text": "x" }).to_string(),
            serde_json::json!({ "schema_version": SCHEMA, "type": "delta", "text": "" }).to_string(),
            serde_json::json!({ "schema_version": SCHEMA, "type": "delta", "text": 7 }).to_string(),
            serde_json::json!({ "schema_version": SCHEMA, "type": "sneak", "text": "x" }).to_string(),
            serde_json::json!({ "schema_version": SCHEMA, "type": "completed" }).to_string(),
            serde_json::json!({ "schema_version": SCHEMA, "type": "completed", "result": 7 }).to_string(),
        ] {
            let mut stream = TurnStream::new();
            assert!(stream.accept(&bad).is_err(), "{bad} must be refused");
        }
        // A terminal whose evidence is missing fails at finish, not at accept.
        let mut stream = TurnStream::new();
        let thin = serde_json::json!({
            "schema_version": SCHEMA, "type": "completed",
            "result": { "outcome": "completed", "session_id": session, "request_id": request, "text": "hi" },
        })
        .to_string();
        stream.accept(&thin).unwrap();
        assert!(stream.finish(&session, &request).is_err());
    }

    #[test]
    fn blank_lines_are_ignored_and_carriage_returns_trimmed() {
        let mut stream = TurnStream::new();
        assert_eq!(stream.accept("").unwrap(), None);
        assert_eq!(stream.accept("\r").unwrap(), None);
        assert_eq!(stream.accept(&format!("{}\r", delta("x"))).unwrap().as_deref(), Some("x"));
    }

    #[test]
    fn the_stream_is_bounded_in_both_directions() {
        let mut stream = TurnStream::new();
        let big = "x".repeat(MAX_DELTA_BYTES + 1);
        assert!(stream.accept(&delta(&big)).is_err(), "a single oversized fragment is refused");

        let mut growing = TurnStream::new();
        let chunk = "y".repeat(MAX_DELTA_BYTES);
        for _ in 0..(MAX_STREAM_BYTES / MAX_DELTA_BYTES) {
            growing.accept(&delta(&chunk)).unwrap();
        }
        assert_eq!(growing.text().len(), MAX_STREAM_BYTES);
        assert!(growing.accept(&delta("z")).is_err(), "the total is bounded too");
    }
}
