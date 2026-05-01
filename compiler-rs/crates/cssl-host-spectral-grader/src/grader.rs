// § T11-W5-SPECTRAL-GRADER · grader.rs
// § I> high-level driver · pixel + bulk-image API · method-pluggable
// § I> input gamma decoded inside grade_pixel for u8 textures (caller-owned)

//! High-level driver wrapping the lower-level upsamplers in a method-pluggable
//! struct suitable for asset-pipeline pre-bake.
//!
//! ## Typical use
//!
//! ```no_run
//! use cssl_host_spectral_grader::{GraderMethod, SpectralGrader};
//! let g = SpectralGrader::new(GraderMethod::SmitsLike).with_input_gamma(2.2);
//! let spd = g.grade_pixel([255, 64, 32]);
//! // hand spd to cssl-spectral-render as a reflectance channel ...
//! # let _ = spd;
//! ```

use crate::spd::Spd;
use crate::upsample::{rgb_to_spd_jakob_simplified, rgb_to_spd_smits_like};

/// Selectable upsampling strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraderMethod {
    /// Smits-1999-style basis decomposition with Newton refinement.
    /// Best round-trip; ~3× the cost of `JakobSimplified`.
    SmitsLike,
    /// 3-Gaussian sum keyed on R/G/B amplitude. Cheaper, looser round-trip.
    JakobSimplified,
    /// Reference / fallback : flat luminance-matched gray.
    FlatGray,
}

/// High-level grader. Stateless apart from configuration.
#[derive(Debug, Clone, Copy)]
pub struct SpectralGrader {
    method: GraderMethod,
    gamma_input: f32,
}

/// Error type for bulk operations on raw byte buffers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraderError {
    /// Pixel buffer length doesn't match `4 * w * h` (RGBA8 expected).
    BufferLengthMismatch {
        /// Expected length given `(w, h)`.
        expected: usize,
        /// Actual buffer length.
        actual: usize,
    },
    /// Width or height is zero.
    EmptyImage,
}

impl core::fmt::Display for GraderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BufferLengthMismatch { expected, actual } => write!(
                f,
                "RGBA8 buffer length mismatch : expected {expected}, got {actual}"
            ),
            Self::EmptyImage => write!(f, "image dimensions are zero"),
        }
    }
}

impl std::error::Error for GraderError {}

impl SpectralGrader {
    /// Construct a grader with the given method, default gamma 1.0 (linear).
    #[must_use]
    pub const fn new(method: GraderMethod) -> Self {
        Self {
            method,
            gamma_input: 1.0,
        }
    }

    /// Builder : set the input gamma (typical sRGB texture = 2.2).
    /// Values ≤ 0 are clamped to 1.0 (linear).
    #[must_use]
    pub fn with_input_gamma(mut self, gamma: f32) -> Self {
        self.gamma_input = if gamma > 0.0 && gamma.is_finite() {
            gamma
        } else {
            1.0
        };
        self
    }

    /// Convert an 8-bit RGB triple to a 16-band SPD.
    #[must_use]
    pub fn grade_pixel(&self, rgb_8bit: [u8; 3]) -> Spd {
        // Map u8 → [0, 1] then optionally undo the input gamma.
        let inv = 1.0 / 255.0;
        let mut rgb_lin = [
            f32::from(rgb_8bit[0]) * inv,
            f32::from(rgb_8bit[1]) * inv,
            f32::from(rgb_8bit[2]) * inv,
        ];
        if (self.gamma_input - 1.0).abs() > 1e-3 {
            for c in &mut rgb_lin {
                *c = c.powf(self.gamma_input);
            }
        }
        match self.method {
            GraderMethod::SmitsLike => rgb_to_spd_smits_like(rgb_lin),
            GraderMethod::JakobSimplified => rgb_to_spd_jakob_simplified(rgb_lin),
            GraderMethod::FlatGray => {
                // Luminance via Rec.709 weights, then a flat SPD at that level.
                let y = 0.072_192f32.mul_add(rgb_lin[2], 0.212_639f32.mul_add(rgb_lin[0], 0.715_169 * rgb_lin[1]));
                let mut s = Spd::zeros();
                for v in &mut s.samples {
                    *v = y;
                }
                s.clamp_to_unit();
                s
            }
        }
    }

    /// Bulk-grade an RGBA8 image. Alpha is ignored. Width or height of zero
    /// is rejected; buffer length must equal `4 * w * h`.
    pub fn grade_image_rgba8(
        &self,
        pixels: &[u8],
        w: u32,
        h: u32,
    ) -> Result<Vec<Spd>, GraderError> {
        if w == 0 || h == 0 {
            return Err(GraderError::EmptyImage);
        }
        let n = (w as usize) * (h as usize);
        let expected = n.checked_mul(4).ok_or(GraderError::BufferLengthMismatch {
            expected: usize::MAX,
            actual: pixels.len(),
        })?;
        if pixels.len() != expected {
            return Err(GraderError::BufferLengthMismatch {
                expected,
                actual: pixels.len(),
            });
        }

        let mut out = Vec::with_capacity(n);
        for chunk in pixels.chunks_exact(4) {
            let rgb = [chunk[0], chunk[1], chunk[2]];
            out.push(self.grade_pixel(rgb));
        }
        Ok(out)
    }

    /// Human-readable description of the configured method (for telemetry / logs).
    #[must_use]
    pub fn report_method_text(&self) -> String {
        let m = match self.method {
            GraderMethod::SmitsLike => "smits-like (basis-decompose + 2-pass Newton refine)",
            GraderMethod::JakobSimplified => "jakob-simplified (3-Gaussian sum, σ adaptive)",
            GraderMethod::FlatGray => "flat-gray (Rec.709 luminance)",
        };
        format!(
            "SpectralGrader{{method={}, gamma_input={:.3}, n_bands=16}}",
            m, self.gamma_input
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grade_pixel_zero_yields_zero_spd() {
        for method in [
            GraderMethod::SmitsLike,
            GraderMethod::JakobSimplified,
            GraderMethod::FlatGray,
        ] {
            let g = SpectralGrader::new(method);
            let spd = g.grade_pixel([0, 0, 0]);
            assert!(spd.is_finite());
            // Black should integrate to ~0 luminance.
            let xyz = crate::cmf::spd_to_xyz(&spd);
            assert!(xyz[1].abs() < 1e-3, "method {:?} : black Y = {}", method, xyz[1]);
        }
    }

    #[test]
    fn grade_pixel_white_is_high_luminance() {
        // Smits-like white should be near-Y=1.0 ; Jakob looser ; FlatGray exact.
        let g = SpectralGrader::new(GraderMethod::SmitsLike);
        let spd = g.grade_pixel([255, 255, 255]);
        let xyz = crate::cmf::spd_to_xyz(&spd);
        assert!(
            xyz[1] > 0.85 && xyz[1] < 1.15,
            "smits white Y out of range : {}",
            xyz[1]
        );

        let g_flat = SpectralGrader::new(GraderMethod::FlatGray);
        let spd_flat = g_flat.grade_pixel([255, 255, 255]);
        let xyz_flat = crate::cmf::spd_to_xyz(&spd_flat);
        assert!((xyz_flat[1] - 1.0).abs() < 1e-2, "flat white Y = {}", xyz_flat[1]);
    }

    #[test]
    fn grade_image_length_matches_pixel_count() {
        let g = SpectralGrader::new(GraderMethod::JakobSimplified);
        // 2x3 image = 6 pixels = 24 bytes (RGBA8).
        let pixels: Vec<u8> = (0..24).map(|i| (i * 7) as u8).collect();
        let result = g.grade_image_rgba8(&pixels, 2, 3).expect("image grade");
        assert_eq!(result.len(), 6);
        for s in &result {
            assert!(s.is_finite());
        }

        // Mismatched buffer length is an error.
        let bad = g.grade_image_rgba8(&pixels[..23], 2, 3);
        assert!(matches!(
            bad,
            Err(GraderError::BufferLengthMismatch { .. })
        ));

        // Zero dims is an error.
        let zeroed = g.grade_image_rgba8(&[], 0, 1);
        assert_eq!(zeroed, Err(GraderError::EmptyImage));
    }

    #[test]
    fn input_gamma_affects_result() {
        let g_lin = SpectralGrader::new(GraderMethod::SmitsLike);
        let g_22 = SpectralGrader::new(GraderMethod::SmitsLike).with_input_gamma(2.2);
        // Mid-gray in 8-bit (128) should integrate differently under gamma 2.2.
        let spd_lin = g_lin.grade_pixel([128, 128, 128]);
        let spd_22 = g_22.grade_pixel([128, 128, 128]);
        let y_lin = crate::cmf::spd_to_xyz(&spd_lin)[1];
        let y_22 = crate::cmf::spd_to_xyz(&spd_22)[1];
        // gamma=2.2 darkens mid-gray.
        assert!(
            y_22 < y_lin - 0.1,
            "gamma should darken : y_lin={y_lin} y_22={y_22}"
        );
        // Method-text always non-empty.
        let txt = g_22.report_method_text();
        assert!(txt.contains("smits-like"));
        assert!(txt.contains("gamma_input"));
    }
}
