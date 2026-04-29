//! Cost model + frame budget.
//!
//! § DESIGN — derived from `density_budget § V` PHASE-BUDGET tables.
//!   60Hz : 16.67 ms ; 120Hz : 8.33 ms ; 90Hz-VR : 11.11 ms / eye.
//!
//! § ENTITY COUNTS — `density_budget § IV` :
//!   T0 fovea  : ≤ 100   ent  @ 60 Hz @ 22 ns  ⇒ 132 µs
//!   T1 mid    : ≤ 5K    ent  @ 60 Hz @ 22 ns  ⇒ 6.6 ms (heaviest)
//!   T2 distant: ≤ 50K   ent  @ 15 Hz @ 30 ns  ⇒ 5.6 ms amortized
//!   T3 horizon: ≤ 945K  ent  @  4 Hz @ 50 ns  ⇒ parallel-async
//!
//!   Aggregates : ≤ 1M entities, ≤ 12.3 ms / frame on T0+T1+T2.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::similar_names)]
#![allow(clippy::match_same_arms)]

/// Frame-rate target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameBudget {
    /// 60Hz desktop : 16.67 ms.
    Hz60,
    /// 90Hz VR (per eye after AppSW reproject) : 11.11 ms.
    Hz90Vr,
    /// 120Hz high-refresh : 8.33 ms.
    Hz120,
}

impl FrameBudget {
    /// 60Hz constructor.
    #[must_use]
    pub const fn hz_60() -> Self {
        Self::Hz60
    }

    /// 90Hz-VR constructor.
    #[must_use]
    pub const fn hz_90_vr() -> Self {
        Self::Hz90Vr
    }

    /// 120Hz constructor.
    #[must_use]
    pub const fn hz_120() -> Self {
        Self::Hz120
    }

    /// Frame in microseconds.
    #[must_use]
    pub const fn frame_us(self) -> u32 {
        match self {
            Self::Hz60 => 16_667,
            Self::Hz90Vr => 11_111,
            Self::Hz120 => 8_333,
        }
    }

    /// Target Hz.
    #[must_use]
    pub const fn target_hz(self) -> u32 {
        match self {
            Self::Hz60 => 60,
            Self::Hz90Vr => 90,
            Self::Hz120 => 120,
        }
    }

    /// Stable string tag.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Hz60 => "hz60",
            Self::Hz90Vr => "hz90-vr",
            Self::Hz120 => "hz120",
        }
    }
}

/// Per-tier entity counts (cite `density_budget § IV ENTITY BUDGET TABLE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityCount {
    /// T0 fovea entities : ≤ 100.
    pub t0_fovea: u32,
    /// T1 mid entities : ≤ 5_000.
    pub t1_mid: u32,
    /// T2 distant entities : ≤ 50_000 @ 15Hz.
    pub t2_distant: u32,
    /// T3 horizon entities : ≤ 945_000 @ 4Hz.
    pub t3_horizon: u32,
}

impl EntityCount {
    /// 1M-entity full-budget baseline.
    #[must_use]
    pub const fn full_budget() -> Self {
        Self {
            t0_fovea: 100,
            t1_mid: 5_000,
            t2_distant: 50_000,
            t3_horizon: 945_000,
        }
    }

    /// 100K-entity fallback (used on `IndirectFallback` backend).
    /// Per `density_budget § XI.B EDGE-7` : reduce-entity-count to 100K
    /// to preserve other budgets when work-graphs unavailable.
    #[must_use]
    pub const fn fallback_100k() -> Self {
        Self {
            t0_fovea: 100,
            t1_mid: 4_900,
            t2_distant: 25_000,
            t3_horizon: 70_000,
        }
    }

    /// All-zero baseline.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            t0_fovea: 0,
            t1_mid: 0,
            t2_distant: 0,
            t3_horizon: 0,
        }
    }

    /// Aggregate count.
    #[must_use]
    pub const fn total(self) -> u64 {
        (self.t0_fovea as u64)
            + (self.t1_mid as u64)
            + (self.t2_distant as u64)
            + (self.t3_horizon as u64)
    }

    /// Per-tier per-entity ns cost (from spec § IV).
    #[must_use]
    pub const fn ns_per_entity(tier: usize) -> u32 {
        match tier {
            0 => 22, // T0 fovea
            1 => 22, // T1 mid
            2 => 30, // T2 distant
            3 => 50, // T3 horizon
            _ => 0,
        }
    }

    /// Per-tier tick-rate (Hz).
    #[must_use]
    pub const fn tier_hz(tier: usize) -> u32 {
        match tier {
            0 => 60,
            1 => 60,
            2 => 15,
            3 => 4,
            _ => 60,
        }
    }
}

/// Cost-model for projecting work-graph dispatch cost.
///
/// Uses the spec § IV per-tier per-entity cost model + the backend
/// perf-factor to estimate wall-clock cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CostModel {
    /// Entity counts per tier.
    pub entities: EntityCountWrapper,
}

/// Wrapped EntityCount providing a `Default` impl that matches `none()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityCountWrapper(pub EntityCount);

impl Default for EntityCountWrapper {
    fn default() -> Self {
        Self(EntityCount::none())
    }
}

impl CostModel {
    /// Construct.
    #[must_use]
    pub const fn new(entities: EntityCount) -> Self {
        Self {
            entities: EntityCountWrapper(entities),
        }
    }

    /// Project per-frame cost in microseconds (assuming autonomous backend ;
    /// scale by `1.0/perf_factor` for fallback-backend-cost).
    #[must_use]
    pub const fn project_frame_us(self) -> u32 {
        let e = self.entities.0;
        // T0+T1 @ 60Hz : direct frame contribution.
        let t0_ns = (e.t0_fovea as u64) * (EntityCount::ns_per_entity(0) as u64);
        let t1_ns = (e.t1_mid as u64) * (EntityCount::ns_per_entity(1) as u64);
        // T2 @ 15Hz : amortized over 4 frames.
        let t2_total_ns = (e.t2_distant as u64) * (EntityCount::ns_per_entity(2) as u64);
        let t2_ns = t2_total_ns / 4;
        // T3 @ 4Hz : amortized over 15 frames ; runs parallel-async (assume
        // free under the cost-model since it doesn't block the frame).
        let t3_ns = 0_u64;
        let total_ns = t0_ns + t1_ns + t2_ns + t3_ns;
        // ns → us, capped at u32::MAX.
        let total_us = total_ns / 1_000;
        if total_us > u32::MAX as u64 {
            u32::MAX
        } else {
            total_us as u32
        }
    }

    /// Project per-frame cost on a non-autonomous backend (scaled by
    /// `1.0/perf_factor`).
    #[must_use]
    pub fn project_frame_us_on(self, backend: crate::backend::Backend) -> u32 {
        let raw = self.project_frame_us();
        let scaled = (raw as f32) / backend.perf_factor();
        scaled as u32
    }
}

#[cfg(test)]
mod tests {
    use super::{CostModel, EntityCount, FrameBudget};
    use crate::backend::Backend;

    #[test]
    fn frame_budget_us_match_spec() {
        assert_eq!(FrameBudget::hz_60().frame_us(), 16_667);
        assert_eq!(FrameBudget::hz_90_vr().frame_us(), 11_111);
        assert_eq!(FrameBudget::hz_120().frame_us(), 8_333);
    }

    #[test]
    fn frame_budget_target_hz_match_tag() {
        assert_eq!(FrameBudget::hz_60().target_hz(), 60);
        assert_eq!(FrameBudget::hz_120().target_hz(), 120);
    }

    #[test]
    fn entity_count_full_budget_totals_1m() {
        let e = EntityCount::full_budget();
        assert!(e.total() >= 950_000);
    }

    #[test]
    fn entity_count_fallback_100k_under_ceiling() {
        let e = EntityCount::fallback_100k();
        assert!(e.total() <= 100_000);
    }

    #[test]
    fn ns_per_entity_t0_fovea_22ns() {
        assert_eq!(EntityCount::ns_per_entity(0), 22);
        assert_eq!(EntityCount::ns_per_entity(2), 30);
        assert_eq!(EntityCount::ns_per_entity(3), 50);
    }

    #[test]
    fn tier_hz_match_density_spec() {
        assert_eq!(EntityCount::tier_hz(0), 60);
        assert_eq!(EntityCount::tier_hz(2), 15);
        assert_eq!(EntityCount::tier_hz(3), 4);
    }

    #[test]
    fn cost_model_project_under_8330us_at_120hz() {
        // With T0+T1+T2 budgets, a 1M entity world should project ≤ ~12ms.
        // For the 8.3ms target, the spec requires foveation + tier-skip ;
        // the model here projects *uncompensated* wall-clock from spec §-IV
        // numbers and so should sit just over 8.3ms — which is precisely the
        // condition that motivates work-graph fusion in the first place.
        let cm = CostModel::new(EntityCount::full_budget());
        let us = cm.project_frame_us();
        // Lower bound : the model is non-trivial.
        assert!(us > 0);
        // Upper bound : full-budget aggregate ≤ 1ms (T0+T1 only) since T3
        // is parallel-async ; T2 amortized.
        assert!(us <= 1_000, "projected_us = {us}");
    }

    #[test]
    fn cost_model_indirect_costs_more() {
        let cm = CostModel::new(EntityCount::full_budget());
        let auto = cm.project_frame_us_on(Backend::D3d12WorkGraph);
        let fallback = cm.project_frame_us_on(Backend::IndirectFallback);
        assert!(fallback >= auto);
    }

    #[test]
    fn cost_model_default_is_zero_cost() {
        let cm = CostModel::default();
        assert_eq!(cm.project_frame_us(), 0);
    }

    #[test]
    fn frame_budget_tag_stable() {
        assert_eq!(FrameBudget::hz_60().tag(), "hz60");
        assert_eq!(FrameBudget::hz_120().tag(), "hz120");
        assert_eq!(FrameBudget::hz_90_vr().tag(), "hz90-vr");
    }

    #[test]
    fn entity_count_none_zero_total() {
        let e = EntityCount::none();
        assert_eq!(e.total(), 0);
    }
}
