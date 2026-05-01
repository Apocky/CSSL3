//! § T11-WAVE3-REPLAY · `event.rs`
//!
//! ReplayEvent = (timestamp_micros, kind) pair.  serde-derived for
//! line-by-line JSONL persistence in `recorder.rs`/`replayer.rs`.
//!
//! § DESIGN
//!
//! - `ts_micros: u64` — microseconds since recorder epoch (Recorder::t0).
//!   u64 = ~584 000 years headroom @ µs ; bounded by recorder lifetime.
//! - `ReplayEventKind` enum is open-ended — adding variants is a schema bump
//!   (see `integrity::SCHEMA_VERSION`).  Replayer skips unknown variants
//!   (forward-compat) ; tests assert round-trip identity for known variants.
//!
//! § PRIME-DIRECTIVE BINDING
//!
//! ReplayEvent is a *consent-of-state-disclosure* primitive : the user records
//! their inputs only when they explicitly enable replay, and replay is
//! transparent (manifest exposes sha256 + count + duration).

use serde::{Deserialize, Serialize};

/// Single replay event = (timestamp, kind).
///
/// Persisted as one JSON line in the replay file.  Stable across schema-v1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayEvent {
    /// Microseconds since recorder epoch (Recorder::t0_micros).
    pub ts_micros: u64,
    /// Discriminated event payload.
    pub kind: ReplayEventKind,
}

impl ReplayEvent {
    /// Construct a replay event with the given timestamp + kind.
    #[must_use]
    pub const fn new(ts_micros: u64, kind: ReplayEventKind) -> Self {
        Self { ts_micros, kind }
    }
}

/// Discriminated enum of all replay-able event types.
///
/// Adding a variant requires a `SCHEMA_VERSION` bump in `integrity.rs`.
/// Replayer treats unknown variants as malformed and skips with a log line
/// (no panic — preserves forward-compat replays).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ReplayEventKind {
    /// Keyboard key-down event.  `code` is the platform-neutral keycode
    /// (mapped by `cssl-host-input` ; opaque to this crate).
    KeyDown(u32),
    /// Keyboard key-up event.
    KeyUp(u32),
    /// Mouse-cursor position update.
    MouseMove {
        /// Cursor x in client-space pixels.
        x: f32,
        /// Cursor y in client-space pixels.
        y: f32,
    },
    /// Mouse-button click.
    MouseClick {
        /// Button index : 0 = left · 1 = right · 2 = middle · 3+ = extra.
        btn: u8,
        /// Click x.
        x: f32,
        /// Click y.
        y: f32,
    },
    /// Mouse scroll-wheel delta in lines (positive = up).
    MouseScroll(f32),
    /// RNG seed — replay-engine should re-seed its PRNG with this value.
    RngSeed(u64),
    /// Free-form text-intent (e.g. natural-language player input).
    IntentText(String),
    /// Frame-tick boundary with delta-time in milliseconds.
    Tick {
        /// Delta-time since last tick in ms.
        dt_ms: u32,
    },
    /// Begin a named span/marker — pairs with `MarkEnd`.
    MarkBegin(String),
    /// End a named span/marker.
    MarkEnd(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(ev: &ReplayEvent) -> ReplayEvent {
        let s = serde_json::to_string(ev).expect("serialize");
        serde_json::from_str(&s).expect("deserialize")
    }

    #[test]
    fn keydown_roundtrip() {
        let ev = ReplayEvent::new(1_234, ReplayEventKind::KeyDown(0x42));
        assert_eq!(roundtrip(&ev), ev);
    }

    #[test]
    fn mousemove_roundtrip() {
        let ev = ReplayEvent::new(
            5_000,
            ReplayEventKind::MouseMove {
                x: 100.5,
                y: 200.25,
            },
        );
        assert_eq!(roundtrip(&ev), ev);
    }

    #[test]
    fn intent_text_roundtrip() {
        let ev = ReplayEvent::new(
            10_000,
            ReplayEventKind::IntentText("look at the door".to_string()),
        );
        assert_eq!(roundtrip(&ev), ev);
    }

    #[test]
    fn marker_roundtrip() {
        let begin = ReplayEvent::new(0, ReplayEventKind::MarkBegin("frame_42".to_string()));
        let end = ReplayEvent::new(16_667, ReplayEventKind::MarkEnd("frame_42".to_string()));
        assert_eq!(roundtrip(&begin), begin);
        assert_eq!(roundtrip(&end), end);
    }
}
