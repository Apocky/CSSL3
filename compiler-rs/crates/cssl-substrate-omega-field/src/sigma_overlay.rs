//! § Σ-overlay — sparse Morton-keyed grid of full SigmaMaskPacked values.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Sparse overlay of full 16-byte [`SigmaMaskPacked`] values, used for
//!   cells that have non-default Σ-masks. Cells absent from this overlay
//!   are treated as carrying [`SigmaMaskPacked::default_mask()`].
//!
//! § INTEGRATION-POINT
//!   The hot-path consent gate is the in-cell `sigma_consent_bits` field
//!   on [`crate::field_cell::FieldCell`] (the low 32-bit consent_bits cache).
//!   The slow path consults THIS overlay for the canonical full mask
//!   (Sovereign-handle, capacity-floor, reversibility-scope, audit-seq,
//!   agency-state).
//!
//! § SPEC
//!   - `Omniverse/04_OMEGA_FIELD/00_FACETS.csl.md` § VIII Σ encoding (16B).
//!   - `Omniverse/04_OMEGA_FIELD/02_STORAGE.csl.md` § VI Σ-overlay (~5%
//!     occupancy, 16B/cell).
//!   - `cssl-substrate-prime-directive::sigma::SigmaMaskPacked` — the
//!     canonical 16B std430-aligned mask.

use crate::morton::MortonKey;
use crate::sparse_grid::{OmegaCellLayout, SparseMortonGrid};
use cssl_substrate_prime_directive::sigma::SigmaMaskPacked;

/// Wrapper struct so [`SigmaMaskPacked`] satisfies [`OmegaCellLayout`]'s
/// trait-object requirements. The wrapper is `#[repr(transparent)]` so it
/// is byte-identical to a bare `SigmaMaskPacked`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(transparent)]
pub struct SigmaOverlayCell {
    pub mask: SigmaMaskPacked,
}

impl OmegaCellLayout for SigmaOverlayCell {
    fn omega_cell_size() -> usize {
        16
    }
    fn omega_cell_align() -> usize {
        8
    }
    fn omega_cell_layout_tag() -> &'static str {
        "SigmaMaskPacked"
    }
}

/// Σ-overlay : sparse grid of full 16B masks, keyed by Morton.
#[derive(Debug, Clone, Default)]
pub struct SigmaOverlay {
    grid: SparseMortonGrid<SigmaOverlayCell>,
}

impl SigmaOverlay {
    /// Construct an empty overlay.
    #[must_use]
    pub fn new() -> Self {
        SigmaOverlay {
            grid: SparseMortonGrid::with_capacity(64),
        }
    }

    /// Set the full mask at `key`, replacing any prior value. Returns the
    /// prior mask if any.
    pub fn set(&mut self, key: MortonKey, mask: SigmaMaskPacked) -> Option<SigmaMaskPacked> {
        let cell = SigmaOverlayCell { mask };
        match self.grid.insert(key, cell) {
            Ok(prev) => prev.map(|p| p.mask),
            Err(_) => None,
        }
    }

    /// Read the full mask at `key`. Returns the default mask if absent (per
    /// the sparse-overlay convention).
    #[must_use]
    pub fn at(&self, key: MortonKey) -> SigmaMaskPacked {
        self.grid
            .at_const(key)
            .map(|c| c.mask)
            .unwrap_or_else(SigmaMaskPacked::default_mask)
    }

    /// True iff `key` has an explicit (non-default) mask in this overlay.
    #[must_use]
    pub fn has_explicit(&self, key: MortonKey) -> bool {
        self.grid.at_const(key).is_some()
    }

    /// Number of cells with explicit masks.
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.grid.len()
    }

    /// Remove the explicit mask at `key`, reverting to default.
    pub fn remove(&mut self, key: MortonKey) -> Option<SigmaMaskPacked> {
        self.grid.remove(key).map(|c| c.mask)
    }

    /// Iterate over `(key, mask)` pairs in MortonKey-ascending order.
    pub fn iter(&self) -> impl Iterator<Item = (MortonKey, SigmaMaskPacked)> + '_ {
        self.grid.iter().map(|(k, c)| (k, c.mask))
    }

    /// Clear the overlay.
    pub fn clear(&mut self) {
        self.grid = SparseMortonGrid::with_capacity(64);
    }
}

#[cfg(test)]
mod tests {
    use super::{OmegaCellLayout, SigmaOverlay, SigmaOverlayCell};
    use crate::morton::MortonKey;
    use cssl_substrate_prime_directive::sigma::{ConsentBit, SigmaMaskPacked, SigmaPolicy};

    #[test]
    fn sigma_overlay_cell_size_16() {
        assert_eq!(<SigmaOverlayCell as OmegaCellLayout>::omega_cell_size(), 16);
    }

    #[test]
    fn empty_overlay_returns_default_mask() {
        let o = SigmaOverlay::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        let m = o.at(k);
        assert_eq!(m, SigmaMaskPacked::default_mask());
        assert!(!o.has_explicit(k));
    }

    #[test]
    fn set_and_read_full_mask() {
        let mut o = SigmaOverlay::new();
        let k = MortonKey::encode(7, 8, 9).unwrap();
        let mask = SigmaMaskPacked::default_mask().with_sovereign(42).with_consent(
            ConsentBit::Modify.bits() | ConsentBit::Observe.bits(),
        );
        o.set(k, mask);
        let read = o.at(k);
        assert_eq!(read.sovereign_handle(), 42);
        assert!(read.can_modify());
        assert!(o.has_explicit(k));
    }

    #[test]
    fn remove_reverts_to_default() {
        let mut o = SigmaOverlay::new();
        let k = MortonKey::encode(1, 0, 0).unwrap();
        o.set(k, SigmaMaskPacked::from_policy(SigmaPolicy::PublicRead));
        let _ = o.remove(k).unwrap();
        assert!(!o.has_explicit(k));
        assert_eq!(o.at(k), SigmaMaskPacked::default_mask());
    }

    #[test]
    fn cell_count_tracks_inserts() {
        let mut o = SigmaOverlay::new();
        for i in 0..5_u64 {
            o.set(
                MortonKey::encode(i, 0, 0).unwrap(),
                SigmaMaskPacked::default_mask(),
            );
        }
        assert_eq!(o.cell_count(), 5);
    }
}
