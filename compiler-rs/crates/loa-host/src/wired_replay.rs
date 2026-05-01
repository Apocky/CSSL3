//! § wired_replay — thin loa-host wrapper around `cssl-host-replay`.
//!
//! § T11-W5c-LOA-HOST-WIRE
//!   Re-exports the most-useful public types from the underlying replay crate
//!   so MCP-tool handlers can construct + drive a recorder/replayer without
//!   each call-site reaching across to the path-dep. The convenience
//!   `record_event` helper provides a short-form recorder-append used by
//!   wave-6 input-pipeline integrations.
//!
//! § wrapped surface
//!   - [`Recorder`] — append-only JSONL writer (64 KiB-buffered).
//!   - [`Replayer`] — streaming reader with timing-gated emission.
//!   - [`ReplayEvent`] / [`ReplayEventKind`] — canonical event envelope.
//!
//! § ATTESTATION ¬ harm — wrapper is a re-export shim ; no new state.

pub use cssl_host_replay::{
    ReplayEvent, ReplayEventKind, Recorder, RecorderStats, Replayer, MAX_BUFFER_BYTES,
};

/// Convenience : append a single typed event to an open [`Recorder`].
///
/// Mirrors `Recorder::append` but takes an already-constructed
/// [`ReplayEvent`] and ignores its `ts_micros` (the recorder always
/// derives the timestamp from its internal `Instant::now() - t0` clock,
/// per the determinism contract). Returns the underlying `io::Result`.
pub fn record_event(rec: &mut Recorder, ev: ReplayEvent) -> std::io::Result<()> {
    rec.append(ev.kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_event_appends_to_recorder() {
        // Build a temp-path recorder, append one event via the wrapper helper,
        // and check the recorder counter advanced.
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("loa-host-wired-replay-{pid}-{nanos}.jsonl"));

        let mut rec = Recorder::new(&p).expect("recorder open");
        let ev = ReplayEvent::new(0, ReplayEventKind::KeyDown(65));
        record_event(&mut rec, ev).expect("record_event ok");
        rec.flush().expect("flush ok");
        assert_eq!(rec.stats().count, 1);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn re_exports_compile() {
        let _ = MAX_BUFFER_BYTES;
        // Construct the canonical event types to confirm the re-exports
        // are usable downstream.
        let _ev = ReplayEvent::new(123, ReplayEventKind::KeyUp(65));
        let _stats = RecorderStats::default();
    }
}
