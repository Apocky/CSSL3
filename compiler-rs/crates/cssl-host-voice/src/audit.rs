//! Audio-input audit-event ledger.
//!
//! § PURPOSE
//!   Every cap-decision (granted / denied) and every capture / transcribe
//!   boundary is recorded as a structured event. The host wires this
//!   ledger into the global R18 telemetry-ring (deferred to wave-5) so
//!   the user has continuous visibility into when the mic is hot and
//!   where audio bytes go.
//!
//! § JSONL FORMAT
//!   One JSON object per line. No pretty-printing. UTF-8. Trailing
//!   newline after every event. Empty input → empty string.

use serde::{Deserialize, Serialize};

/// Event kinds emitted along the voice-input boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioAuditKind {
    /// Capture frame about to be ingested by the ring.
    CaptureBegin,
    /// Capture frame ingested.
    CaptureEnd,
    /// Transcribe call about to be issued to the backend.
    TranscribeBegin,
    /// Transcribe call returned (success OR failure ; status carries which).
    TranscribeEnd,
    /// Cap-bit denial recorded for visibility.
    CapDenied,
}

/// Pass / deny / failure indicator for an audit event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioAuditStatus {
    Ok,
    Denied,
    Failed(String),
}

/// One audit-ledger event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioAuditEvent {
    /// Best-effort UTC timestamp.
    pub ts_iso: String,
    /// Event kind tag.
    pub kind: AudioAuditKind,
    /// Number of samples involved (0 when not applicable).
    pub samples: u64,
    /// Sample rate the ring was constructed with.
    pub sample_rate_hz: u32,
    /// True iff sovereign-cap bypass converted a deny → allow on this event.
    pub sovereign_cap: bool,
    /// Backend name (or "<none>" for early-deny capture events).
    pub backend: String,
    /// Backend-reported latency for transcribe events ; 0 otherwise.
    pub latency_ms: u32,
    /// Status indicator.
    pub status: AudioAuditStatus,
}

/// Render an event slice as JSONL (one object per line, trailing newline
/// after each). Empty input → empty string.
#[must_use]
pub fn render_jsonl(events: &[AudioAuditEvent]) -> String {
    if events.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for ev in events {
        // serde_json::to_string never errors for these structurally-sound
        // types ; if it ever did, we'd surface as an empty line rather
        // than panic — preserves the no-panic guarantee.
        match serde_json::to_string(ev) {
            Ok(s) => {
                out.push_str(&s);
                out.push('\n');
            }
            Err(_) => out.push('\n'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(kind: AudioAuditKind, status: AudioAuditStatus) -> AudioAuditEvent {
        AudioAuditEvent {
            ts_iso: "epoch:1700000000.000".into(),
            kind,
            samples: 1024,
            sample_rate_hz: 16_000,
            sovereign_cap: false,
            backend: "stub".into(),
            latency_ms: 0,
            status,
        }
    }

    #[test]
    fn empty_jsonl() {
        let s = render_jsonl(&[]);
        assert_eq!(s, "");
    }

    #[test]
    fn single_event_roundtrip() {
        let ev = mk(AudioAuditKind::CaptureBegin, AudioAuditStatus::Ok);
        let s = render_jsonl(&[ev.clone()]);
        // Exactly one line ; deserializes back identical.
        assert_eq!(s.matches('\n').count(), 1);
        let line = s.trim_end_matches('\n');
        let back: AudioAuditEvent = serde_json::from_str(line).expect("roundtrip");
        assert_eq!(back, ev);
        // Field presence sanity.
        assert!(line.contains("\"kind\":\"capture_begin\""));
        assert!(line.contains("\"status\":\"ok\""));
        assert!(line.contains("\"backend\":\"stub\""));
    }

    #[test]
    fn multi_event_jsonl_newline_separated() {
        let events = vec![
            mk(AudioAuditKind::CaptureBegin, AudioAuditStatus::Ok),
            mk(AudioAuditKind::CaptureEnd, AudioAuditStatus::Ok),
            mk(
                AudioAuditKind::CapDenied,
                AudioAuditStatus::Denied,
            ),
            mk(
                AudioAuditKind::TranscribeEnd,
                AudioAuditStatus::Failed("bad".into()),
            ),
        ];
        let s = render_jsonl(&events);
        assert_eq!(s.matches('\n').count(), 4);
        // Each line parses standalone.
        for line in s.lines() {
            let _: AudioAuditEvent =
                serde_json::from_str(line).expect("each line parses");
        }
        assert!(s.contains("capture_begin"));
        assert!(s.contains("capture_end"));
        assert!(s.contains("cap_denied"));
        assert!(s.contains("\"failed\""));
    }
}
