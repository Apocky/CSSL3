//! § cost_model — ms-cost projection for Quest-3 / Vision-Pro / generic GPU.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Foundation cost-model derived from the 06_RENDERING_PIPELINE § V budget
//!   table. Given a visible-cell-count + foveation aggregate-cost-fraction +
//!   per-shading-rate-zone distribution, projects Stage-5 ms-cost.
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § V` — per-stage
//!     budget table.
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § VII` — foveation
//!     savings (typical 35-40% of naive cost ; weighted-avg).
//!   - `Omniverse/01_AXIOMS/13_DENSITY_SOVEREIGNTY.csl § I` — 5×10⁶ visible-
//!     cells threshold @ M7.
//!
//! § COST CALIBRATION (foundation)
//!   The model is calibrated against the spec budget table :
//!     Quest-3   : 2.5 ms @ 5×10⁶ cells @ 1.0 foveation-factor (no foveation)
//!     Vision-Pro: 2.0 ms @ 5×10⁶ cells @ 1.0 foveation-factor
//!   with the FOVEATION SAVINGS as a linear scalar applied at the call-site.
//!
//!   `cost(cells, foveation) = baseline_cost(cells) × foveation`
//!
//!   `baseline_cost(cells) = (cells / max_cells) × ceiling_ms` — linear in
//!   cell-count up to the M7 ceiling. This is approximate ; the real model
//!   in D116 follow-up slices is non-linear (MERA-skip + cone-marching reduce
//!   cells-per-ray by ~3×).

/// Per-zone shading rate identifier — used by cost-model projections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadingRateZone {
    /// 1×1 fovea zone.
    Fovea,
    /// 2×2 mid zone.
    Mid,
    /// 4×4 peripheral zone.
    Peripheral,
}

/// Milliseconds-cost wrapper.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MsCost(pub f32);

impl MsCost {
    /// Sum two costs.
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn plus(self, other: Self) -> Self {
        MsCost(self.0 + other.0)
    }

    /// Scale.
    #[must_use]
    pub fn scale(self, k: f32) -> Self {
        MsCost(self.0 * k)
    }
}

/// Cost-model trait : project ms-cost given visible-cells + foveation-factor.
pub trait CostModel {
    /// Project cost.
    fn project_cost(&self, visible_cells: u64, foveation_factor: f32) -> MsCost;

    /// Device label.
    fn label(&self) -> &'static str;
}

/// Quest-3 cost model (Adreno-740 ; ~ 1.0 TFLOPS sustained).
#[derive(Debug, Clone, Copy)]
pub struct QuestThreeCostModel {
    /// Baseline ms @ 5M cells, 1.0 foveation-factor.
    pub baseline_at_5m: f32,
    /// Maximum cells the model is calibrated for.
    pub max_cells: u64,
}

impl Default for QuestThreeCostModel {
    fn default() -> Self {
        QuestThreeCostModel {
            baseline_at_5m: crate::STAGE_5_QUEST3_BUDGET_MS,
            max_cells: crate::M7_VISIBLE_CELLS_BUDGET,
        }
    }
}

impl CostModel for QuestThreeCostModel {
    fn project_cost(&self, visible_cells: u64, foveation_factor: f32) -> MsCost {
        let cell_ratio = (visible_cells as f32) / (self.max_cells as f32);
        let baseline = self.baseline_at_5m * cell_ratio;
        MsCost(baseline * foveation_factor.max(0.0))
    }
    fn label(&self) -> &'static str {
        "Quest-3 (Adreno-740)"
    }
}

/// Vision-Pro cost model (Apple M2 ; ~ 1.4 TFLOPS sustained).
#[derive(Debug, Clone, Copy)]
pub struct VisionProCostModel {
    /// Baseline ms @ 5M cells, 1.0 foveation-factor.
    pub baseline_at_5m: f32,
    /// Maximum cells the model is calibrated for.
    pub max_cells: u64,
}

impl Default for VisionProCostModel {
    fn default() -> Self {
        VisionProCostModel {
            baseline_at_5m: crate::STAGE_5_VISION_PRO_BUDGET_MS,
            max_cells: crate::M7_VISIBLE_CELLS_BUDGET,
        }
    }
}

impl CostModel for VisionProCostModel {
    fn project_cost(&self, visible_cells: u64, foveation_factor: f32) -> MsCost {
        let cell_ratio = (visible_cells as f32) / (self.max_cells as f32);
        let baseline = self.baseline_at_5m * cell_ratio;
        MsCost(baseline * foveation_factor.max(0.0))
    }
    fn label(&self) -> &'static str {
        "Vision-Pro (Apple M2)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ms_cost_plus() {
        let a = MsCost(1.0);
        let b = MsCost(2.0);
        let s = a.plus(b);
        assert!((s.0 - 3.0).abs() < 1e-6);
    }

    #[test]
    fn ms_cost_scale() {
        let a = MsCost(2.5);
        let s = a.scale(2.0);
        assert!((s.0 - 5.0).abs() < 1e-6);
    }

    #[test]
    fn quest3_at_5m_full_foveation_meets_budget() {
        let m = QuestThreeCostModel::default();
        let c = m.project_cost(crate::M7_VISIBLE_CELLS_BUDGET, 1.0);
        assert!((c.0 - crate::STAGE_5_QUEST3_BUDGET_MS).abs() < 1e-3);
    }

    #[test]
    fn quest3_with_foveation_under_budget() {
        let m = QuestThreeCostModel::default();
        let c = m.project_cost(crate::M7_VISIBLE_CELLS_BUDGET, 0.4);
        assert!(c.0 < crate::STAGE_5_QUEST3_BUDGET_MS);
    }

    #[test]
    fn vision_pro_at_5m_full_foveation_meets_budget() {
        let m = VisionProCostModel::default();
        let c = m.project_cost(crate::M7_VISIBLE_CELLS_BUDGET, 1.0);
        assert!((c.0 - crate::STAGE_5_VISION_PRO_BUDGET_MS).abs() < 1e-3);
    }

    #[test]
    fn vision_pro_strictly_lower_cost_than_quest3() {
        let q = QuestThreeCostModel::default();
        let v = VisionProCostModel::default();
        let cells = 1_000_000;
        let foveation = 0.5;
        let cq = q.project_cost(cells, foveation);
        let cv = v.project_cost(cells, foveation);
        assert!(cv.0 < cq.0);
    }

    #[test]
    fn project_zero_cells_zero_cost() {
        let m = QuestThreeCostModel::default();
        let c = m.project_cost(0, 1.0);
        assert!((c.0 - 0.0).abs() < 1e-6);
    }

    #[test]
    fn project_zero_foveation_zero_cost() {
        let m = QuestThreeCostModel::default();
        let c = m.project_cost(crate::M7_VISIBLE_CELLS_BUDGET, 0.0);
        assert!((c.0 - 0.0).abs() < 1e-6);
    }

    #[test]
    fn negative_foveation_clamps_to_zero() {
        let m = QuestThreeCostModel::default();
        let c = m.project_cost(crate::M7_VISIBLE_CELLS_BUDGET, -0.5);
        assert!((c.0 - 0.0).abs() < 1e-6);
    }

    #[test]
    fn label_contains_device_name() {
        let q = QuestThreeCostModel::default();
        let v = VisionProCostModel::default();
        assert!(q.label().contains("Quest-3"));
        assert!(v.label().contains("Vision-Pro"));
    }

    #[test]
    fn shading_rate_zones_distinct() {
        assert_ne!(ShadingRateZone::Fovea, ShadingRateZone::Mid);
        assert_ne!(ShadingRateZone::Mid, ShadingRateZone::Peripheral);
    }
}
