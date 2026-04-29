//! § mera_skip — MERA-pyramid hierarchical traversal for SDF ray-marching.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Drop-in replacement for the legacy "BVH + octree + chunk-grid" acceleration-
//!   structure trio. The MERA-pyramid (4 tiers : T0 fovea / T1 mid / T2 distant /
//!   T3 horizon ; `cssl-substrate-omega-field::MeraPyramid`) ALREADY summarizes
//!   the dense FieldCell grid hierarchically. Stage-5 walks rays against this
//!   pyramid : large step when the ray is in a coarse-summary region, bisection
//!   refine when the ray approaches the surface.
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/01_SDF_NATIVE_RENDER.csl.md § V` — MERA replaces
//!     BVH/octree/chunkgrid (one source of truth = MERA).
//!   - `Omniverse/07_AESTHETIC/01_SDF_NATIVE_RENDER.csl.md § IV` — sphere-tracing
//!     with MERA-skip + bisection-refine.
//!   - `Omniverse/04_OMEGA_FIELD/02_STORAGE.csl.md § III` — MERA-tier
//!     coarsening discipline (fovea = 1 cm, mid = 4 cm, distant = 16 cm,
//!     horizon = 64 cm).
//!
//! § HIERARCHICAL TRAVERSAL ALGORITHM
//!   For a ray at position `p`, distance-bound `b` from the MERA-summary at the
//!   current tier is :
//!
//!   ```text
//!   bound_at_tier(p, tier) = max(0, summary_density_distance_estimate)
//!   ```
//!
//!   The walker picks the COARSEST tier where `b > 0` — meaning the ray is
//!   confidently far from any surface in that summary block — and steps by `b`.
//!   When `b → 0` (approaching surface), it descends to the finer tier and
//!   refines via the analytic SDF (which is L=1, sphere-traceable).
//!
//!   Worst-case traversal count : `O(log N)` where N = visible-cells in the
//!   active region (Axiom 13 § I 5 × 10⁶ at M7).

use cssl_substrate_omega_field::{CellTier, MeraPyramid, MortonKey};

/// Distance-bound returned by a MERA-summary lookup. The `tier` indicates
/// which tier provided the bound ; the `bound` is the conservative lower
/// bound on distance to the nearest surface in that summary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SummaryBound {
    /// Conservative distance lower-bound (≥ 0).
    pub bound: f32,
    /// Tier that produced the bound (T0..T3).
    pub tier: CellTier,
}

impl SummaryBound {
    /// New bound at the given tier.
    #[must_use]
    pub fn new(bound: f32, tier: CellTier) -> Self {
        SummaryBound {
            bound: bound.max(0.0),
            tier,
        }
    }

    /// Sentinel : "no summary available" — fall through to analytic SDF.
    #[must_use]
    pub fn none() -> Self {
        SummaryBound {
            bound: 0.0,
            tier: CellTier::T0Fovea,
        }
    }
}

/// Result of a MeraSkipDispatcher step lookup. Either the walker has a
/// large-step bound to advance ([`MeraSkipResult::LargeStep`]), or it must
/// bisection-refine ([`MeraSkipResult::BisectionRefine`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MeraSkipResult {
    /// Step by `bound` (the ray is in a coarse-summary region).
    LargeStep {
        /// Distance to advance.
        bound: f32,
        /// Tier that supplied the bound.
        tier: CellTier,
    },
    /// Approaching surface — drop to analytic SDF + bisection.
    BisectionRefine,
    /// Out-of-region : no MERA-tier covers `p`. Default to fine-tier sphere-trace.
    OutOfRegion,
}

/// Per-tier cell-size in meters. Tier-0 = 1 cm, T1 = 4 cm, T2 = 16 cm, T3 = 64 cm.
#[must_use]
pub fn tier_cell_size_meters(tier: CellTier) -> f32 {
    match tier {
        CellTier::T0Fovea => 0.01,
        CellTier::T1Mid => 0.04,
        CellTier::T2Distant => 0.16,
        CellTier::T3Horizon => 0.64,
    }
}

/// MERA-skip dispatcher. Wraps a borrow of [`MeraPyramid`] + the world-to-grid
/// origin/scale + the per-tier bound resolver.
#[derive(Debug)]
pub struct MeraSkipDispatcher<'p> {
    /// The MERA pyramid to walk.
    pyramid: &'p MeraPyramid,
    /// Origin of the grid in world-space (where Morton (0,0,0) lives).
    grid_origin: [f32; 3],
    /// World-units per Morton-axis-unit at T0Fovea (default 1 cm = 0.01 m).
    fovea_scale: f32,
    /// Surface-proximity threshold below which the dispatcher returns
    /// `BisectionRefine`. Default `0.005 m` (5 mm).
    refine_epsilon: f32,
}

impl<'p> MeraSkipDispatcher<'p> {
    /// Construct a dispatcher.
    #[must_use]
    pub fn new(pyramid: &'p MeraPyramid) -> Self {
        MeraSkipDispatcher {
            pyramid,
            grid_origin: [0.0, 0.0, 0.0],
            fovea_scale: 0.01,
            refine_epsilon: 0.005,
        }
    }

    /// Configure the world-grid origin (in meters).
    #[must_use]
    pub fn with_grid_origin(mut self, origin: [f32; 3]) -> Self {
        self.grid_origin = origin;
        self
    }

    /// Configure the fovea (T0) cell-size in meters.
    #[must_use]
    pub fn with_fovea_scale(mut self, scale: f32) -> Self {
        self.fovea_scale = scale.max(1e-6);
        self
    }

    /// Configure the refine epsilon (surface-proximity threshold).
    #[must_use]
    pub fn with_refine_epsilon(mut self, eps: f32) -> Self {
        self.refine_epsilon = eps.max(0.0);
        self
    }

    /// Get the configured refine epsilon.
    #[must_use]
    pub fn refine_epsilon(&self) -> f32 {
        self.refine_epsilon
    }

    /// Convert a world-space point `p` to a Morton-key at the given tier. The
    /// world→grid map is `floor((p - origin) / (fovea_scale * tier_factor))`
    /// where `tier_factor = 4^tier` (since each MERA-tier coarsens 2× per axis,
    /// so 8 cells coarsen-to-one in 3D ; in linear-axis terms `2^tier`).
    fn world_to_morton_key(&self, p: [f32; 3], tier: CellTier) -> Option<MortonKey> {
        let tier_factor = (1u64 << tier.mera_layer()) as f32;
        let cell_size = self.fovea_scale * tier_factor;
        let scale_inv = 1.0 / cell_size;
        let local = [
            (p[0] - self.grid_origin[0]) * scale_inv,
            (p[1] - self.grid_origin[1]) * scale_inv,
            (p[2] - self.grid_origin[2]) * scale_inv,
        ];
        // Reject negative (out-of-region).
        if local[0] < 0.0 || local[1] < 0.0 || local[2] < 0.0 {
            return None;
        }
        let i = local[0].floor() as u64;
        let j = local[1].floor() as u64;
        let k = local[2].floor() as u64;
        MortonKey::encode(i, j, k).ok()
    }

    /// Lookup the conservative summary-bound at world-position `p` for the given
    /// tier. Returns `SummaryBound::none()` if no cell at that tier covers `p`.
    #[must_use]
    pub fn bound_at(&self, p: [f32; 3], tier: CellTier) -> SummaryBound {
        let Some(key) = self.world_to_morton_key(p, tier) else {
            return SummaryBound::none();
        };
        let Some(cell) = self.pyramid.tier(tier).at_const(key) else {
            return SummaryBound::none();
        };
        // Conservative-bound estimate : if the cell density is zero (air),
        // the entire 2× tier-cell is air — we can step at least
        // half-cell-size (the ray could be near the boundary).
        // If the cell has surface-content (density != 0), the bound is the
        // refine-epsilon (must descend to refine).
        let cell_size = tier_cell_size_meters(tier);
        if cell.density.abs() < 1e-6 {
            // Air cell : a full half-tier-cell-size step is safe.
            SummaryBound::new(0.5 * cell_size, tier)
        } else {
            // Surface-bearing cell : conservative below refine eps.
            SummaryBound::new(self.refine_epsilon * 0.5, tier)
        }
    }

    /// Return the coarsest tier where `bound_at(p, tier).bound > refine_epsilon`.
    /// This is the canonical "step boldly when far from surface" path. Falls
    /// back to `T0Fovea` when no coarser tier qualifies.
    #[must_use]
    pub fn coarsest_safe_tier(&self, p: [f32; 3]) -> CellTier {
        for tier in [
            CellTier::T3Horizon,
            CellTier::T2Distant,
            CellTier::T1Mid,
            CellTier::T0Fovea,
        ] {
            let b = self.bound_at(p, tier);
            if b.bound > self.refine_epsilon {
                return tier;
            }
        }
        CellTier::T0Fovea
    }

    /// Top-level step lookup. Returns either `LargeStep` (advance by the
    /// summary bound at the coarsest-safe tier), `BisectionRefine` (drop to
    /// fine SDF), or `OutOfRegion` (no tier covers `p`).
    #[must_use]
    pub fn step_at(&self, p: [f32; 3]) -> MeraSkipResult {
        // Try coarsest-first. If we find a safe step at any tier, take it.
        let mut tier_covered = false;
        for tier in [
            CellTier::T3Horizon,
            CellTier::T2Distant,
            CellTier::T1Mid,
            CellTier::T0Fovea,
        ] {
            let b = self.bound_at(p, tier);
            if b.bound > 0.0 {
                tier_covered = true;
                if b.bound > self.refine_epsilon {
                    return MeraSkipResult::LargeStep {
                        bound: b.bound,
                        tier,
                    };
                }
            }
        }
        if tier_covered {
            MeraSkipResult::BisectionRefine
        } else {
            MeraSkipResult::OutOfRegion
        }
    }

    /// Total dense + summary cell-count across all tiers (for budget projection).
    #[must_use]
    pub fn total_summary_cells(&self) -> usize {
        self.pyramid.total_cell_count()
    }

    /// Per-tier cell counts.
    #[must_use]
    pub fn per_tier_summary_cells(&self) -> [usize; 4] {
        self.pyramid.per_tier_counts()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_substrate_omega_field::{FieldCell, MortonKey};

    #[test]
    fn tier_cell_sizes_meet_spec() {
        // 1, 4, 16, 64 cm : the canonical voxel cascade.
        assert!((tier_cell_size_meters(CellTier::T0Fovea) - 0.01).abs() < 1e-6);
        assert!((tier_cell_size_meters(CellTier::T1Mid) - 0.04).abs() < 1e-6);
        assert!((tier_cell_size_meters(CellTier::T2Distant) - 0.16).abs() < 1e-6);
        assert!((tier_cell_size_meters(CellTier::T3Horizon) - 0.64).abs() < 1e-6);
    }

    #[test]
    fn summary_bound_clamps_negative_to_zero() {
        let b = SummaryBound::new(-0.5, CellTier::T0Fovea);
        assert!((b.bound - 0.0).abs() < 1e-6);
    }

    #[test]
    fn summary_bound_none_is_sentinel() {
        let b = SummaryBound::none();
        assert!((b.bound - 0.0).abs() < 1e-6);
    }

    #[test]
    fn dispatcher_bound_at_empty_is_zero() {
        let p = MeraPyramid::new();
        let d = MeraSkipDispatcher::new(&p);
        let bound = d.bound_at([0.0, 0.0, 0.0], CellTier::T0Fovea);
        assert!((bound.bound - 0.0).abs() < 1e-6);
    }

    #[test]
    fn dispatcher_bound_at_air_cell_full_step() {
        let mut p = MeraPyramid::new();
        let cell = FieldCell::default();
        let key = MortonKey::encode(0, 0, 0).unwrap();
        p.insert_at(CellTier::T1Mid, key, cell).unwrap();
        let d = MeraSkipDispatcher::new(&p).with_fovea_scale(0.01);
        let bound = d.bound_at([0.0, 0.0, 0.0], CellTier::T1Mid);
        // Air cell : bound = half tier-1 cell-size = 2 cm.
        assert!((bound.bound - 0.02).abs() < 1e-4);
    }

    #[test]
    fn dispatcher_bound_at_surface_cell_below_refine() {
        let mut p = MeraPyramid::new();
        let mut cell = FieldCell::default();
        cell.density = 1.0; // surface present.
        let key = MortonKey::encode(0, 0, 0).unwrap();
        p.insert_at(CellTier::T0Fovea, key, cell).unwrap();
        let d = MeraSkipDispatcher::new(&p)
            .with_fovea_scale(0.01)
            .with_refine_epsilon(0.005);
        let bound = d.bound_at([0.0, 0.0, 0.0], CellTier::T0Fovea);
        assert!(bound.bound < d.refine_epsilon());
    }

    #[test]
    fn dispatcher_step_out_of_region() {
        let p = MeraPyramid::new();
        let d = MeraSkipDispatcher::new(&p);
        let r = d.step_at([5.0, 5.0, 5.0]);
        assert_eq!(r, MeraSkipResult::OutOfRegion);
    }

    #[test]
    fn dispatcher_step_large_step_with_air_cell() {
        let mut p = MeraPyramid::new();
        let cell = FieldCell::default();
        let key = MortonKey::encode(0, 0, 0).unwrap();
        p.insert_at(CellTier::T1Mid, key, cell).unwrap();
        let d = MeraSkipDispatcher::new(&p).with_fovea_scale(0.01);
        let r = d.step_at([0.0, 0.0, 0.0]);
        assert!(matches!(r, MeraSkipResult::LargeStep { .. }));
    }

    #[test]
    fn dispatcher_step_bisection_refine_at_surface() {
        let mut p = MeraPyramid::new();
        let mut cell = FieldCell::default();
        cell.density = 1.0;
        // Insert a surface-bearing cell at the fine tier ONLY ; coarser tiers
        // are empty so they return SummaryBound::none() (out of region for them).
        let key = MortonKey::encode(0, 0, 0).unwrap();
        p.insert_at(CellTier::T0Fovea, key, cell).unwrap();
        let d = MeraSkipDispatcher::new(&p).with_fovea_scale(0.01);
        let r = d.step_at([0.0, 0.0, 0.0]);
        // Surface-bearing finest cell + tier-coverage means refine.
        assert_eq!(r, MeraSkipResult::BisectionRefine);
    }

    #[test]
    fn coarsest_safe_tier_is_finest_when_only_fovea_covers() {
        let mut p = MeraPyramid::new();
        let cell = FieldCell::default();
        let key = MortonKey::encode(0, 0, 0).unwrap();
        p.insert_at(CellTier::T0Fovea, key, cell).unwrap();
        let d = MeraSkipDispatcher::new(&p).with_fovea_scale(0.01);
        let t = d.coarsest_safe_tier([0.0, 0.0, 0.0]);
        assert_eq!(t, CellTier::T0Fovea);
    }

    #[test]
    fn coarsest_safe_tier_uses_horizon_when_available() {
        let mut p = MeraPyramid::new();
        let cell = FieldCell::default();
        let key = MortonKey::encode(0, 0, 0).unwrap();
        p.insert_at(CellTier::T3Horizon, key, cell).unwrap();
        let d = MeraSkipDispatcher::new(&p).with_fovea_scale(0.01);
        let t = d.coarsest_safe_tier([0.0, 0.0, 0.0]);
        assert_eq!(t, CellTier::T3Horizon);
    }

    #[test]
    fn world_to_morton_negative_is_none() {
        let p = MeraPyramid::new();
        let d = MeraSkipDispatcher::new(&p);
        assert!(d.world_to_morton_key([-1.0, 0.0, 0.0], CellTier::T0Fovea).is_none());
    }

    #[test]
    fn total_summary_cells_zero_on_empty() {
        let p = MeraPyramid::new();
        let d = MeraSkipDispatcher::new(&p);
        assert_eq!(d.total_summary_cells(), 0);
        assert_eq!(d.per_tier_summary_cells(), [0, 0, 0, 0]);
    }
}
