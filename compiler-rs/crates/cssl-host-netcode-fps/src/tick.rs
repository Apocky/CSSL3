// § tick.rs : TickId + monotonic-tick math + ring-index helpers
//
// Networking timeline is sliced into integer ticks ; the wire-protocol carries
// `TickId` not wall-clock-microseconds because rollback / reconciliation /
// lag-comp all need cheap monotonic comparison + modulo-ring indexing.
//
// Tick rate is FIXED at construction (typical : 60 Hz = 16.667 ms ; competitive
// FPS : 128 Hz = 7.81 ms). Wall-clock conversion lives in `from_micros` /
// `to_micros` so callers can timestamp inputs at native resolution and still
// align to a tick.
//
// ─ Sawyer/Pokémon-OG : TickId = u32 (≥ 800-day budget @ 60 Hz · ≥ 388-day @ 128 Hz)
//   wraparound is well-defined via wrapping-arithmetic ; sliding-window compares
//   use signed-delta to handle wrap correctly.

use serde::{Deserialize, Serialize};

/// Default simulation tick-rate (Hz). 60 = console-FPS-baseline.
pub const DEFAULT_TICK_RATE_HZ: u32 = 60;
/// Competitive-FPS tick-rate (Hz). 128 = CS / Valorant target.
pub const COMPETITIVE_TICK_RATE_HZ: u32 = 128;

/// Monotonic per-session tick identifier. Wraps cheaply via `wrapping_*`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct TickId(pub u32);

impl TickId {
    /// Tick-zero ; session genesis.
    pub const ZERO: TickId = TickId(0);

    /// Advance by one tick (wrapping).
    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// Advance by `n` ticks (wrapping).
    #[must_use]
    pub fn forward(self, n: u32) -> Self {
        Self(self.0.wrapping_add(n))
    }

    /// Rewind by `n` ticks (wrapping).
    #[must_use]
    pub fn back(self, n: u32) -> Self {
        Self(self.0.wrapping_sub(n))
    }

    /// Signed delta from `self` to `other` ; correct across wraparound when
    /// the gap is < 2^31. Positive = `other` is in the future of `self`.
    #[must_use]
    pub fn delta(self, other: TickId) -> i32 {
        // wrap-correct via cast-through-i32
        other.0.wrapping_sub(self.0) as i32
    }

    /// Index into a ring buffer of size `cap` (must be power-of-two for the
    /// fast path ; falls back to `%` otherwise).
    #[must_use]
    pub fn ring_index(self, cap: usize) -> usize {
        if cap.is_power_of_two() {
            (self.0 as usize) & (cap - 1)
        } else {
            (self.0 as usize) % cap.max(1)
        }
    }
}

impl core::fmt::Display for TickId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "t#{}", self.0)
    }
}

/// Convert microseconds-since-epoch to a `TickId` at `rate_hz`.
#[must_use]
pub fn micros_to_tick(micros: u64, rate_hz: u32) -> TickId {
    let ticks_per_sec = u64::from(rate_hz.max(1));
    let t = (micros.saturating_mul(ticks_per_sec)) / 1_000_000;
    TickId(t as u32)
}

/// Convert a `TickId` back to microseconds-since-epoch at `rate_hz`.
#[must_use]
pub fn tick_to_micros(t: TickId, rate_hz: u32) -> u64 {
    let ticks_per_sec = u64::from(rate_hz.max(1));
    (u64::from(t.0).saturating_mul(1_000_000)) / ticks_per_sec.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_wraps_correctly() {
        let a = TickId(u32::MAX - 2);
        let b = TickId(3); // wrapped past max
        assert_eq!(a.delta(b), 6, "delta should be +6 across wrap");
        assert_eq!(b.delta(a), -6, "reverse delta should be -6");
    }

    #[test]
    fn ring_index_pow2_fast_path() {
        let cap = 64;
        assert_eq!(TickId(127).ring_index(cap), 63);
        assert_eq!(TickId(64).ring_index(cap), 0);
    }

    #[test]
    fn ring_index_non_pow2_fallback() {
        let cap = 60; // 60 Hz @ 1 sec — not power of two
        assert_eq!(TickId(60).ring_index(cap), 0);
        assert_eq!(TickId(61).ring_index(cap), 1);
        assert_eq!(TickId(119).ring_index(cap), 59);
    }

    #[test]
    fn micros_round_trip_60hz() {
        let t = TickId(120); // 2 sec
        let m = tick_to_micros(t, 60);
        assert_eq!(m, 2_000_000);
        let back = micros_to_tick(m, 60);
        assert_eq!(back, t);
    }

    #[test]
    fn micros_round_trip_128hz() {
        let t = TickId(128); // 1 sec
        let m = tick_to_micros(t, 128);
        assert!(
            (999_000..=1_001_000).contains(&m),
            "round-trip within 1ms tolerance"
        );
    }
}
