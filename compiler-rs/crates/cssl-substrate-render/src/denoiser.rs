//! § denoiser — spatio-temporal denoiser for CFER residual-noise.
//!
//! ## Role
//! Per spec § 36 § PERFORMANCE-TARGETS the denoiser is the residual-noise
//! filter that runs on the rendered image AFTER the per-cell convergence
//! has settled. CFER's noise floor is much lower than path-tracing's (no
//! sample-noise), but small residual fluctuations remain when convergence
//! halts at the budget-cap or when a cell is foveation-down-prioritized.
//! The denoiser is variance-driven : per-pixel variance estimates a sigma
//! and the spatio-temporal kernel weights the bilateral blend.
//!
//! ## Algorithm
//!   for each pixel p :
//!     spatial_filter[p] = Σ_q w_spatial(p,q) · I[q]
//!     temporal_filter[p] = α · spatial_filter[p] + (1-α) · prev_frame[p]
//!     output[p] = temporal_filter[p]
//!
//! ## Variance gating
//!   variance[p] computed from local 3×3 neighborhood ;
//!   sigma_spatial = variance[p].sqrt() · sigma_scale.

use thiserror::Error;

/// Error class for denoiser failures.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum DenoiserError {
    /// Image dimensions mismatch between current and previous frames.
    #[error("dimension mismatch : current = {0}x{1} vs previous = {2}x{3}")]
    DimensionMismatch(u32, u32, u32, u32),
    /// Image dimensions zero.
    #[error("image dimensions must be non-zero ; got {0}x{1}")]
    ZeroDimensions(u32, u32),
    /// Sigma out of [0, 16] range.
    #[error("sigma out of [0, 16] ; got {0}")]
    BadSigma(f32),
}

/// Denoiser configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct DenoiserConfig {
    /// Spatial Gaussian sigma (pixels). 0 = no spatial blur.
    pub sigma_spatial: f32,
    /// Temporal blend factor α ∈ [0,1] : weight of new vs prev frame.
    pub alpha_temporal: f32,
    /// Variance-multiplier : higher → more aggressive smoothing in noisy areas.
    pub sigma_scale: f32,
    /// Spatial kernel half-radius (pixels).
    pub spatial_radius: u32,
}

impl Default for DenoiserConfig {
    fn default() -> Self {
        Self {
            sigma_spatial: 1.5,
            alpha_temporal: 0.7,
            sigma_scale: 1.0,
            spatial_radius: 2,
        }
    }
}

impl DenoiserConfig {
    /// Validate.
    pub fn validate(&self) -> Result<(), DenoiserError> {
        if !(0.0..=16.0).contains(&self.sigma_spatial) {
            return Err(DenoiserError::BadSigma(self.sigma_spatial));
        }
        if !(0.0..=16.0).contains(&self.sigma_scale) {
            return Err(DenoiserError::BadSigma(self.sigma_scale));
        }
        Ok(())
    }
}

/// Spatio-temporal denoiser.
#[derive(Debug, Clone)]
pub struct Denoiser {
    pub config: DenoiserConfig,
    /// Previous frame for temporal blending. None on first frame.
    pub prev_frame: Option<Vec<[f32; 3]>>,
    pub prev_width: u32,
    pub prev_height: u32,
}

impl Default for Denoiser {
    fn default() -> Self {
        Self {
            config: DenoiserConfig::default(),
            prev_frame: None,
            prev_width: 0,
            prev_height: 0,
        }
    }
}

impl Denoiser {
    /// Construct.
    pub fn new(config: DenoiserConfig) -> Result<Self, DenoiserError> {
        config.validate()?;
        Ok(Self {
            config,
            prev_frame: None,
            prev_width: 0,
            prev_height: 0,
        })
    }

    /// Compute per-pixel local variance over a 3×3 neighborhood.
    pub fn local_variance(pixels: &[[f32; 3]], width: u32, height: u32) -> Vec<f32> {
        let n = (width * height) as usize;
        let mut var = vec![0.0_f32; n];
        let w = width as i32;
        let h = height as i32;
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize;
                let mut sum = [0.0_f32; 3];
                let mut sum_sq = [0.0_f32; 3];
                let mut count = 0_f32;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let nx = x + dx;
                        let ny = y + dy;
                        if nx >= 0 && nx < w && ny >= 0 && ny < h {
                            let n_idx = (ny * w + nx) as usize;
                            for c in 0..3 {
                                sum[c] += pixels[n_idx][c];
                                sum_sq[c] += pixels[n_idx][c] * pixels[n_idx][c];
                            }
                            count += 1.0;
                        }
                    }
                }
                let inv = 1.0 / count.max(1.0);
                let mut v = 0.0_f32;
                for c in 0..3 {
                    let mean = sum[c] * inv;
                    v += sum_sq[c] * inv - mean * mean;
                }
                var[idx] = (v / 3.0).max(0.0);
            }
        }
        var
    }

    /// Apply spatial Gaussian blur with variance-modulated sigma.
    pub fn spatial_pass(&self, pixels: &[[f32; 3]], width: u32, height: u32) -> Vec<[f32; 3]> {
        let variance = Self::local_variance(pixels, width, height);
        let n = (width * height) as usize;
        let mut out = vec![[0.0_f32; 3]; n];
        let w = width as i32;
        let h = height as i32;
        let r = self.config.spatial_radius as i32;
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize;
                let sigma = (self.config.sigma_spatial
                    + variance[idx].sqrt() * self.config.sigma_scale)
                    .max(0.1);
                let two_sig2 = 2.0 * sigma * sigma;

                let mut sum = [0.0_f32; 3];
                let mut weight_sum = 0.0_f32;
                for dy in -r..=r {
                    for dx in -r..=r {
                        let nx = x + dx;
                        let ny = y + dy;
                        if nx >= 0 && nx < w && ny >= 0 && ny < h {
                            let n_idx = (ny * w + nx) as usize;
                            let dist2 = (dx * dx + dy * dy) as f32;
                            let wgt = (-dist2 / two_sig2).exp();
                            for c in 0..3 {
                                sum[c] += pixels[n_idx][c] * wgt;
                            }
                            weight_sum += wgt;
                        }
                    }
                }
                let inv = 1.0 / weight_sum.max(1e-6);
                for c in 0..3 {
                    out[idx][c] = sum[c] * inv;
                }
            }
        }
        out
    }

    /// Temporal blend with the previous frame (if dimensions match).
    pub fn temporal_pass(&self, pixels: &[[f32; 3]], width: u32, height: u32) -> Vec<[f32; 3]> {
        match (&self.prev_frame, self.prev_width == width && self.prev_height == height) {
            (Some(prev), true) if prev.len() == pixels.len() => {
                let alpha = self.config.alpha_temporal.clamp(0.0, 1.0);
                pixels
                    .iter()
                    .zip(prev.iter())
                    .map(|(p, q)| {
                        [
                            p[0] * alpha + q[0] * (1.0 - alpha),
                            p[1] * alpha + q[1] * (1.0 - alpha),
                            p[2] * alpha + q[2] * (1.0 - alpha),
                        ]
                    })
                    .collect()
            }
            _ => pixels.to_vec(),
        }
    }

    /// Run a full denoiser pass : spatial → temporal. Updates prev_frame.
    pub fn denoise(
        &mut self,
        pixels: &[[f32; 3]],
        width: u32,
        height: u32,
    ) -> Result<Vec<[f32; 3]>, DenoiserError> {
        if width == 0 || height == 0 {
            return Err(DenoiserError::ZeroDimensions(width, height));
        }
        if pixels.len() != (width * height) as usize {
            return Err(DenoiserError::DimensionMismatch(
                width,
                height,
                self.prev_width,
                self.prev_height,
            ));
        }
        let spatial = self.spatial_pass(pixels, width, height);
        let temporal = self.temporal_pass(&spatial, width, height);
        self.prev_frame = Some(temporal.clone());
        self.prev_width = width;
        self.prev_height = height;
        Ok(temporal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_validates() {
        let c = DenoiserConfig::default();
        assert!(c.validate().is_ok());
    }

    #[test]
    fn local_variance_zero_for_constant_image() {
        let pixels = vec![[0.5_f32; 3]; 4 * 4];
        let v = Denoiser::local_variance(&pixels, 4, 4);
        for i in 0..16 {
            assert!(v[i] < 1e-6);
        }
    }

    #[test]
    fn local_variance_nonzero_for_checker() {
        let mut pixels = vec![[0.0_f32; 3]; 4 * 4];
        for y in 0..4 {
            for x in 0..4 {
                let v = if (x + y) % 2 == 0 { 0.0 } else { 1.0 };
                pixels[y * 4 + x] = [v; 3];
            }
        }
        let v = Denoiser::local_variance(&pixels, 4, 4);
        // Interior pixels should see non-zero variance.
        assert!(v[5] > 0.0);
    }

    #[test]
    fn spatial_pass_preserves_constant_image() {
        let d = Denoiser::default();
        let pixels = vec![[0.5_f32; 3]; 4 * 4];
        let out = d.spatial_pass(&pixels, 4, 4);
        for p in &out {
            for c in 0..3 {
                assert!((p[c] - 0.5).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn temporal_pass_first_frame_returns_input() {
        let d = Denoiser::default();
        let pixels = vec![[0.7_f32; 3]; 9];
        let out = d.temporal_pass(&pixels, 3, 3);
        for (a, b) in pixels.iter().zip(out.iter()) {
            for c in 0..3 {
                assert_eq!(a[c], b[c]);
            }
        }
    }

    #[test]
    fn temporal_pass_blends_with_prev() {
        let mut d = Denoiser::default();
        let prev = vec![[1.0_f32; 3]; 4];
        d.prev_frame = Some(prev);
        d.prev_width = 2;
        d.prev_height = 2;
        let new = vec![[0.0_f32; 3]; 4];
        let out = d.temporal_pass(&new, 2, 2);
        // alpha = 0.7 → out = 0.7·new + 0.3·prev = 0.3
        for p in &out {
            for c in 0..3 {
                assert!((p[c] - 0.3).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn denoise_records_prev_frame() {
        let mut d = Denoiser::default();
        let pixels = vec![[0.5_f32; 3]; 4 * 4];
        let _ = d.denoise(&pixels, 4, 4).unwrap();
        assert!(d.prev_frame.is_some());
        assert_eq!(d.prev_width, 4);
        assert_eq!(d.prev_height, 4);
    }

    #[test]
    fn denoise_zero_dim_errors() {
        let mut d = Denoiser::default();
        let pixels = vec![];
        let err = d.denoise(&pixels, 0, 0);
        assert!(matches!(err, Err(DenoiserError::ZeroDimensions(0, 0))));
    }

    #[test]
    fn variance_driven_smoothing_reduces_noise() {
        let mut d = Denoiser::default();
        let mut noisy = vec![[0.5_f32; 3]; 8 * 8];
        // Inject noise
        for i in 0..(8 * 8) {
            let n = (i as f32).sin() * 0.1;
            for c in 0..3 {
                noisy[i][c] = (0.5 + n).clamp(0.0, 1.0);
            }
        }
        let original_var = Denoiser::local_variance(&noisy, 8, 8).iter().sum::<f32>();
        let out = d.denoise(&noisy, 8, 8).unwrap();
        let denoised_var = Denoiser::local_variance(&out, 8, 8).iter().sum::<f32>();
        assert!(denoised_var <= original_var);
    }
}
