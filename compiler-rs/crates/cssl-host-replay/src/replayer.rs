//! § T11-WAVE3-REPLAY · `replayer.rs`
//!
//! Streaming playback of a recorded JSONL replay-file with
//! microsecond-accurate timing.
//!
//! § TIMING MODEL
//!
//! `started_at` is captured on first `next_due()` call (lazy-start).  An
//! event with `ts_micros = T` is "due" once `now - started_at >= T µs`.  This
//! means the replay reproduces the *relative* timing of the original
//! recording — absolute walltime offsets are intentionally lost.
//!
//! § FORWARD-COMPAT
//!
//! Lines that fail to deserialize are skipped (logged via `eprintln!` to keep
//! stdlib-only ; can be redirected by the host).  Empty lines are also
//! skipped.  This means future schema-versions with extra variants can be
//! played back partially by older replayers (they'll silently drop unknowns).

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::time::Instant;

use crate::event::ReplayEvent;

/// Streaming replayer with timestamp-respecting playback.
pub struct Replayer {
    events: Vec<ReplayEvent>,
    cursor: usize,
    started_at: Option<Instant>,
}

impl Replayer {
    /// Load events from a JSONL file at `path`.  Empty + malformed lines are
    /// skipped (logged to stderr).  Events are kept in file-order.
    pub fn from_path(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut events: Vec<ReplayEvent> = Vec::new();
        for (line_no, line_result) in reader.lines().enumerate() {
            let line = line_result?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<ReplayEvent>(trimmed) {
                Ok(ev) => events.push(ev),
                Err(e) => {
                    eprintln!(
                        "cssl-host-replay : skip malformed line {} : {e}",
                        line_no + 1
                    );
                }
            }
        }
        Ok(Self::from_events(events))
    }

    /// Construct directly from an in-memory event list (test / fuzz helper).
    #[must_use]
    pub fn from_events(events: Vec<ReplayEvent>) -> Self {
        Self {
            events,
            cursor: 0,
            started_at: None,
        }
    }

    /// Return the next event whose `ts_micros` has elapsed since the replayer
    /// started ; `None` if no event is due yet OR replay is exhausted.
    ///
    /// On the *first* call, `started_at` is set to `now` (lazy-start).  This
    /// allows the caller to construct the replayer ahead-of-time without
    /// burning replay-time during construction.  The cursor advances on
    /// successful consume (i.e. the returned event is no longer eligible
    /// for re-issue without `rewind()`).
    pub fn next_due(&mut self, now: Instant) -> Option<&ReplayEvent> {
        if self.cursor >= self.events.len() {
            return None;
        }
        let started = *self.started_at.get_or_insert(now);
        let elapsed_micros = u64::try_from(now.saturating_duration_since(started).as_micros())
            .unwrap_or(u64::MAX);
        let next = &self.events[self.cursor];
        if next.ts_micros <= elapsed_micros {
            self.cursor += 1;
            // Re-borrow with the advanced index for return ; the value at
            // `cursor - 1` is the event we just consumed.
            Some(&self.events[self.cursor - 1])
        } else {
            None
        }
    }

    /// True once every event has been consumed.
    #[must_use]
    pub fn at_end(&self) -> bool {
        self.cursor >= self.events.len()
    }

    /// Reset the cursor to 0 and clear `started_at`.  Subsequent
    /// `next_due()` calls will re-issue the entire stream.
    pub fn rewind(&mut self) {
        self.cursor = 0;
        self.started_at = None;
    }

    /// Number of events loaded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// True iff zero events were loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Borrow the underlying event vector (test / inspection helper).
    #[must_use]
    pub fn events(&self) -> &[ReplayEvent] {
        &self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::ReplayEventKind;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::Duration;

    fn temp_path(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("cssl-host-replay-replayer-{tag}-{pid}-{nanos}.jsonl"));
        p
    }

    #[test]
    fn empty_file_yields_empty_replayer() {
        let p = temp_path("empty");
        std::fs::write(&p, "").expect("write");
        let r = Replayer::from_path(&p).expect("load");
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
        assert!(r.at_end());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn skip_malformed_lines() {
        let p = temp_path("malformed");
        let mut f = File::create(&p).expect("create");
        // good · blank · garbage · good
        let good1 = ReplayEvent::new(0, ReplayEventKind::KeyDown(1));
        let good2 = ReplayEvent::new(100, ReplayEventKind::KeyUp(1));
        writeln!(f, "{}", serde_json::to_string(&good1).unwrap()).unwrap();
        writeln!(f).unwrap();
        writeln!(f, "{{not json}}").unwrap();
        writeln!(f, "{}", serde_json::to_string(&good2).unwrap()).unwrap();
        f.flush().unwrap();
        drop(f);
        let r = Replayer::from_path(&p).expect("load");
        assert_eq!(r.len(), 2, "two valid events must survive parse");
        assert_eq!(r.events()[0], good1);
        assert_eq!(r.events()[1], good2);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn timing_respects_deltas() {
        let events = vec![
            ReplayEvent::new(0, ReplayEventKind::KeyDown(1)),
            ReplayEvent::new(50_000, ReplayEventKind::KeyUp(1)), // 50 ms
        ];
        let mut r = Replayer::from_events(events);
        let start = Instant::now();
        // First event @ ts_micros=0 must be due immediately on first call.
        let first = r.next_due(start).expect("first due");
        assert_eq!(first.ts_micros, 0);
        // Second event NOT due yet (only 0 ms elapsed since start).
        assert!(r.next_due(start).is_none(), "second must wait");
        // After 60 ms, second event must be due.
        let later = start + Duration::from_millis(60);
        let second = r.next_due(later).expect("second due");
        assert_eq!(second.ts_micros, 50_000);
        assert!(r.at_end());
    }

    #[test]
    fn rewind_returns_to_start() {
        let events = vec![
            ReplayEvent::new(0, ReplayEventKind::KeyDown(1)),
            ReplayEvent::new(0, ReplayEventKind::KeyUp(1)),
        ];
        let mut r = Replayer::from_events(events);
        let now = Instant::now();
        assert!(r.next_due(now).is_some());
        assert!(r.next_due(now).is_some());
        assert!(r.at_end());
        r.rewind();
        assert!(!r.at_end());
        assert_eq!(r.events().len(), 2);
        // After rewind, started_at is cleared — first call re-issues event 0.
        let now2 = Instant::now();
        let again = r.next_due(now2).expect("rewound first");
        assert_eq!(again.ts_micros, 0);
    }

    #[test]
    fn at_end_after_drain() {
        let events = vec![ReplayEvent::new(0, ReplayEventKind::Tick { dt_ms: 16 })];
        let mut r = Replayer::from_events(events);
        assert!(!r.at_end());
        let _ = r.next_due(Instant::now());
        assert!(r.at_end(), "after one consume of one-event stream, at_end");
        // Further next_due calls return None.
        let now = Instant::now();
        assert!(r.next_due(now).is_none());
    }
}
