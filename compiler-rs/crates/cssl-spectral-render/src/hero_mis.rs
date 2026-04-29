//! § HeroWavelengthMIS — Manuka-style hero + accompanying-N MIS sampler
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per `07_AES/03 § III` :
//!     "Hero-wavelength method :
//!       @ each ray ⊗ sample one-hero-wavelength-from-uniform-distribution
//!       @ accompanying samples ⊗ at-quintic-pattern around-hero
//!       @ result-accumulation ⊗ maintains-PDF-correctness"
//!
//!   This module owns the hero-wavelength selection + accompaniment-sample
//!   placement + MIS-weight computation. The end product is a [`HeroSample`]
//!   value ready for the BRDF evaluator's `(view, light, λ_hero)` call site,
//!   plus a [`MisWeights`] struct for downstream PDF-correct accumulation.
//!
//! § PDF-CORRECTNESS DISCIPLINE
//!   The hero is uniform over the visible 380-780 nm range. Accompanying
//!   samples are placed at quintic offsets (Manuka spaces them at deltas of
//!   `[-Δ, -Δ/2, +Δ/2, +Δ]` for the 4-sample case ; we generalize to N).
//!   The MIS weight for each sample is `1 / N+1` (balance heuristic) before
//!   any per-sample BRDF-importance reweighting.
//!
//!   Cost expectation per `07_AES/03 § III` : "spectral-rendering-cost ≈
//!   1.5× RGB-cost (not 8×)". The 1.5× factor is the `(N+1)` accompaniment
//!   overhead amortized against the BRDF-eval critical path.

use crate::band::{BandTable, BAND_VISIBLE_END, BAND_VISIBLE_START};
use crate::radiance::{HeroAccompaniment, SpectralRadiance, ACCOMPANIMENT_MAX};

/// § A sampled hero-wavelength + an offset table of accompanying samples.
#[derive(Debug, Clone, Copy)]
pub struct HeroSample {
    /// § The hero wavelength in nm. Uniformly sampled over the visible range.
    pub hero_wavelength_nm: f32,
    /// § The PDF for picking this hero (uniform over visible range).
    pub hero_pdf: f32,
    /// § The number of accompaniment samples placed (1..=ACCOMPANIMENT_MAX).
    pub accompaniment_count: u8,
    /// § Wavelength of each accompaniment sample (only first
    ///   `accompaniment_count` entries are valid).
    pub accompaniment_wavelengths_nm: [f32; ACCOMPANIMENT_MAX],
    /// § PDF for each accompaniment sample.
    pub accompaniment_pdfs: [f32; ACCOMPANIMENT_MAX],
}

impl HeroSample {
    /// § Construct an empty sample (used as a starting point in tests).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            hero_wavelength_nm: 550.0,
            hero_pdf: 0.0,
            accompaniment_count: 0,
            accompaniment_wavelengths_nm: [0.0; ACCOMPANIMENT_MAX],
            accompaniment_pdfs: [0.0; ACCOMPANIMENT_MAX],
        }
    }
}

/// § The MIS weights for a hero-wavelength sample. The total weight across
///   all (1 + accompaniment_count) samples equals 1.0 by construction.
#[derive(Debug, Clone, Copy)]
pub struct MisWeights {
    /// § Hero-sample weight.
    pub hero_weight: f32,
    /// § Per-accompaniment weights.
    pub accompaniment_weights: [f32; ACCOMPANIMENT_MAX],
    /// § The number of valid accompaniment-weight entries.
    pub accompaniment_count: u8,
}

impl MisWeights {
    /// § Balance-heuristic weights : every sample (hero + accompaniment)
    ///   gets weight `1/(1 + N)`. This is the simplest valid MIS choice ;
    ///   future slices may swap to power-heuristic with `β = 2`.
    #[must_use]
    pub fn balance(accompaniment_count: u8) -> Self {
        let n = (1 + accompaniment_count as usize) as f32;
        let w = 1.0_f32 / n;
        Self {
            hero_weight: w,
            accompaniment_weights: [w; ACCOMPANIMENT_MAX],
            accompaniment_count,
        }
    }

    /// § Total weight summed across all samples — should equal 1.0.
    #[must_use]
    pub fn total(&self) -> f32 {
        let mut s = self.hero_weight;
        for i in 0..self.accompaniment_count as usize {
            s += self.accompaniment_weights[i];
        }
        s
    }
}

/// § The Manuka-style hero-wavelength MIS sampler. Stateless ; takes a
///   `seed` (typically the per-fragment LCG) + the `BandTable`.
#[derive(Debug, Clone, Copy)]
pub struct HeroWavelengthMIS {
    /// § The number of accompaniment samples to place around the hero.
    ///   Per `07_AES/03 § II` : "(4-8 typical)".
    pub n_accompaniment: u8,
    /// § The accompaniment offset spacing (delta) in nm.
    pub spacing_nm: f32,
}

impl HeroWavelengthMIS {
    /// § Default Manuka 4-accompaniment configuration : ±10 nm and ±20 nm
    ///   offsets from the hero.
    #[must_use]
    pub fn manuka_default() -> Self {
        Self {
            n_accompaniment: 4,
            spacing_nm: 10.0,
        }
    }

    /// § Construct a custom configuration. `n_accompaniment` is clamped to
    ///   `[0, ACCOMPANIMENT_MAX]`.
    #[must_use]
    pub fn new(n_accompaniment: u8, spacing_nm: f32) -> Self {
        let n = (n_accompaniment as usize).min(ACCOMPANIMENT_MAX) as u8;
        Self {
            n_accompaniment: n,
            spacing_nm,
        }
    }

    /// § Sample a hero wavelength using a uniform-fraction `xi ∈ [0, 1)`.
    ///   The hero is placed in the visible range ; accompaniment samples
    ///   are clamped to stay inside the visible band.
    #[must_use]
    pub fn sample(&self, xi: f32, table: &BandTable) -> HeroSample {
        // § The hero wavelength = uniform draw on the visible range.
        let lo = table.band(BAND_VISIBLE_START).lo_nm();
        let hi = table.band(BAND_VISIBLE_END - 1).hi_nm();
        let xi_c = xi.max(0.0).min(0.999_999);
        let hero = lo + xi_c * (hi - lo);
        let hero_pdf = 1.0_f32 / (hi - lo);

        // § Accompaniment samples placed at stratified offsets around the
        //   hero. We use the symmetric-quintic pattern : for n=4 :
        //     [-2Δ, -Δ, +Δ, +2Δ]
        //   For n=8 we extend to ±3Δ + ±4Δ.
        let mut wavelengths = [0.0_f32; ACCOMPANIMENT_MAX];
        let mut pdfs = [0.0_f32; ACCOMPANIMENT_MAX];

        let n = self.n_accompaniment as usize;
        for k in 0..n {
            // Offset index : 0..n maps to a quintic-symmetric offset sequence
            // [-(n/2 + n%2 - k.. ), …]. Practically : we place samples
            // linearly spaced around the hero with sign alternating then
            // increasing magnitude.
            let off = self.symmetric_offset(k);
            let lambda = (hero + off * self.spacing_nm).max(lo).min(hi - 0.001);
            wavelengths[k] = lambda;
            // § Each accompaniment shares the same PDF as the hero (since
            //   we draw with the same uniform proposal).
            pdfs[k] = hero_pdf;
        }

        HeroSample {
            hero_wavelength_nm: hero,
            hero_pdf,
            accompaniment_count: n as u8,
            accompaniment_wavelengths_nm: wavelengths,
            accompaniment_pdfs: pdfs,
        }
    }

    /// § The symmetric-quintic offset for the k-th accompaniment sample.
    ///   For n=4, k=0..3 produces [-2, -1, +1, +2] — skipping zero (which
    ///   IS the hero).
    fn symmetric_offset(&self, k: usize) -> f32 {
        // For k = 0..n-1 : alternate sign starting with negative, magnitude
        // ceil((k+2) / 2). This produces : k=0 → -1, k=1 → +1, k=2 → -2, ...
        // We skip the zero offset since that's the hero.
        let pair = (k / 2 + 1) as f32;
        if k % 2 == 0 {
            -pair
        } else {
            pair
        }
    }

    /// § Combine BRDF-evaluations at the hero + accompaniment back into a
    ///   single radiance value with PDF-correct MIS weighting. The input
    ///   `brdf_evals` has length `1 + accompaniment_count` ; index 0 is the
    ///   hero, indices 1.. are the accompaniment. The result has hero-set
    ///   wavelength + accompaniment populated with MIS-weighted intensities.
    #[must_use]
    pub fn combine(
        &self,
        sample: &HeroSample,
        brdf_evals: &[f32],
        weights: MisWeights,
    ) -> SpectralRadiance {
        let n = (sample.accompaniment_count as usize)
            .min(ACCOMPANIMENT_MAX)
            .min(brdf_evals.len().saturating_sub(1));
        let hero_intensity = if brdf_evals.is_empty() {
            0.0
        } else {
            brdf_evals[0] * weights.hero_weight
        };
        let mut radiance = SpectralRadiance::from_hero(sample.hero_wavelength_nm, hero_intensity);
        for k in 0..n {
            let lambda = sample.accompaniment_wavelengths_nm[k];
            let intensity = brdf_evals[1 + k] * weights.accompaniment_weights[k];
            radiance.push_accompaniment(HeroAccompaniment::new(lambda, intensity));
        }
        radiance
    }
}

impl Default for HeroWavelengthMIS {
    fn default() -> Self {
        Self::manuka_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § Default sampler is Manuka-default (n=4).
    #[test]
    fn default_manuka() {
        let s = HeroWavelengthMIS::default();
        assert_eq!(s.n_accompaniment, 4);
    }

    /// § new() clamps n_accompaniment to ACCOMPANIMENT_MAX.
    #[test]
    fn new_clamps_n() {
        let s = HeroWavelengthMIS::new(20, 5.0);
        assert_eq!(s.n_accompaniment as usize, ACCOMPANIMENT_MAX);
    }

    /// § sample() puts hero in the visible range.
    #[test]
    fn hero_in_visible_range() {
        let t = BandTable::d65();
        let s = HeroWavelengthMIS::manuka_default();
        for xi in [0.0_f32, 0.25, 0.5, 0.75, 0.99] {
            let sm = s.sample(xi, &t);
            let lo = t.band(BAND_VISIBLE_START).lo_nm();
            let hi = t.band(BAND_VISIBLE_END - 1).hi_nm();
            assert!(sm.hero_wavelength_nm >= lo);
            assert!(sm.hero_wavelength_nm <= hi);
        }
    }

    /// § Hero PDF is positive + finite.
    #[test]
    fn hero_pdf_positive() {
        let t = BandTable::d65();
        let s = HeroWavelengthMIS::manuka_default().sample(0.5, &t);
        assert!(s.hero_pdf > 0.0);
        assert!(s.hero_pdf.is_finite());
    }

    /// § Accompaniment count matches request.
    #[test]
    fn accompaniment_count_matches() {
        let t = BandTable::d65();
        let s = HeroWavelengthMIS::new(6, 8.0).sample(0.5, &t);
        assert_eq!(s.accompaniment_count, 6);
    }

    /// § Accompaniment wavelengths are clamped to visible range.
    #[test]
    fn accompaniment_clamped_to_visible() {
        let t = BandTable::d65();
        let s = HeroWavelengthMIS::new(4, 50.0).sample(0.0, &t);
        let lo = t.band(BAND_VISIBLE_START).lo_nm();
        let hi = t.band(BAND_VISIBLE_END - 1).hi_nm();
        for k in 0..s.accompaniment_count as usize {
            let w = s.accompaniment_wavelengths_nm[k];
            assert!(w >= lo);
            assert!(w <= hi);
        }
    }

    /// § symmetric_offset produces alternating signs.
    #[test]
    fn symmetric_offset_alternates() {
        let s = HeroWavelengthMIS::manuka_default();
        let o0 = s.symmetric_offset(0);
        let o1 = s.symmetric_offset(1);
        assert!(o0 < 0.0);
        assert!(o1 > 0.0);
    }

    /// § Symmetric offsets grow in magnitude.
    #[test]
    fn symmetric_offset_grows() {
        let s = HeroWavelengthMIS::manuka_default();
        let o2 = s.symmetric_offset(2);
        let o0 = s.symmetric_offset(0);
        assert!(o2.abs() > o0.abs() - 1e-6);
    }

    /// § balance MIS weights sum to 1.0 across hero + accompaniment.
    #[test]
    fn balance_weights_sum_one() {
        let w = MisWeights::balance(4);
        assert!((w.total() - 1.0).abs() < 1e-6);
    }

    /// § balance with 0 accompaniment = hero gets 1.0.
    #[test]
    fn balance_no_accompaniment() {
        let w = MisWeights::balance(0);
        assert!((w.hero_weight - 1.0).abs() < 1e-6);
    }

    /// § combine produces SpectralRadiance with hero + N accompaniment.
    #[test]
    fn combine_produces_radiance() {
        let t = BandTable::d65();
        let s = HeroWavelengthMIS::new(2, 10.0);
        let sample = s.sample(0.5, &t);
        let evals = [0.4_f32, 0.3, 0.2];
        let w = MisWeights::balance(2);
        let r = s.combine(&sample, &evals, w);
        // Hero intensity is brdf_evals[0] * weight = 0.4 * 1/3
        assert!((r.hero_intensity - 0.4 / 3.0).abs() < 1e-6);
        assert_eq!(r.accompaniment_count(), 2);
    }

    /// § combine handles empty brdf_evals gracefully.
    #[test]
    fn combine_empty_evals_zero_hero() {
        let t = BandTable::d65();
        let s = HeroWavelengthMIS::manuka_default();
        let sample = s.sample(0.5, &t);
        let evals: [f32; 0] = [];
        let w = MisWeights::balance(0);
        let r = s.combine(&sample, &evals, w);
        assert_eq!(r.hero_intensity, 0.0);
    }

    /// § Hero PDF * visible-range = 1.0 (uniform PDF normalization).
    #[test]
    fn hero_pdf_normalizes_to_visible() {
        let t = BandTable::d65();
        let sample = HeroWavelengthMIS::manuka_default().sample(0.5, &t);
        let lo = t.band(BAND_VISIBLE_START).lo_nm();
        let hi = t.band(BAND_VISIBLE_END - 1).hi_nm();
        let prod = sample.hero_pdf * (hi - lo);
        assert!((prod - 1.0).abs() < 1e-6);
    }

    /// § Different xi values produce different heros.
    #[test]
    fn different_xi_different_hero() {
        let t = BandTable::d65();
        let s = HeroWavelengthMIS::manuka_default();
        let a = s.sample(0.1, &t).hero_wavelength_nm;
        let b = s.sample(0.9, &t).hero_wavelength_nm;
        assert!((a - b).abs() > 50.0);
    }

    /// § Each accompaniment PDF equals the hero PDF.
    #[test]
    fn accompaniment_pdf_equals_hero() {
        let t = BandTable::d65();
        let s = HeroWavelengthMIS::manuka_default();
        let sm = s.sample(0.5, &t);
        for k in 0..sm.accompaniment_count as usize {
            assert!((sm.accompaniment_pdfs[k] - sm.hero_pdf).abs() < 1e-9);
        }
    }
}
