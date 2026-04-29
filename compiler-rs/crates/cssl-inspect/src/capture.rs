//! § Capture-frame surface.
//!
//! Phase-J § 2.5 mandates :
//!   - Three formats supported : PNG_sRGB / EXR_HDR / SpectralBin.
//!   - The output path is recorded as a 32-byte hash ; the raw path is
//!     NEVER logged (privacy-audit invariant per landmine L8).
//!   - CaptureHandle carries `format`, `output_path_hash`, `size_bytes`,
//!     `audit_seq`. `region` is `Option<AABB>` ; the MVP does not implement
//!     AABB so it is omitted from the surface and is implicit "whole frame".
//!   - Callsite must hold `Cap<TelemetryEgress>`.
//!
//! § This MVP slice produces a fake hash + format-tag + a synthesised
//! size. The real-impl will engage the render-graph fence + invoke the
//! per-format encoder. The path-hash is BLAKE3 in production ; this MVP
//! uses a deterministic fold of the format-tag so tests can assert on it.

use crate::{
    mock_substrate::{Cap, TelemetryEgress},
    InspectError,
};

/// Capture format. Phase-J § 2.5 enumerates exactly these three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureFormat {
    /// 8 or 16-bit PNG, sRGB color-space.
    PngSrgb {
        /// Bit depth (8 or 16 ; mock validates).
        bit_depth: u8,
    },
    /// EXR HDR. half- or full-precision float.
    ExrHdr {
        /// Whether the encoded floats are half-precision.
        half_precision: bool,
    },
    /// Raw spectral binary ; n_bands typically 16.
    SpectralBin {
        /// Number of spectral bands (e.g. 16).
        n_bands: u8,
    },
}

impl CaptureFormat {
    /// Stable string tag for the format. Used by the MVP path-hasher and
    /// by the audit-log entry. Stable across versions of this crate.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            CaptureFormat::PngSrgb { .. } => "png_srgb",
            CaptureFormat::ExrHdr { .. } => "exr_hdr",
            CaptureFormat::SpectralBin { .. } => "spectral_bin",
        }
    }

    /// Whether this format variant carries a valid parameter set.
    /// PNG bit_depth must be 8 or 16 ; spectral n_bands must be > 0 and
    /// ≤ 64.
    ///
    /// # Errors
    /// Returns `CaptureFormatUnsupported` if the variant carries an
    /// invalid parameter (PNG bit-depth other than 8/16, spectral n-bands
    /// outside 1..=64).
    pub fn validate(self) -> Result<(), InspectError> {
        match self {
            CaptureFormat::PngSrgb { bit_depth } if bit_depth == 8 || bit_depth == 16 => Ok(()),
            CaptureFormat::PngSrgb { bit_depth } => Err(InspectError::CaptureFormatUnsupported {
                tag: format!("png_srgb (bit_depth={bit_depth} ; expected 8 or 16)"),
            }),
            CaptureFormat::ExrHdr { .. } => Ok(()),
            CaptureFormat::SpectralBin { n_bands } if n_bands > 0 && n_bands <= 64 => Ok(()),
            CaptureFormat::SpectralBin { n_bands } => Err(InspectError::CaptureFormatUnsupported {
                tag: format!("spectral_bin (n_bands={n_bands} ; expected 1..=64)"),
            }),
        }
    }
}

/// A 32-byte path-hash. Phase-J § 2.5 mandates BLAKE3 ; this MVP uses a
/// deterministic synthesised hash so tests can assert on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathHash(pub [u8; 32]);

impl PathHash {
    /// Construct a synthesised hash from a format-tag + audit-seq. The
    /// real-impl will hash the actual output path.
    #[must_use]
    pub fn synth(tag: &str, audit_seq: u64) -> Self {
        let mut buf = [0u8; 32];
        // Fold the tag bytes
        for (i, byte) in tag.bytes().enumerate() {
            buf[i % 32] ^= byte;
        }
        // Fold the audit_seq across the back half so different captures
        // get different hashes.
        let seq_bytes = audit_seq.to_le_bytes();
        for (i, byte) in seq_bytes.iter().enumerate() {
            buf[16 + (i % 16)] ^= byte;
        }
        Self(buf)
    }

    /// The raw 32-byte hash.
    #[must_use]
    pub fn raw(self) -> [u8; 32] {
        self.0
    }
}

/// A capture-handle returned by `capture_frame`. Read-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureHandle {
    /// Format of the captured frame.
    pub format: CaptureFormat,
    /// Format tag string (for convenient inspection).
    pub format_tag: &'static str,
    /// Hash of the output path. Raw path is NEVER exposed.
    pub output_path_hash: PathHash,
    /// Synthesised size in bytes (mock).
    pub size_bytes: u64,
    /// Audit sequence at capture time.
    pub audit_seq: u64,
}

/// Mock size-estimator. Returns a per-format synthesised value so tests can
/// assert different formats produce different sizes.
fn synth_size(format: CaptureFormat) -> u64 {
    match format {
        CaptureFormat::PngSrgb { bit_depth } => 1024 * u64::from(bit_depth),
        CaptureFormat::ExrHdr { half_precision } => {
            if half_precision {
                4 * 1024
            } else {
                8 * 1024
            }
        }
        CaptureFormat::SpectralBin { n_bands } => 2048 * u64::from(n_bands),
    }
}

/// Capture a frame. The MVP returns a CaptureHandle containing a fake
/// path-hash with the format tag folded in. Real-impl engages the
/// render-graph fence + per-format encoder.
///
/// # Errors
/// - `CapabilityMissing` if `egress` does not actually grant telemetry-egress
/// - `CaptureFormatUnsupported` if `format` carries invalid parameters
pub fn capture_frame(
    egress: &Cap<TelemetryEgress>,
    format: CaptureFormat,
    audit_seq: u64,
) -> Result<CaptureHandle, InspectError> {
    if !egress.permits_egress() {
        return Err(InspectError::CapabilityMissing {
            needed: "Cap<TelemetryEgress>".into(),
        });
    }
    format.validate()?;
    let tag = format.tag();
    Ok(CaptureHandle {
        format,
        format_tag: tag,
        output_path_hash: PathHash::synth(tag, audit_seq),
        size_bytes: synth_size(format),
        audit_seq,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_tag_is_png_srgb() {
        assert_eq!(CaptureFormat::PngSrgb { bit_depth: 8 }.tag(), "png_srgb");
    }

    #[test]
    fn exr_tag_is_exr_hdr() {
        assert_eq!(
            CaptureFormat::ExrHdr {
                half_precision: false
            }
            .tag(),
            "exr_hdr"
        );
    }

    #[test]
    fn spectral_tag_is_spectral_bin() {
        assert_eq!(
            CaptureFormat::SpectralBin { n_bands: 16 }.tag(),
            "spectral_bin"
        );
    }

    #[test]
    fn png_validate_accepts_8() {
        assert!(CaptureFormat::PngSrgb { bit_depth: 8 }.validate().is_ok());
    }

    #[test]
    fn png_validate_accepts_16() {
        assert!(CaptureFormat::PngSrgb { bit_depth: 16 }.validate().is_ok());
    }

    #[test]
    fn png_validate_refuses_other() {
        assert!(CaptureFormat::PngSrgb { bit_depth: 12 }.validate().is_err());
    }

    #[test]
    fn spectral_validate_refuses_zero() {
        assert!(CaptureFormat::SpectralBin { n_bands: 0 }
            .validate()
            .is_err());
    }

    #[test]
    fn spectral_validate_refuses_overlarge() {
        assert!(CaptureFormat::SpectralBin { n_bands: 200 }
            .validate()
            .is_err());
    }

    #[test]
    fn capture_with_valid_egress_succeeds() {
        let egress = Cap::<TelemetryEgress>::egress_for_tests();
        let h = capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 8 }, 1).unwrap();
        assert_eq!(h.format_tag, "png_srgb");
        assert_eq!(h.audit_seq, 1);
    }

    #[test]
    fn capture_without_egress_refused() {
        let bad = Cap::<TelemetryEgress>::synthetic_nonegress_for_tests();
        let r = capture_frame(&bad, CaptureFormat::PngSrgb { bit_depth: 8 }, 1);
        assert!(matches!(r, Err(InspectError::CapabilityMissing { .. })));
    }

    #[test]
    fn capture_three_formats_produce_distinct_hashes() {
        let egress = Cap::<TelemetryEgress>::egress_for_tests();
        let h1 = capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 8 }, 1).unwrap();
        let h2 = capture_frame(
            &egress,
            CaptureFormat::ExrHdr {
                half_precision: false,
            },
            1,
        )
        .unwrap();
        let h3 = capture_frame(&egress, CaptureFormat::SpectralBin { n_bands: 16 }, 1).unwrap();
        assert_ne!(h1.output_path_hash, h2.output_path_hash);
        assert_ne!(h2.output_path_hash, h3.output_path_hash);
        assert_ne!(h1.output_path_hash, h3.output_path_hash);
    }

    #[test]
    fn capture_distinct_audit_seq_distinct_hashes() {
        let egress = Cap::<TelemetryEgress>::egress_for_tests();
        let h1 = capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 8 }, 1).unwrap();
        let h2 = capture_frame(&egress, CaptureFormat::PngSrgb { bit_depth: 8 }, 2).unwrap();
        assert_ne!(h1.output_path_hash, h2.output_path_hash);
    }

    #[test]
    fn synth_size_png_scales_with_bit_depth() {
        assert!(
            synth_size(CaptureFormat::PngSrgb { bit_depth: 16 })
                > synth_size(CaptureFormat::PngSrgb { bit_depth: 8 })
        );
    }

    #[test]
    fn synth_size_spectral_scales_with_bands() {
        assert!(
            synth_size(CaptureFormat::SpectralBin { n_bands: 32 })
                > synth_size(CaptureFormat::SpectralBin { n_bands: 4 })
        );
    }

    #[test]
    fn path_hash_is_32_bytes() {
        let h = PathHash::synth("test", 0);
        assert_eq!(h.raw().len(), 32);
    }

    #[test]
    fn path_hash_distinct_across_seq() {
        assert_ne!(PathHash::synth("test", 1), PathHash::synth("test", 2));
    }
}
