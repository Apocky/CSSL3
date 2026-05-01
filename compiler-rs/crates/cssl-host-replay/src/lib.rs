//! # cssl-host-replay
//!
//! Deterministic input + RNG-seed replay capture/playback for the LoA host —
//! a debug + test apparatus per the CSSLv3 "iterate-everywhere" directive.
//!
//! ## Why a Replay Crate?
//!
//! Procedurally-generated game sessions hinge on a chain of `(seed, inputs) →
//! state`.  Reproducing a bug, regression-testing the renderer, or comparing
//! two engine builds all require the *same* (seed, input-stream) tuple.
//! `cssl-host-replay` is the recorder/player for that tuple :
//!
//! - [`Recorder`] — append-only JSONL writer for `(timestamp, event)` pairs,
//!   bounded to 64 KiB of buffered IO and safe to drop mid-session.
//! - [`Replayer`] — streaming reader that re-issues events in order, gating
//!   each emission on real elapsed time (relative to the first read).
//! - [`integrity`] — sidecar SHA-256 manifest so a replay can be detected as
//!   tampered or schema-stale before the engine attempts to consume it.
//!
//! The crate is intentionally standalone — it is NOT yet wired into
//! `loa-host`, allowing wave-3 and wave-4 to integrate it on different
//! cadences without coupling.
//!
//! ## Forbidden Patterns
//!
//! - `unsafe` is forbidden crate-wide via `#![forbid(unsafe_code)]`.
//! - Library code never panics ; every fallible path returns `io::Result`
//!   or an explicit `Option`.
//! - Buffers are bounded (`recorder::MAX_BUFFER_BYTES = 64 KiB`).
//!
//! ## Schema Versioning
//!
//! [`integrity::SCHEMA_VERSION`] increments any time `ReplayEventKind`
//! gains a variant or `ReplayEvent` gains a field.  Manifests that
//! disagree on schema fail [`integrity::verify_manifest`].

#![forbid(unsafe_code)]

pub mod event;
pub mod integrity;
pub mod recorder;
pub mod replayer;

pub use event::{ReplayEvent, ReplayEventKind};
pub use integrity::{
    sha256_file, verify_manifest, write_manifest, ReplayManifest, SCHEMA_VERSION,
};
pub use recorder::{Recorder, RecorderStats, MAX_BUFFER_BYTES};
pub use replayer::Replayer;

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    fn temp_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("cssl-host-replay-int-{tag}-{pid}-{nanos}.jsonl"));
        p
    }

    /// Record a small session, drain it through the replayer, and assert
    /// the *kind*-sequence is identical (timing is not bit-exact across
    /// real clocks but ordering is deterministic).
    #[test]
    fn round_trip_record_then_replay() {
        let p = temp_path("rt-record");
        // RECORD : seed → keys → tick.
        {
            let mut r = Recorder::new(&p).expect("open");
            r.append(ReplayEventKind::RngSeed(0xDEAD_BEEF_CAFE_BABE))
                .unwrap();
            r.append(ReplayEventKind::KeyDown(0x41)).unwrap();
            r.append(ReplayEventKind::KeyUp(0x41)).unwrap();
            r.append(ReplayEventKind::IntentText("walk north".into()))
                .unwrap();
            r.append(ReplayEventKind::Tick { dt_ms: 16 }).unwrap();
            r.flush().unwrap();
            assert_eq!(r.stats().count, 5);
        }
        // REPLAY : drain via next_due() with a generous time-jump so all
        // events become due ; assert kind-order matches.
        //
        // The first next_due() call captures `started_at` from its `now`
        // argument ; subsequent calls measure elapsed-since-started_at
        // against each event's ts_micros.  To drain in a single loop, prime
        // started_at on the first call with `start`, then advance `now` to
        // `start + 60s` for the actual draining.
        let mut player = Replayer::from_path(&p).expect("load");
        let start = Instant::now();
        // Prime started_at = start (first call : ts_micros=0 event is due
        // and consumed, since 0 <= 0).
        let mut got: Vec<ReplayEventKind> = Vec::new();
        if let Some(ev) = player.next_due(start) {
            got.push(ev.kind.clone());
        }
        // Now advance to +60s ; remaining 4 events all become due.
        let later = start + Duration::from_secs(60);
        while let Some(ev) = player.next_due(later) {
            got.push(ev.kind.clone());
        }
        assert_eq!(got.len(), 5, "all 5 events drained");
        assert!(matches!(got[0], ReplayEventKind::RngSeed(0xDEAD_BEEF_CAFE_BABE)));
        assert!(matches!(got[1], ReplayEventKind::KeyDown(0x41)));
        assert!(matches!(got[2], ReplayEventKind::KeyUp(0x41)));
        assert!(matches!(&got[3], ReplayEventKind::IntentText(s) if s == "walk north"));
        assert!(matches!(got[4], ReplayEventKind::Tick { dt_ms: 16 }));
        assert!(player.at_end());
        let _ = std::fs::remove_file(&p);
    }

    /// Record + write manifest + verify : the complete integrity loop.
    #[test]
    fn round_trip_with_manifest_verifies() {
        let r_path = temp_path("rt-manifest-replay");
        let m_path = temp_path("rt-manifest-sidecar");
        // RECORD.
        {
            let mut rec = Recorder::new(&r_path).expect("open");
            rec.append(ReplayEventKind::RngSeed(7)).unwrap();
            rec.append(ReplayEventKind::MouseMove { x: 1.5, y: 2.5 })
                .unwrap();
            rec.append(ReplayEventKind::MouseClick {
                btn: 0,
                x: 10.0,
                y: 20.0,
            })
            .unwrap();
            rec.append(ReplayEventKind::MarkBegin("scene".into()))
                .unwrap();
            rec.append(ReplayEventKind::MarkEnd("scene".into())).unwrap();
            rec.flush().unwrap();
        }
        // MANIFEST + VERIFY.
        write_manifest(&r_path, &m_path).expect("write manifest");
        let ok = verify_manifest(&r_path, &m_path).expect("verify");
        assert!(ok, "freshly-written manifest must verify");
        // Inspect the manifest matches the recorded session.
        let body = std::fs::read_to_string(&m_path).expect("read");
        let manifest: ReplayManifest = serde_json::from_str(&body).expect("parse");
        assert_eq!(manifest.event_count, 5);
        assert_eq!(manifest.schema_version, SCHEMA_VERSION);
        assert_eq!(manifest.sha256_hex.len(), 64);
        // Tamper-check : appending one event invalidates the manifest.
        {
            let mut rec = Recorder::new(&r_path).expect("reopen");
            rec.append(ReplayEventKind::Tick { dt_ms: 1 }).unwrap();
            rec.flush().unwrap();
        }
        let ok2 = verify_manifest(&r_path, &m_path).expect("verify");
        assert!(!ok2, "tampered replay must fail verification");
        let _ = std::fs::remove_file(&r_path);
        let _ = std::fs::remove_file(&m_path);
    }
}
