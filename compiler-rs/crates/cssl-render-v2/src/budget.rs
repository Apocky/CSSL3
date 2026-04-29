//! § budget — Stage-5 timing budget tracking + degrade-lever discipline.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage-5 has a 2.5 ms @ Quest-3 / 2.0 ms @ Vision-Pro budget (06_RENDERING_
//!   PIPELINE § V). This module tracks per-region march-step counts +
//!   surface-hit fractions, projects ms-cost via the [`crate::cost_model`],
//!   and exposes the canonical "graceful-degrade" levers per § V.
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § V` — budget table.
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § V` — budget-pulldown
//!     levers : KANDetailBudget → RecursionDepthMax → companion-view skip →
//!     KAN-BRDF band-count → raymarch step-count.
//!   - `Omniverse/01_AXIOMS/13_DENSITY_SOVEREIGNTY.csl § II` — pulldown
//!     discipline.

use thiserror::Error;

use crate::cost_model::{CostModel, MsCost, QuestThreeCostModel, VisionProCostModel};

/// Telemetry counters per Stage-5 invocation.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Stage5BudgetTelemetry {
    /// Total march-steps issued across all rays.
    pub total_steps: u64,
    /// Number of rays that hit a surface.
    pub hit_count: u64,
    /// Number of rays that missed (out of distance).
    pub miss_count: u64,
    /// Number of rays that exhausted step-budget (telemetry-flagged).
    pub budget_exhausted_count: u64,
    /// MERA large-step success count.
    pub mera_skip_count: u64,
    /// Bisection-refine count.
    pub bisection_refine_count: u64,
    /// Frame-time spent on Stage-5 (ms ; populated by the host).
    pub stage_ms: f32,
}

impl Stage5BudgetTelemetry {
    /// Aggregate two telemetry buckets.
    #[must_use]
    pub fn merge(a: Self, b: Self) -> Self {
        Stage5BudgetTelemetry {
            total_steps: a.total_steps + b.total_steps,
            hit_count: a.hit_count + b.hit_count,
            miss_count: a.miss_count + b.miss_count,
            budget_exhausted_count: a.budget_exhausted_count + b.budget_exhausted_count,
            mera_skip_count: a.mera_skip_count + b.mera_skip_count,
            bisection_refine_count: a.bisection_refine_count + b.bisection_refine_count,
            stage_ms: a.stage_ms + b.stage_ms,
        }
    }

    /// Hit-rate as a fraction of total rays.
    #[must_use]
    pub fn hit_rate(&self) -> f32 {
        let total = self.hit_count + self.miss_count + self.budget_exhausted_count;
        if total == 0 {
            return 0.0;
        }
        self.hit_count as f32 / total as f32
    }

    /// Average steps per ray.
    #[must_use]
    pub fn avg_steps_per_ray(&self) -> f32 {
        let total = self.hit_count + self.miss_count + self.budget_exhausted_count;
        if total == 0 {
            return 0.0;
        }
        self.total_steps as f32 / total as f32
    }
}

/// Stage-5 budget tracker. Holds the per-frame telemetry + the projected
/// ms-cost from the configured cost-model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stage5Budget {
    /// Telemetry counters.
    pub telemetry: Stage5BudgetTelemetry,
    /// Projected cost-model output.
    pub projected_ms: MsCost,
    /// Hard ceiling (budget) for this device.
    pub ceiling_ms: f32,
}

impl Stage5Budget {
    /// New budget with the Quest-3 ceiling.
    #[must_use]
    pub fn quest3() -> Self {
        Stage5Budget {
            telemetry: Stage5BudgetTelemetry::default(),
            projected_ms: MsCost(0.0),
            ceiling_ms: crate::STAGE_5_QUEST3_BUDGET_MS,
        }
    }

    /// New budget with the Vision-Pro ceiling.
    #[must_use]
    pub fn vision_pro() -> Self {
        Stage5Budget {
            telemetry: Stage5BudgetTelemetry::default(),
            projected_ms: MsCost(0.0),
            ceiling_ms: crate::STAGE_5_VISION_PRO_BUDGET_MS,
        }
    }

    /// Project the cost using the Quest-3 cost-model.
    pub fn project_quest3(&mut self, total_visible_cells: u64, foveation_factor: f32) {
        let model = QuestThreeCostModel::default();
        self.projected_ms = model.project_cost(total_visible_cells, foveation_factor);
    }

    /// Project the cost using the Vision-Pro cost-model.
    pub fn project_vision_pro(&mut self, total_visible_cells: u64, foveation_factor: f32) {
        let model = VisionProCostModel::default();
        self.projected_ms = model.project_cost(total_visible_cells, foveation_factor);
    }

    /// Headroom (positive = under budget) in milliseconds.
    #[must_use]
    pub fn headroom_ms(&self) -> f32 {
        self.ceiling_ms - self.projected_ms.0
    }

    /// Whether the projected cost exceeds the ceiling.
    #[must_use]
    pub fn over_budget(&self) -> bool {
        self.projected_ms.0 > self.ceiling_ms
    }
}

/// Errors from the budget validator.
#[derive(Debug, Error, PartialEq)]
pub enum BudgetError {
    /// Projected ms-cost exceeds the ceiling.
    #[error("projected stage-5 cost {projected:.3}ms > ceiling {ceiling:.3}ms")]
    Exceeded { projected: f32, ceiling: f32 },
    /// Visible-cell-count exceeds the M7 budget threshold.
    #[error("visible-cell-count {cells} > M7 budget {budget}")]
    CellBudgetExceeded { cells: u64, budget: u64 },
}

/// Validator for Stage-5 budget configuration. Used at config-time to ensure
/// the chosen render-config + cell-count + foveation-aggressiveness fits the
/// device-budget.
#[derive(Debug, Clone, Copy)]
pub struct BudgetValidator;

impl BudgetValidator {
    /// Check that `total_cells` × `foveation_factor` fits the Quest-3 budget.
    pub fn check_quest3(
        total_cells: u64,
        foveation_factor: f32,
    ) -> Result<MsCost, BudgetError> {
        if total_cells > crate::M7_VISIBLE_CELLS_BUDGET {
            return Err(BudgetError::CellBudgetExceeded {
                cells: total_cells,
                budget: crate::M7_VISIBLE_CELLS_BUDGET,
            });
        }
        let model = QuestThreeCostModel::default();
        let projected = model.project_cost(total_cells, foveation_factor);
        if projected.0 > crate::STAGE_5_QUEST3_BUDGET_MS {
            return Err(BudgetError::Exceeded {
                projected: projected.0,
                ceiling: crate::STAGE_5_QUEST3_BUDGET_MS,
            });
        }
        Ok(projected)
    }

    /// Check Vision-Pro budget.
    pub fn check_vision_pro(
        total_cells: u64,
        foveation_factor: f32,
    ) -> Result<MsCost, BudgetError> {
        if total_cells > crate::M7_VISIBLE_CELLS_BUDGET {
            return Err(BudgetError::CellBudgetExceeded {
                cells: total_cells,
                budget: crate::M7_VISIBLE_CELLS_BUDGET,
            });
        }
        let model = VisionProCostModel::default();
        let projected = model.project_cost(total_cells, foveation_factor);
        if projected.0 > crate::STAGE_5_VISION_PRO_BUDGET_MS {
            return Err(BudgetError::Exceeded {
                projected: projected.0,
                ceiling: crate::STAGE_5_VISION_PRO_BUDGET_MS,
            });
        }
        Ok(projected)
    }
}

/// Graceful-degrade lever per § V budget-pulldown discipline. Selecting a
/// lever returns a "throttled" config the host can swap to. Lower values are
/// less aggressive (preferred when only mild over-budget detected).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage5DegradeLever {
    /// First lever : reduce KANDetailBudget per region.
    KanDetailBudget,
    /// Second lever : drop mise-en-abyme recursion-depth.
    RecursionDepth,
    /// Third lever : skip companion-view this frame.
    CompanionViewSkip,
    /// Fourth lever : reduce KAN-BRDF band-count from 16 → 8 → 4.
    KanBrdfBands,
    /// Last-resort lever : cut raymarch step-count.
    RaymarchSteps,
}

impl Stage5DegradeLever {
    /// Return the canonical pulldown order. Earlier = preferred.
    #[must_use]
    pub fn pulldown_order() -> &'static [Stage5DegradeLever] {
        &[
            Stage5DegradeLever::KanDetailBudget,
            Stage5DegradeLever::RecursionDepth,
            Stage5DegradeLever::CompanionViewSkip,
            Stage5DegradeLever::KanBrdfBands,
            Stage5DegradeLever::RaymarchSteps,
        ]
    }

    /// Aesthetic-cost score of pulling this lever (0.0 = invisible to user,
    /// 1.0 = significantly visible). Used by the budget-validator to rank.
    #[must_use]
    pub fn aesthetic_cost(self) -> f32 {
        match self {
            Stage5DegradeLever::KanDetailBudget => 0.10,
            Stage5DegradeLever::RecursionDepth => 0.20,
            Stage5DegradeLever::CompanionViewSkip => 0.30,
            Stage5DegradeLever::KanBrdfBands => 0.50,
            Stage5DegradeLever::RaymarchSteps => 0.80,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_default_zero() {
        let t = Stage5BudgetTelemetry::default();
        assert_eq!(t.total_steps, 0);
        assert_eq!(t.hit_count, 0);
    }

    #[test]
    fn telemetry_merge_sums() {
        let a = Stage5BudgetTelemetry {
            total_steps: 100,
            hit_count: 10,
            miss_count: 5,
            budget_exhausted_count: 0,
            mera_skip_count: 20,
            bisection_refine_count: 3,
            stage_ms: 1.0,
        };
        let b = Stage5BudgetTelemetry {
            total_steps: 50,
            hit_count: 5,
            miss_count: 3,
            budget_exhausted_count: 1,
            mera_skip_count: 10,
            bisection_refine_count: 1,
            stage_ms: 0.5,
        };
        let m = Stage5BudgetTelemetry::merge(a, b);
        assert_eq!(m.total_steps, 150);
        assert_eq!(m.hit_count, 15);
        assert_eq!(m.miss_count, 8);
        assert!((m.stage_ms - 1.5).abs() < 1e-6);
    }

    #[test]
    fn telemetry_hit_rate_proper() {
        let t = Stage5BudgetTelemetry {
            hit_count: 30,
            miss_count: 60,
            budget_exhausted_count: 10,
            ..Default::default()
        };
        assert!((t.hit_rate() - 0.30).abs() < 1e-6);
    }

    #[test]
    fn telemetry_avg_steps_per_ray() {
        let t = Stage5BudgetTelemetry {
            total_steps: 1000,
            hit_count: 50,
            miss_count: 50,
            ..Default::default()
        };
        assert!((t.avg_steps_per_ray() - 10.0).abs() < 1e-6);
    }

    #[test]
    fn budget_quest3_ceiling_2_5() {
        let b = Stage5Budget::quest3();
        assert!((b.ceiling_ms - 2.5).abs() < 1e-6);
    }

    #[test]
    fn budget_vision_pro_ceiling_2_0() {
        let b = Stage5Budget::vision_pro();
        assert!((b.ceiling_ms - 2.0).abs() < 1e-6);
    }

    #[test]
    fn budget_headroom_positive_when_under() {
        let mut b = Stage5Budget::quest3();
        b.projected_ms = MsCost(1.0);
        assert!(b.headroom_ms() > 0.0);
        assert!(!b.over_budget());
    }

    #[test]
    fn budget_over_budget_when_exceeded() {
        let mut b = Stage5Budget::quest3();
        b.projected_ms = MsCost(5.0);
        assert!(b.over_budget());
        assert!(b.headroom_ms() < 0.0);
    }

    #[test]
    fn validator_quest3_under_budget_ok() {
        let r = BudgetValidator::check_quest3(1_000_000, 0.4);
        assert!(r.is_ok());
    }

    #[test]
    fn validator_quest3_over_cell_budget_errors() {
        let r = BudgetValidator::check_quest3(10_000_000, 0.4);
        assert!(matches!(r, Err(BudgetError::CellBudgetExceeded { .. })));
    }

    #[test]
    fn validator_quest3_over_ms_budget_errors() {
        // Force a path that overshoots ceiling : 5M cells × 2.0 foveation = 5.0 ms > 2.5 ceiling.
        let r = BudgetValidator::check_quest3(crate::M7_VISIBLE_CELLS_BUDGET, 2.0);
        assert!(matches!(r, Err(BudgetError::Exceeded { .. })));
    }

    #[test]
    fn project_quest3_writes_projected_ms() {
        let mut b = Stage5Budget::quest3();
        b.project_quest3(1_000_000, 0.4);
        assert!(b.projected_ms.0 > 0.0);
    }

    #[test]
    fn pulldown_order_first_is_kan_detail_budget() {
        assert_eq!(
            Stage5DegradeLever::pulldown_order()[0],
            Stage5DegradeLever::KanDetailBudget
        );
    }

    #[test]
    fn pulldown_aesthetic_cost_increases() {
        let order = Stage5DegradeLever::pulldown_order();
        for w in order.windows(2) {
            assert!(w[0].aesthetic_cost() <= w[1].aesthetic_cost());
        }
    }

    #[test]
    fn pulldown_raymarch_steps_is_last_resort() {
        let last = Stage5DegradeLever::pulldown_order().last().copied().unwrap();
        assert_eq!(last, Stage5DegradeLever::RaymarchSteps);
    }
}
