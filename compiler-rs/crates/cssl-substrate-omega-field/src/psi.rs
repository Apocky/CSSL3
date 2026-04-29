//! § Ψ-overlay — sparse Morton-keyed grid of Wigner-negativity scalars.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Sparse overlay holding the Ψ-facet (quasi-probability / "magic" /
//!   insight). Per `Omniverse/04_OMEGA_FIELD/00_FACETS § VI` Ψ is a single
//!   f32 per cell representing the Wigner-negativity scalar :
//!
//!   ```text
//!   negativity > 0   ⇒  "magic / insight / quantum-of-consciousness"
//!   negativity == 0  ⇒  classical-cell
//!   ```
//!
//! § SPEC
//!   - `Omniverse/04_OMEGA_FIELD/00_FACETS.csl.md` § VI Ψ encoding (4B/cell).
//!   - `Omniverse/04_OMEGA_FIELD/02_STORAGE.csl.md` § V Ψ-overlay (sparse,
//!     ~1K active cells in M7).
//!
//! § STORAGE
//!   We wrap the f32 in a `PsiCell` newtype so the `OmegaCellLayout` trait's
//!   `Copy + Default` bound is satisfied + a stable `omega_cell_layout_tag`
//!   string is available for telemetry.

use crate::morton::MortonKey;
use crate::sparse_grid::{OmegaCellLayout, SparseMortonGrid};

/// One Ψ-cell : a single Wigner-negativity scalar in `f32`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C, align(4))]
pub struct PsiCell {
    /// Wigner-negativity (signed). Sign encodes the "magic" gradient :
    /// positive = magical, zero = classical, negative is reserved.
    pub negativity: f32,
}

impl OmegaCellLayout for PsiCell {
    fn omega_cell_size() -> usize {
        4
    }
    fn omega_cell_align() -> usize {
        4
    }
    fn omega_cell_layout_tag() -> &'static str {
        "PsiCell"
    }
}

impl PsiCell {
    /// Construct a new Ψ-cell with the given negativity.
    #[must_use]
    pub const fn new(negativity: f32) -> Self {
        PsiCell { negativity }
    }

    /// True iff this cell is classical (negativity exactly zero).
    #[inline]
    #[must_use]
    pub fn is_classical(&self) -> bool {
        self.negativity == 0.0
    }

    /// True iff this cell is magical (negativity > 0).
    #[inline]
    #[must_use]
    pub fn is_magical(&self) -> bool {
        self.negativity > 0.0
    }
}

/// Ψ-overlay : sparse grid keyed by Morton.
#[derive(Debug, Clone, Default)]
pub struct PsiOverlay {
    grid: SparseMortonGrid<PsiCell>,
}

impl PsiOverlay {
    /// Construct a new empty Ψ-overlay.
    #[must_use]
    pub fn new() -> Self {
        PsiOverlay {
            grid: SparseMortonGrid::with_capacity(32),
        }
    }

    /// Set the Wigner-negativity at `key`. Replaces any prior value. Returns
    /// the prior value if any.
    pub fn set(&mut self, key: MortonKey, negativity: f32) -> Option<f32> {
        let cell = PsiCell::new(negativity);
        match self.grid.insert(key, cell) {
            Ok(prev) => prev.map(|p| p.negativity),
            Err(_) => None, // saturated probe ; treated as silent failure here
        }
    }

    /// Read the Wigner-negativity at `key`. Returns 0.0 (classical) for
    /// missing cells (the spec's "all-other-cells ⊗ classical" default).
    #[must_use]
    pub fn at(&self, key: MortonKey) -> f32 {
        self.grid.at_const(key).map(|c| c.negativity).unwrap_or(0.0)
    }

    /// True iff the cell at `key` is "magical" (negativity > 0).
    #[must_use]
    pub fn is_magical_at(&self, key: MortonKey) -> bool {
        self.at(key) > 0.0
    }

    /// Number of cells with non-default Ψ.
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.grid.len()
    }

    /// Compute the L1 norm of the Ψ-field — the "mana" derivation per
    /// `Omniverse/04_OMEGA_FIELD/00_FACETS § VI.mana-from-Ψ` :
    ///
    /// ```text
    /// 𝓜 = log‖Ψ‖₁
    /// ```
    ///
    /// We expose the unlogged L1 norm here ; the log is the consumer's job.
    #[must_use]
    pub fn l1_norm(&self) -> f32 {
        self.grid
            .iter_unordered()
            .map(|(_, c)| c.negativity.abs())
            .sum()
    }

    /// Iterate cells in MortonKey-ascending order.
    pub fn iter(&self) -> impl Iterator<Item = (MortonKey, &PsiCell)> {
        self.grid.iter()
    }

    /// Remove the cell at `key`. Returns the prior value (or None).
    pub fn remove(&mut self, key: MortonKey) -> Option<f32> {
        self.grid.remove(key).map(|c| c.negativity)
    }

    /// Clear the entire overlay.
    pub fn clear(&mut self) {
        self.grid = SparseMortonGrid::with_capacity(32);
    }
}

#[cfg(test)]
mod tests {
    use super::{OmegaCellLayout, PsiCell, PsiOverlay};
    use crate::morton::MortonKey;

    #[test]
    fn psi_cell_size_is_4_bytes() {
        assert_eq!(<PsiCell as OmegaCellLayout>::omega_cell_size(), 4);
        assert_eq!(core::mem::size_of::<PsiCell>(), 4);
    }

    #[test]
    fn psi_default_is_classical() {
        let c = PsiCell::default();
        assert!(c.is_classical());
        assert!(!c.is_magical());
    }

    #[test]
    fn psi_positive_is_magical() {
        let c = PsiCell::new(0.5);
        assert!(!c.is_classical());
        assert!(c.is_magical());
    }

    #[test]
    fn psi_overlay_default_at_unset_is_zero() {
        let o = PsiOverlay::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        assert_eq!(o.at(k), 0.0);
    }

    #[test]
    fn psi_overlay_set_and_read() {
        let mut o = PsiOverlay::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        o.set(k, 1.5);
        assert!((o.at(k) - 1.5).abs() < 1e-6);
        assert!(o.is_magical_at(k));
    }

    #[test]
    fn psi_overlay_l1_norm() {
        let mut o = PsiOverlay::new();
        o.set(MortonKey::encode(0, 0, 0).unwrap(), 1.0);
        o.set(MortonKey::encode(1, 0, 0).unwrap(), -2.0);
        o.set(MortonKey::encode(2, 0, 0).unwrap(), 0.5);
        assert!((o.l1_norm() - 3.5).abs() < 1e-5);
    }

    #[test]
    fn psi_overlay_remove_restores_default() {
        let mut o = PsiOverlay::new();
        let k = MortonKey::encode(1, 0, 0).unwrap();
        o.set(k, 0.7);
        assert!(o.remove(k).is_some());
        assert_eq!(o.at(k), 0.0);
    }
}
