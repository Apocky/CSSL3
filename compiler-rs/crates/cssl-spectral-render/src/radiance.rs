//! § SpectralRadiance — hero-wavelength + accompaniment storage
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per `07_AES/03 § II` the canonical radiance carrier is hero-wavelength
//!   plus a small "accompanying-N" set of co-sampled wavelengths (4-8
//!   typical). This module surfaces that storage, plus a 16-band full-
//!   spectrum view (the KAN-BRDF output dim is 16 ; we expand the
//!   hero/accompaniment into the 16-band buffer for downstream cascade /
//!   tonemap consumers).
//!
//!   Two views of the same radiance signal :
//!     - **Hero + accompaniment** : compact, used during the per-bounce
//!       BRDF-evaluation hot-path. PDF-correct under hero-wavelength MIS.
//!     - **Full 16-band** : dense, used during per-band multiplication,
//!       cascade integration, and the final tristimulus convert.
//!
//!   Conversion between the two is `expand_to_bands()` /
//!   `accumulate_from_bands()`.

use crate::band::{BandTable, BAND_COUNT, BAND_VISIBLE_END, BAND_VISIBLE_START};

/// § Maximum number of accompanying spectral samples around the hero
///   wavelength. Per `07_AES/03 § II` the spec literal is "(wavelength,
///   intensity), 7" — so the max is 7. We keep this as a const for
///   call-sites that allocate stack-buffers.
pub const ACCOMPANIMENT_MAX: usize = 7;

/// § A single (wavelength, intensity) pair for the accompanying-N set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeroAccompaniment {
    /// § Wavelength in nm, in the visible-band range.
    pub wavelength_nm: f32,
    /// § Intensity at this wavelength (linear, post-BRDF, pre-tonemap).
    pub intensity: f32,
}

impl HeroAccompaniment {
    /// § Construct an accompaniment sample.
    #[must_use]
    pub const fn new(wavelength_nm: f32, intensity: f32) -> Self {
        Self {
            wavelength_nm,
            intensity,
        }
    }

    /// § Dim by a scalar factor — used by the MIS combine step.
    #[must_use]
    pub fn scale(self, k: f32) -> Self {
        Self {
            wavelength_nm: self.wavelength_nm,
            intensity: self.intensity * k,
        }
    }
}

/// § A spectral-radiance carrier with hero + accompaniment view, plus a
///   16-band view. The two views are kept in sync by the constructors.
#[derive(Debug, Clone)]
pub struct SpectralRadiance {
    /// § The hero wavelength in nm. Must lie in the visible band range
    ///   (380-780 nm) to be processed by the tonemap.
    pub hero_wavelength_nm: f32,
    /// § Hero intensity (linear).
    pub hero_intensity: f32,
    /// § Number of valid accompaniment samples (0..=ACCOMPANIMENT_MAX).
    pub accompaniment_count: u8,
    /// § Accompaniment samples. Only the first `accompaniment_count` are
    ///   meaningful.
    pub accompaniment: [HeroAccompaniment; ACCOMPANIMENT_MAX],
    /// § Full 16-band view. Mostly populated by `expand_to_bands` after
    ///   BRDF eval ; the hero+accompaniment view is the primary store.
    pub bands: [f32; BAND_COUNT],
}

impl SpectralRadiance {
    /// § Construct a black radiance (zero everywhere). Used as a starting
    ///   point for accumulation in the GI cascade.
    #[must_use]
    pub fn black() -> Self {
        Self {
            hero_wavelength_nm: 550.0,
            hero_intensity: 0.0,
            accompaniment_count: 0,
            accompaniment: [HeroAccompaniment::new(0.0, 0.0); ACCOMPANIMENT_MAX],
            bands: [0.0; BAND_COUNT],
        }
    }

    /// § Construct from a hero wavelength + intensity. Accompaniment is
    ///   empty until the hero-MIS sampler populates it.
    #[must_use]
    pub fn from_hero(hero_wavelength_nm: f32, hero_intensity: f32) -> Self {
        let mut s = Self::black();
        s.hero_wavelength_nm = hero_wavelength_nm;
        s.hero_intensity = hero_intensity;
        s
    }

    /// § Construct directly from a full 16-band buffer (e.g. the KAN-BRDF
    ///   output). The hero is set to the band with maximum intensity.
    #[must_use]
    pub fn from_bands(bands: [f32; BAND_COUNT], table: &BandTable) -> Self {
        // § Find the visible band with the maximum intensity ; that
        //   wavelength becomes the hero. Band indices outside the visible
        //   range are not eligible for hero status (the hero must be in
        //   the visible spectrum per `07_AES/03 § II` invariant).
        let mut max_i = BAND_VISIBLE_START;
        let mut max_v = bands[BAND_VISIBLE_START];
        for i in BAND_VISIBLE_START..BAND_VISIBLE_END {
            if bands[i] > max_v {
                max_v = bands[i];
                max_i = i;
            }
        }
        let hero_wavelength_nm = table.band(max_i).center_nm;
        Self {
            hero_wavelength_nm,
            hero_intensity: max_v,
            accompaniment_count: 0,
            accompaniment: [HeroAccompaniment::new(0.0, 0.0); ACCOMPANIMENT_MAX],
            bands,
        }
    }

    /// § Push an accompaniment sample. Returns `false` if the buffer is
    ///   full (no-op).
    pub fn push_accompaniment(&mut self, sample: HeroAccompaniment) -> bool {
        let n = self.accompaniment_count as usize;
        if n >= ACCOMPANIMENT_MAX {
            return false;
        }
        self.accompaniment[n] = sample;
        self.accompaniment_count += 1;
        true
    }

    /// § Number of valid accompaniment samples.
    #[must_use]
    pub fn accompaniment_count(&self) -> usize {
        self.accompaniment_count as usize
    }

    /// § Iterator over the valid accompaniment samples.
    pub fn accompaniment_slice(&self) -> &[HeroAccompaniment] {
        &self.accompaniment[..self.accompaniment_count()]
    }

    /// § Sample the radiance at an arbitrary wavelength via hero +
    ///   accompaniment + nearest-band interpolation. Used by tonemap +
    ///   tristimulus integrators.
    #[must_use]
    pub fn at_wavelength(&self, lambda_nm: f32, table: &BandTable) -> f32 {
        // § If we have an exact hero hit, return hero intensity.
        if (self.hero_wavelength_nm - lambda_nm).abs() < 1e-3 {
            return self.hero_intensity;
        }
        // § Otherwise scan accompaniment for a match.
        for s in self.accompaniment_slice() {
            if (s.wavelength_nm - lambda_nm).abs() < 1e-3 {
                return s.intensity;
            }
        }
        // § Fall back to the band-bucketed value.
        if let Some(idx) = table.band_index_at_nm(lambda_nm) {
            return self.bands[idx];
        }
        0.0
    }

    /// § Expand the hero + accompaniment into the dense 16-band buffer.
    ///   Existing band content is overwritten ; bands not covered by the
    ///   hero or any accompaniment are zeroed. Used after MIS combine to
    ///   produce a dense buffer for cascade integration.
    pub fn expand_to_bands(&mut self, table: &BandTable) {
        self.bands = [0.0; BAND_COUNT];
        if let Some(idx) = table.band_index_at_nm(self.hero_wavelength_nm) {
            self.bands[idx] = self.hero_intensity;
        }
        for s in &self.accompaniment[..self.accompaniment_count()] {
            if let Some(idx) = table.band_index_at_nm(s.wavelength_nm) {
                // § Multiple samples may land in the same band ; sum them
                //   to preserve total energy.
                self.bands[idx] += s.intensity;
            }
        }
    }

    /// § Reduce the dense 16-band buffer back to a hero + accompaniment
    ///   form, picking the largest visible band as the new hero and the
    ///   next `n_accomp` largest as accompaniment. Used after cascade
    ///   integration to recover the compact form for the next bounce.
    pub fn accumulate_from_bands(&mut self, n_accomp: usize, table: &BandTable) {
        let n_accomp = n_accomp.min(ACCOMPANIMENT_MAX);
        // § Collect (band-idx, value) pairs for the visible bands, sort by
        //   value descending, take top (1 + n_accomp).
        let mut visible: [(usize, f32); BAND_COUNT] = [(0, 0.0); BAND_COUNT];
        let mut count = 0usize;
        for i in BAND_VISIBLE_START..BAND_VISIBLE_END {
            visible[count] = (i, self.bands[i]);
            count += 1;
        }
        // § Selection-sort-style top-K (count is at most 10 — well under
        //   any threshold where insertion-sort is suboptimal).
        for k in 0..(n_accomp + 1).min(count) {
            let mut best = k;
            for j in (k + 1)..count {
                if visible[j].1 > visible[best].1 {
                    best = j;
                }
            }
            visible.swap(k, best);
        }
        // § visible[0] = hero ; visible[1..=n_accomp] = accompaniment.
        let (hero_idx, hero_val) = visible[0];
        self.hero_wavelength_nm = table.band(hero_idx).center_nm;
        self.hero_intensity = hero_val;
        self.accompaniment_count = 0;
        for i in 1..=n_accomp.min(count.saturating_sub(1)) {
            let (idx, v) = visible[i];
            let w = table.band(idx).center_nm;
            self.push_accompaniment(HeroAccompaniment::new(w, v));
        }
    }

    /// § Multiply per-band by another spectral radiance (Hadamard product).
    ///   This is the per-bounce spectral multiplication per `07_AES/03 §
    ///   II` ; never a vec3 mul.
    #[must_use]
    pub fn multiply_per_band(&self, rhs: &Self) -> Self {
        let mut bands = [0.0_f32; BAND_COUNT];
        for i in 0..BAND_COUNT {
            bands[i] = self.bands[i] * rhs.bands[i];
        }
        Self {
            hero_wavelength_nm: self.hero_wavelength_nm,
            hero_intensity: self.hero_intensity * rhs.hero_intensity,
            accompaniment_count: 0,
            accompaniment: [HeroAccompaniment::new(0.0, 0.0); ACCOMPANIMENT_MAX],
            bands,
        }
    }

    /// § Scale all bands + hero by `k`.
    #[must_use]
    pub fn scale(&self, k: f32) -> Self {
        let mut bands = [0.0_f32; BAND_COUNT];
        for i in 0..BAND_COUNT {
            bands[i] = self.bands[i] * k;
        }
        let mut accomp = self.accompaniment;
        for s in accomp.iter_mut().take(self.accompaniment_count()) {
            s.intensity *= k;
        }
        Self {
            hero_wavelength_nm: self.hero_wavelength_nm,
            hero_intensity: self.hero_intensity * k,
            accompaniment_count: self.accompaniment_count,
            accompaniment: accomp,
            bands,
        }
    }

    /// § Total integrated radiance across all visible bands. Used in the
    ///   tristimulus integrator + the per-fragment perceptual gate.
    #[must_use]
    pub fn integrate_visible(&self, table: &BandTable) -> f32 {
        let mut s = 0.0_f32;
        for i in BAND_VISIBLE_START..BAND_VISIBLE_END {
            s += self.bands[i] * table.band(i).d65_weight;
        }
        s
    }

    /// § True iff every band is non-negative + no NaN / Inf. Used as a
    ///   debug-assert helper at pipeline boundaries.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        if self.hero_intensity.is_nan() || self.hero_intensity.is_infinite() {
            return false;
        }
        if self.hero_intensity < 0.0 {
            return false;
        }
        for v in &self.bands {
            if v.is_nan() || v.is_infinite() || *v < 0.0 {
                return false;
            }
        }
        for s in self.accompaniment_slice() {
            if s.intensity.is_nan() || s.intensity.is_infinite() || s.intensity < 0.0 {
                return false;
            }
        }
        true
    }
}

impl Default for SpectralRadiance {
    fn default() -> Self {
        Self::black()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::band::BAND_VISIBLE_START;

    /// § Black radiance has all bands zero.
    #[test]
    fn black_is_zero() {
        let r = SpectralRadiance::black();
        assert_eq!(r.hero_intensity, 0.0);
        for v in &r.bands {
            assert_eq!(*v, 0.0);
        }
        assert_eq!(r.accompaniment_count(), 0);
    }

    /// § from_hero sets hero fields.
    #[test]
    fn from_hero_sets_hero() {
        let r = SpectralRadiance::from_hero(550.0, 1.5);
        assert_eq!(r.hero_wavelength_nm, 550.0);
        assert_eq!(r.hero_intensity, 1.5);
    }

    /// § push_accompaniment adds samples up to ACCOMPANIMENT_MAX.
    #[test]
    fn push_accompaniment_caps_at_max() {
        let mut r = SpectralRadiance::black();
        for i in 0..ACCOMPANIMENT_MAX {
            assert!(r.push_accompaniment(HeroAccompaniment::new(
                400.0 + i as f32 * 30.0,
                0.1 * (i + 1) as f32
            )));
        }
        assert_eq!(r.accompaniment_count(), ACCOMPANIMENT_MAX);
        let extra = r.push_accompaniment(HeroAccompaniment::new(700.0, 1.0));
        assert!(!extra);
        assert_eq!(r.accompaniment_count(), ACCOMPANIMENT_MAX);
    }

    /// § from_bands picks max-visible band as hero.
    #[test]
    fn from_bands_picks_max_hero() {
        let t = BandTable::d65();
        let mut bands = [0.0_f32; BAND_COUNT];
        bands[BAND_VISIBLE_START + 3] = 2.0; // brightest
        bands[BAND_VISIBLE_START + 5] = 1.0;
        let r = SpectralRadiance::from_bands(bands, &t);
        assert_eq!(r.hero_intensity, 2.0);
        let expected_lambda = t.band(BAND_VISIBLE_START + 3).center_nm;
        assert!((r.hero_wavelength_nm - expected_lambda).abs() < 1e-3);
    }

    /// § expand_to_bands populates the band buffer.
    #[test]
    fn expand_to_bands_populates() {
        let t = BandTable::d65();
        let mut r = SpectralRadiance::from_hero(560.0, 1.0);
        r.push_accompaniment(HeroAccompaniment::new(440.0, 0.5));
        r.expand_to_bands(&t);
        let hero_idx = t.band_index_at_nm(560.0).unwrap();
        let acc_idx = t.band_index_at_nm(440.0).unwrap();
        assert!(r.bands[hero_idx] > 0.99);
        assert!(r.bands[acc_idx] > 0.49);
    }

    /// § accumulate_from_bands picks correct hero + ranks accompaniment.
    #[test]
    fn accumulate_from_bands_picks_top() {
        let t = BandTable::d65();
        let mut bands = [0.0_f32; BAND_COUNT];
        for (i, v) in bands
            .iter_mut()
            .enumerate()
            .take(BAND_VISIBLE_END)
            .skip(BAND_VISIBLE_START)
        {
            *v = (i as f32) * 0.1;
        }
        let mut r = SpectralRadiance::black();
        r.bands = bands;
        r.accumulate_from_bands(3, &t);
        // The maximum visible band gets the highest value (BAND_VISIBLE_END - 1).
        let max_idx = BAND_VISIBLE_END - 1;
        let expected_hero = t.band(max_idx).center_nm;
        assert!((r.hero_wavelength_nm - expected_hero).abs() < 1e-3);
        // Accompaniment count = 3.
        assert_eq!(r.accompaniment_count(), 3);
        // First accompaniment is the second-largest band (max_idx - 1).
        let exp_first = t.band(max_idx - 1).center_nm;
        assert!((r.accompaniment[0].wavelength_nm - exp_first).abs() < 1e-3);
    }

    /// § multiply_per_band is element-wise.
    #[test]
    fn multiply_per_band_is_elementwise() {
        let t = BandTable::d65();
        let mut a = SpectralRadiance::black();
        let mut b = SpectralRadiance::black();
        a.bands[BAND_VISIBLE_START] = 0.5;
        b.bands[BAND_VISIBLE_START] = 0.4;
        a.hero_intensity = 0.5;
        b.hero_intensity = 0.4;
        let p = a.multiply_per_band(&b);
        assert!((p.bands[BAND_VISIBLE_START] - 0.2).abs() < 1e-6);
        assert!((p.hero_intensity - 0.2).abs() < 1e-6);
        let _ = t; // silence unused
    }

    /// § scale scales bands + hero + accompaniment uniformly.
    #[test]
    fn scale_uniform() {
        let mut r = SpectralRadiance::from_hero(550.0, 1.0);
        r.bands[BAND_VISIBLE_START] = 2.0;
        r.push_accompaniment(HeroAccompaniment::new(500.0, 0.3));
        let s = r.scale(0.5);
        assert!((s.hero_intensity - 0.5).abs() < 1e-6);
        assert!((s.bands[BAND_VISIBLE_START] - 1.0).abs() < 1e-6);
        assert!((s.accompaniment[0].intensity - 0.15).abs() < 1e-6);
    }

    /// § integrate_visible weighs by D65.
    #[test]
    fn integrate_visible_weights_by_d65() {
        let t = BandTable::d65();
        let mut r = SpectralRadiance::black();
        for i in BAND_VISIBLE_START..BAND_VISIBLE_END {
            r.bands[i] = 1.0;
        }
        let s = r.integrate_visible(&t);
        // § With every band at 1.0 the integrated value = sum of D65
        //   weights = ~1.0 (within rounding).
        assert!((s - 1.0).abs() < 1e-3, "integrate = {s}");
    }

    /// § is_well_formed catches negatives + NaN.
    #[test]
    fn is_well_formed_catches_negatives() {
        let mut r = SpectralRadiance::black();
        assert!(r.is_well_formed());
        r.bands[0] = -0.1;
        assert!(!r.is_well_formed());
        r.bands[0] = f32::NAN;
        assert!(!r.is_well_formed());
        r.bands[0] = f32::INFINITY;
        assert!(!r.is_well_formed());
    }

    /// § at_wavelength returns hero intensity at hero wavelength.
    #[test]
    fn at_wavelength_returns_hero() {
        let t = BandTable::d65();
        let r = SpectralRadiance::from_hero(560.0, 1.5);
        assert!((r.at_wavelength(560.0, &t) - 1.5).abs() < 1e-6);
    }

    /// § at_wavelength falls back to band content.
    #[test]
    fn at_wavelength_falls_back_to_bands() {
        let t = BandTable::d65();
        let mut r = SpectralRadiance::from_hero(560.0, 1.5);
        r.bands[BAND_VISIBLE_START + 1] = 0.7; // 440-nm band
        assert!((r.at_wavelength(440.0, &t) - 0.7).abs() < 1e-6);
    }

    /// § HeroAccompaniment::scale scales intensity but not wavelength.
    #[test]
    fn accompaniment_scale_preserves_wavelength() {
        let s = HeroAccompaniment::new(550.0, 1.0);
        let s2 = s.scale(0.5);
        assert_eq!(s2.wavelength_nm, 550.0);
        assert!((s2.intensity - 0.5).abs() < 1e-6);
    }
}
