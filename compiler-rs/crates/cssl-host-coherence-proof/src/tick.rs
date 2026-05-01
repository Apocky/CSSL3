// § tick.rs · ServerTick monotonic-counter timestamp-trust-anchor
// ══════════════════════════════════════════════════════════════════════════════
// § I> ServerTick { tick_id · monotonic_counter · ts } anchors event-time
// § I> monotonic_counter NEVER-decrease :
//   - debug : panic-on-decrease (immediate-fail-fast)
//   - release : skip-on-decrease + return TickError::NonMonotonic (caller chooses)
// § I> Events without an associated ServerTick are UNTRUSTED-TS (validator rejects)
// ══════════════════════════════════════════════════════════════════════════════
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors from ServerTick stream-validation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TickError {
    /// Counter decreased — possible clock-skew or replay.
    #[error("ServerTick monotonic_counter decreased ({prev} → {next})")]
    NonMonotonic { prev: u64, next: u64 },
    /// Event ts is outside the bracketed [tick_lo, tick_hi] window.
    #[error("event ts {ts} not bracketed by tick {tick_id}")]
    UntrustedTs { ts: u64, tick_id: u64 },
}

/// Monotonic timestamp-trust-anchor.
///
/// `tick_id` is a sequential identifier ; `monotonic_counter` is the
/// strictly-non-decreasing logical-time ; `ts` is the wall-clock approximation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerTick {
    pub tick_id: u64,
    pub monotonic_counter: u64,
    pub ts: u64,
}

impl ServerTick {
    /// Construct a new tick.
    pub fn new(tick_id: u64, monotonic_counter: u64, ts: u64) -> Self {
        Self {
            tick_id,
            monotonic_counter,
            ts,
        }
    }
}

/// Append-only stream of ticks with monotonic-counter enforcement.
#[derive(Debug, Default, Clone)]
pub struct TickStream {
    ticks: Vec<ServerTick>,
}

impl TickStream {
    /// Empty stream.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a tick. Enforces monotonic-counter.
    ///
    /// - Debug builds : panics if counter decreases (loud-fail).
    /// - Release builds : returns `TickError::NonMonotonic` and skips append.
    pub fn append(&mut self, tick: ServerTick) -> Result<(), TickError> {
        if let Some(prev) = self.ticks.last() {
            let decreased = tick.monotonic_counter < prev.monotonic_counter;
            #[cfg(debug_assertions)]
            assert!(
                !decreased,
                "ServerTick monotonic_counter decreased ({} → {}) — refusing append",
                prev.monotonic_counter, tick.monotonic_counter
            );
            #[cfg(not(debug_assertions))]
            if decreased {
                return Err(TickError::NonMonotonic {
                    prev: prev.monotonic_counter,
                    next: tick.monotonic_counter,
                });
            }
        }
        self.ticks.push(tick);
        Ok(())
    }

    /// All ticks in append-order.
    pub fn ticks(&self) -> &[ServerTick] {
        &self.ticks
    }

    /// Most-recent tick (latest by append-order).
    pub fn latest(&self) -> Option<&ServerTick> {
        self.ticks.last()
    }

    /// Validate that an event-ts is bracketed by a known tick.
    ///
    /// Returns Ok if the event-ts is `≤ latest_tick.ts` AND a tick exists.
    /// Returns Err(UntrustedTs) when stream is empty OR ts is in the future.
    pub fn validate_ts(&self, event_ts: u64) -> Result<&ServerTick, TickError> {
        let latest = self.latest().ok_or(TickError::UntrustedTs {
            ts: event_ts,
            tick_id: 0,
        })?;
        if event_ts > latest.ts {
            return Err(TickError::UntrustedTs {
                ts: event_ts,
                tick_id: latest.tick_id,
            });
        }
        Ok(latest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_construct() {
        let t = ServerTick::new(1, 100, 1000);
        assert_eq!(t.tick_id, 1);
        assert_eq!(t.monotonic_counter, 100);
        assert_eq!(t.ts, 1000);
    }

    #[test]
    fn monotonic_counter_strict_increase_ok() {
        let mut s = TickStream::new();
        s.append(ServerTick::new(1, 10, 100)).unwrap();
        s.append(ServerTick::new(2, 20, 200)).unwrap();
        s.append(ServerTick::new(3, 20, 300)).unwrap(); // equal-OK
        assert_eq!(s.ticks().len(), 3);
    }

    // The following test is debug-build-only because we panic in debug.
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "ServerTick monotonic_counter decreased")]
    fn monotonic_decrease_panics_in_debug() {
        let mut s = TickStream::new();
        s.append(ServerTick::new(1, 50, 100)).unwrap();
        // Decrease — should panic in debug.
        s.append(ServerTick::new(2, 10, 200)).unwrap();
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn monotonic_decrease_errors_in_release() {
        let mut s = TickStream::new();
        s.append(ServerTick::new(1, 50, 100)).unwrap();
        let err = s.append(ServerTick::new(2, 10, 200)).unwrap_err();
        assert_eq!(err, TickError::NonMonotonic { prev: 50, next: 10 });
    }

    #[test]
    fn untrusted_ts_when_empty_stream() {
        let s = TickStream::new();
        let err = s.validate_ts(123).unwrap_err();
        let TickError::UntrustedTs { ts, .. } = err else {
            panic!("expected UntrustedTs");
        };
        assert_eq!(ts, 123);
    }

    #[test]
    fn ts_after_latest_tick_is_untrusted() {
        let mut s = TickStream::new();
        s.append(ServerTick::new(1, 10, 100)).unwrap();
        // Future-ts beyond latest tick → UntrustedTs.
        let err = s.validate_ts(500).unwrap_err();
        let TickError::UntrustedTs { tick_id, .. } = err else {
            panic!("expected UntrustedTs");
        };
        assert_eq!(tick_id, 1);
    }
}
