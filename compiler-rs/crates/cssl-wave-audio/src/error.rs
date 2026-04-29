//! § Errors — wave-audio failure modes.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Error type used across the wave-audio surface : Σ-consent refusals,
//!   storage saturation, source-spectrum-out-of-band conditions, KAN-
//!   network shape mismatches, and SDF-vocal-tract under-specification.
//!
//! § SPEC
//!   - `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl § XVII.1` consent-at-
//!     every-op : every ψ-injection that fails the Σ-check returns
//!     [`WaveAudioError::ConsentDenied`].
//!   - `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl § VIII.3` spill-
//!     strategy : when the LBM hash-grid saturates beyond the active-
//!     region budget the LBM step returns [`WaveAudioError::Storage`].

use thiserror::Error;

/// Result alias used across the wave-audio surface.
pub type Result<T> = std::result::Result<T, WaveAudioError>;

/// Failure modes for wave-audio operations.
#[derive(Debug, Clone, Error)]
pub enum WaveAudioError {
    /// Σ-mask refused the requested op-class on this cell. Per spec §
    /// XVII.1 ψ-injections must check Σ.consent_bits before writing.
    #[error(
        "WAV0001 — Σ-mask refused '{requested}' op at MortonKey {key:#018x} \
         (no Modify-bit set ; grant consent before injection)"
    )]
    ConsentDenied {
        /// The MortonKey of the cell that refused the op.
        key: u64,
        /// Canonical name of the op-class that was refused.
        requested: &'static str,
    },

    /// The underlying sparse-grid hit a storage limit (probe-saturation,
    /// out-of-memory, or capacity-budget exceeded).
    #[error("WAV0002 — sparse-grid storage failure : {0}")]
    Storage(String),

    /// A source's frequency content exceeded the AUDIO band's nominal
    /// range. The legacy mixer would silently clamp ; the wave-audio
    /// path refuses + emits this error so the caller can adjust the
    /// source synthesizer.
    #[error(
        "WAV0003 — source frequency {freq_hz:.1} Hz outside AUDIO band \
         [{band_lo:.1}..{band_hi:.1}] Hz"
    )]
    OutOfBand {
        /// The offending frequency in Hz.
        freq_hz: f32,
        /// Band lower edge.
        band_lo: f32,
        /// Band upper edge.
        band_hi: f32,
    },

    /// The KAN spline-network's input/output dimensions did not match
    /// the expected shape for this caller (e.g. a 4-band impedance
    /// matrix was supplied where a 2-band binaural KAN was expected).
    #[error("WAV0004 — KAN shape mismatch : expected {expected}, got {actual}")]
    KanShape {
        /// Expected shape description.
        expected: &'static str,
        /// Actual shape description.
        actual: String,
    },

    /// The SDF-vocal-tract definition was malformed (e.g. zero length,
    /// overlapping segments, non-monotone radius profile).
    #[error("WAV0005 — SDF-vocal-tract malformed : {0}")]
    VocalTract(&'static str),

    /// The LBM stream-collide step detected ψ-norm growth beyond the
    /// allowed ε threshold (a numerical-instability indicator). Per
    /// spec § XII.3 entropy_book Phase-6 violation.
    #[error(
        "WAV0006 — ψ-norm conservation violated : prev={prev:.6} next={next:.6} \
         ε={epsilon:.6}"
    )]
    ConservationViolation {
        /// Energy before the LBM step.
        prev: f32,
        /// Energy after the LBM step.
        next: f32,
        /// Allowed tolerance.
        epsilon: f32,
    },

    /// The cross-band coupling matrix carried an entry that violates
    /// the AGENCY-INVARIANT — e.g. a non-zero LIGHT→MANA entry per
    /// spec § XI which states `LIGHT → MANA = 0` to enforce "light
    /// doesn't make magic".
    #[error("WAV0007 — cross-band coupling AGENCY-violation : {explanation}")]
    AgencyViolation {
        /// Human-readable explanation of the violated agency clause.
        explanation: &'static str,
    },
}

impl WaveAudioError {
    /// Stable error-code prefix (matches the spec's diagnostic-id
    /// scheme : WAV<NNNN>).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::ConsentDenied { .. } => "WAV0001",
            Self::Storage(_) => "WAV0002",
            Self::OutOfBand { .. } => "WAV0003",
            Self::KanShape { .. } => "WAV0004",
            Self::VocalTract(_) => "WAV0005",
            Self::ConservationViolation { .. } => "WAV0006",
            Self::AgencyViolation { .. } => "WAV0007",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WaveAudioError;

    #[test]
    fn consent_denied_carries_key() {
        let e = WaveAudioError::ConsentDenied {
            key: 0xDEAD_BEEF,
            requested: "modify",
        };
        assert_eq!(e.code(), "WAV0001");
        // The format-string uses lowercase `:#018x` width-padded hex.
        let s = format!("{e}");
        assert!(
            s.to_ascii_lowercase().contains("deadbeef"),
            "expected key in error message, got: {s}"
        );
    }

    #[test]
    fn storage_carries_inner_message() {
        let e = WaveAudioError::Storage("probe saturated".into());
        assert_eq!(e.code(), "WAV0002");
        assert!(format!("{e}").contains("probe saturated"));
    }

    #[test]
    fn out_of_band_formats_band() {
        let e = WaveAudioError::OutOfBand {
            freq_hz: 25_000.0,
            band_lo: 20.0,
            band_hi: 20_000.0,
        };
        assert_eq!(e.code(), "WAV0003");
        let s = format!("{e}");
        assert!(s.contains("25000"));
        assert!(s.contains("20000"));
    }

    #[test]
    fn kan_shape_carries_expected() {
        let e = WaveAudioError::KanShape {
            expected: "(I=2, O=4)",
            actual: "(I=4, O=4)".into(),
        };
        assert_eq!(e.code(), "WAV0004");
        assert!(format!("{e}").contains("I=2"));
    }

    #[test]
    fn vocal_tract_static_message() {
        let e = WaveAudioError::VocalTract("zero length");
        assert_eq!(e.code(), "WAV0005");
        assert!(format!("{e}").contains("zero length"));
    }

    #[test]
    fn conservation_violation_carries_delta() {
        let e = WaveAudioError::ConservationViolation {
            prev: 1.0,
            next: 1.001,
            epsilon: 1e-4,
        };
        assert_eq!(e.code(), "WAV0006");
    }

    #[test]
    fn agency_violation_carries_explanation() {
        let e = WaveAudioError::AgencyViolation {
            explanation: "LIGHT→MANA must be zero",
        };
        assert_eq!(e.code(), "WAV0007");
        assert!(format!("{e}").contains("LIGHT→MANA"));
    }
}
