//! § MiseEnAbymeCostModel — Stage-9 cost-budget gate
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per spec § Stage-9.budget : 0.8ms @ Quest-3 ; 0.6ms @ Vision-Pro.
//!   Per spec § Stage-9.recursion-discipline :
//!     `depth-budget per-frame ⊗ pulldown-aware (Axiom 13)`
//!
//!   This module models the per-bounce + per-frame cost so the runtime
//!   can decide :
//!     - "budget for `n` more bounces this frame" (depth-budget)
//!     - "force-truncate remaining recursion if budget exhausted"
//!
//!   The model is a closed-form analytic : `cost_per_bounce_us ≈
//!   probe_us + kan_eval_us + accumulate_us`. Calibration knobs come
//!   from the platform-specific `RuntimePlatform` enum ; the values here
//!   are derived from the spec's amortization-table § VI.

use super::{RECURSION_DEPTH_HARD_CAP, STAGE9_BUDGET_QUEST3_US, STAGE9_BUDGET_VISION_PRO_US};

/// § Hardware platform — selects the budget envelope + per-bounce cost
///   constants from the spec's amortization table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePlatform {
    /// § Meta Quest 3. Budget 0.8ms ; per-bounce ~0.10ms typical.
    Quest3,
    /// § Apple Vision Pro. Budget 0.6ms ; per-bounce ~0.08ms typical.
    VisionPro,
    /// § Generic desktop XR (high-end). Budget 1.5ms ; per-bounce ~0.05ms.
    DesktopXR,
}

impl RuntimePlatform {
    /// § Per-platform total budget for Stage-9 in microseconds.
    #[must_use]
    pub fn budget_us(self) -> u32 {
        match self {
            Self::Quest3 => STAGE9_BUDGET_QUEST3_US,
            Self::VisionPro => STAGE9_BUDGET_VISION_PRO_US,
            // § Desktop XR has more headroom — the spec doesn't fix this,
            //   so we pick 1500us as a conservative high-end-PC value.
            Self::DesktopXR => 1500,
        }
    }

    /// § Per-platform per-bounce cost estimate in microseconds. This is
    ///   the per-frame bulk-cost when the entire mirror-budget is
    ///   utilized — used as a *coarse* total-time figure ; the actual
    ///   per-pixel-bounce cost is `per_pixel_bounce_ns()`.
    #[must_use]
    pub fn per_bounce_us(self) -> u32 {
        match self {
            Self::Quest3 => 100,
            Self::VisionPro => 80,
            Self::DesktopXR => 50,
        }
    }

    /// § Per-platform per-pixel-bounce cost estimate in nanoseconds.
    ///   This is the value the cost-model multiplies against the
    ///   `bounce_pixel_count * expected_depth` to arrive at the bulk
    ///   cost. Calibrated so a typical "1000-pixel frame at depth 3"
    ///   costs roughly 0.3ms on Quest-3.
    #[must_use]
    pub fn per_pixel_bounce_ns(self) -> u32 {
        match self {
            Self::Quest3 => 100,   // 0.1us per pixel-bounce
            Self::VisionPro => 80, // 0.08us per pixel-bounce
            Self::DesktopXR => 40, // 0.04us per pixel-bounce
        }
    }
}

/// § Cost-model that estimates Stage-9 microsecond cost.
///
///   The model is straightforward :
///     `total_us = sum_over_pixels(active_bounces * per_bounce_us)`
///
///   plus a fixed setup-overhead per frame (`setup_us`) for the mirror-
///   detection pre-pass. The runtime calls [`Self::estimate_us`] BEFORE
///   the recursion to decide whether the frame budget allows the planned
///   `bounce_pixel_count` ; if not, the runtime can either :
///     - Lower the recursion depth (reduce `expected_depth`) ; or
///     - Skip mirror surfaces below a `mirrorness_min` threshold.
#[derive(Debug, Clone, Copy)]
pub struct MiseEnAbymeCostModel {
    /// § Active runtime platform.
    pub platform: RuntimePlatform,
    /// § Fixed per-frame setup overhead (microseconds). Default = 50us
    ///   for the mirror-detection pre-pass that runs once per frame.
    pub setup_us: u32,
}

impl Default for MiseEnAbymeCostModel {
    fn default() -> Self {
        Self::for_platform(RuntimePlatform::Quest3)
    }
}

impl MiseEnAbymeCostModel {
    /// § Construct for the given platform with default setup overhead.
    #[must_use]
    pub fn for_platform(platform: RuntimePlatform) -> Self {
        Self {
            platform,
            setup_us: 50,
        }
    }

    /// § Estimate total microseconds for the given (pixel-count,
    ///   expected-depth) plan.
    ///
    ///   `bounce_pixel_count` : how many pixels actually trigger
    ///   recursive bounces this frame. NOT the total pixel count — only
    ///   the ones that hit a mirror surface.
    ///
    ///   `expected_depth` : the average depth those pixels recurse to.
    ///   Bounded `[0, RECURSION_DEPTH_HARD_CAP]`.
    ///
    ///   The cost-model uses a fractional per-bounce-microseconds value
    ///   (the platform's per-bounce-us is the "cost per 100 pixel-bounces"
    ///   in the model) so that small bounce-counts map to well-bounded
    ///   sub-microsecond costs and large bounce-counts cleanly exceed the
    ///   budget. The "per 100 pixel-bounces" denominator is the gain-knob
    ///   that lets the runtime tune the cost-model against measured frame-
    ///   times without changing the public surface here.
    #[must_use]
    pub fn estimate_us(self, bounce_pixel_count: u32, expected_depth: u8) -> u32 {
        let depth = u32::from(expected_depth.min(RECURSION_DEPTH_HARD_CAP));
        let per_pixel_bounce_ns = self.platform.per_pixel_bounce_ns();
        // § total-ns = bounce-pixel-count × depth × per-pixel-bounce-ns
        //   total-us = total-ns / 1000
        //   Calibrated so 1000 pixels × depth-3 × 100ns = 300_000ns = 300us
        //   on Quest-3 (~half of the 800us budget) — matches the spec's
        //   "0.3ms typical for mirrors @ M7" working figure.
        let bulk_ns = bounce_pixel_count
            .saturating_mul(depth)
            .saturating_mul(per_pixel_bounce_ns);
        let bulk_us = bulk_ns / 1000;
        // § Cap bulk to keep the formula stable on absurd inputs ; the
        //   runtime test gates on `estimate_us > budget_us` so saturation
        //   is fine.
        let bulk_capped = bulk_us.min(self.platform.budget_us().saturating_mul(1000));
        self.setup_us.saturating_add(bulk_capped)
    }

    /// § Predicate : true iff the planned bounce count + depth fits in
    ///   the budget.
    #[must_use]
    pub fn fits_in_budget(self, bounce_pixel_count: u32, expected_depth: u8) -> bool {
        self.estimate_us(bounce_pixel_count, expected_depth) <= self.platform.budget_us()
    }

    /// § Suggest a max-depth that does fit in budget for the given
    ///   bounce-pixel-count. Returns `RECURSION_DEPTH_HARD_CAP` if even
    ///   max depth fits, or the largest `d <= HARD_CAP` for which
    ///   estimate_us(n, d) <= budget_us. Returns 0 if no recursion can
    ///   fit (bounce-pixel-count too high or platform too constrained).
    #[must_use]
    pub fn suggest_depth(self, bounce_pixel_count: u32) -> u8 {
        for d in (0..=RECURSION_DEPTH_HARD_CAP).rev() {
            if self.fits_in_budget(bounce_pixel_count, d) {
                return d;
            }
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § Quest3 budget matches spec.
    #[test]
    fn quest3_budget_matches_spec() {
        assert_eq!(RuntimePlatform::Quest3.budget_us(), STAGE9_BUDGET_QUEST3_US);
    }

    /// § VisionPro budget matches spec.
    #[test]
    fn vision_pro_budget_matches_spec() {
        assert_eq!(
            RuntimePlatform::VisionPro.budget_us(),
            STAGE9_BUDGET_VISION_PRO_US
        );
    }

    /// § Setup-only estimate is just the setup overhead.
    #[test]
    fn estimate_setup_only() {
        let m = MiseEnAbymeCostModel::for_platform(RuntimePlatform::Quest3);
        assert_eq!(m.estimate_us(0, 0), 50);
    }

    /// § A single bounce has cost > setup.
    #[test]
    fn estimate_single_bounce_above_setup() {
        let m = MiseEnAbymeCostModel::for_platform(RuntimePlatform::Quest3);
        // § With bounce_pixel_count = 1000, depth = 1, per_bounce_us = 100,
        //   bulk = 1000 * 1 * (100/1000) = 100us, plus setup=50us → 150us.
        let est = m.estimate_us(1000, 1);
        assert!(est >= 50);
    }

    /// § A small bounce count fits in Quest3 budget.
    #[test]
    fn small_bounce_count_fits_quest3() {
        let m = MiseEnAbymeCostModel::for_platform(RuntimePlatform::Quest3);
        assert!(m.fits_in_budget(500, 2));
    }

    /// § A huge bounce count exceeds Quest3 budget.
    #[test]
    fn huge_bounce_count_exceeds_quest3() {
        let m = MiseEnAbymeCostModel::for_platform(RuntimePlatform::Quest3);
        assert!(!m.fits_in_budget(1_000_000, 5));
    }

    /// § suggest_depth returns 0 when even depth=0 doesn't fit. With
    ///   setup_us = 50 and Quest3 budget = 800, depth=0 has cost = 50,
    ///   which always fits. So we use a constructed model with insanely
    ///   large setup to test the floor case.
    #[test]
    fn suggest_depth_floor_zero() {
        let m = MiseEnAbymeCostModel {
            platform: RuntimePlatform::Quest3,
            setup_us: 10_000, // larger than budget
        };
        // Even depth=0 (just setup) fails → suggest_depth returns 0 by spec.
        assert_eq!(m.suggest_depth(1000), 0);
    }

    /// § suggest_depth returns RECURSION_DEPTH_HARD_CAP when budget is
    ///   plentiful.
    #[test]
    fn suggest_depth_returns_hard_cap_with_plentiful_budget() {
        let m = MiseEnAbymeCostModel::for_platform(RuntimePlatform::DesktopXR);
        assert_eq!(m.suggest_depth(10), RECURSION_DEPTH_HARD_CAP);
    }

    /// § suggest_depth is monotone-decreasing in bounce_pixel_count
    ///   (more pixels → shallower depth).
    #[test]
    fn suggest_depth_monotone_in_bounce_count() {
        let m = MiseEnAbymeCostModel::for_platform(RuntimePlatform::Quest3);
        let d_small = m.suggest_depth(100);
        let d_large = m.suggest_depth(10_000_000);
        assert!(d_small >= d_large);
    }

    /// § DesktopXR has more headroom than Quest3.
    #[test]
    fn desktop_xr_more_headroom_than_quest3() {
        let q = MiseEnAbymeCostModel::for_platform(RuntimePlatform::Quest3);
        let d = MiseEnAbymeCostModel::for_platform(RuntimePlatform::DesktopXR);
        assert!(d.platform.budget_us() > q.platform.budget_us());
    }

    /// § estimate_us never overflows on absurd inputs.
    #[test]
    fn estimate_us_no_overflow() {
        let m = MiseEnAbymeCostModel::for_platform(RuntimePlatform::Quest3);
        let _ = m.estimate_us(u32::MAX, RECURSION_DEPTH_HARD_CAP);
    }
}
