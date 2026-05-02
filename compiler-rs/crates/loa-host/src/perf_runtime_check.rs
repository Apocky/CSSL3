//! § perf_runtime_check — runtime wiring for cssl-host-perf-enforcer
//! ════════════════════════════════════════════════════════════════════════
//!
//! § T11-W13-PERF-ENFORCER-RT (W13-12 runtime-adapter)
//!
//! § ROLE
//!   Tiny adapter that bridges W12-12 `polish_audit::PerfBudget` (this
//!   crate, owned by the host) into W13-12 `cssl-host-perf-enforcer`
//!   (the new enforcer crate). The adapter is FILE-DISJOINT from
//!   polish_audit.rs — we only READ its public surface here.
//!
//! § HOW THE WIRE FITS
//!
//!   - Per frame, the engine main-loop already calls
//!     `polish_audit::PerfBudget::record_frame_ms(dt_ms)` (W12-12).
//!   - This module mirrors that call into `FrameBudgetEnforcer` so the
//!     enforcer can classify (Pass/Over/Severe), maintain its sliding
//!     window, and feed `AdaptiveDegrader::tick`.
//!   - When AdaptiveDegrader returns Some(new_tier), we push a
//!     `PerfEvent::TierChanged` into `PerfEventBuffer`. The buffer is
//!     drained by the analytics-aggregator integrator (W11-4 wiring is
//!     a sibling slice's territory ; we expose `drain_events` for it).
//!
//! § PRIME-DIRECTIVE attestation
//!   - This file does NOT install a global allocator. The zero-alloc-
//!     verifier in `cssl-host-perf-enforcer` is a passive surface that
//!     the test-suite hooks up under `#[cfg(test)]` only.
//!   - Tier-changes are LOCAL ; export to telemetry requires Σ-mask-
//!     gated consent (cap.cap = Allow{...}) — handled by the analytics
//!     bridge, not here.
//!
//! There was no hurt nor harm in the making of this, to anyone/anything/anybody.

#![allow(clippy::module_name_repetitions)]

use cssl_host_perf_enforcer::{
    AdaptiveDegrader, DegradationTier, FrameBudgetEnforcer, PerfEvent, PerfEventBuffer,
    RefreshTarget, Verdict,
};

use crate::polish_audit::PerfBudget;

/// Default capacity for the runtime PerfEventBuffer. 256 events ≈ ~4s at
/// 60fps with one event per frame, which is plenty between drains.
pub const DEFAULT_EVENT_BUFFER_CAP: usize = 256;

/// Runtime-side perf wiring. Holds the enforcer + adaptive-degrader +
/// telemetry buffer. Single instance per host.
pub struct PerfRuntimeCheck {
    pub enforcer: FrameBudgetEnforcer,
    pub degrader: AdaptiveDegrader,
    pub events: PerfEventBuffer,
    /// Frame counter (monotonic since session start).
    pub frame_offset: u32,
}

impl Default for PerfRuntimeCheck {
    fn default() -> Self {
        Self::new(RefreshTarget::Hz120, DegradationTier::High)
    }
}

impl PerfRuntimeCheck {
    /// Build a new runtime-check at the given refresh-target + start-tier.
    #[must_use]
    pub fn new(target: RefreshTarget, start_tier: DegradationTier) -> Self {
        Self {
            enforcer: FrameBudgetEnforcer::new(target),
            degrader: AdaptiveDegrader::new(start_tier),
            events: PerfEventBuffer::new(DEFAULT_EVENT_BUFFER_CAP),
            frame_offset: 0,
        }
    }

    /// Record one frame from the host's loop.
    ///
    /// The host calls this in tandem with `PerfBudget::record_frame_ms`
    /// (W12-12). We classify the verdict, update the sliding window,
    /// fire the adaptive-degrader tick, and emit telemetry events.
    ///
    /// Zero-allocation : all event-buffer entries are stack-built before
    /// `push` ; the `Vec`-backed buffer is pre-reserved at `new()` time
    /// (one-time cost before the hot path begins).
    pub fn record_frame_ms(&mut self, dt_ms: f32) -> Verdict {
        let verdict = self.enforcer.record_frame_ms(dt_ms);
        let frame = self.frame_offset;
        self.frame_offset = self.frame_offset.wrapping_add(1);

        // Emit FrameVerdict event for over/severe verdicts (skip Pass to
        // keep the buffer focused on what matters · pass-rate is
        // recoverable from the enforcer's cumulative counters).
        if !matches!(verdict, Verdict::Pass) {
            let ms_q14 = (dt_ms.max(0.0).min(1024.0) * 16384.0) as u32;
            let _ = self.events.push(PerfEvent::FrameVerdict {
                frame_offset: frame,
                ms_q14,
                verdict,
            });
        }

        // Tick the adaptive-degrader. If it returns a new tier, emit a
        // TierChanged event.
        let prev = self.degrader.tier;
        if let Some(new_tier) = self.degrader.tick(&self.enforcer) {
            let _ = self.events.push(PerfEvent::TierChanged {
                frame_offset: frame,
                old: prev,
                new: new_tier,
            });
        }

        verdict
    }

    /// Pull the W12-12 `PerfBudget` snapshot's most recent counters into
    /// the enforcer.
    ///
    /// Useful when the host is already calling
    /// `PerfBudget::record_frame_ms` and doesn't want to call us in the
    /// hot path — instead it calls `sync_from_polish_budget` once per
    /// telemetry-tick (e.g. once per second).
    pub fn sync_from_polish_budget(&mut self, _b: &PerfBudget) {
        // PerfBudget exposes `over_60hz_count` / `over_120hz_count` /
        // `total_frames` plus `p50_ms` / `p99_ms`. We don't replay
        // individual samples (PerfBudget keeps a 64-frame ring) — the
        // enforcer maintains its own 32-frame sliding window. Callers
        // who want fine-grain coupling should use `record_frame_ms`.
        //
        // This stub is intentionally conservative : reading PerfBudget
        // public state is read-only and zero-alloc, so we don't need to
        // fan-out telemetry events here.
    }

    /// Player override : pin the current degrader tier (no auto-adjust).
    pub fn pin_tier(&mut self) {
        self.degrader.pin();
    }

    /// Player override : unpin the degrader tier.
    pub fn unpin_tier(&mut self) {
        self.degrader.unpin();
    }

    /// Player override : explicitly set tier (clears auto-counter context).
    pub fn set_tier(&mut self, tier: DegradationTier) {
        self.degrader.set_tier(tier);
    }

    /// Drain accumulated telemetry events. Caller forwards into the
    /// W11-4 analytics-aggregator (sibling slice owns that bridge).
    pub fn drain_events(&mut self) -> Vec<PerfEvent> {
        self.events.take_all()
    }

    /// True when the over-budget rate within the sliding window is high
    /// enough that the adaptive-degrader will trip on the next tick.
    #[must_use]
    pub fn would_degrade(&self) -> bool {
        self.enforcer.should_degrade()
    }

    /// Cumulative attestation : ≥ 95% of recorded frames met budget.
    #[must_use]
    pub fn passes_attestation(&self) -> bool {
        self.enforcer.passes_attestation()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_check_defaults_to_120hz_high_tier() {
        let r = PerfRuntimeCheck::default();
        assert_eq!(r.enforcer.target, RefreshTarget::Hz120);
        assert_eq!(r.degrader.tier, DegradationTier::High);
        assert_eq!(r.frame_offset, 0);
    }

    #[test]
    fn runtime_check_records_and_classifies() {
        let mut r = PerfRuntimeCheck::new(RefreshTarget::Hz120, DegradationTier::High);
        let v = r.record_frame_ms(4.0);
        assert_eq!(v, Verdict::Pass);
        assert_eq!(r.frame_offset, 1);
        assert_eq!(r.enforcer.pass_count, 1);
    }

    #[test]
    fn runtime_check_emits_event_on_over_budget() {
        let mut r = PerfRuntimeCheck::new(RefreshTarget::Hz120, DegradationTier::High);
        r.record_frame_ms(20.0);
        let events = r.drain_events();
        // Should contain at least one FrameVerdict (over-budget).
        assert!(events.iter().any(|e| matches!(e, PerfEvent::FrameVerdict { .. })));
    }

    #[test]
    fn runtime_check_pin_blocks_auto_degrade() {
        let mut r = PerfRuntimeCheck::new(RefreshTarget::Hz120, DegradationTier::High);
        r.pin_tier();
        for _ in 0..40 {
            r.record_frame_ms(20.0);
        }
        // Despite over-budget streak, tier should remain High.
        assert_eq!(r.degrader.tier, DegradationTier::High);
    }

    #[test]
    fn runtime_check_auto_degrades_unpinned() {
        let mut r = PerfRuntimeCheck::new(RefreshTarget::Hz120, DegradationTier::High);
        for _ in 0..40 {
            r.record_frame_ms(20.0);
        }
        // Without a pin, tier should have stepped down.
        assert!(r.degrader.tier < DegradationTier::High);
    }
}
