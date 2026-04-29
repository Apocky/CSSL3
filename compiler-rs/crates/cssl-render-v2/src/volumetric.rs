//! § volumetric — in-scattering accumulation along the ray-march path.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage-5 emits a `VolumetricAccum` buffer that captures the in-scattering
//!   integral along each ray (volumetric fog, atmospheric scatter, mana-band
//!   aurora). The value is sampled from `PsiField<LIGHT, 16>` (Stage-4
//!   wave-solver output) at evenly-spaced steps along the ray and accumulated
//!   into a single per-pixel scalar (or a 16-band vector when the spectral path
//!   is wired in via D118).
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § III` Stage-5 step 3 :
//!     volumetric-pass : along-ray sample PsiField<LIGHT> for-in-scattering.
//!
//! § IMPLEMENTATION (foundation slice)
//!   The full PsiField sampling is wired through the [`crate::spectral_hook`]
//!   integration trait (D118). At foundation, the volumetric accumulator
//!   integrates a constant per-step in-scattering coefficient × step-length,
//!   which is sufficient to plumb the buffer into Stage-6 + Stage-10 ; the
//!   real physics lands in D118 + D114 (wave-solver).

/// One sample of the in-scattering field at a point along a ray.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumetricSample {
    /// World-space sample position.
    pub p: [f32; 3],
    /// Incoming-radiance contribution at this point (single-band foundation).
    pub radiance: f32,
    /// Step-length that produced this sample.
    pub step_length: f32,
}

impl VolumetricSample {
    /// Construct a sample.
    #[must_use]
    pub fn new(p: [f32; 3], radiance: f32, step_length: f32) -> Self {
        VolumetricSample {
            p,
            radiance,
            step_length,
        }
    }

    /// Accumulator-contribution = radiance × step_length.
    #[must_use]
    pub fn contribution(&self) -> f32 {
        self.radiance * self.step_length
    }
}

/// Per-pixel in-scattering accumulator. Aggregates samples along one ray.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumetricAccum {
    /// Total accumulated in-scattering radiance (single-band foundation).
    pub total: f32,
    /// Number of samples folded into the accumulator.
    pub sample_count: u32,
    /// Total step-length integrated (= total ray distance walked).
    pub integration_length: f32,
}

impl Default for VolumetricAccum {
    fn default() -> Self {
        VolumetricAccum {
            total: 0.0,
            sample_count: 0,
            integration_length: 0.0,
        }
    }
}

impl VolumetricAccum {
    /// New empty accumulator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one sample to the accumulator.
    pub fn add(&mut self, s: VolumetricSample) {
        self.total += s.contribution();
        self.sample_count += 1;
        self.integration_length += s.step_length;
    }

    /// Mean in-scattering coefficient (= total / integration_length).
    #[must_use]
    pub fn mean_coefficient(&self) -> f32 {
        if self.integration_length > 1e-9 {
            self.total / self.integration_length
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_contribution_is_radiance_times_step() {
        let s = VolumetricSample::new([0.0; 3], 0.5, 0.1);
        assert!((s.contribution() - 0.05).abs() < 1e-6);
    }

    #[test]
    fn accum_default_zero() {
        let a = VolumetricAccum::new();
        assert_eq!(a.total, 0.0);
        assert_eq!(a.sample_count, 0);
        assert_eq!(a.integration_length, 0.0);
    }

    #[test]
    fn accum_add_increments_counters() {
        let mut a = VolumetricAccum::new();
        a.add(VolumetricSample::new([0.0; 3], 1.0, 0.1));
        a.add(VolumetricSample::new([0.0; 3], 2.0, 0.1));
        assert_eq!(a.sample_count, 2);
        assert!((a.integration_length - 0.2).abs() < 1e-6);
        assert!((a.total - 0.3).abs() < 1e-6);
    }

    #[test]
    fn mean_coefficient_zero_when_empty() {
        let a = VolumetricAccum::new();
        assert!((a.mean_coefficient() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn mean_coefficient_matches_average() {
        let mut a = VolumetricAccum::new();
        a.add(VolumetricSample::new([0.0; 3], 1.0, 0.5));
        a.add(VolumetricSample::new([0.0; 3], 3.0, 0.5));
        // total = 0.5 + 1.5 = 2.0 ; length = 1.0 ; mean = 2.0
        assert!((a.mean_coefficient() - 2.0).abs() < 1e-6);
    }
}
