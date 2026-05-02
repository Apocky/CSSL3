//! § spatial_index — uniform-grid acceleration for crystal-near queries.
//!
//! § T11-W18-B-PERF · canonical : `Labyrinth of Apocalypse/systems/alien_materialization.csl`
//!
//! § THE PROBLEM
//!
//! Substrate-resonance pixel-field walks every pixel through a ray of
//! `RAY_SAMPLES` (=8) sample points, and at each sample asks "what crystal
//! is near here?". Stage-0 ray.rs answered that with a brute-force linear
//! scan : O(P × S × C) where P = pixels, S = ray-samples-per-pixel,
//! C = crystals. At 256×256 pixels × 8 samples × 1000 crystals that's
//! 524M tests per frame — far over the 120Hz budget (8.3 ms).
//!
//! § THE FIX (without abandoning replay-determinism)
//!
//! Bucket every crystal into a uniform 3D grid keyed by floor(world / cell).
//! For each near-query :
//!
//!   1. Compute the inclusive cell-range covering [center − radius − max_extent,
//!      center + radius + max_extent]. The max-extent pad guarantees that a
//!      crystal whose bounding sphere overlaps the query-radius is never
//!      missed even if its center lies outside the query-radius cell-range.
//!   2. Walk the cells in deterministic ascending-cell-key order.
//!   3. For each crystal-id in each visited cell, perform the same dist-sq
//!      check ray.rs::crystals_near already performs (shared truth).
//!
//! Bucket iteration uses Vec<usize> per cell (insertion-order ≡ deterministic).
//! The cell-map is a BTreeMap<(i32,i32,i32), Vec<usize>> ; BTreeMap iteration
//! is sorted-key-order, which preserves replay-stability across runs.
//!
//! § COMPLEXITY
//!
//! With cell-size = 2000mm (≈ default crystal extent) and crystals roughly
//! uniformly distributed over a 64×64×64 m room, expected crystals-per-cell
//! is ≪ 1, and per-query cell-count is bounded by ⌈radius / cell⌉^3 ≈ 27
//! cells for the canonical 1500mm near-radius. Net :
//!   per-query work : O( (radius / cell)^3 × avg-crystals-per-cell )
//!   replaces       : O(C) brute force
//! Net speedup at 1000 crystals : ≈ 30×.
//!
//! § DETERMINISM CONTRACT
//!
//! `crystals_near_grid` returns the SAME SET of crystal-indices as
//! `ray::crystals_near` for the same inputs. Order is BTreeMap-key-major +
//! within-cell insertion-order ; ray-version is ascending-crystal-index.
//! Tests assert SET equality, not order — pixel_field consumers iterate
//! all returned indices and accumulate via associative ops, so order is
//! irrelevant downstream.

use cssl_host_crystallization::{Crystal, WorldPos};

use std::collections::BTreeMap;

/// Cell side-length in millimeters. Chosen to roughly equal the default
/// `CRYSTAL_DEFAULT_EXTENT_MM` (2000mm) so each cell fits ≈ 1 crystal at
/// dense scenes and ≪ 1 crystal at typical scenes.
pub const CELL_SIZE_MM: i32 = 2000;

/// (cell_x, cell_y, cell_z) — integer-world cell coordinates.
pub type CellKey = (i32, i32, i32);

/// Uniform 3D bucketing over crystal positions. Build once per frame (fast :
/// O(C) BTreeMap-inserts) ; query thousands of times (each query
/// O(cells-in-bbox)).
#[derive(Debug, Default, Clone)]
pub struct UniformGrid {
    /// (cx, cy, cz) → indices-into-crystals (stable insertion-order).
    cells: BTreeMap<CellKey, Vec<usize>>,
    /// Largest crystal-extent observed during build. Queries pad their
    /// cell-range bbox by this to be conservative-correct.
    max_extent_mm: i32,
}

impl UniformGrid {
    /// Build a grid from the crystal slice. Single-threaded by contract :
    /// determinism rests on insertion-order = ascending crystal-index.
    pub fn build(crystals: &[Crystal]) -> Self {
        let mut grid = Self {
            cells: BTreeMap::new(),
            max_extent_mm: 0,
        };
        for (i, c) in crystals.iter().enumerate() {
            let key = cell_key(c.world_pos);
            grid.cells.entry(key).or_default().push(i);
            if c.extent_mm > grid.max_extent_mm {
                grid.max_extent_mm = c.extent_mm;
            }
        }
        grid
    }

    /// Number of populated cells (diagnostics + tests).
    pub fn n_cells(&self) -> usize {
        self.cells.len()
    }

    /// Largest crystal-extent observed at build-time.
    pub fn max_extent_mm(&self) -> i32 {
        self.max_extent_mm
    }

    /// Iterate crystal-indices whose bounding-spheres overlap the query
    /// sphere `(world, radius_mm)`. Output order is BTreeMap-key-major,
    /// then within-cell insertion-order. Predicate identical to
    /// `ray::crystals_near` so set-equivalence is guaranteed.
    pub fn crystals_near_grid(
        &self,
        crystals: &[Crystal],
        world: WorldPos,
        radius_mm: i32,
    ) -> Vec<usize> {
        let radius_sq = (radius_mm as i64) * (radius_mm as i64);

        // BBox padding : a crystal whose CENTER is up to (max_extent_mm +
        // radius_mm) away can still pass the test, so widen the cell-bbox
        // by that amount.
        let pad = (radius_mm as i64) + (self.max_extent_mm as i64);
        let pad_i32: i32 = if pad >= i32::MAX as i64 {
            i32::MAX
        } else {
            pad as i32
        };

        let lo = WorldPos::new(
            world.x_mm.saturating_sub(pad_i32),
            world.y_mm.saturating_sub(pad_i32),
            world.z_mm.saturating_sub(pad_i32),
        );
        let hi = WorldPos::new(
            world.x_mm.saturating_add(pad_i32),
            world.y_mm.saturating_add(pad_i32),
            world.z_mm.saturating_add(pad_i32),
        );
        let (cx_lo, cy_lo, cz_lo) = cell_key(lo);
        let (cx_hi, cy_hi, cz_hi) = cell_key(hi);

        // Pre-size : 27 = 3³ typical bbox-cell count at radius ≈ cell-size.
        let mut out: Vec<usize> = Vec::with_capacity(32);

        // BTreeMap range over a 3D-tuple key includes keys that are lex-
        // sorted between lo_key and hi_key — but lex-sort allows e.g.
        // (cx_lo, cy_hi+1, cz_lo) which is lex-greater than lo_key but
        // out of bbox on the y axis. Filter explicitly per axis.
        let lo_key = (cx_lo, cy_lo, cz_lo);
        let hi_key = (cx_hi, cy_hi, cz_hi);
        for ((cx, cy, cz), bucket) in self.cells.range(lo_key..=hi_key) {
            if *cx < cx_lo || *cx > cx_hi {
                continue;
            }
            if *cy < cy_lo || *cy > cy_hi {
                continue;
            }
            if *cz < cz_lo || *cz > cz_hi {
                continue;
            }

            for &i in bucket {
                let c = &crystals[i];
                let dx = (c.world_pos.x_mm - world.x_mm) as i64;
                let dy = (c.world_pos.y_mm - world.y_mm) as i64;
                let dz = (c.world_pos.z_mm - world.z_mm) as i64;
                let d_sq = dx * dx + dy * dy + dz * dz;
                let r_total = (c.extent_mm as i64) + (radius_mm as i64);
                let r_total_sq = r_total * r_total;
                if d_sq <= r_total_sq.min(radius_sq * 4) {
                    out.push(i);
                }
            }
        }

        out
    }
}

/// Compute the integer cell coordinates that contain `pos`. Negative
/// world-coords floor toward −∞ (matches mathematical floor-division).
#[inline]
pub fn cell_key(pos: WorldPos) -> CellKey {
    (
        floor_div(pos.x_mm, CELL_SIZE_MM),
        floor_div(pos.y_mm, CELL_SIZE_MM),
        floor_div(pos.z_mm, CELL_SIZE_MM),
    )
}

/// Integer division that floors toward −∞ (Rust's `/` truncates toward 0).
/// Required so that pos.x_mm = -1 maps into cell −1, not 0.
#[inline]
fn floor_div(a: i32, b: i32) -> i32 {
    let q = a / b;
    let r = a % b;
    if (r != 0) && ((r < 0) != (b < 0)) {
        q - 1
    } else {
        q
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ray::crystals_near;
    use cssl_host_crystallization::{CrystalClass, WorldPos};

    fn mk(seed: u64, x: i32, y: i32, z: i32) -> Crystal {
        Crystal::allocate(CrystalClass::Object, seed, WorldPos::new(x, y, z))
    }

    #[test]
    fn floor_div_handles_negatives() {
        assert_eq!(floor_div(-1, 2000), -1);
        assert_eq!(floor_div(-2000, 2000), -1);
        assert_eq!(floor_div(-2001, 2000), -2);
        assert_eq!(floor_div(0, 2000), 0);
        assert_eq!(floor_div(1999, 2000), 0);
        assert_eq!(floor_div(2000, 2000), 1);
    }

    #[test]
    fn empty_grid_is_empty() {
        let grid = UniformGrid::build(&[]);
        assert_eq!(grid.n_cells(), 0);
        assert_eq!(grid.max_extent_mm(), 0);
    }

    #[test]
    fn single_crystal_one_cell() {
        let crystals = vec![mk(1, 100, 200, 300)];
        let grid = UniformGrid::build(&crystals);
        assert_eq!(grid.n_cells(), 1);
        assert_eq!(grid.max_extent_mm(), crystals[0].extent_mm);
    }

    #[test]
    fn grid_query_set_equivalent_to_brute_force() {
        // 100 crystals spread across a 32×32×32m volume.
        let mut crystals = Vec::new();
        for i in 0..100u64 {
            let x = ((i as i32) * 311) % 32_000 - 16_000;
            let y = ((i as i32) * 757) % 32_000 - 16_000;
            let z = ((i as i32) * 1129) % 32_000 - 16_000;
            crystals.push(mk(i, x, y, z));
        }
        let grid = UniformGrid::build(&crystals);

        for query_seed in 0_i32..20 {
            let qx = (query_seed * 953) % 32_000 - 16_000;
            let qy = (query_seed * 1429) % 32_000 - 16_000;
            let qz = (query_seed * 1801) % 32_000 - 16_000;
            let world = WorldPos::new(qx, qy, qz);

            for &radius in &[500_i32, 1500, 4000, 8000] {
                let brute: std::collections::BTreeSet<usize> =
                    crystals_near(&crystals, world, radius).collect();
                let grid_out: std::collections::BTreeSet<usize> = grid
                    .crystals_near_grid(&crystals, world, radius)
                    .into_iter()
                    .collect();
                assert_eq!(
                    brute, grid_out,
                    "grid != brute @ query=({},{},{}) r={}",
                    qx, qy, qz, radius
                );
            }
        }
    }

    #[test]
    fn grid_query_at_origin_finds_local_only() {
        let crystals = vec![
            mk(1, 0, 0, 0),
            mk(2, 100_000, 0, 0), // far away — outside bbox
            mk(3, 500, 0, 0),     // in same cell or adjacent
        ];
        let grid = UniformGrid::build(&crystals);
        let near = grid.crystals_near_grid(&crystals, WorldPos::new(0, 0, 0), 100);
        // Crystal 0 + 2 are within (extent + radius) of origin ; crystal 1
        // (100_000 mm away) is not.
        assert!(near.contains(&0));
        assert!(!near.contains(&1));
    }

    #[test]
    fn grid_build_is_deterministic() {
        let crystals: Vec<_> = (0..50u64).map(|i| mk(i, i as i32 * 137, 0, 0)).collect();
        let g1 = UniformGrid::build(&crystals);
        let g2 = UniformGrid::build(&crystals);
        assert_eq!(g1.n_cells(), g2.n_cells());
        let q = WorldPos::new(0, 0, 0);
        for r in [500_i32, 2000, 8000] {
            let a = g1.crystals_near_grid(&crystals, q, r);
            let b = g2.crystals_near_grid(&crystals, q, r);
            assert_eq!(a, b);
        }
    }
}
