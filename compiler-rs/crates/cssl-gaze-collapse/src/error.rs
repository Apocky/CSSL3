//! Error types for the gaze-collapse pass.
//!
//! § DESIGN
//!   The most load-bearing variant is [`GazeCollapseError::EgressRefused`]
//!   which wraps `cssl_ifc::EgressGrantError`. This is the surface where the
//!   T11-D132 biometric structural-egress-gate appears at the pass-API
//!   boundary : if any caller tries to flow a gaze-bearing value to a
//!   destination that does not accept biometrics (e.g. a telemetry sink),
//!   the cssl-ifc `validate_egress` returns `BiometricRefused` and we
//!   propagate it here.
//!
//!   Other variants cover the natural pass-failure modes : invalid input
//!   (NaN gaze direction, negative confidence), consent revoked mid-frame,
//!   prediction-horizon out of range, KAN-evaluation failure.

use thiserror::Error;

use cssl_ifc::EgressGrantError;

/// Errors emitted by the gaze-collapse pass.
///
/// Note : `Eq` is *not* derived because `InvalidConfidence(f32)` carries an
/// `f32` payload (which has no `Eq` due to NaN). `PartialEq` is sufficient
/// for the test-comparisons below ; matching against specific variants in
/// production uses `matches!` so structural equality is not required.
#[derive(Debug, Clone, Error, PartialEq)]
pub enum GazeCollapseError {
    /// Telemetry-egress refused for a gaze-bearing value. Wraps the
    /// cssl-ifc structural-gate refusal so callers see the canonical
    /// `BiometricRefused` diagnostic at the pass-API boundary.
    #[error(
        "gaze-egress refused — gaze data is biometric-family per PRIME-DIRECTIVE §1 \
         (cssl-ifc structural-gate ; T11-D132) ; no override exists ({0})"
    )]
    EgressRefused(#[from] EgressGrantError),

    /// Gaze input invalid (NaN direction, non-finite, or unit-vector violated).
    #[error("invalid gaze input : {field} = {value} ; expected finite normalized 3-vector")]
    InvalidGazeInput {
        /// Which field failed validation.
        field: &'static str,
        /// String representation of the offending value.
        value: String,
    },

    /// Confidence value outside [0.0, 1.0].
    #[error("invalid gaze confidence {0} — expected [0.0, 1.0]")]
    InvalidConfidence(f32),

    /// Consent was revoked between `prepare` + `execute`. The pass returns
    /// the center-bias-foveation fallback in this case ; this error is
    /// only raised in strict-mode (when the caller explicitly disabled
    /// fallback).
    #[error("gaze-consent revoked mid-frame ; strict-mode rejected center-bias fallback")]
    ConsentRevokedStrict,

    /// Prediction horizon out of supported range (1..=8 ms).
    #[error("prediction-horizon {0} ms out of range [1, 8]")]
    PredictionHorizonOutOfRange(u8),

    /// KAN-conditioned-evolution determinism check failed (re-run produced
    /// a different output for the same input ; this is a bug per Axiom 5
    /// §VII determinism-test acceptance).
    #[error(
        "KAN-conditioned evolution non-deterministic at glance-history-hash {hash:#x} ; \
         Axiom 5 §VII acceptance violated"
    )]
    EvolutionNonDeterministic {
        /// 64-bit hash of the input glance-history that triggered the divergence.
        hash: u64,
    },

    /// FoveaMask resolution mismatch (e.g. attempted to compose a 1024×1024
    /// mask with a 2048×2048 mask without re-projection).
    #[error(
        "FoveaMask resolution mismatch : left = {left_w}×{left_h} ; right = {right_w}×{right_h}"
    )]
    FoveaMaskResolutionMismatch {
        /// Left mask width.
        left_w: u32,
        /// Left mask height.
        left_h: u32,
        /// Right mask width.
        right_w: u32,
        /// Right mask height.
        right_h: u32,
    },

    /// Σ-private-region collision : the gaze ray-cast resolved into a cell
    /// whose `SigmaMaskPacked` denied observation. The pass falls-back to
    /// the previous-frame fovea position rather than violating consent.
    #[error(
        "gaze-ray resolved into Σ-private-cell at sovereignty-handle {handle} ; \
         falling back to prev-frame fovea per cell-level consent"
    )]
    SigmaPrivateCollision {
        /// The sovereign-handle of the cell that denied observation.
        handle: u16,
    },
}

#[cfg(test)]
mod tests {
    use cssl_ifc::SensitiveDomain;

    use super::{EgressGrantError, GazeCollapseError};

    #[test]
    fn egress_refused_carries_biometric_refusal() {
        let inner = EgressGrantError::BiometricRefused {
            domain: SensitiveDomain::Gaze,
        };
        let err: GazeCollapseError = inner.into();
        let s = format!("{}", err);
        assert!(s.contains("biometric-family"));
        assert!(s.contains("PRIME-DIRECTIVE"));
        assert!(s.contains("T11-D132"));
    }

    #[test]
    fn invalid_gaze_input_message_carries_field_and_value() {
        let err = GazeCollapseError::InvalidGazeInput {
            field: "left.direction.z",
            value: "NaN".to_string(),
        };
        let s = format!("{}", err);
        assert!(s.contains("left.direction.z"));
        assert!(s.contains("NaN"));
    }

    #[test]
    fn invalid_confidence_carries_offending_value() {
        let err = GazeCollapseError::InvalidConfidence(2.5);
        let s = format!("{}", err);
        assert!(s.contains("2.5"));
        assert!(s.contains("[0.0, 1.0]"));
    }

    #[test]
    fn consent_revoked_strict_message_explicit() {
        let err = GazeCollapseError::ConsentRevokedStrict;
        let s = format!("{}", err);
        assert!(s.contains("revoked"));
        assert!(s.contains("strict"));
    }

    #[test]
    fn prediction_horizon_out_of_range_quotes_value() {
        let err = GazeCollapseError::PredictionHorizonOutOfRange(12);
        let s = format!("{}", err);
        assert!(s.contains("12"));
        assert!(s.contains("[1, 8]"));
    }

    #[test]
    fn evolution_non_deterministic_quotes_axiom_5() {
        let err = GazeCollapseError::EvolutionNonDeterministic { hash: 0xCAFE };
        let s = format!("{}", err);
        assert!(s.contains("Axiom 5"));
        assert!(s.contains("0xcafe"));
    }

    #[test]
    fn fovea_mask_resolution_mismatch_quotes_dims() {
        let err = GazeCollapseError::FoveaMaskResolutionMismatch {
            left_w: 1024,
            left_h: 1024,
            right_w: 2048,
            right_h: 2048,
        };
        let s = format!("{}", err);
        assert!(s.contains("1024"));
        assert!(s.contains("2048"));
    }

    #[test]
    fn sigma_private_collision_quotes_handle() {
        let err = GazeCollapseError::SigmaPrivateCollision { handle: 42 };
        let s = format!("{}", err);
        assert!(s.contains("42"));
        assert!(s.contains("Σ-private"));
    }
}
