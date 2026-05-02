// § coord.rs · NdCoord<N> + arithmetic + bounds
// ══════════════════════════════════════════════════════════════════
// § Const-generic N-D position. Each axis is i32 (signed extents
// support "before-origin" temporal navigation + "below-baseline" mood).
// ══════════════════════════════════════════════════════════════════

use thiserror::Error;

/// § N-D coordinate. `axes[i]` = position along the i-th semantic dimension.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NdCoord<const N: usize> {
    axes: [i32; N],
}

/// § Coord-construction + lens-application errors.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoordError {
    /// Caller indexed past the coordinate's dimensionality.
    #[error("axis {axis} out of range for N={n}")]
    AxisOutOfRange { axis: u8, n: usize },
    /// Lens references an axis that's missing from this coord's source-set.
    #[error("axis {axis} not exposed by current lens")]
    NotInLens { axis: u8 },
    /// Arithmetic would wrap i32 ; substrate refuses silent rollover.
    #[error("arithmetic overflow on axis {axis}")]
    Overflow { axis: u8 },
}

impl<const N: usize> NdCoord<N> {
    /// § Origin = all-zeros. Stage-0 worlds spawn here by default.
    #[inline]
    pub const fn origin() -> Self {
        Self { axes: [0; N] }
    }

    /// § Build directly from an array.
    #[inline]
    pub const fn from_axes(axes: [i32; N]) -> Self {
        Self { axes }
    }

    /// § Borrow the underlying axis-array.
    #[inline]
    pub const fn axes(&self) -> &[i32; N] {
        &self.axes
    }

    /// § Read a single axis, with bounds-check. Returns `AxisOutOfRange` if
    /// `axis as usize >= N`. (Const-generic N already prevents most misuse but
    /// dynamic call-sites need a runtime-check.)
    #[inline]
    pub fn get(&self, axis: u8) -> Result<i32, CoordError> {
        if (axis as usize) >= N {
            Err(CoordError::AxisOutOfRange { axis, n: N })
        } else {
            Ok(self.axes[axis as usize])
        }
    }

    /// § Set a single axis with bounds-check.
    pub fn set(&mut self, axis: u8, value: i32) -> Result<(), CoordError> {
        if (axis as usize) >= N {
            Err(CoordError::AxisOutOfRange { axis, n: N })
        } else {
            self.axes[axis as usize] = value;
            Ok(())
        }
    }

    /// § Component-wise addition with checked-overflow.
    /// Refuses silent i32 wrap (substrate-discipline : no UB-shaped behavior).
    pub fn checked_add(self, other: Self) -> Result<Self, CoordError> {
        let mut out = [0i32; N];
        for ((i, slot), (a, b)) in out
            .iter_mut()
            .enumerate()
            .zip(self.axes.iter().zip(other.axes.iter()))
        {
            match a.checked_add(*b) {
                Some(v) => *slot = v,
                None => return Err(CoordError::Overflow { axis: i as u8 }),
            }
        }
        Ok(Self { axes: out })
    }

    /// § Component-wise subtraction with checked-overflow.
    pub fn checked_sub(self, other: Self) -> Result<Self, CoordError> {
        let mut out = [0i32; N];
        for ((i, slot), (a, b)) in out
            .iter_mut()
            .enumerate()
            .zip(self.axes.iter().zip(other.axes.iter()))
        {
            match a.checked_sub(*b) {
                Some(v) => *slot = v,
                None => return Err(CoordError::Overflow { axis: i as u8 }),
            }
        }
        Ok(Self { axes: out })
    }

    /// § L1 (Manhattan) magnitude across all axes. Returns u64 to avoid
    /// summation-overflow on long-extent fields.
    pub fn manhattan(&self) -> u64 {
        self.axes
            .iter()
            .map(|v| u64::from(v.unsigned_abs()))
            .sum()
    }

    /// § L∞ (Chebyshev) magnitude — largest single-axis displacement.
    pub fn chebyshev(&self) -> u32 {
        self.axes
            .iter()
            .map(|v| v.unsigned_abs())
            .max()
            .unwrap_or(0)
    }

    /// § Squared-L2 across a SUBSET of axes (for spatial-only or mood-only
    /// distance metrics under a particular lens). Returns `Err` if any axis
    /// is out-of-range.
    pub fn squared_distance_along(
        &self,
        other: &Self,
        axes: &[u8],
    ) -> Result<u64, CoordError> {
        let mut acc: u64 = 0;
        for &axis in axes {
            if (axis as usize) >= N {
                return Err(CoordError::AxisOutOfRange { axis, n: N });
            }
            let d = self.axes[axis as usize] - other.axes[axis as usize];
            // i32 → i64 widen prevents 32-bit-square overflow.
            let dd = i64::from(d);
            acc = acc.saturating_add((dd * dd) as u64);
        }
        Ok(acc)
    }

    /// § Project this coord onto a different N′-coordinate by selecting axes.
    /// Caller supplies an axis-index per output-position. Returns `NotInLens`
    /// if any selector references an axis ≥ N.
    pub fn select_axes<const M: usize>(
        &self,
        selectors: [u8; M],
    ) -> Result<NdCoord<M>, CoordError> {
        let mut out = [0i32; M];
        for (i, &axis) in selectors.iter().enumerate() {
            if (axis as usize) >= N {
                return Err(CoordError::NotInLens { axis });
            }
            out[i] = self.axes[axis as usize];
        }
        Ok(NdCoord { axes: out })
    }
}

impl<const N: usize> Default for NdCoord<N> {
    fn default() -> Self {
        Self::origin()
    }
}

// ══════════════════════════════════════════════════════════════════
// § tests · coord-arithmetic + bounds + selection
// ══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_is_zero() {
        let c: NdCoord<8> = NdCoord::origin();
        assert!(c.axes().iter().all(|&v| v == 0));
    }

    #[test]
    fn get_set_in_range() {
        let mut c: NdCoord<8> = NdCoord::origin();
        c.set(4, 7).unwrap();
        assert_eq!(c.get(4).unwrap(), 7);
    }

    #[test]
    fn get_out_of_range() {
        let c: NdCoord<8> = NdCoord::origin();
        assert!(matches!(
            c.get(8),
            Err(CoordError::AxisOutOfRange { axis: 8, n: 8 })
        ));
    }

    #[test]
    fn checked_add_normal() {
        let a: NdCoord<4> = NdCoord::from_axes([1, 2, 3, 4]);
        let b: NdCoord<4> = NdCoord::from_axes([10, 20, 30, 40]);
        let c = a.checked_add(b).unwrap();
        assert_eq!(c.axes(), &[11, 22, 33, 44]);
    }

    #[test]
    fn checked_add_overflow_refused() {
        let a: NdCoord<2> = NdCoord::from_axes([i32::MAX, 0]);
        let b: NdCoord<2> = NdCoord::from_axes([1, 0]);
        assert!(matches!(
            a.checked_add(b),
            Err(CoordError::Overflow { axis: 0 })
        ));
    }

    #[test]
    fn checked_sub_normal() {
        let a: NdCoord<3> = NdCoord::from_axes([10, 10, 10]);
        let b: NdCoord<3> = NdCoord::from_axes([3, 4, 5]);
        let c = a.checked_sub(b).unwrap();
        assert_eq!(c.axes(), &[7, 6, 5]);
    }

    #[test]
    fn manhattan_signed() {
        let c: NdCoord<4> = NdCoord::from_axes([-3, 4, -5, 6]);
        assert_eq!(c.manhattan(), 18);
    }

    #[test]
    fn chebyshev_picks_max() {
        let c: NdCoord<5> = NdCoord::from_axes([1, -7, 3, 4, -2]);
        assert_eq!(c.chebyshev(), 7);
    }

    #[test]
    fn squared_distance_spatial_only() {
        let a: NdCoord<8> = NdCoord::from_axes([0, 0, 0, 100, 100, 100, 100, 100]);
        let b: NdCoord<8> = NdCoord::from_axes([3, 4, 0, 0, 0, 0, 0, 0]);
        // spatial-only (axes 0..2) ignores the wildly-different semantic axes.
        let d = a.squared_distance_along(&b, &[0, 1, 2]).unwrap();
        assert_eq!(d, 9 + 16);
    }

    #[test]
    fn select_axes_extracts_subset() {
        let c: NdCoord<8> = NdCoord::from_axes([1, 2, 3, 4, 5, 6, 7, 8]);
        let projected = c.select_axes::<3>([0, 1, 2]).unwrap();
        assert_eq!(projected.axes(), &[1, 2, 3]);
    }

    #[test]
    fn select_axes_rejects_out_of_range() {
        let c: NdCoord<4> = NdCoord::from_axes([1, 2, 3, 4]);
        assert!(matches!(
            c.select_axes::<2>([0, 9]),
            Err(CoordError::NotInLens { axis: 9 })
        ));
    }
}
