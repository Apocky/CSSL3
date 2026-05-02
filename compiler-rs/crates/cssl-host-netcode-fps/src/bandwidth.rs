// § bandwidth.rs : adaptive per-client bandwidth budget
//
// Each connected client has a target send-rate based on observed network
// conditions. The budget tells `NetcodeServer` whether to ship a delta this
// tick or coalesce. We bound floor (32 kbps minimum, never starve) and target
// (256 kbps default — broadband-comfortable, console-FPS-norm).
//
// AIMD-style adaptive : bandwidth grows linearly when packet-loss < 1%,
// halves on packet-loss > 5%. No surveillance — we observe loss only via
// caller-reported counters, not deep-packet-inspection.

use serde::{Deserialize, Serialize};

/// Floor : never throttle below this (kbps).
pub const MIN_KBPS: u32 = 32;
/// Default target bandwidth (kbps).
pub const TARGET_KBPS: u32 = 256;
/// Maximum bandwidth ceiling (kbps).
pub const MAX_KBPS: u32 = 2048;

/// Per-client budget tracker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthBudget {
    /// Current target (kbps).
    pub current_kbps: u32,
    /// Bytes sent in the current 1-second window.
    pub bytes_this_window: u32,
    /// Elapsed micros in current window.
    pub window_micros: u64,
    /// Loss-window observed (0..1000 = per-mille).
    pub recent_loss_pml: u32,
}

impl Default for BandwidthBudget {
    fn default() -> Self {
        Self::new(TARGET_KBPS)
    }
}

impl BandwidthBudget {
    #[must_use]
    pub fn new(initial_kbps: u32) -> Self {
        Self {
            current_kbps: initial_kbps.clamp(MIN_KBPS, MAX_KBPS),
            bytes_this_window: 0,
            window_micros: 0,
            recent_loss_pml: 0,
        }
    }

    /// Bytes-per-second budget for the current `current_kbps`.
    #[must_use]
    pub fn bytes_per_sec(&self) -> u32 {
        self.current_kbps.saturating_mul(125) // kbps × 1000 / 8 = bytes/s
    }

    /// May we send `bytes` this tick ? Tracks usage in the rolling window.
    pub fn may_send(&mut self, bytes: u32, dt_micros: u64) -> bool {
        self.window_micros = self.window_micros.saturating_add(dt_micros);
        if self.window_micros >= 1_000_000 {
            self.window_micros = 0;
            self.bytes_this_window = 0;
        }
        let cap = self.bytes_per_sec();
        let next = self.bytes_this_window.saturating_add(bytes);
        if next > cap {
            return false;
        }
        self.bytes_this_window = next;
        true
    }

    /// AIMD : grow linearly on low-loss, multiplicative-decrease on loss.
    /// Call once per second-window with the latest observed packet-loss
    /// in per-mille (parts per thousand).
    pub fn observe_loss(&mut self, loss_pml: u32) {
        self.recent_loss_pml = loss_pml;
        if loss_pml > 50 {
            // > 5% loss → multiplicative decrease (halve, floored).
            let halved = self.current_kbps / 2;
            self.current_kbps = halved.max(MIN_KBPS);
        } else if loss_pml < 10 {
            // < 1% loss → additive increase (32 kbps step, capped).
            let bumped = self.current_kbps.saturating_add(32);
            self.current_kbps = bumped.min(MAX_KBPS);
        }
        // Else : steady-state hold.
    }

    /// Compression ratio : delta_bytes vs full_bytes (where full > 0).
    /// Returns Q-fraction in [0, 1] as Q16.16. Useful for tests + telemetry.
    #[must_use]
    pub fn compression_ratio(delta_bytes: usize, full_bytes: usize) -> u32 {
        if full_bytes == 0 {
            return 0;
        }
        ((delta_bytes as u64 * 65536) / full_bytes as u64) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_floor_holds() {
        let mut b = BandwidthBudget::new(MIN_KBPS);
        for _ in 0..10 {
            b.observe_loss(1000); // 100% loss
        }
        assert_eq!(b.current_kbps, MIN_KBPS);
    }

    #[test]
    fn high_loss_halves() {
        let mut b = BandwidthBudget::new(512);
        b.observe_loss(100); // 10% loss > 5% threshold
        assert_eq!(b.current_kbps, 256);
    }

    #[test]
    fn low_loss_grows() {
        let mut b = BandwidthBudget::new(256);
        b.observe_loss(0);
        assert_eq!(b.current_kbps, 288);
    }

    #[test]
    fn max_ceiling_holds() {
        let mut b = BandwidthBudget::new(MAX_KBPS);
        for _ in 0..10 {
            b.observe_loss(0);
        }
        assert_eq!(b.current_kbps, MAX_KBPS);
    }

    #[test]
    fn budget_blocks_oversend() {
        let mut b = BandwidthBudget::new(MIN_KBPS); // 32 kbps = 4000 B/s
        let dt = 100_000u64; // 0.1 sec window-progress
        assert!(b.may_send(1000, dt));
        assert!(b.may_send(1000, dt));
        assert!(b.may_send(1000, dt));
        assert!(b.may_send(900, dt));
        // We've shipped 3900 / 4000. 200 bytes pushes us over → deny.
        assert!(!b.may_send(200, dt));
    }

    #[test]
    fn compression_ratio_q16() {
        // 250 / 1000 = 0.25 = 16384 Q16.16
        assert_eq!(BandwidthBudget::compression_ratio(250, 1000), 16384);
        assert_eq!(BandwidthBudget::compression_ratio(0, 1000), 0);
        assert_eq!(BandwidthBudget::compression_ratio(100, 0), 0);
    }
}
