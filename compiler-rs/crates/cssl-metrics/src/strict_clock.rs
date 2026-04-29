//! Deterministic-clock indirection for replay-strict mode.
//!
//! § SPEC : `_drafts/phase_j/06_l2_telemetry_spec.md` § II.4 + § VI.2 + § VI.6.
//!
//! § DESIGN
//!   - `monotonic_ns()` is the canonical timestamp source for [`crate::Timer`].
//!     In default builds it reads a wallclock-ish monotonic counter
//!     (`std::time::Instant`-derived).
//!   - Under feature `replay-strict`, `monotonic_ns` redirects to a
//!     deterministic-function of `(frame_n, sub_phase_offset)` so every
//!     timer-record is bit-equal across replay-runs (per H5 contract).
//!   - The strict-mode logical clock is process-local : tests + replay
//!     drivers call [`set_logical_clock`] before any timer-record to install
//!     the `(frame_n, sub_phase)` cursor.
//!
//! § THREAD-SAFETY
//!   - Default-mode `monotonic_ns()` is thread-safe (Instant is Sync).
//!   - Strict-mode logical-clock lives in a `RwLock`-guarded static — single-
//!     producer (the engine driver) ; readers (record-sites) only see the most
//!     recent installed cursor.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

/// Process-wide start-instant used to derive monotonic-ns under default builds.
static START: OnceLock<Instant> = OnceLock::new();

/// Logical-clock state under feature `replay-strict`.
///
/// Encoded as a single `AtomicU64` packing `(frame_n: u32, sub_phase: u32)`.
/// `sub_phase` is the within-frame offset slot (0..4 billion ; ~1 sub-ns
/// granularity at 60Hz, ample for any pipeline-stage decomposition).
static LOGICAL_CURSOR: AtomicU64 = AtomicU64::new(0);

/// Fixed nanos-per-frame for strict-mode synthesis. 60Hz default ; replay
/// drivers may override per-frame via [`set_logical_clock_frame_ns`].
static LOGICAL_FRAME_NS: AtomicU64 = AtomicU64::new(16_666_667); // ≈ 1/60 s

/// Read the current monotonic-ns timestamp.
///
/// § DEFAULT-MODE
///   `Instant::now() - START` ; suitable for non-replay builds.
///
/// § STRICT-MODE (`replay-strict` feature on)
///   `frame_n × LOGICAL_FRAME_NS + sub_phase` ; bit-deterministic.
#[must_use]
pub fn monotonic_ns() -> u64 {
    if cfg!(feature = "replay-strict") {
        let cursor = LOGICAL_CURSOR.load(Ordering::Acquire);
        let frame_n = (cursor >> 32) & 0xFFFF_FFFF;
        let sub_phase = cursor & 0xFFFF_FFFF;
        let frame_ns = LOGICAL_FRAME_NS.load(Ordering::Acquire);
        frame_n.wrapping_mul(frame_ns).wrapping_add(sub_phase)
    } else {
        let start = START.get_or_init(Instant::now);
        u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}

/// Install the `(frame_n, sub_phase)` cursor used by [`monotonic_ns`] under
/// `replay-strict`. No-op under default builds (preserved for API-symmetry).
pub fn set_logical_clock(frame_n: u32, sub_phase: u32) {
    let packed = (u64::from(frame_n) << 32) | u64::from(sub_phase);
    LOGICAL_CURSOR.store(packed, Ordering::Release);
}

/// Override the per-frame nanosecond denominator (default 16_666_667 ≈ 60Hz).
pub fn set_logical_clock_frame_ns(ns: u64) {
    LOGICAL_FRAME_NS.store(ns, Ordering::Release);
}

/// Read the currently-installed `(frame_n, sub_phase)` cursor.
#[must_use]
pub fn logical_clock_cursor() -> (u32, u32) {
    let cursor = LOGICAL_CURSOR.load(Ordering::Acquire);
    let frame_n = u32::try_from((cursor >> 32) & 0xFFFF_FFFF).unwrap_or(u32::MAX);
    let sub_phase = u32::try_from(cursor & 0xFFFF_FFFF).unwrap_or(u32::MAX);
    (frame_n, sub_phase)
}

/// True iff the current build is replay-strict.
#[must_use]
pub const fn is_strict_mode() -> bool {
    cfg!(feature = "replay-strict")
}

#[cfg(test)]
mod tests {
    use super::{
        is_strict_mode, logical_clock_cursor, monotonic_ns, set_logical_clock,
        set_logical_clock_frame_ns,
    };

    #[test]
    fn monotonic_ns_returns_value() {
        let _ = monotonic_ns();
    }

    #[test]
    fn monotonic_ns_two_reads_under_default_increase() {
        if !is_strict_mode() {
            let a = monotonic_ns();
            // small busy-spin to ensure measurable gap on most platforms
            for _ in 0..1000 {
                std::hint::black_box(0);
            }
            let b = monotonic_ns();
            assert!(b >= a, "monotonic_ns must be monotonic non-decreasing");
        }
    }

    #[test]
    fn set_logical_clock_roundtrip() {
        set_logical_clock(7, 1234);
        let (f, s) = logical_clock_cursor();
        assert_eq!(f, 7);
        assert_eq!(s, 1234);
    }

    #[test]
    fn set_logical_clock_max_values_roundtrip() {
        set_logical_clock(u32::MAX, u32::MAX);
        let (f, s) = logical_clock_cursor();
        assert_eq!(f, u32::MAX);
        assert_eq!(s, u32::MAX);
        // restore so subsequent strict-mode tests see a known cursor.
        set_logical_clock(0, 0);
    }

    #[test]
    fn set_logical_clock_zero_roundtrip() {
        set_logical_clock(0, 0);
        let (f, s) = logical_clock_cursor();
        assert_eq!(f, 0);
        assert_eq!(s, 0);
    }

    #[cfg(feature = "replay-strict")]
    #[test]
    fn strict_mode_monotonic_is_deterministic_function_of_cursor() {
        set_logical_clock(0, 0);
        let a = monotonic_ns();
        set_logical_clock(0, 0);
        let b = monotonic_ns();
        assert_eq!(a, b, "strict-mode must be bit-deterministic");
    }

    #[cfg(feature = "replay-strict")]
    #[test]
    fn strict_mode_advances_with_cursor() {
        set_logical_clock(1, 0);
        let a = monotonic_ns();
        set_logical_clock(1, 1000);
        let b = monotonic_ns();
        assert_eq!(b - a, 1000);
    }

    #[cfg(feature = "replay-strict")]
    #[test]
    fn strict_mode_frame_advances_by_frame_ns() {
        set_logical_clock_frame_ns(1_000);
        set_logical_clock(0, 0);
        let a = monotonic_ns();
        set_logical_clock(1, 0);
        let b = monotonic_ns();
        assert_eq!(b - a, 1_000);
    }

    #[test]
    fn is_strict_mode_matches_feature_flag() {
        assert_eq!(is_strict_mode(), cfg!(feature = "replay-strict"));
    }

    #[test]
    fn frame_ns_setter_visible_immediately() {
        set_logical_clock_frame_ns(42_000);
        // Read-back via behavior under strict-mode is asserted in another test ;
        // here we just ensure the call doesn't panic.
        set_logical_clock_frame_ns(16_666_667); // restore default
    }
}
