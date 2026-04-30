//! § LoaKanExtension — per-cell KAN-extension specialization.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The bundle that ties together a [`ParametricActivation`] and a
//!   [`LoaKanCellModulation`] for a single cell. This is the primary
//!   surface a scene-author uses to specialize one Sovereign-claimed
//!   region's KAN behavior.
//!
//! § DESIGN
//!   An extension carries :
//!     - `activation`         : per-cell parametric activation function
//!     - `modulation`         : per-cell modulation coefficients
//!     - `pattern_handle`     : the Φ-handle this extension is bound to
//!                              (defaults to PATTERN_HANDLE_NULL ; Sovereign
//!                              binds it to a specific Pattern when known)
//!     - `version_tag`        : surface-version stamp for forward-compat
//!     - `bound_kan_handle`   : optional handle into a parent KanNetwork
//!                              that this extension specializes
//!
//! § PRIME-DIRECTIVE
//!   Extensions cannot be silently aliased between Sovereigns. The
//!   modulation's sovereign_handle MUST match a the authoring-Sovereign's
//!   declared handle ; mismatches refuse with [`LoaKanExtensionError::SovereignMismatch`].

use crate::activation::{ActivationKind, ParametricActivation};
use crate::modulation::LoaKanCellModulation;
use cssl_substrate_omega_field::PATTERN_HANDLE_NULL;

/// § Surface-version tag embedded in every extension. Bumped when the
///   extension public ABI changes.
pub const EXTENSION_VERSION_TAG: u16 = 1;

/// § Per-cell LoA-KAN extension : activation + modulation + Pattern-binding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoaKanExtension {
    /// § Per-cell parametric activation.
    pub activation: ParametricActivation,
    /// § Per-cell modulation coefficients.
    pub modulation: LoaKanCellModulation,
    /// § Φ-handle this extension is bound to (PATTERN_HANDLE_NULL = unbound).
    pub pattern_handle: u32,
    /// § Surface-version tag for ABI forward-compat.
    pub version_tag: u16,
    /// § Optional parent KanNetwork handle (0 = no parent network).
    pub bound_kan_handle: u64,
}

impl LoaKanExtension {
    /// § Construct an identity extension : Identity activation + dormant
    ///   modulation + unbound. The default for cells without explicit
    ///   specialization.
    #[must_use]
    pub const fn identity() -> LoaKanExtension {
        LoaKanExtension {
            activation: ParametricActivation::identity(),
            modulation: LoaKanCellModulation::identity(),
            pattern_handle: PATTERN_HANDLE_NULL,
            version_tag: EXTENSION_VERSION_TAG,
            bound_kan_handle: 0,
        }
    }

    /// § Construct from explicit activation + modulation. The Sovereign
    ///   that authors this extension is taken from the modulation's
    ///   sovereign_handle field.
    ///
    /// # Errors
    /// Returns [`LoaKanExtensionError::IdentityWithSovereign`] if the
    /// activation is Identity AND the modulation declares a non-zero
    /// Sovereign — this is incoherent (an identity-extension shouldn't
    /// claim authoring-authority).
    pub fn new(
        activation: ParametricActivation,
        modulation: LoaKanCellModulation,
    ) -> Result<LoaKanExtension, LoaKanExtensionError> {
        if activation.is_identity() && !modulation.active && modulation.sovereign_handle != 0 {
            return Err(LoaKanExtensionError::IdentityWithSovereign {
                sovereign: modulation.sovereign_handle,
            });
        }
        Ok(LoaKanExtension {
            activation,
            modulation,
            pattern_handle: PATTERN_HANDLE_NULL,
            version_tag: EXTENSION_VERSION_TAG,
            bound_kan_handle: 0,
        })
    }

    /// § Bind the extension to a Φ pattern-handle. Used after the cell's
    ///   Φ-table append returns a handle.
    #[must_use]
    pub fn with_pattern(mut self, pattern_handle: u32) -> LoaKanExtension {
        self.pattern_handle = pattern_handle;
        self
    }

    /// § Bind the extension to a parent KanNetwork handle.
    #[must_use]
    pub fn with_kan_handle(mut self, handle: u64) -> LoaKanExtension {
        self.bound_kan_handle = handle;
        self
    }

    /// § True iff the extension is the identity (no specialization).
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.activation.is_identity() && self.modulation.is_identity()
    }

    /// § Authoring-Sovereign handle (forwarded from modulation).
    #[must_use]
    pub const fn sovereign_handle(&self) -> u16 {
        self.modulation.sovereign_handle
    }

    /// § True iff the extension declares Sovereign-authority.
    #[must_use]
    pub const fn is_sovereign(&self) -> bool {
        self.modulation.sovereign_handle != 0
    }

    /// § True iff the extension is bound to a Φ pattern.
    #[must_use]
    pub const fn is_bound_to_pattern(&self) -> bool {
        self.pattern_handle != PATTERN_HANDLE_NULL
    }

    /// § True iff the extension is bound to a parent KanNetwork.
    #[must_use]
    pub const fn is_bound_to_kan(&self) -> bool {
        self.bound_kan_handle != 0
    }

    /// § Apply the per-cell activation to a scalar input then apply the
    ///   modulation to the resulting downstream-output vector. This is
    ///   the canonical "evaluate a cell" path used by the Phase-3 COMPOSE
    ///   hook + Stage-6 BRDF.
    ///
    /// § DESIGN-NOTE : this is a ONE-INPUT eval — the multi-dimensional
    ///   input case threads through the parent KanNetwork::eval and then
    ///   element-wise applies via [`LoaKanCellModulation::apply_to`]. The
    ///   shape-preserving design lets the prototype path be exercised
    ///   without taking a dep on the full spline-evaluator.
    pub fn evaluate(&self, x: f32, output: &mut [f32]) {
        // Step 1 : per-cell activation produces a scalar.
        let activated = self.activation.apply(x);
        // Step 2 : broadcast the activated scalar into the output prefix.
        let n = output.len();
        for i in 0..n {
            output[i] = activated;
        }
        // Step 3 : apply modulation element-wise (no-op when dormant).
        self.modulation.apply_to(output);
    }

    /// § Validate the extension's internal coherence. Used by the overlay-
    ///   insert path to refuse malformed extensions.
    pub fn validate(&self) -> Result<(), LoaKanExtensionError> {
        if self.version_tag != EXTENSION_VERSION_TAG {
            return Err(LoaKanExtensionError::VersionTagMismatch {
                expected: EXTENSION_VERSION_TAG,
                got: self.version_tag,
            });
        }
        if !self.activation.unused_tail_zeroed() {
            return Err(LoaKanExtensionError::ActivationTailNotZero {
                kind: self.activation.kind,
            });
        }
        if self.activation.is_identity()
            && !self.modulation.active
            && self.modulation.sovereign_handle != 0
        {
            return Err(LoaKanExtensionError::IdentityWithSovereign {
                sovereign: self.modulation.sovereign_handle,
            });
        }
        Ok(())
    }
}

impl Default for LoaKanExtension {
    fn default() -> Self {
        Self::identity()
    }
}

/// § Failure modes for [`LoaKanExtension`].
#[derive(Debug, thiserror::Error)]
pub enum LoaKanExtensionError {
    /// § The extension's surface version-tag does not match the current
    ///   crate's [`EXTENSION_VERSION_TAG`]. Cross-version compatibility
    ///   is opt-in via explicit migration (not yet implemented).
    #[error("LK0010 — extension version-tag mismatch : expected={expected}, got={got}")]
    VersionTagMismatch { expected: u16, got: u16 },

    /// § An identity-extension cannot declare a Sovereign — a no-op
    ///   extension should not be claiming authoring authority.
    #[error("LK0011 — identity extension declared non-zero Sovereign={sovereign}")]
    IdentityWithSovereign { sovereign: u16 },

    /// § The activation's unused-tail parameters are not zero-filled,
    ///   violating the parameter-buffer discipline.
    #[error("LK0012 — activation tail not zeroed for kind={kind:?}")]
    ActivationTailNotZero { kind: ActivationKind },

    /// § Sovereign-handle mismatch on a mutation that requires authorizing
    ///   authority. This wraps the underlying ModulationError::SovereignMismatch.
    #[error("LK0013 — Sovereign-handle mismatch : expected={expected}, got={got}")]
    SovereignMismatch { expected: u16, got: u16 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::ParametricActivation;
    use crate::modulation::LoaKanCellModulation;

    // ── Construction ────────────────────────────────────────────────

    #[test]
    fn identity_extension_is_identity() {
        let ext = LoaKanExtension::identity();
        assert!(ext.is_identity());
        assert!(!ext.is_sovereign());
        assert!(!ext.is_bound_to_pattern());
        assert!(!ext.is_bound_to_kan());
        assert_eq!(ext.version_tag, EXTENSION_VERSION_TAG);
    }

    #[test]
    fn new_with_activation_only() {
        let act = ParametricActivation::sigmoid(1.0, 0.0);
        let modu = LoaKanCellModulation::identity();
        let ext = LoaKanExtension::new(act, modu).unwrap();
        // Identity-modulation but non-identity activation.
        assert!(!ext.is_identity());
    }

    #[test]
    fn new_with_modulation_only() {
        let act = ParametricActivation::identity();
        let modu = LoaKanCellModulation::uniform(2.0, 7).unwrap();
        let ext = LoaKanExtension::new(act, modu).unwrap();
        assert!(!ext.is_identity());
        assert!(ext.is_sovereign());
        assert_eq!(ext.sovereign_handle(), 7);
    }

    #[test]
    fn new_identity_with_sovereign_refused() {
        let act = ParametricActivation::identity();
        let mut modu = LoaKanCellModulation::identity();
        modu.sovereign_handle = 42;
        let err = LoaKanExtension::new(act, modu).unwrap_err();
        assert!(matches!(
            err,
            LoaKanExtensionError::IdentityWithSovereign { sovereign: 42 }
        ));
    }

    // ── Builder methods ────────────────────────────────────────────

    #[test]
    fn with_pattern_binds_handle() {
        let ext = LoaKanExtension::identity().with_pattern(7);
        assert_eq!(ext.pattern_handle, 7);
        assert!(ext.is_bound_to_pattern());
    }

    #[test]
    fn with_kan_handle_binds_kan() {
        let ext = LoaKanExtension::identity().with_kan_handle(99);
        assert_eq!(ext.bound_kan_handle, 99);
        assert!(ext.is_bound_to_kan());
    }

    // ── Evaluate ───────────────────────────────────────────────────

    #[test]
    fn evaluate_identity_passthrough() {
        let ext = LoaKanExtension::identity();
        let mut out = [0.0_f32; 4];
        ext.evaluate(2.5, &mut out);
        // Identity activation passes through ; identity modulation no-op.
        for i in 0..4 {
            assert_eq!(out[i], 2.5);
        }
    }

    #[test]
    fn evaluate_with_sigmoid_activation() {
        let act = ParametricActivation::sigmoid(1.0, 0.0);
        let modu = LoaKanCellModulation::identity();
        let ext = LoaKanExtension::new(act, modu).unwrap();
        let mut out = [0.0_f32; 4];
        ext.evaluate(0.0, &mut out);
        // σ(0) = 0.5
        for i in 0..4 {
            assert!((out[i] - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn evaluate_with_active_modulation_scales() {
        let act = ParametricActivation::identity();
        let modu = LoaKanCellModulation::uniform(2.0, 7).unwrap();
        let ext = LoaKanExtension::new(act, modu).unwrap();
        let mut out = [0.0_f32; 4];
        ext.evaluate(3.0, &mut out);
        // Identity(3.0) = 3.0 ; 3.0 * 2.0 = 6.0.
        for i in 0..4 {
            assert_eq!(out[i], 6.0);
        }
    }

    // ── Validation ─────────────────────────────────────────────────

    #[test]
    fn validate_identity_succeeds() {
        let ext = LoaKanExtension::identity();
        ext.validate().unwrap();
    }

    #[test]
    fn validate_version_mismatch_refused() {
        let mut ext = LoaKanExtension::identity();
        ext.version_tag = EXTENSION_VERSION_TAG + 1;
        let err = ext.validate().unwrap_err();
        assert!(matches!(
            err,
            LoaKanExtensionError::VersionTagMismatch { .. }
        ));
    }

    #[test]
    fn validate_activation_tail_not_zero_refused() {
        let mut act = ParametricActivation::sigmoid(1.0, 0.0);
        // Sigmoid uses 2 params ; force a tail entry to non-zero.
        act.params[5] = 0.5;
        let modu = LoaKanCellModulation::identity();
        let ext = LoaKanExtension {
            activation: act,
            modulation: modu,
            pattern_handle: PATTERN_HANDLE_NULL,
            version_tag: EXTENSION_VERSION_TAG,
            bound_kan_handle: 0,
        };
        let err = ext.validate().unwrap_err();
        assert!(matches!(
            err,
            LoaKanExtensionError::ActivationTailNotZero { .. }
        ));
    }
}
