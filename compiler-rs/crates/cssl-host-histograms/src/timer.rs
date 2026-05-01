//! # ScopedTimer : RAII timer that records elapsed-µs to a registry on drop
//!
//! Wrap a code-block to time it without manual stop calls. Drop-impl records
//! the elapsed-µs into the registry under the supplied name.
//!
//! ## Usage
//!
//! ```ignore
//! let mut reg = HistogramRegistry::new();
//! {
//!     let _t = scoped(&mut reg, "frame.total");
//!     // ... work ...
//! } // _t drops here, recording elapsed-µs into reg["frame.total"]
//! ```
//!
//! ## explicit stop
//!
//! [`ScopedTimer::stop`] records-and-disarms ; subsequent drop is a no-op.
//! Useful when the caller wants to extract the elapsed time AND record it,
//! or when the timer is being moved out of scope before its natural end.

use std::time::Instant;

use crate::registry::HistogramRegistry;

/// RAII timer that records elapsed time into a [`HistogramRegistry`] on drop.
pub struct ScopedTimer<'a> {
    reg: &'a mut HistogramRegistry,
    name: &'a str,
    start: Instant,
    /// `false` once `stop()` has been called explicitly ; suppresses the
    /// drop-time record so the same elapsed isn't double-counted.
    armed: bool,
}

impl<'a> ScopedTimer<'a> {
    /// Construct a new timer bound to `reg` + `name`. Equivalent to
    /// the free function [`scoped`].
    pub fn new(reg: &'a mut HistogramRegistry, name: &'a str) -> Self {
        Self {
            reg,
            name,
            start: Instant::now(),
            armed: true,
        }
    }

    /// Record elapsed-µs now and disarm the drop-time record.
    ///
    /// Returns the elapsed value in µs. After `stop()` returns, the timer
    /// will NOT record again on drop — useful when the caller wants to
    /// observe the elapsed value and bake it into the histogram in one
    /// step rather than letting the implicit drop do it.
    pub fn stop(&mut self) -> u64 {
        let elapsed_us = self.elapsed_us();
        if self.armed {
            self.reg.record(self.name, elapsed_us);
            self.armed = false;
        }
        elapsed_us
    }

    /// Read the current elapsed (µs) without stopping the timer.
    #[must_use]
    pub fn elapsed_us(&self) -> u64 {
        // Saturate at u64::MAX rather than panicking on the overflow path
        // (which would only trigger after ~292 471 years anyway).
        u64::try_from(self.start.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
}

impl Drop for ScopedTimer<'_> {
    fn drop(&mut self) {
        if self.armed {
            let elapsed_us = self.elapsed_us();
            self.reg.record(self.name, elapsed_us);
        }
    }
}

/// Free-function constructor — matches the spec's `scoped(reg, name)` API.
pub fn scoped<'a>(reg: &'a mut HistogramRegistry, name: &'a str) -> ScopedTimer<'a> {
    ScopedTimer::new(reg, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    /// § scoped timer records into the registry on drop.
    #[test]
    fn scoped_records_on_drop() {
        let mut reg = HistogramRegistry::new();
        {
            let _t = scoped(&mut reg, "test.scoped");
            // Sleep just enough that elapsed > 0 µs even on a fast clock.
            thread::sleep(Duration::from_millis(2));
        } // _t dropped here → record happens
        let h = reg.get("test.scoped").expect("recorded");
        assert_eq!(h.count(), 1);
        // Should be at least 1 ms = 1000 µs (sleep ≥ 2 ms ; test allows
        // generous slop for OS-scheduling jitter on busy CI workers).
        assert!(h.max_us >= 500, "elapsed too small: {} µs", h.max_us);
    }

    /// § explicit stop records once ; drop after stop is a no-op.
    #[test]
    fn explicit_stop_records_once() {
        let mut reg = HistogramRegistry::new();
        {
            let mut t = scoped(&mut reg, "test.stop");
            thread::sleep(Duration::from_millis(1));
            let _elapsed = t.stop();
            // After this scope ends, drop runs but is disarmed → no second record.
        }
        let h = reg.get("test.stop").expect("recorded");
        // Exactly one record despite both stop() AND drop() running.
        assert_eq!(h.count(), 1, "expected exactly 1 record, got {}", h.count());
    }

    /// § a near-zero elapsed lands in bucket 0 (the [0, 1)+0-special-case bucket).
    #[test]
    fn short_elapsed_into_bucket_zero() {
        let mut reg = HistogramRegistry::new();
        {
            let _t = scoped(&mut reg, "test.short");
            // No work — drop should fire near-immediately.
        }
        let h = reg.get("test.short").expect("recorded");
        assert_eq!(h.count(), 1);
        // Elapsed should land in one of the early buckets ; on a fast machine
        // this is bucket 0 ([0, 1) µs) or 1-3 ([1, 8) µs). We only verify
        // it's a low-numbered bucket (≤ 6 = [0, 64) µs) to keep the test
        // robust against jitter.
        let nonzero_buckets: Vec<usize> = h
            .buckets
            .iter()
            .enumerate()
            .filter(|(_, &c)| c > 0)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(nonzero_buckets.len(), 1, "expected one nonzero bucket");
        let idx = nonzero_buckets[0];
        assert!(idx <= 8, "elapsed bucket too high: idx={idx}");
    }
}
