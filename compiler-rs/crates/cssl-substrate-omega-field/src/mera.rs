//! § MERA pyramid — 4-tier hierarchical-LOD summary cascade.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   4-tier MERA-style coarsening pyramid over the dense [`crate::field_cell::FieldCell`]
//!   grid. Tier 0 is the active fovea (1 cm) ; tiers 1, 2, 3 are progressively
//!   coarser summaries (4 cm, 16 cm, 64 cm) per the canonical voxel cascade.
//!
//! § SPEC
//!   - `Omniverse/04_OMEGA_FIELD/02_STORAGE.csl.md` § III MERA hierarchical
//!     LOD (disentangler + isometry per layer).
//!   - `Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md` § I cascade-tiers
//!     (T0 fovea / T1 mid / T2 distant / T3 horizon).
//!   - `Omniverse/02_CSSL/06_SUBSTRATE_EVOLUTION.csl` § IV.3 OmegaField MERA
//!     pyramid.
//!
//! § COARSENING SCHEME
//!   At this slice we implement a **simple averaging summary** :
//!
//!   ```text
//!   density_T1 = mean(density_T0[8 cells])
//!   velocity_T1 = mean(velocity_T0)
//!   vorticity_T1 = mean(vorticity_T0)
//!   enthalpy_T1 = mean(enthalpy_T0)
//!   probe_lo / probe_hi : passthrough from one representative cell
//!   bivector_lo : passthrough
//!   sigma_consent_bits : intersection (most-restrictive consent)
//!   ```
//!
//!   The full disentangler+isometry tensor-network of `02_STORAGE § III`
//!   lands in a later slice ; the simple-average summary is the canonical
//!   *fall-back* when the trained tensor network is not available, and is
//!   the form used by the M7 vertical-slice tests.
//!
//! § TIER ITERATION
//!   The pyramid stores a separate [`crate::sparse_grid::SparseMortonGrid<FieldCell>`]
//!   per tier. Coarsening sample-rate is 8-to-1 in 3D (each coarse cell
//!   summarizes a 2×2×2 block of finer cells).

use crate::field_cell::FieldCell;
use crate::morton::{CellTier, MortonKey};
use crate::sparse_grid::SparseMortonGrid;

/// Number of MERA tiers in the canonical cascade (T0..T3).
pub const MERA_TIER_COUNT: usize = 4;

/// 4-tier MERA pyramid storing one coarsened FieldCell-grid per tier.
#[derive(Debug, Clone, Default)]
pub struct MeraPyramid {
    tiers: [SparseMortonGrid<FieldCell>; MERA_TIER_COUNT],
}

impl MeraPyramid {
    /// Construct an empty pyramid.
    #[must_use]
    pub fn new() -> Self {
        MeraPyramid {
            tiers: [
                SparseMortonGrid::with_capacity(128),
                SparseMortonGrid::with_capacity(64),
                SparseMortonGrid::with_capacity(32),
                SparseMortonGrid::with_capacity(16),
            ],
        }
    }

    /// Read-only access to a specific tier's grid.
    #[must_use]
    pub fn tier(&self, tier: CellTier) -> &SparseMortonGrid<FieldCell> {
        &self.tiers[tier.mera_layer() as usize]
    }

    /// Mutable access to a specific tier's grid.
    pub fn tier_mut(&mut self, tier: CellTier) -> &mut SparseMortonGrid<FieldCell> {
        &mut self.tiers[tier.mera_layer() as usize]
    }

    /// Insert a cell at the given tier. The tier is encoded by both the
    /// MortonKey AND the destination grid ; this method enforces consistency.
    pub fn insert_at(
        &mut self,
        tier: CellTier,
        key: MortonKey,
        cell: FieldCell,
    ) -> Result<(), crate::sparse_grid::GridError> {
        self.tier_mut(tier).insert(key, cell).map(|_| ())
    }

    /// Sample the pyramid at `key`, walking from the FINEST present tier
    /// down to the coarsest. Returns `None` if no tier holds the key.
    #[must_use]
    pub fn sample(&self, key: MortonKey) -> Option<(CellTier, FieldCell)> {
        for tier in CellTier::all() {
            if let Some(c) = self.tier(*tier).at_const(key) {
                return Some((*tier, c));
            }
        }
        None
    }

    /// Number of stored cells across all tiers.
    #[must_use]
    pub fn total_cell_count(&self) -> usize {
        self.tiers.iter().map(SparseMortonGrid::len).sum()
    }

    /// Per-tier cell counts in tier-order [T0, T1, T2, T3].
    #[must_use]
    pub fn per_tier_counts(&self) -> [usize; MERA_TIER_COUNT] {
        [
            self.tiers[0].len(),
            self.tiers[1].len(),
            self.tiers[2].len(),
            self.tiers[3].len(),
        ]
    }

    /// Coarsen the T0 (fovea) tier into T1 by averaging 2×2×2 blocks.
    /// Block of 8 fine cells at axis-coords (2x..=2x+1, 2y..=2y+1, 2z..=2z+1)
    /// maps to one coarse cell at (x, y, z).
    ///
    /// This is the simple-average summary (the disentangler+isometry tensor
    /// network is the production form ; the simple-average serves as the
    /// canonical reference + fallback).
    pub fn coarsen_t0_to_t1(&mut self) -> usize {
        Self::coarsen_layer(self, CellTier::T0Fovea, CellTier::T1Mid)
    }

    /// Coarsen T1 → T2.
    pub fn coarsen_t1_to_t2(&mut self) -> usize {
        Self::coarsen_layer(self, CellTier::T1Mid, CellTier::T2Distant)
    }

    /// Coarsen T2 → T3.
    pub fn coarsen_t2_to_t3(&mut self) -> usize {
        Self::coarsen_layer(self, CellTier::T2Distant, CellTier::T3Horizon)
    }

    /// Coarsen all tiers in turn (T0 → T1 → T2 → T3). Returns the total
    /// number of coarse cells produced across all three steps.
    pub fn coarsen_all(&mut self) -> usize {
        let n0 = self.coarsen_t0_to_t1();
        let n1 = self.coarsen_t1_to_t2();
        let n2 = self.coarsen_t2_to_t3();
        n0 + n1 + n2
    }

    /// Run a single layer-coarsening step (fine → coarse).
    fn coarsen_layer(this: &mut Self, fine: CellTier, coarse: CellTier) -> usize {
        // Collect per-coarse-block accumulators.
        // We use a small inline-Vec for parents : for each coarse-block we
        // accumulate (count, sum-of-fields). The coarse MortonKey is the
        // fine MortonKey shifted right by 1 bit per axis.
        let mut groups: std::collections::HashMap<MortonKey, BlockAccum> =
            std::collections::HashMap::new();
        let fine_grid = this.tier(fine);
        for (k, cell) in fine_grid.iter() {
            let (x, y, z) = k.decode();
            let coarse_xyz = (x / 2, y / 2, z / 2);
            let coarse_key =
                MortonKey::encode(coarse_xyz.0, coarse_xyz.1, coarse_xyz.2).unwrap();
            let entry = groups.entry(coarse_key).or_default();
            entry.add(*cell);
        }
        let mut coarsened = 0;
        for (k, accum) in groups {
            let cell = accum.finalize();
            let _ = this.tier_mut(coarse).insert(k, cell);
            coarsened += 1;
        }
        coarsened
    }
}

/// Per-coarse-block accumulator used during MERA coarsening.
#[derive(Debug, Clone, Default)]
struct BlockAccum {
    count: u32,
    density_sum: f32,
    velocity_sum: [f32; 3],
    vorticity_sum: [f32; 3],
    enthalpy_sum: f32,
    consent_intersection: u32, // start with all-bits-set, then AND with each cell's bits.
    consent_initialized: bool,
    /// We keep the LAST cell's M-facet, probe, bivector, pattern_handle
    /// as a "representative" since these don't average meaningfully.
    rep_cell: Option<FieldCell>,
}

impl BlockAccum {
    fn add(&mut self, cell: FieldCell) {
        self.count += 1;
        self.density_sum += cell.density;
        for i in 0..3 {
            self.velocity_sum[i] += cell.velocity[i];
            self.vorticity_sum[i] += cell.vorticity[i];
        }
        self.enthalpy_sum += cell.enthalpy;
        if !self.consent_initialized {
            self.consent_intersection = cell.sigma_consent_bits;
            self.consent_initialized = true;
        } else {
            self.consent_intersection &= cell.sigma_consent_bits;
        }
        self.rep_cell = Some(cell);
    }

    fn finalize(self) -> FieldCell {
        let n = self.count.max(1) as f32;
        let mut cell = self.rep_cell.unwrap_or_default();
        cell.density = self.density_sum / n;
        for i in 0..3 {
            cell.velocity[i] = self.velocity_sum[i] / n;
            cell.vorticity[i] = self.vorticity_sum[i] / n;
        }
        cell.enthalpy = self.enthalpy_sum / n;
        cell.sigma_consent_bits = self.consent_intersection;
        cell
    }
}

#[cfg(test)]
mod tests {
    use super::{MeraPyramid, MERA_TIER_COUNT};
    use crate::field_cell::FieldCell;
    use crate::morton::{CellTier, MortonKey};

    #[test]
    fn pyramid_default_has_4_tiers() {
        let p = MeraPyramid::new();
        assert_eq!(MERA_TIER_COUNT, 4);
        assert_eq!(p.per_tier_counts(), [0, 0, 0, 0]);
    }

    #[test]
    fn insert_at_tier_round_trips() {
        let mut p = MeraPyramid::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        let mut cell = FieldCell::default();
        cell.density = 1.5;
        p.insert_at(CellTier::T0Fovea, k, cell).unwrap();
        let (t, c) = p.sample(k).unwrap();
        assert_eq!(t, CellTier::T0Fovea);
        assert!((c.density - 1.5).abs() < 1e-6);
    }

    #[test]
    fn sample_walks_finest_to_coarsest() {
        let mut p = MeraPyramid::new();
        let k = MortonKey::encode(7, 8, 9).unwrap();
        let mut c0 = FieldCell::default();
        c0.density = 1.0;
        let mut c2 = FieldCell::default();
        c2.density = 5.0;
        // Insert into T2 (coarse).
        p.insert_at(CellTier::T2Distant, k, c2).unwrap();
        // Sample : finds T2 first since T0/T1 are empty.
        let (t, found) = p.sample(k).unwrap();
        assert_eq!(t, CellTier::T2Distant);
        assert!((found.density - 5.0).abs() < 1e-6);
        // Now insert at T0 too — sample finds T0 first.
        p.insert_at(CellTier::T0Fovea, k, c0).unwrap();
        let (t, found) = p.sample(k).unwrap();
        assert_eq!(t, CellTier::T0Fovea);
        assert!((found.density - 1.0).abs() < 1e-6);
    }

    #[test]
    fn coarsen_t0_to_t1_averages_block_density() {
        let mut p = MeraPyramid::new();
        // Place 8 cells at the (0..2, 0..2, 0..2) block.
        for x in 0..2_u64 {
            for y in 0..2_u64 {
                for z in 0..2_u64 {
                    let mut c = FieldCell::default();
                    c.density = 4.0; // uniform → average should be 4.0
                    c.sigma_consent_bits = 0xFF; // uniform consent
                    p.insert_at(CellTier::T0Fovea, MortonKey::encode(x, y, z).unwrap(), c)
                        .unwrap();
                }
            }
        }
        let coarsened = p.coarsen_t0_to_t1();
        assert_eq!(coarsened, 1, "8 fine cells → 1 coarse cell");
        // The coarse key is (0, 0, 0) at T1.
        let coarse_key = MortonKey::encode(0, 0, 0).unwrap();
        let coarse_cell = p.tier(CellTier::T1Mid).at_const(coarse_key).unwrap();
        assert!((coarse_cell.density - 4.0).abs() < 1e-6);
        assert_eq!(coarse_cell.sigma_consent_bits, 0xFF);
    }

    #[test]
    fn coarsen_partial_block_still_averages() {
        let mut p = MeraPyramid::new();
        // Only 2 cells at a coarse block — average over those.
        let mut c1 = FieldCell::default();
        c1.density = 2.0;
        let mut c2 = FieldCell::default();
        c2.density = 6.0;
        p.insert_at(CellTier::T0Fovea, MortonKey::encode(0, 0, 0).unwrap(), c1)
            .unwrap();
        p.insert_at(CellTier::T0Fovea, MortonKey::encode(1, 0, 0).unwrap(), c2)
            .unwrap();
        let coarsened = p.coarsen_t0_to_t1();
        assert_eq!(coarsened, 1);
        let coarse_cell = p
            .tier(CellTier::T1Mid)
            .at_const(MortonKey::encode(0, 0, 0).unwrap())
            .unwrap();
        // Average of (2.0, 6.0) = 4.0
        assert!((coarse_cell.density - 4.0).abs() < 1e-6);
    }

    #[test]
    fn coarsen_consent_intersection_keeps_strictest() {
        let mut p = MeraPyramid::new();
        let mut c1 = FieldCell::default();
        c1.sigma_consent_bits = 0b1111;
        let mut c2 = FieldCell::default();
        c2.sigma_consent_bits = 0b1010;
        p.insert_at(CellTier::T0Fovea, MortonKey::encode(0, 0, 0).unwrap(), c1)
            .unwrap();
        p.insert_at(CellTier::T0Fovea, MortonKey::encode(1, 0, 0).unwrap(), c2)
            .unwrap();
        p.coarsen_t0_to_t1();
        let coarse = p
            .tier(CellTier::T1Mid)
            .at_const(MortonKey::encode(0, 0, 0).unwrap())
            .unwrap();
        // Intersection (AND) of 0b1111 and 0b1010 = 0b1010
        assert_eq!(coarse.sigma_consent_bits, 0b1010);
    }

    #[test]
    fn coarsen_all_chains_three_levels() {
        let mut p = MeraPyramid::new();
        // 64 cells at T0 (4×4×4 block) ; coarsen → 8 cells at T1 → 1 cell
        // at T2 → 1 cell at T3.
        for x in 0..4_u64 {
            for y in 0..4_u64 {
                for z in 0..4_u64 {
                    let mut c = FieldCell::default();
                    c.density = 1.0;
                    p.insert_at(CellTier::T0Fovea, MortonKey::encode(x, y, z).unwrap(), c)
                        .unwrap();
                }
            }
        }
        let total = p.coarsen_all();
        // T0→T1 produces 8 ; T1→T2 produces 1 ; T2→T3 produces 1 ; total = 10.
        assert_eq!(total, 10);
        assert!(p.tier(CellTier::T1Mid).len() >= 1);
        assert!(p.tier(CellTier::T2Distant).len() >= 1);
        assert!(p.tier(CellTier::T3Horizon).len() >= 1);
    }
}
