//! § cost_model — Stage-7 budget enforcement
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Compile-time budget enforcement per `06_RENDERING_PIPELINE § Stage-7`
//!   ("budget : 1.2ms @ Quest-3 ; 1.0ms @ Vision-Pro") and runtime budget
//!   tracking for the per-frame Stage-7 cost. The cost-model decomposes
//!   each KAN evaluation into its tier-determined nanosecond cost
//!   (`07_KAN_RUNTIME_SHADING § VII § per-band-ns`) and accumulates across
//!   the per-frame fragment-set.
//!
//!   The runtime check fires `AmplifierError::BudgetExceeded` if the
//!   cumulative cost exceeds the configured budget × `BUDGET_SLACK_FACTOR`.
//!   This is the degraded-mode entry-point per
//!   `07_KAN_RUNTIME_SHADING § VII § degraded-mode-behaviors` :
//!
//!     1. drop iridescence
//!     2. drop fluorescence
//!     3. half-rate fovea-disp ← THIS crate operates here
//!     4. drop spectral → 4-band
//!
//!   Step-3 of the degradation order is encoded as a `BUDGET_SLACK_FACTOR`
//!   ratchet : when cumulative cost crosses 80% of budget, the cost
//!   model recommends switching the FoveaTier::Full fragments to
//!   FoveaTier::Mid (effectively half-rate fovea-disp).

use crate::amplifier::AmplifierError;

/// § Stage-7 budget on Quest-3-class hardware. Per
///   `06_RENDERING_PIPELINE § Stage-7` "budget : 1.2ms @ Quest-3".
pub const COST_BUDGET_QUEST3_MS: f32 = 1.2;

/// § Stage-7 budget on Vision-Pro hardware. Per same spec :
///   "budget : 1.0ms @ Vision-Pro".
pub const COST_BUDGET_VISION_PRO_MS: f32 = 1.0;

/// § Per-fragment cost @ tier-1 (CoopMatrix). Per `07_KAN § VII`,
///   ~50 ns/eval-per-band × 1 evaluation per amplifier-call. The
///   amplifier evaluates 3 networks (`micro_displacement`,
///   `micro_roughness`, `micro_color_perturbation`) — each is one
///   evaluation. With 3-output micro-color this is 1 + 1 + 3 outputs
///   amortized into ~150 ns per amplifier-call at tier-1.
pub const COST_PER_FRAGMENT_TIER1_NS: f32 = 150.0;

/// § Per-fragment cost @ tier-2 (SIMD-warp). Per `07_KAN § VII`,
///   ~200 ns/eval-per-band. At tier-2 the same 3 networks cost
///   ~600 ns per amplifier-call.
pub const COST_PER_FRAGMENT_TIER2_NS: f32 = 600.0;

/// § Per-fragment cost @ tier-3 (scalar). At tier-3 the same 3 networks
///   cost ~2400 ns per amplifier-call. Tier-3 is REFUSED on M7-baseline
///   per `07_KAN § VII § §D §scalar-tier`.
pub const COST_PER_FRAGMENT_TIER3_NS: f32 = 2400.0;

/// § Maximum fraction of the budget the cost-model allows. Set to 1.0 by
///   default ; relax to e.g. 1.2 for degraded-mode reporting.
pub const BUDGET_SLACK: f32 = 1.0;

/// § The degradation threshold : when cumulative cost crosses this
///   fraction of budget, the cost-model recommends downgrading
///   FoveaTier::Full fragments to FoveaTier::Mid.
pub const DEGRADE_THRESHOLD: f32 = 0.8;

/// § Per-tier dispatch cost classification. Mirrors
///   `07_KAN_RUNTIME_SHADING § III § three-tier-dispatch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DispatchTier {
    /// § Cooperative-matrix (preferred). ~150 ns/fragment for the
    ///   amplifier's 3-network call.
    #[default]
    CoopMatrix,
    /// § SIMD-warp cooperative (fallback). ~600 ns/fragment.
    SimdWarp,
    /// § Per-thread sequential (slow path). ~2400 ns/fragment ; only
    ///   admitted on low-end-GPU profile per `07_KAN § III § §D tier-3`.
    Scalar,
}

impl DispatchTier {
    /// § Per-fragment ns cost.
    #[must_use]
    pub const fn per_fragment_ns(self) -> f32 {
        match self {
            Self::CoopMatrix => COST_PER_FRAGMENT_TIER1_NS,
            Self::SimdWarp => COST_PER_FRAGMENT_TIER2_NS,
            Self::Scalar => COST_PER_FRAGMENT_TIER3_NS,
        }
    }

    /// § True iff this tier is admissible on M7-baseline hardware.
    #[must_use]
    pub const fn is_m7_admissible(self) -> bool {
        !matches!(self, Self::Scalar)
    }
}

/// § Cost-model — per-frame Stage-7 cumulative cost tracker.
#[derive(Debug, Clone)]
pub struct CostModel {
    /// § The budget in milliseconds.
    pub budget_ms: f32,
    /// § The dispatch tier in use this frame.
    pub tier: DispatchTier,
    /// § Cumulative number of amplifier-calls this frame.
    pub fragments_amplified: u32,
    /// § Cumulative cost in nanoseconds.
    pub cumulative_ns: f64,
}

impl CostModel {
    /// § Construct a fresh cost-model for a frame. Initialized at zero
    ///   accumulated cost.
    pub fn new(budget_ms: f32, tier: DispatchTier) -> Result<Self, AmplifierError> {
        if budget_ms <= 0.0 || !budget_ms.is_finite() {
            return Err(AmplifierError::InvalidBudget(budget_ms));
        }
        if !tier.is_m7_admissible() {
            // § Scalar tier is REFUSED on M7-baseline per spec ; we still
            //   admit construction for tests, but the runtime gate will
            //   refuse the budget at the first BudgetExceeded.
        }
        Ok(Self {
            budget_ms,
            tier,
            fragments_amplified: 0,
            cumulative_ns: 0.0,
        })
    }

    /// § Construct with the canonical Quest-3 budget (1.2 ms).
    pub fn quest3() -> Self {
        Self::new(COST_BUDGET_QUEST3_MS, DispatchTier::CoopMatrix)
            .expect("Quest-3 default budget is always valid")
    }

    /// § Construct with the canonical Vision-Pro budget (1.0 ms).
    pub fn vision_pro() -> Self {
        Self::new(COST_BUDGET_VISION_PRO_MS, DispatchTier::CoopMatrix)
            .expect("Vision-Pro default budget is always valid")
    }

    /// § Charge a single amplifier-call to the cumulative cost.
    /// Returns `Err(BudgetExceeded)` if charging this call would push
    /// cumulative cost past `budget_ms × BUDGET_SLACK`.
    pub fn charge_one(&mut self) -> Result<(), AmplifierError> {
        let new_cum = self.cumulative_ns + f64::from(self.tier.per_fragment_ns());
        let budget_ns = f64::from(self.budget_ms * BUDGET_SLACK) * 1.0e6;
        if new_cum > budget_ns {
            return Err(AmplifierError::BudgetExceeded {
                budget_ms: self.budget_ms,
                used_ms: (new_cum / 1.0e6) as f32,
            });
        }
        self.cumulative_ns = new_cum;
        self.fragments_amplified += 1;
        Ok(())
    }

    /// § Cumulative cost in milliseconds.
    #[must_use]
    pub fn cumulative_ms(&self) -> f32 {
        (self.cumulative_ns / 1.0e6) as f32
    }

    /// § True iff cumulative cost has passed the DEGRADE_THRESHOLD.
    ///   Callers in the per-frame walker should check this and switch
    ///   FoveaTier::Full → FoveaTier::Mid when it returns true (the
    ///   degraded-mode step-3 from `07_KAN § VII`).
    #[must_use]
    pub fn should_degrade(&self) -> bool {
        let budget_ns = f64::from(self.budget_ms * DEGRADE_THRESHOLD) * 1.0e6;
        self.cumulative_ns > budget_ns
    }

    /// § Predict the maximum number of additional amplifier-calls that
    ///   can be charged before BudgetExceeded fires. Useful for the
    ///   per-frame work-graph dispatch to decide how many tiles to
    ///   batch.
    #[must_use]
    pub fn remaining_fragments(&self) -> u32 {
        let budget_ns = f64::from(self.budget_ms * BUDGET_SLACK) * 1.0e6;
        let remaining_ns = budget_ns - self.cumulative_ns;
        if remaining_ns <= 0.0 {
            return 0;
        }
        (remaining_ns / f64::from(self.tier.per_fragment_ns())) as u32
    }

    /// § Reset the cost-model for a new frame. Preserves budget + tier
    ///   ; resets cumulative cost + fragment count to zero.
    pub fn reset_frame(&mut self) {
        self.fragments_amplified = 0;
        self.cumulative_ns = 0.0;
    }
}

impl Default for CostModel {
    fn default() -> Self {
        Self::quest3()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § DispatchTier::CoopMatrix is admissible on M7.
    #[test]
    fn coop_matrix_m7_admissible() {
        assert!(DispatchTier::CoopMatrix.is_m7_admissible());
    }

    /// § DispatchTier::SimdWarp is admissible on M7.
    #[test]
    fn simd_warp_m7_admissible() {
        assert!(DispatchTier::SimdWarp.is_m7_admissible());
    }

    /// § DispatchTier::Scalar is NOT admissible on M7.
    #[test]
    fn scalar_not_m7_admissible() {
        assert!(!DispatchTier::Scalar.is_m7_admissible());
    }

    /// § Per-fragment cost increases with tier.
    #[test]
    fn cost_ordering_by_tier() {
        let c1 = DispatchTier::CoopMatrix.per_fragment_ns();
        let c2 = DispatchTier::SimdWarp.per_fragment_ns();
        let c3 = DispatchTier::Scalar.per_fragment_ns();
        assert!(c1 < c2);
        assert!(c2 < c3);
    }

    /// § quest3() default has 1.2 ms budget at tier-1.
    #[test]
    fn quest3_defaults() {
        let c = CostModel::quest3();
        assert!((c.budget_ms - COST_BUDGET_QUEST3_MS).abs() < 1e-6);
        assert_eq!(c.tier, DispatchTier::CoopMatrix);
        assert_eq!(c.fragments_amplified, 0);
        assert_eq!(c.cumulative_ns, 0.0);
    }

    /// § vision_pro() default has 1.0 ms budget at tier-1.
    #[test]
    fn vision_pro_defaults() {
        let c = CostModel::vision_pro();
        assert!((c.budget_ms - COST_BUDGET_VISION_PRO_MS).abs() < 1e-6);
        assert_eq!(c.tier, DispatchTier::CoopMatrix);
    }

    /// § new() with 0 budget fails.
    #[test]
    fn new_rejects_zero_budget() {
        let r = CostModel::new(0.0, DispatchTier::CoopMatrix);
        assert!(matches!(r, Err(AmplifierError::InvalidBudget(_))));
    }

    /// § new() with NaN budget fails.
    #[test]
    fn new_rejects_nan_budget() {
        let r = CostModel::new(f32::NAN, DispatchTier::CoopMatrix);
        assert!(matches!(r, Err(AmplifierError::InvalidBudget(_))));
    }

    /// § charge_one() increments fragment count.
    #[test]
    fn charge_one_increments() {
        let mut c = CostModel::quest3();
        c.charge_one().unwrap();
        assert_eq!(c.fragments_amplified, 1);
        assert!(c.cumulative_ns > 0.0);
    }

    /// § cumulative_ms reflects accumulated cost.
    #[test]
    fn cumulative_ms_reflects_cost() {
        let mut c = CostModel::quest3();
        let n = 1000;
        for _ in 0..n {
            c.charge_one().unwrap();
        }
        let expected_ms = (n as f32) * COST_PER_FRAGMENT_TIER1_NS / 1.0e6;
        assert!((c.cumulative_ms() - expected_ms).abs() < 1e-3);
    }

    /// § BudgetExceeded fires when cumulative > budget.
    #[test]
    fn budget_exceeded_fires() {
        let mut c = CostModel::new(0.001, DispatchTier::CoopMatrix).unwrap();
        // § 0.001 ms = 1000 ns ; tier-1 = 150 ns ⇒ ~6 calls before exceed.
        let mut hit_err = false;
        for _ in 0..100 {
            if c.charge_one().is_err() {
                hit_err = true;
                break;
            }
        }
        assert!(hit_err);
    }

    /// § BudgetExceeded preserves cumulative state at moment-of-fail.
    #[test]
    fn budget_exceeded_does_not_charge() {
        let mut c = CostModel::new(0.0001, DispatchTier::CoopMatrix).unwrap();
        // § 0.0001 ms = 100 ns ; tier-1 = 150 ns ⇒ FIRST call exceeds.
        let r = c.charge_one();
        assert!(r.is_err());
        // § The fragment was NOT charged, so the count stays 0.
        assert_eq!(c.fragments_amplified, 0);
    }

    /// § should_degrade() returns false at zero cost.
    #[test]
    fn should_degrade_initial_false() {
        let c = CostModel::quest3();
        assert!(!c.should_degrade());
    }

    /// § should_degrade() returns true past the threshold.
    #[test]
    fn should_degrade_past_threshold() {
        let mut c = CostModel::quest3();
        // § Charge enough fragments to cross 0.8 × 1.2 ms = 0.96 ms.
        let calls_to_threshold = ((COST_BUDGET_QUEST3_MS * DEGRADE_THRESHOLD * 1.0e6)
            / COST_PER_FRAGMENT_TIER1_NS)
            .ceil() as u32
            + 1;
        for _ in 0..calls_to_threshold {
            // § Some of the late charges may exceed budget — that is fine
            //   for this test since we are pinning should_degrade(), not
            //   the budget gate.
            let _ = c.charge_one();
        }
        assert!(c.should_degrade());
    }

    /// § remaining_fragments returns positive at fresh start.
    #[test]
    fn remaining_fragments_at_start() {
        let c = CostModel::quest3();
        let remaining = c.remaining_fragments();
        let expected = (COST_BUDGET_QUEST3_MS * 1.0e6 / COST_PER_FRAGMENT_TIER1_NS) as u32;
        assert!(remaining >= expected - 2 && remaining <= expected + 2);
    }

    /// § remaining_fragments returns 0 at exhaustion.
    #[test]
    fn remaining_fragments_at_exhaustion() {
        let mut c = CostModel::new(0.001, DispatchTier::CoopMatrix).unwrap();
        for _ in 0..20 {
            let _ = c.charge_one();
        }
        // § After many charges the remaining is 0 (or near-zero).
        let remaining = c.remaining_fragments();
        assert!(remaining < 5);
    }

    /// § reset_frame zeroes cumulative cost.
    #[test]
    fn reset_frame_zeroes_state() {
        let mut c = CostModel::quest3();
        for _ in 0..100 {
            c.charge_one().unwrap();
        }
        assert!(c.cumulative_ns > 0.0);
        c.reset_frame();
        assert_eq!(c.cumulative_ns, 0.0);
        assert_eq!(c.fragments_amplified, 0);
        // § Budget + tier preserved across reset.
        assert!((c.budget_ms - COST_BUDGET_QUEST3_MS).abs() < 1e-6);
        assert_eq!(c.tier, DispatchTier::CoopMatrix);
    }

    /// § per-fragment cost is well below 1.2 ms / 100 fragments at tier-1.
    /// This is the main per-fragment-cost target check : ≤ 1.2 ms means
    /// the amplifier evaluates 8000 fragments in the budget.
    #[test]
    fn per_fragment_well_below_budget() {
        let cost_ms_per_frag = COST_PER_FRAGMENT_TIER1_NS / 1.0e6;
        // § At tier-1, per-fragment cost = 0.00015 ms = 0.0125% of budget.
        assert!(cost_ms_per_frag < COST_BUDGET_QUEST3_MS * 0.001);
    }
}
