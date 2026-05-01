//! § chrome://tracing JSON exporter
//!
//! Produces documents conforming to the
//! [Chrome Trace Event Format](https://docs.google.com/document/d/1CvAClvFfyA5R-PhYUmn5OOQtYMH4h6I0nSsKchNAySU/preview)
//! consumable by `chrome://tracing` in any Chromium-derived browser and by
//! Perfetto's web UI. Uses only complete-events (`ph: "X"`) which carry both
//! timestamp + duration in a single record — most compact representation for
//! the begin/end pairs we get out of [`crate::pair_marks`].
//!
//! ## Field naming
//!
//! Chrome's spec uses `traceEvents` (camelCase) at the document root. Our
//! Rust struct fields use snake_case ; `#[serde(rename = "...")]` translates.
//! Inside each event the fields `name` / `cat` / `ph` / `ts` / `dur` / `pid` /
//! `tid` / `args` already match the wire format and need no rename.

use crate::pair::MarkPair;
use serde::{Deserialize, Serialize};

/// § a chrome-tracing document : just a list of trace events.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChromeTracingDoc {
    /// The list of trace events. Renamed at-rest to the wire-format
    /// `"traceEvents"` key required by the spec.
    #[serde(rename = "traceEvents")]
    pub trace_events: Vec<ChromeTracingEvent>,
}

/// § a single chrome-tracing event in complete-event (`X`) phase.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChromeTracingEvent {
    /// Display name shown in the timeline ; typically the mark label.
    pub name: String,
    /// Comma-separated category list — used for filtering in the viewer.
    pub cat: String,
    /// Phase code ; for paired marks this is always `"X"` (complete-event).
    pub ph: String,
    /// Timestamp in microseconds since epoch (or process-start).
    pub ts: u64,
    /// Duration in microseconds. `None` for non-paired events
    /// (this exporter only emits paired ones, so always `Some`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dur: Option<u64>,
    /// Process id. We use `1` as a placeholder ; the LoA host can rewrite
    /// for multi-process scenarios.
    pub pid: u32,
    /// Thread id. We encode `MarkPair::depth` here so each nesting level
    /// shows on its own swimlane in the viewer.
    pub tid: u32,
    /// Free-form arguments rendered in the viewer's detail pane.
    pub args: serde_json::Value,
}

/// § convert a slice of [`MarkPair`]s to a chrome-tracing document.
#[must_use]
pub fn marks_to_chrome_tracing(pairs: &[MarkPair]) -> ChromeTracingDoc {
    let trace_events: Vec<ChromeTracingEvent> = pairs
        .iter()
        .map(|p| ChromeTracingEvent {
            name: p.label.clone(),
            cat: "cssl-rt".to_owned(),
            ph: "X".to_owned(),
            ts: p.start_ts,
            dur: Some(p.duration_us),
            pid: 1,
            tid: u32::from(p.depth),
            args: serde_json::json!({
                "depth": p.depth,
                "duration_us": p.duration_us,
            }),
        })
        .collect();
    ChromeTracingDoc { trace_events }
}

/// § render the document to pretty-printed JSON.
///
/// Errors only on serializer panics (impossible for our well-formed
/// in-memory document). On serialization failure returns `"{}"` to keep
/// the library panic-free per the design contract.
#[must_use]
pub fn render_json(doc: &ChromeTracingDoc) -> String {
    serde_json::to_string_pretty(doc).unwrap_or_else(|_| "{}".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(label: &str, depth: u8, start: u64, dur: u64) -> MarkPair {
        MarkPair {
            label: label.to_owned(),
            start_ts: start,
            end_ts: start + dur,
            duration_us: dur,
            depth,
        }
    }

    #[test]
    fn empty_pairs_yield_empty_document() {
        let doc = marks_to_chrome_tracing(&[]);
        assert!(doc.trace_events.is_empty());
        let json = render_json(&doc);
        // The pretty-printed empty form still includes the `traceEvents` key.
        assert!(json.contains("traceEvents"));
    }

    #[test]
    fn single_pair_emits_complete_event() {
        let pairs = vec![pair("frame", 0, 100, 50)];
        let doc = marks_to_chrome_tracing(&pairs);
        assert_eq!(doc.trace_events.len(), 1);
        let ev = &doc.trace_events[0];
        assert_eq!(ev.name, "frame");
        assert_eq!(ev.cat, "cssl-rt");
        assert_eq!(ev.ph, "X", "paired marks must use phase X");
        assert_eq!(ev.ts, 100);
        assert_eq!(ev.dur, Some(50));
        assert_eq!(ev.pid, 1);
        assert_eq!(ev.tid, 0);
    }

    #[test]
    fn nested_pair_uses_depth_in_tid() {
        let pairs = vec![pair("outer", 0, 0, 100), pair("inner", 2, 20, 30)];
        let doc = marks_to_chrome_tracing(&pairs);
        assert_eq!(doc.trace_events.len(), 2);
        assert_eq!(doc.trace_events[0].tid, 0);
        assert_eq!(doc.trace_events[1].tid, 2, "depth → tid mapping");
    }

    #[test]
    fn json_roundtrip_preserves_data() {
        let pairs = vec![pair("a", 0, 100, 50), pair("b", 1, 110, 30)];
        let doc = marks_to_chrome_tracing(&pairs);
        let json = render_json(&doc);
        let back: ChromeTracingDoc = serde_json::from_str(&json).expect("roundtrip");
        assert_eq!(back, doc);
    }

    #[test]
    fn rename_to_trace_events_camel_case_respected_in_json() {
        let pairs = vec![pair("frame", 0, 0, 100)];
        let doc = marks_to_chrome_tracing(&pairs);
        let json = render_json(&doc);
        // The wire format must carry "traceEvents" not "trace_events".
        assert!(
            json.contains("\"traceEvents\""),
            "wire format requires camelCase : {json}"
        );
        assert!(
            !json.contains("\"trace_events\""),
            "snake_case must not leak : {json}"
        );
    }
}
