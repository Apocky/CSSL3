// § field.rs · NdField<T, N> · sparse N-D cell-store
// ══════════════════════════════════════════════════════════════════
// § Stage-0 sparse-store : HashMap<NdCoord<N>, T>. Const-generic N
// keeps the API stable as we graduate to chunk-tile + Morton-cascade
// in stage-1 without breaking call-sites.
//
// § Crystals can have N-D EXTENTS : `insert_extent()` writes a payload
// into a hyper-rectangle bounded by inclusive lo/hi corners. A single
// crystal can therefore "exist" in 3 spatial cells AND 2 mood-bands
// AND 1 temporal-tick — non-conventional 7D hyper-volume.
//
// § Lens-aware queries : `query_visible_through_lens` walks every cell
// in the field, projects through the supplied lens, and yields only
// the [x,y,z] + payload-ref pairs the observer is consented to see.
// ══════════════════════════════════════════════════════════════════

use std::collections::HashMap;

use crate::coord::{CoordError, NdCoord};
use crate::lens::{ConsentError, DimensionalLens};

/// § Sparse N-D ω-field cell-store.
/// `T` = arbitrary cell-payload (substrate-Σ-mask, crystal-handle, mood-tag…).
#[derive(Clone, Debug)]
pub struct NdField<T, const N: usize> {
    cells: HashMap<NdCoord<N>, T>,
}

/// § Stats useful for telemetry + tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NdFieldStats {
    pub cell_count: usize,
}

impl<T, const N: usize> Default for NdField<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> NdField<T, N> {
    /// § Empty field.
    pub fn new() -> Self {
        Self {
            cells: HashMap::new(),
        }
    }

    /// § Reserve capacity hint.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            cells: HashMap::with_capacity(cap),
        }
    }

    /// § Insert a single cell. Returns the prior payload if it existed.
    pub fn insert(&mut self, coord: NdCoord<N>, payload: T) -> Option<T> {
        self.cells.insert(coord, payload)
    }

    /// § Read a cell.
    pub fn get(&self, coord: &NdCoord<N>) -> Option<&T> {
        self.cells.get(coord)
    }

    /// § Mutable read.
    pub fn get_mut(&mut self, coord: &NdCoord<N>) -> Option<&mut T> {
        self.cells.get_mut(coord)
    }

    /// § Remove a cell.
    pub fn remove(&mut self, coord: &NdCoord<N>) -> Option<T> {
        self.cells.remove(coord)
    }

    /// § Clear all cells.
    pub fn clear(&mut self) {
        self.cells.clear();
    }

    /// § Cell-count.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// § Empty-check.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// § Stats snapshot.
    pub fn stats(&self) -> NdFieldStats {
        NdFieldStats {
            cell_count: self.cells.len(),
        }
    }

    /// § Iterate all (coord, payload) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&NdCoord<N>, &T)> {
        self.cells.iter()
    }
}

impl<T: Clone, const N: usize> NdField<T, N> {
    /// § Insert a hyper-rectangular EXTENT : `lo` and `hi` are inclusive
    /// corners. Every coord between (component-wise) is filled with
    /// `payload.clone()`.
    ///
    /// Returns the count of cells written, or `CoordError::AxisOutOfRange`
    /// if any axis i has lo > hi.
    pub fn insert_extent(
        &mut self,
        lo: &NdCoord<N>,
        hi: &NdCoord<N>,
        payload: &T,
    ) -> Result<usize, CoordError> {
        // Validate lo ≤ hi component-wise. Reject inverted boxes.
        for (i, (lo_v, hi_v)) in lo.axes().iter().zip(hi.axes().iter()).enumerate() {
            if lo_v > hi_v {
                return Err(CoordError::AxisOutOfRange { axis: i as u8, n: N });
            }
        }

        // Iterative odometer-style traversal across all N axes.
        let mut total: i64 = 1;
        for (lo_v, hi_v) in lo.axes().iter().zip(hi.axes().iter()) {
            let span = i64::from(*hi_v) - i64::from(*lo_v) + 1;
            total = total.saturating_mul(span);
        }
        // Substrate-discipline : refuse if extent is absurd.
        if total <= 0 || total > 1_000_000 {
            return Err(CoordError::AxisOutOfRange { axis: 0, n: N });
        }

        let total_usize = total as usize;
        let mut written = 0usize;
        let mut current = *lo.axes();
        for _ in 0..total_usize {
            self.cells
                .insert(NdCoord::from_axes(current), payload.clone());
            written += 1;
            // Increment odometer.
            let mut axis = 0;
            loop {
                if axis >= N {
                    break;
                }
                if current[axis] < hi.axes()[axis] {
                    current[axis] += 1;
                    break;
                }
                current[axis] = lo.axes()[axis];
                axis += 1;
            }
        }
        Ok(written)
    }
}

impl<T, const N: usize> NdField<T, N> {
    /// § Yield every cell that's visible through the supplied lens, mapped
    /// down to its 3D-projection. Refuses entirely if the lens is
    /// unconsented or revoked.
    pub fn query_visible_through_lens(
        &self,
        lens: &DimensionalLens,
    ) -> Result<Vec<([i32; 3], &T)>, ConsentError> {
        let mut out = Vec::with_capacity(self.cells.len());
        for (coord, payload) in &self.cells {
            let xyz = lens.project_to_3d(coord)?;
            out.push((xyz, payload));
        }
        Ok(out)
    }

    /// § Yield cells whose projection lies inside an inclusive 3D-AABB.
    /// Useful for the conventional spatial-renderer that only cares about
    /// what's currently in the camera frustum.
    pub fn query_box_through_lens(
        &self,
        lens: &DimensionalLens,
        lo: [i32; 3],
        hi: [i32; 3],
    ) -> Result<Vec<([i32; 3], &T)>, ConsentError> {
        let mut out = Vec::new();
        for (coord, payload) in &self.cells {
            let xyz = lens.project_to_3d(coord)?;
            if xyz[0] >= lo[0]
                && xyz[0] <= hi[0]
                && xyz[1] >= lo[1]
                && xyz[1] <= hi[1]
                && xyz[2] >= lo[2]
                && xyz[2] <= hi[2]
            {
                out.push((xyz, payload));
            }
        }
        Ok(out)
    }
}

// ══════════════════════════════════════════════════════════════════
// § tests
// ══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lens::{spatial_xyz_for_stage0, DimensionalLens};

    #[test]
    fn insert_get_remove() {
        let mut f: NdField<u32, 4> = NdField::new();
        let c: NdCoord<4> = NdCoord::from_axes([1, 2, 3, 4]);
        assert!(f.is_empty());
        assert_eq!(f.insert(c, 42), None);
        assert_eq!(f.get(&c), Some(&42));
        assert_eq!(f.len(), 1);
        assert_eq!(f.remove(&c), Some(42));
        assert!(f.is_empty());
    }

    #[test]
    fn insert_overwrites_returns_prior() {
        let mut f: NdField<u32, 4> = NdField::new();
        let c: NdCoord<4> = NdCoord::from_axes([0, 0, 0, 0]);
        f.insert(c, 1);
        assert_eq!(f.insert(c, 2), Some(1));
        assert_eq!(f.get(&c), Some(&2));
    }

    #[test]
    fn insert_extent_fills_hyperbox() {
        let mut f: NdField<&'static str, 4> = NdField::new();
        let lo: NdCoord<4> = NdCoord::from_axes([0, 0, 0, 0]);
        let hi: NdCoord<4> = NdCoord::from_axes([1, 1, 1, 1]);
        let written = f.insert_extent(&lo, &hi, &"crystal").unwrap();
        // 2 × 2 × 2 × 2 = 16 cells.
        assert_eq!(written, 16);
        assert_eq!(f.len(), 16);
        // Spot-check a corner.
        assert_eq!(f.get(&NdCoord::from_axes([1, 0, 1, 0])), Some(&"crystal"));
    }

    #[test]
    fn insert_extent_rejects_inverted_box() {
        let mut f: NdField<u32, 3> = NdField::new();
        let lo: NdCoord<3> = NdCoord::from_axes([5, 0, 0]);
        let hi: NdCoord<3> = NdCoord::from_axes([0, 0, 0]);
        assert!(f.insert_extent(&lo, &hi, &1).is_err());
    }

    #[test]
    fn query_visible_through_lens_projects_all() {
        let mut f: NdField<u32, 8> = NdField::new();
        f.insert(NdCoord::from_axes([1, 2, 3, 4, 5, 6, 7, 8]), 100);
        f.insert(NdCoord::from_axes([10, 20, 30, 40, 50, 60, 70, 80]), 200);
        let lens = spatial_xyz_for_stage0();
        let mut visible = f.query_visible_through_lens(&lens).unwrap();
        visible.sort_by_key(|(xyz, _)| (xyz[0], xyz[1], xyz[2]));
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].0, [1, 2, 3]);
        assert_eq!(visible[1].0, [10, 20, 30]);
    }

    #[test]
    fn query_box_through_lens_filters() {
        let mut f: NdField<u32, 8> = NdField::new();
        f.insert(NdCoord::from_axes([5, 5, 5, 0, 0, 0, 0, 0]), 1);
        f.insert(NdCoord::from_axes([100, 100, 100, 0, 0, 0, 0, 0]), 2);
        let lens = spatial_xyz_for_stage0();
        let visible = f
            .query_box_through_lens(&lens, [0, 0, 0], [10, 10, 10])
            .unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].0, [5, 5, 5]);
    }

    #[test]
    fn query_unconsented_refuses() {
        let mut f: NdField<u32, 4> = NdField::new();
        f.insert(NdCoord::from_axes([1, 2, 3, 4]), 1);
        let lens =
            DimensionalLens::unconsented(vec![0, 1, 2, 3], [0, 1, 2], 4).unwrap();
        assert!(f.query_visible_through_lens(&lens).is_err());
    }

    #[test]
    fn const_generic_n_4_compiles_and_works() {
        let mut f: NdField<u8, 4> = NdField::new();
        let c: NdCoord<4> = NdCoord::from_axes([0, 0, 0, 0]);
        f.insert(c, 7);
        assert_eq!(f.get(&c), Some(&7));
    }

    #[test]
    fn const_generic_n_16_compiles_and_works() {
        let mut f: NdField<u8, 16> = NdField::new();
        let c: NdCoord<16> =
            NdCoord::from_axes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        f.insert(c, 99);
        assert_eq!(f.get(&c), Some(&99));
        assert_eq!(f.stats().cell_count, 1);
    }

    #[test]
    fn iter_returns_all_pairs() {
        let mut f: NdField<u32, 3> = NdField::new();
        f.insert(NdCoord::from_axes([0, 0, 0]), 10);
        f.insert(NdCoord::from_axes([1, 1, 1]), 20);
        let collected: Vec<u32> = f.iter().map(|(_, v)| *v).collect();
        assert_eq!(collected.len(), 2);
        assert!(collected.contains(&10));
        assert!(collected.contains(&20));
    }

    #[test]
    fn clear_empties_field() {
        let mut f: NdField<u32, 3> = NdField::new();
        f.insert(NdCoord::origin(), 1);
        assert_eq!(f.len(), 1);
        f.clear();
        assert!(f.is_empty());
    }
}
