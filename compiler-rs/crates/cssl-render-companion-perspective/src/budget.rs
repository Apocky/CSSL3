//! § Stage8Budget — deadline-tracker for the companion-perspective pass.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Tracks the per-frame execution cost of Stage-8 against a target
//!   deadline. The orchestrator's KAN-detail-budget pulldown reads from
//!   this tracker to decide whether to throttle salience evaluation in
//!   the next frame.
//!
//! § BUDGET CONSTANTS
//!   - `BUDGET_NS_QUEST3`     = 600_000 ns  (0.6ms ; spec § Stage-8 baseline)
//!   - `BUDGET_NS_VISION_PRO` = 500_000 ns  (0.5ms ; spec § Stage-8 high-end)
//!
//!   These are SOFT budgets — the pass does not hard-stop at deadline.
//!   Instead, the orchestrator's pulldown discipline (Axiom 13) reduces
//!   detail in subsequent frames if the deadline is consistently missed.
//!
//! § COST-MODEL
//!   Stage-8 is intrinsically a SAMPLING pass — it costs ≈ K × (per-cell
//!   salience-eval) where K is the number of attended cells. The
//!   orchestrator selects K to fit budget. This crate's responsibility is
//!   only to TRACK + REPORT cost, not to set K (which is a 05_INTELLIGENCE
//!   policy decision).
//!
//! § ZERO-COST-WHEN-OFF
//!   When the consent-gate refuses, this tracker reports a zero-cost
//!   sample. This is intentional — Stage-8 is gated, and the gate-OFF
//!   case is the dominant case. The pulldown discipline never throttles
//!   based on gate-OFF samples.
//!
//! § STAGE-0 COST-SAMPLING
//!   The render-path records a SYNTHETIC cost-sample proportional to the
//!   number of cells evaluated (`cells_evaluated * 100ns`). This is a
//!   STAGE-0 placeholder ; the production path replaces this with a real
//!   GPU-timer readback once `cssl-host-d3d12 / -vulkan / -metal /
//!   -webgpu` wires up the per-pass timestamp-query mechanism. The
//!   synthetic accounting is sufficient for the orchestrator to verify
//!   pulldown-discipline + cell-count-budget conformance ; it does not
//!   capture GPU-side stalls or contention with concurrent passes.

/// Stage-8 budget ceiling in nanoseconds for Quest-3 (M7 baseline).
pub const BUDGET_NS_QUEST3: u64 = 600_000;

/// Stage-8 budget ceiling in nanoseconds for Vision-Pro (M7 high-end).
pub const BUDGET_NS_VISION_PRO: u64 = 500_000;

/// Maximum number of frames retained for the rolling-cost histogram.
pub const COST_HISTORY_LEN: usize = 16;

/// A nanosecond-precision deadline tracker. Records observed-cost samples
/// + reports whether the rolling average is above budget.
///
/// § MULTI-SAMPLE-DISCIPLINE
///   A single-frame over-budget event is NOT a violation. The pulldown
///   logic looks for K-of-N over-budget events out of the rolling buffer
///   to avoid spurious-throttle on a single GPU-stall.
#[derive(Debug, Clone)]
pub struct Stage8Budget {
    /// Budget ceiling in nanoseconds.
    budget_ns: u64,
    /// Rolling-history buffer of observed-cost samples in nanoseconds.
    /// `history[i]` is overwritten in round-robin via `history_head`.
    history: [u64; COST_HISTORY_LEN],
    /// Next index in `history` to overwrite. Advances mod COST_HISTORY_LEN.
    history_head: usize,
    /// True iff `history` has been filled at least once. Until then,
    /// rolling-average computations only use the `history_head` prefix.
    history_filled: bool,
}

impl Stage8Budget {
    /// Construct with a per-platform default deadline.
    #[must_use]
    pub fn quest3() -> Self {
        Self::with_budget_ns(BUDGET_NS_QUEST3)
    }

    /// Construct with the Vision-Pro deadline.
    #[must_use]
    pub fn vision_pro() -> Self {
        Self::with_budget_ns(BUDGET_NS_VISION_PRO)
    }

    /// Construct with an explicit budget.
    #[must_use]
    pub fn with_budget_ns(budget_ns: u64) -> Self {
        Self {
            budget_ns,
            history: [0; COST_HISTORY_LEN],
            history_head: 0,
            history_filled: false,
        }
    }

    /// Read the configured budget ceiling.
    #[must_use]
    pub fn budget_ns(&self) -> u64 {
        self.budget_ns
    }

    /// Record a cost sample for this frame. The orchestrator calls this
    /// once after Stage-8 completes ; gate-OFF frames record 0.
    pub fn record_cost(&mut self, ns: u64) {
        self.history[self.history_head] = ns;
        self.history_head = (self.history_head + 1) % COST_HISTORY_LEN;
        if self.history_head == 0 {
            self.history_filled = true;
        }
    }

    /// Number of samples in the rolling buffer.
    #[must_use]
    pub fn sample_count(&self) -> usize {
        if self.history_filled {
            COST_HISTORY_LEN
        } else {
            self.history_head
        }
    }

    /// Mean cost over the rolling buffer (or 0 if empty).
    #[must_use]
    pub fn mean_cost_ns(&self) -> u64 {
        let n = self.sample_count();
        if n == 0 {
            return 0;
        }
        let sum: u64 = self.history.iter().take(n).sum();
        sum / (n as u64)
    }

    /// Maximum-observed cost in the rolling buffer.
    #[must_use]
    pub fn max_cost_ns(&self) -> u64 {
        let n = self.sample_count();
        if n == 0 {
            return 0;
        }
        *self.history.iter().take(n).max().unwrap_or(&0)
    }

    /// Number of samples in the buffer that exceeded the budget. The
    /// pulldown logic uses this as the K-of-N counter.
    #[must_use]
    pub fn over_budget_count(&self) -> u32 {
        let n = self.sample_count();
        let mut c = 0_u32;
        for i in 0..n {
            if self.history[i] > self.budget_ns {
                c += 1;
            }
        }
        c
    }

    /// True iff the rolling average is above budget.
    #[must_use]
    pub fn mean_over_budget(&self) -> bool {
        self.mean_cost_ns() > self.budget_ns
    }

    /// True iff strict K-of-N over-budget — defaults to ≥ K out of the
    /// last N samples. Used by the orchestrator's pulldown trigger.
    #[must_use]
    pub fn k_of_n_over_budget(&self, k: u32) -> bool {
        self.over_budget_count() >= k
    }

    /// Reset the rolling buffer (used by tests + by the orchestrator
    /// when the player rebinds the toggle key).
    pub fn reset(&mut self) {
        self.history = [0; COST_HISTORY_LEN];
        self.history_head = 0;
        self.history_filled = false;
    }
}

impl Default for Stage8Budget {
    fn default() -> Self {
        Self::quest3()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quest3_budget_is_six_hundred_micros() {
        let b = Stage8Budget::quest3();
        assert_eq!(b.budget_ns(), 600_000);
    }

    #[test]
    fn vision_pro_budget_is_five_hundred_micros() {
        let b = Stage8Budget::vision_pro();
        assert_eq!(b.budget_ns(), 500_000);
    }

    #[test]
    fn fresh_budget_has_zero_samples() {
        let b = Stage8Budget::quest3();
        assert_eq!(b.sample_count(), 0);
        assert_eq!(b.mean_cost_ns(), 0);
        assert_eq!(b.max_cost_ns(), 0);
        assert_eq!(b.over_budget_count(), 0);
    }

    #[test]
    fn record_cost_advances_sample_count() {
        let mut b = Stage8Budget::quest3();
        b.record_cost(100_000);
        b.record_cost(200_000);
        b.record_cost(50_000);
        assert_eq!(b.sample_count(), 3);
    }

    #[test]
    fn mean_cost_is_arithmetic_mean() {
        let mut b = Stage8Budget::quest3();
        b.record_cost(100);
        b.record_cost(200);
        b.record_cost(300);
        // (100 + 200 + 300) / 3 = 200.
        assert_eq!(b.mean_cost_ns(), 200);
    }

    #[test]
    fn max_cost_is_max() {
        let mut b = Stage8Budget::quest3();
        b.record_cost(100);
        b.record_cost(900);
        b.record_cost(500);
        assert_eq!(b.max_cost_ns(), 900);
    }

    #[test]
    fn over_budget_count_filters_above_budget() {
        let mut b = Stage8Budget::with_budget_ns(500);
        b.record_cost(100);
        b.record_cost(600);
        b.record_cost(200);
        b.record_cost(700);
        assert_eq!(b.over_budget_count(), 2);
    }

    #[test]
    fn mean_over_budget_predicate() {
        let mut b = Stage8Budget::with_budget_ns(100);
        b.record_cost(50);
        b.record_cost(60);
        assert!(!b.mean_over_budget());
        b.record_cost(500);
        b.record_cost(500);
        assert!(b.mean_over_budget());
    }

    #[test]
    fn k_of_n_over_budget_threshold() {
        let mut b = Stage8Budget::with_budget_ns(100);
        for _ in 0..3 {
            b.record_cost(1000); // over
        }
        for _ in 0..3 {
            b.record_cost(50); // under
        }
        assert!(b.k_of_n_over_budget(2));
        assert!(b.k_of_n_over_budget(3));
        assert!(!b.k_of_n_over_budget(4));
    }

    #[test]
    fn reset_clears_history() {
        let mut b = Stage8Budget::quest3();
        b.record_cost(1000);
        b.record_cost(2000);
        b.reset();
        assert_eq!(b.sample_count(), 0);
        assert_eq!(b.mean_cost_ns(), 0);
    }

    #[test]
    fn rolling_buffer_overwrites_after_full() {
        let mut b = Stage8Budget::with_budget_ns(100);
        for i in 0..(COST_HISTORY_LEN * 2) {
            b.record_cost(i as u64);
        }
        assert_eq!(b.sample_count(), COST_HISTORY_LEN);
        // The oldest samples (0..16) should have been overwritten by 16..32.
        // Mean should reflect 16..32.
        let expected_sum: u64 =
            ((COST_HISTORY_LEN..(COST_HISTORY_LEN * 2)).map(|x| x as u64)).sum();
        let expected_mean = expected_sum / (COST_HISTORY_LEN as u64);
        assert_eq!(b.mean_cost_ns(), expected_mean);
    }
}
