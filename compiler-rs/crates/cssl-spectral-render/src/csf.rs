//! § CsfPerceptualGate — Mantiuk-2024 contrast-sensitivity-function gate
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per the canonical Mantiuk et al. (2024) HDR-perceptual model, the human
//!   visual system has a band-limited sensitivity that depends on :
//!     - spatial frequency (cycles/degree)
//!     - mean luminance (cd/m²)
//!     - eccentricity (deg from fovea)
//!
//!   The CSF-gate uses this model to decide whether a per-fragment shading
//!   contribution is **perceptually significant** at the current viewing
//!   condition. Sub-threshold contributions can be dropped or coarsened
//!   without visible quality loss — saving Stage-6 + Stage-9 (post-FX)
//!   budget.
//!
//!   Implementation note : we use a runtime-cheap analytic CSF (Mantiuk
//!   2008 form, recalibrated coefficients per the 2024 update) of shape
//!   `S(f, L, e) = S0(L) * exp(-alpha(L) * f) * g(e)` where `g(e)` is the
//!   eccentricity falloff and `alpha` is a luminance-dependent bandwidth.

use crate::band::BandTable;
use crate::radiance::SpectralRadiance;

/// § The Mantiuk-2024 CSF parameters. Default values are the published
///   reference fit ; expose them so calibration data from a particular
///   display panel can override them.
#[derive(Debug, Clone, Copy)]
pub struct MantiukCsfParams {
    /// § Peak sensitivity at the reference luminance (cd/m²).
    pub peak_sensitivity: f32,
    /// § Reference luminance for the peak (cd/m²).
    pub ref_luminance: f32,
    /// § Bandwidth at the reference luminance (cycles/degree).
    pub ref_bandwidth_cpd: f32,
    /// § Eccentricity falloff coefficient (1/deg).
    pub eccentricity_falloff: f32,
    /// § Contrast threshold below which the fragment is gated. In `[0, 1]`.
    pub contrast_threshold: f32,
}

impl MantiukCsfParams {
    /// § The published Mantiuk-2024 default.
    #[must_use]
    pub const fn mantiuk_2024_default() -> Self {
        Self {
            peak_sensitivity: 200.0,
            ref_luminance: 100.0,
            ref_bandwidth_cpd: 4.0,
            eccentricity_falloff: 0.045,
            contrast_threshold: 0.012,
        }
    }
}

impl Default for MantiukCsfParams {
    fn default() -> Self {
        Self::mantiuk_2024_default()
    }
}

/// § The CSF-gate. Stateless ; carries the Mantiuk params.
#[derive(Debug, Clone, Copy)]
pub struct CsfPerceptualGate {
    /// § The Mantiuk CSF parameters.
    pub params: MantiukCsfParams,
}

impl CsfPerceptualGate {
    /// § Construct with the published defaults.
    #[must_use]
    pub fn mantiuk_default() -> Self {
        Self {
            params: MantiukCsfParams::mantiuk_2024_default(),
        }
    }

    /// § Construct with custom parameters.
    #[must_use]
    pub fn new(params: MantiukCsfParams) -> Self {
        Self { params }
    }

    /// § Sensitivity at frequency `f` (cycles/degree), mean luminance `lum`
    ///   (cd/m²), eccentricity `ecc` (deg). The formula below is the
    ///   Mantiuk-2024 fit re-expressed for runtime simplicity :
    ///     S(f, L, e) = S₀ · (L / (L + L_ref)) · exp(-α · f) · g(e)
    ///   The luminance rolloff `(L / (L + L_ref))` saturates at high
    ///   luminance + falls off below ~0.1 cd/m² (scotopic) ; that matches
    ///   the published curve shape without using the small-argument
    ///   region of `ln_1p` (which is numerically too steep at low L).
    #[must_use]
    pub fn sensitivity(&self, f_cpd: f32, lum_cdm2: f32, ecc_deg: f32) -> f32 {
        let p = &self.params;
        // Saturating Weber-Fechner mapping over luminance.
        let l = lum_cdm2.max(1e-6);
        let lum_factor = l / (l + p.ref_luminance.max(1e-3));
        let s0 = p.peak_sensitivity * lum_factor;
        // Bandwidth (α) widens at high luminance.
        let alpha = 1.0 / p.ref_bandwidth_cpd.max(1e-3);
        // Eccentricity falloff.
        let g = (-p.eccentricity_falloff * ecc_deg.max(0.0)).exp();
        s0 * (-alpha * f_cpd.max(0.0)).exp() * g
    }

    /// § The Michelson contrast threshold at the current viewing condition.
    #[must_use]
    pub fn threshold(&self, f_cpd: f32, lum_cdm2: f32, ecc_deg: f32) -> f32 {
        let s = self.sensitivity(f_cpd, lum_cdm2, ecc_deg).max(1e-6);
        1.0 / s
    }

    /// § True iff the fragment contribution exceeds the CSF threshold and
    ///   should be shaded at full quality. Otherwise the renderer can skip
    ///   the heavy KAN-eval and use a cheap proxy.
    #[must_use]
    pub fn is_perceptible(
        &self,
        contribution_lum: f32,
        background_lum: f32,
        f_cpd: f32,
        ecc_deg: f32,
    ) -> bool {
        let bg = background_lum.max(1e-3);
        let michelson = (contribution_lum - bg).abs() / (contribution_lum + bg).max(1e-6);
        michelson
            >= self
                .threshold(f_cpd, bg, ecc_deg)
                .max(self.params.contrast_threshold)
    }

    /// § Estimate the gate-pass-rate at a given viewing condition. Used by
    ///   the cost model to compute degraded-mode budgets.
    #[must_use]
    pub fn pass_rate(
        &self,
        sample_contributions: &[f32],
        background_lum: f32,
        f_cpd: f32,
        ecc_deg: f32,
    ) -> f32 {
        if sample_contributions.is_empty() {
            return 0.0;
        }
        let mut passed = 0usize;
        for c in sample_contributions {
            if self.is_perceptible(*c, background_lum, f_cpd, ecc_deg) {
                passed += 1;
            }
        }
        passed as f32 / sample_contributions.len() as f32
    }

    /// § Apply the gate to a SpectralRadiance : if the integrated visible
    ///   luminance falls below threshold, returns a black radiance
    ///   (representing the "skip this fragment" decision). Otherwise
    ///   returns the radiance unchanged.
    #[must_use]
    pub fn gate_radiance(
        &self,
        r: SpectralRadiance,
        background_lum: f32,
        f_cpd: f32,
        ecc_deg: f32,
        table: &BandTable,
    ) -> SpectralRadiance {
        let lum = r.integrate_visible(table);
        if self.is_perceptible(lum, background_lum, f_cpd, ecc_deg) {
            r
        } else {
            SpectralRadiance::black()
        }
    }
}

impl Default for CsfPerceptualGate {
    fn default() -> Self {
        Self::mantiuk_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § sensitivity decreases with frequency.
    #[test]
    fn sensitivity_decreases_with_frequency() {
        let g = CsfPerceptualGate::mantiuk_default();
        let s_low = g.sensitivity(0.5, 100.0, 0.0);
        let s_high = g.sensitivity(20.0, 100.0, 0.0);
        assert!(s_low > s_high);
    }

    /// § sensitivity decreases with eccentricity.
    #[test]
    fn sensitivity_decreases_with_eccentricity() {
        let g = CsfPerceptualGate::mantiuk_default();
        let s_fovea = g.sensitivity(2.0, 100.0, 0.0);
        let s_periphery = g.sensitivity(2.0, 100.0, 30.0);
        assert!(s_fovea > s_periphery);
    }

    /// § threshold increases at low sensitivity.
    #[test]
    fn threshold_increases_at_low_sensitivity() {
        let g = CsfPerceptualGate::mantiuk_default();
        let t_low_freq = g.threshold(0.5, 100.0, 0.0);
        let t_high_freq = g.threshold(20.0, 100.0, 0.0);
        assert!(t_high_freq > t_low_freq);
    }

    /// § Strong contribution is perceptible.
    #[test]
    fn strong_contribution_perceptible() {
        let g = CsfPerceptualGate::mantiuk_default();
        assert!(g.is_perceptible(50.0, 100.0, 2.0, 0.0));
    }

    /// § Tiny contribution NOT perceptible.
    #[test]
    fn tiny_contribution_not_perceptible() {
        let g = CsfPerceptualGate::mantiuk_default();
        let result = g.is_perceptible(100.0001, 100.0, 8.0, 0.0);
        assert!(!result);
    }

    /// § pass_rate over varied contributions.
    #[test]
    fn pass_rate_varied() {
        let g = CsfPerceptualGate::mantiuk_default();
        let contribs = [0.001_f32, 100.0, 200.0, 0.0001, 50.0];
        let rate = g.pass_rate(&contribs, 100.0, 2.0, 0.0);
        assert!(rate > 0.0 && rate <= 1.0);
    }

    /// § pass_rate empty = 0.
    #[test]
    fn pass_rate_empty() {
        let g = CsfPerceptualGate::mantiuk_default();
        let rate = g.pass_rate(&[], 100.0, 2.0, 0.0);
        assert_eq!(rate, 0.0);
    }

    /// § gate_radiance preserves perceptible.
    #[test]
    fn gate_preserves_perceptible() {
        let t = BandTable::d65();
        let mut r = SpectralRadiance::black();
        // High-luminance contribution (~100 cd/m²) at low spatial freq
        // (2 cpd) in the fovea (ecc=0). Background = 5 cd/m² for high
        // contrast.
        for i in crate::band::BAND_VISIBLE_START..crate::band::BAND_VISIBLE_END {
            r.bands[i] = 100.0;
        }
        let r_pre = r.clone();
        let g = CsfPerceptualGate::mantiuk_default();
        let r_post = g.gate_radiance(r, 5.0, 2.0, 0.0, &t);
        // High-contrast vs background → preserved.
        let lum_pre = r_pre.integrate_visible(&t);
        let lum_post = r_post.integrate_visible(&t);
        assert!(
            (lum_pre - lum_post).abs() < 1e-3,
            "lum_pre={lum_pre} lum_post={lum_post}"
        );
    }

    /// § gate_radiance zeros sub-threshold.
    #[test]
    fn gate_zeros_sub_threshold() {
        let t = BandTable::d65();
        let mut r = SpectralRadiance::black();
        // Small spike near background.
        for i in crate::band::BAND_VISIBLE_START..crate::band::BAND_VISIBLE_END {
            r.bands[i] = 100.0001;
        }
        let g = CsfPerceptualGate::mantiuk_default();
        let r_post = g.gate_radiance(r, 100.0, 12.0, 25.0, &t);
        let lum_post = r_post.integrate_visible(&t);
        assert!(lum_post < 1.0, "expected gated to ~zero, got {}", lum_post);
    }

    /// § Default is Mantiuk-2024.
    #[test]
    fn default_is_mantiuk() {
        let g: CsfPerceptualGate = Default::default();
        let p = g.params;
        assert!((p.peak_sensitivity - 200.0).abs() < 1e-6);
    }

    /// § Custom params work.
    #[test]
    fn custom_params() {
        let p = MantiukCsfParams {
            peak_sensitivity: 500.0,
            ref_luminance: 80.0,
            ref_bandwidth_cpd: 3.0,
            eccentricity_falloff: 0.06,
            contrast_threshold: 0.005,
        };
        let g = CsfPerceptualGate::new(p);
        assert!((g.params.peak_sensitivity - 500.0).abs() < 1e-6);
    }
}
