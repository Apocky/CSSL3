//! § LoaKanOverlay — sparse Morton-keyed grid of LoaKanExtension cells.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The sparse-overlay layer that mirrors the SigmaOverlay pattern :
//!   stores per-cell [`LoaKanExtension`] entries keyed by MortonKey.
//!   Cells absent from this overlay are treated as carrying
//!   [`LoaKanExtension::identity()`] (no specialization).
//!
//! § STORAGE-DISCIPLINE
//!   Per spec § STORAGE-DISCIPLINE — the overlay shares the same open-
//!   addressing FxHash table semantics as the SigmaOverlay :
//!     - load-factor 0.75 default
//!     - linear-probing
//!     - iteration in MortonKey-ascending order (replay-determinism)
//!     - ~5% occupancy expected (Sovereign-claimed regions only)
//!
//! § PRIME-DIRECTIVE
//!   Insertion gates Σ-mask : the cell's Σ-mask MUST permit Reconfigure
//!   (the structural-change bit) for an extension to be installed. This
//!   matches the spec § BIT-LAYOUT bit 5 = Reconfigure semantics.

use crate::extension::{LoaKanExtension, LoaKanExtensionError};
use cssl_substrate_omega_field::{MortonKey, OmegaCellLayout, SparseMortonGrid};
use cssl_substrate_prime_directive::sigma::SigmaMaskPacked;

/// § Wrapper struct so [`LoaKanExtension`] satisfies [`OmegaCellLayout`]'s
///   trait-object requirements. The wrapper is `#[repr(transparent)]` so it
///   is byte-identical to a bare `LoaKanExtension`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(transparent)]
pub struct LoaKanOverlayCell {
    pub extension: LoaKanExtension,
}

impl OmegaCellLayout for LoaKanOverlayCell {
    fn omega_cell_size() -> usize {
        // Activation 1 + 1 + 64 = 66B + Modulation 64 + 2 + 1 + 1 = 68B +
        // pattern_handle 4 + version_tag 2 + bound_kan_handle 8 = padding-
        // dependent. Use the natural Rust size as the canonical figure.
        core::mem::size_of::<LoaKanExtension>()
    }
    fn omega_cell_align() -> usize {
        core::mem::align_of::<LoaKanExtension>().max(8)
    }
    fn omega_cell_layout_tag() -> &'static str {
        "LoaKanExtension"
    }
}

/// § LoaKanOverlay : sparse grid of per-cell [`LoaKanExtension`] entries.
#[derive(Debug, Clone, Default)]
pub struct LoaKanOverlay {
    grid: SparseMortonGrid<LoaKanOverlayCell>,
}

impl LoaKanOverlay {
    /// § Construct an empty overlay.
    #[must_use]
    pub fn new() -> Self {
        LoaKanOverlay {
            grid: SparseMortonGrid::with_capacity(64),
        }
    }

    /// § Set the per-cell extension at `key`. The cell's Σ-mask is checked
    ///   to permit Reconfigure ; a non-permitting mask refuses with
    ///   [`OverlayError::ReconfigureRefused`].
    ///
    /// # Errors
    /// - [`OverlayError::ReconfigureRefused`] when the Σ-mask does not
    ///   permit Reconfigure.
    /// - [`OverlayError::SovereignMismatch`] when the extension's
    ///   sovereign_handle does not match the Σ-mask's sovereign_handle on
    ///   a Σ-claimed cell.
    /// - [`OverlayError::ValidationFailed`] when the extension fails
    ///   internal validation.
    pub fn set(
        &mut self,
        key: MortonKey,
        extension: LoaKanExtension,
        sigma: SigmaMaskPacked,
    ) -> Result<Option<LoaKanExtension>, OverlayError> {
        // Step 1 : Σ-mask gates structural change.
        if !sigma.can_reconfigure() {
            return Err(OverlayError::ReconfigureRefused {
                consent_bits: sigma.consent_bits(),
            });
        }
        // Step 2 : Sovereign-handle authorization on claimed cells.
        if sigma.is_sovereign() && extension.sovereign_handle() != sigma.sovereign_handle() {
            return Err(OverlayError::SovereignMismatch {
                expected: sigma.sovereign_handle(),
                got: extension.sovereign_handle(),
            });
        }
        // Step 3 : extension internal coherence validation.
        if let Err(e) = extension.validate() {
            return Err(OverlayError::ValidationFailed(e));
        }
        // Step 4 : insert.
        let cell = LoaKanOverlayCell { extension };
        match self.grid.insert(key, cell) {
            Ok(prev) => Ok(prev.map(|p| p.extension)),
            Err(_) => Err(OverlayError::GridSaturated),
        }
    }

    /// § Set the per-cell extension at `key` BYPASSING the Σ-mask + Sovereign
    ///   check. Used at scene-load when stamping extensions before any
    ///   consent contracts exist.
    pub fn stamp_bootstrap(
        &mut self,
        key: MortonKey,
        extension: LoaKanExtension,
    ) -> Result<Option<LoaKanExtension>, OverlayError> {
        if let Err(e) = extension.validate() {
            return Err(OverlayError::ValidationFailed(e));
        }
        let cell = LoaKanOverlayCell { extension };
        match self.grid.insert(key, cell) {
            Ok(prev) => Ok(prev.map(|p| p.extension)),
            Err(_) => Err(OverlayError::GridSaturated),
        }
    }

    /// § Read the extension at `key`. Returns identity when absent.
    #[must_use]
    pub fn at(&self, key: MortonKey) -> LoaKanExtension {
        self.grid
            .at_const(key)
            .map(|c| c.extension)
            .unwrap_or_else(LoaKanExtension::identity)
    }

    /// § True iff `key` has an explicit (non-default) extension.
    #[must_use]
    pub fn has_explicit(&self, key: MortonKey) -> bool {
        self.grid.at_const(key).is_some()
    }

    /// § Number of cells with explicit extensions.
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.grid.len()
    }

    /// § Remove the explicit extension at `key`, reverting to identity.
    pub fn remove(&mut self, key: MortonKey) -> Option<LoaKanExtension> {
        self.grid.remove(key).map(|c| c.extension)
    }

    /// § Iterate over `(key, extension)` pairs in MortonKey-ascending
    ///   order (replay-determinism).
    pub fn iter(&self) -> impl Iterator<Item = (MortonKey, LoaKanExtension)> + '_ {
        self.grid.iter().map(|(k, c)| (k, c.extension))
    }

    /// § Clear the overlay.
    pub fn clear(&mut self) {
        self.grid = SparseMortonGrid::with_capacity(64);
    }
}

/// § Failure modes for [`LoaKanOverlay`] mutations.
#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    /// § Σ-mask does not permit Reconfigure.
    #[error(
        "LK0040 — extension-overlay set refused : Σ-mask does not permit Reconfigure (consent_bits=0x{consent_bits:08x})"
    )]
    ReconfigureRefused { consent_bits: u32 },
    /// § Extension's Sovereign-handle does not match cell's Σ-mask Sovereign.
    #[error("LK0041 — Sovereign-handle mismatch on overlay-set : expected={expected}, got={got}")]
    SovereignMismatch { expected: u16, got: u16 },
    /// § Extension failed internal validation.
    #[error("LK0042 — extension validation failed : {0}")]
    ValidationFailed(#[from] LoaKanExtensionError),
    /// § Underlying sparse-grid saturation (rare ; usually means too-many
    ///   collisions).
    #[error("LK0043 — sparse-grid saturated during extension insert")]
    GridSaturated,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::ParametricActivation;
    use crate::modulation::LoaKanCellModulation;
    use cssl_substrate_prime_directive::sigma::{ConsentBit, SigmaMaskPacked, SigmaPolicy};

    fn permissive_mask() -> SigmaMaskPacked {
        SigmaMaskPacked::default_mask().with_consent(
            ConsentBit::Observe.bits()
                | ConsentBit::Sample.bits()
                | ConsentBit::Modify.bits()
                | ConsentBit::Reconfigure.bits(),
        )
    }

    fn permissive_mask_with_sovereign(s: u16) -> SigmaMaskPacked {
        permissive_mask().with_sovereign(s)
    }

    // ── Empty overlay ──────────────────────────────────────────────

    #[test]
    fn empty_overlay_returns_identity() {
        let o = LoaKanOverlay::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        let e = o.at(k);
        assert!(e.is_identity());
        assert!(!o.has_explicit(k));
        assert_eq!(o.cell_count(), 0);
    }

    // ── Set with Σ-mask gating ─────────────────────────────────────

    #[test]
    fn set_without_reconfigure_consent_refused() {
        let mut o = LoaKanOverlay::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        let mask = SigmaMaskPacked::default_mask(); // Observe-only
        let ext = LoaKanExtension::identity();
        let err = o.set(k, ext, mask).unwrap_err();
        assert!(matches!(err, OverlayError::ReconfigureRefused { .. }));
    }

    #[test]
    fn set_with_reconfigure_consent_succeeds() {
        let mut o = LoaKanOverlay::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        let act = ParametricActivation::sigmoid(1.0, 0.0);
        let modu = LoaKanCellModulation::identity();
        let ext = LoaKanExtension::new(act, modu).unwrap();
        let prev = o.set(k, ext, permissive_mask()).unwrap();
        assert!(prev.is_none());
        assert_eq!(o.cell_count(), 1);
        assert!(o.has_explicit(k));
    }

    #[test]
    fn set_with_sovereign_match_succeeds() {
        let mut o = LoaKanOverlay::new();
        let k = MortonKey::encode(0, 0, 0).unwrap();
        let act = ParametricActivation::sigmoid(1.0, 0.0);
        let modu = LoaKanCellModulation::uniform(2.0, 7).unwrap();
        let ext = LoaKanExtension::new(act, modu).unwrap();
        let mask = permissive_mask_with_sovereign(7);
        o.set(k, ext, mask).unwrap();
    }

    #[test]
    fn set_with_sovereign_mismatch_refused() {
        let mut o = LoaKanOverlay::new();
        let k = MortonKey::encode(0, 0, 0).unwrap();
        let act = ParametricActivation::sigmoid(1.0, 0.0);
        let modu = LoaKanCellModulation::uniform(2.0, 7).unwrap();
        let ext = LoaKanExtension::new(act, modu).unwrap();
        // Cell claimed by Sovereign 99 ; extension authored by 7.
        let mask = permissive_mask_with_sovereign(99);
        let err = o.set(k, ext, mask).unwrap_err();
        assert!(matches!(err, OverlayError::SovereignMismatch { .. }));
    }

    // ── Bootstrap stamp ────────────────────────────────────────────

    #[test]
    fn stamp_bootstrap_bypasses_sigma_check() {
        let mut o = LoaKanOverlay::new();
        let k = MortonKey::encode(0, 0, 0).unwrap();
        let act = ParametricActivation::sigmoid(1.0, 0.0);
        let modu = LoaKanCellModulation::uniform(2.0, 7).unwrap();
        let ext = LoaKanExtension::new(act, modu).unwrap();
        let prev = o.stamp_bootstrap(k, ext).unwrap();
        assert!(prev.is_none());
        assert!(o.has_explicit(k));
    }

    // ── Read / overwrite / remove ─────────────────────────────────

    #[test]
    fn set_replaces_returns_prev() {
        let mut o = LoaKanOverlay::new();
        let k = MortonKey::encode(1, 1, 1).unwrap();
        let act_a = ParametricActivation::sigmoid(1.0, 0.0);
        let act_b = ParametricActivation::tanh(1.0, 0.0);
        let modu = LoaKanCellModulation::uniform(2.0, 7).unwrap();
        let mask = permissive_mask_with_sovereign(7);
        let ext_a = LoaKanExtension::new(act_a, modu).unwrap();
        let ext_b = LoaKanExtension::new(act_b, modu).unwrap();
        let _ = o.set(k, ext_a, mask).unwrap();
        let prev = o.set(k, ext_b, mask).unwrap();
        assert!(prev.is_some());
    }

    #[test]
    fn remove_reverts_to_identity() {
        let mut o = LoaKanOverlay::new();
        let k = MortonKey::encode(1, 0, 0).unwrap();
        let act = ParametricActivation::sigmoid(1.0, 0.0);
        let modu = LoaKanCellModulation::identity();
        let ext = LoaKanExtension::new(act, modu).unwrap();
        o.set(k, ext, permissive_mask()).unwrap();
        let removed = o.remove(k).unwrap();
        assert_eq!(
            removed.activation.kind,
            super::super::ActivationKind::Sigmoid
        );
        assert!(!o.has_explicit(k));
        assert!(o.at(k).is_identity());
    }

    #[test]
    fn iter_visits_all_keys() {
        let mut o = LoaKanOverlay::new();
        for i in 0..5_u64 {
            let k = MortonKey::encode(i, 0, 0).unwrap();
            let act = ParametricActivation::sigmoid(1.0, 0.0);
            let modu = LoaKanCellModulation::identity();
            let ext = LoaKanExtension::new(act, modu).unwrap();
            o.set(k, ext, permissive_mask()).unwrap();
        }
        let count = o.iter().count();
        assert_eq!(count, 5);
        assert_eq!(o.cell_count(), 5);
    }

    #[test]
    fn clear_drops_all_entries() {
        let mut o = LoaKanOverlay::new();
        let k = MortonKey::encode(0, 0, 0).unwrap();
        let act = ParametricActivation::sigmoid(1.0, 0.0);
        let modu = LoaKanCellModulation::identity();
        let ext = LoaKanExtension::new(act, modu).unwrap();
        o.set(k, ext, permissive_mask()).unwrap();
        assert_eq!(o.cell_count(), 1);
        o.clear();
        assert_eq!(o.cell_count(), 0);
    }
}
