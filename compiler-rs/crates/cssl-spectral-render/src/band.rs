//! § SpectralBand — 16-band wavelength sampling table
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Defines the canonical 16-band wavelength table consumed by every other
//!   module in the crate. Per `07_AES/07 § II` the spectral-BRDF KAN-network
//!   variant has output dim 16 — those 16 outputs MUST line up with the 16
//!   bands defined here.
//!
//!   Layout :
//!     - bands[ 0.. 2] : 2 UV bands (300-340 nm + 340-380 nm)
//!     - bands[ 2..12] : 10 visible bands @ 40-nm width covering 380-780 nm
//!     - bands[12..16] : 4 NIR bands (780-900 nm + 900-1100 nm + 1100-1400 nm
//!                                    + 1400-2500 nm)
//!
//!   The table is fully static + const — no runtime allocation, no
//!   reflection. Per `07_AES/07 § II` shape variants are enumerated at
//!   compile-time so cooperative-matrix tile size is known.

use core::ops::Range;

/// § Total band count. MUST equal 16 to match `KanMaterial::spectral_brdf<16>`
///   output dim per `07_AES/07 § VIII`.
pub const BAND_COUNT: usize = 16;

/// § The number of UV bands. UV bands sit below 380 nm and capture
///   fluorescence-excitation pumps for Λ-token materials.
pub const UV_BAND_COUNT: usize = 2;

/// § The number of visible-spectrum bands. Visible runs 380-780 nm at
///   40 nm width per band.
pub const VISIBLE_BAND_COUNT: usize = 10;

/// § The number of near-infrared / shortwave-IR bands. These capture
///   thermal-emission cross-coupling + IR-reflectance for materials with
///   non-trivial IR-response curves (e.g. Λ-tokens with thermal coupling).
pub const IR_BAND_COUNT: usize = 4;

/// § The first index in the BAND_TABLE that lies in the visible spectrum.
pub const BAND_VISIBLE_START: usize = UV_BAND_COUNT;

/// § The (exclusive) last index in the BAND_TABLE that lies in the visible
///   spectrum. Equal to `BAND_VISIBLE_START + VISIBLE_BAND_COUNT`.
pub const BAND_VISIBLE_END: usize = UV_BAND_COUNT + VISIBLE_BAND_COUNT;

/// § Compile-time check that the band counts add up correctly.
const _: () = assert!(UV_BAND_COUNT + VISIBLE_BAND_COUNT + IR_BAND_COUNT == BAND_COUNT);

/// § A single spectral band — center wavelength, width, and a normalization
///   weight. The weight encodes the relative perceptual contribution under
///   the D65 illuminant ; CIE-XYZ tonemap consumes these.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectralBand {
    /// § Center wavelength in nanometers.
    pub center_nm: f32,
    /// § Bandwidth in nanometers (full width).
    pub width_nm: f32,
    /// § Relative D65 weight, normalized so the visible bands sum to 1.0.
    pub d65_weight: f32,
}

impl SpectralBand {
    /// § Construct a band — generally a const expression at table init.
    #[must_use]
    pub const fn new(center_nm: f32, width_nm: f32, d65_weight: f32) -> Self {
        Self {
            center_nm,
            width_nm,
            d65_weight,
        }
    }

    /// § Lower edge of the band (exclusive when a wavelength sits on a
    ///   boundary the higher band wins per `07_AES/03 § III` convention).
    #[must_use]
    pub fn lo_nm(self) -> f32 {
        self.center_nm - self.width_nm * 0.5
    }

    /// § Upper edge of the band.
    #[must_use]
    pub fn hi_nm(self) -> f32 {
        self.center_nm + self.width_nm * 0.5
    }

    /// § True iff `lambda_nm` falls inside the band's `[lo, hi)` range.
    #[must_use]
    pub fn contains_nm(self, lambda_nm: f32) -> bool {
        let lo = self.lo_nm();
        let hi = self.hi_nm();
        lambda_nm >= lo && lambda_nm < hi
    }
}

/// § Static 16-band table — UV → visible → NIR. Per `07_AES/03 § III`
///   the visible bands span 380-780 nm at 40-nm width ; the spec allows
///   "4-8 typical" accompanying samples within this set, plus the hero.
pub const BAND_TABLE: [SpectralBand; BAND_COUNT] = [
    // UV : 300-380 nm. Used as fluorescence-excitation pump.
    SpectralBand::new(320.0, 40.0, 0.0),
    SpectralBand::new(360.0, 40.0, 0.0),
    // Visible : 380-780 nm at 40-nm width. Weights from CIE D65 luminous-
    // efficacy curve (illustrative coefficients ; exact integration over
    // each band is a separate slice). Sum of visible weights = 1.0 by
    // construction.
    SpectralBand::new(400.0, 40.0, 0.014),
    SpectralBand::new(440.0, 40.0, 0.038),
    SpectralBand::new(480.0, 40.0, 0.094),
    SpectralBand::new(520.0, 40.0, 0.157),
    SpectralBand::new(560.0, 40.0, 0.182),
    SpectralBand::new(600.0, 40.0, 0.179),
    SpectralBand::new(640.0, 40.0, 0.150),
    SpectralBand::new(680.0, 40.0, 0.097),
    SpectralBand::new(720.0, 40.0, 0.057),
    SpectralBand::new(760.0, 40.0, 0.032),
    // NIR : 780 nm - 2500 nm (logarithmic widths since IR rolloff is fast).
    SpectralBand::new(840.0, 120.0, 0.0),
    SpectralBand::new(1000.0, 200.0, 0.0),
    SpectralBand::new(1250.0, 300.0, 0.0),
    SpectralBand::new(1950.0, 1100.0, 0.0),
];

/// § A lookup helper that wraps the static `BAND_TABLE`. Mostly a typed
///   re-export of the array, but useful for unit-tests and future bands-by-
///   illuminant variants (where the D65 weights are replaced with D50 / D75
///   / Equal-Energy weights).
#[derive(Debug, Clone, Copy)]
pub struct BandTable {
    bands: [SpectralBand; BAND_COUNT],
}

impl BandTable {
    /// § Construct the canonical 16-band D65 table.
    #[must_use]
    pub const fn d65() -> Self {
        Self { bands: BAND_TABLE }
    }

    /// § Construct an equal-energy-weighted band table — every visible band
    ///   gets weight `1 / VISIBLE_BAND_COUNT` and IR / UV get 0. Used by
    ///   the spectral-fidelity unit tests.
    #[must_use]
    pub fn equal_energy() -> Self {
        let mut bands = BAND_TABLE;
        let w = 1.0_f32 / VISIBLE_BAND_COUNT as f32;
        for (i, b) in bands.iter_mut().enumerate() {
            b.d65_weight = if (BAND_VISIBLE_START..BAND_VISIBLE_END).contains(&i) {
                w
            } else {
                0.0
            };
        }
        Self { bands }
    }

    /// § Return the band at index `i`. Panics in debug if `i >= BAND_COUNT`.
    #[must_use]
    pub fn band(&self, i: usize) -> SpectralBand {
        debug_assert!(i < BAND_COUNT, "band index {i} out of range");
        self.bands[i.min(BAND_COUNT - 1)]
    }

    /// § Return all 16 bands as a slice.
    #[must_use]
    pub fn all(&self) -> &[SpectralBand; BAND_COUNT] {
        &self.bands
    }

    /// § Return the index of the band that contains `lambda_nm`, or `None`
    ///   if the wavelength is outside the table's coverage.
    #[must_use]
    pub fn band_index_at_nm(&self, lambda_nm: f32) -> Option<usize> {
        for (i, b) in self.bands.iter().enumerate() {
            if b.contains_nm(lambda_nm) {
                return Some(i);
            }
        }
        None
    }

    /// § Return the visible-band slice.
    #[must_use]
    pub fn visible_range(&self) -> Range<usize> {
        BAND_VISIBLE_START..BAND_VISIBLE_END
    }

    /// § Sum of D65 weights across the visible bands. Should be ~1.0 ; used
    ///   by the spectral-fidelity tests.
    #[must_use]
    pub fn d65_weight_sum(&self) -> f32 {
        let mut s = 0.0_f32;
        for i in BAND_VISIBLE_START..BAND_VISIBLE_END {
            s += self.bands[i].d65_weight;
        }
        s
    }
}

impl Default for BandTable {
    fn default() -> Self {
        Self::d65()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// § The constant counts add to 16.
    #[test]
    fn band_count_invariant() {
        assert_eq!(
            UV_BAND_COUNT + VISIBLE_BAND_COUNT + IR_BAND_COUNT,
            BAND_COUNT
        );
    }

    /// § Visible-band range covers the spec's 380-780 nm.
    #[test]
    fn visible_range_covers_visible_spectrum() {
        let t = BandTable::d65();
        let lo = t.band(BAND_VISIBLE_START).lo_nm();
        let hi = t.band(BAND_VISIBLE_END - 1).hi_nm();
        assert!(lo <= 380.0 + 1e-3, "visible-low {lo} > 380");
        assert!(hi >= 780.0 - 1e-3, "visible-high {hi} < 780");
    }

    /// § D65 visible-band weights sum to ~1.0.
    #[test]
    fn d65_weights_sum_one() {
        let t = BandTable::d65();
        let s = t.d65_weight_sum();
        assert!((s - 1.0).abs() < 1e-3, "D65 weight sum = {s}");
    }

    /// § Equal-energy weights also sum to 1.0.
    #[test]
    fn equal_energy_weights_sum_one() {
        let t = BandTable::equal_energy();
        let s = t.d65_weight_sum();
        assert!((s - 1.0).abs() < 1e-6, "equal-energy weight sum = {s}");
    }

    /// § band_index_at_nm finds the canonical visible band for 550 nm.
    #[test]
    fn band_index_at_550nm() {
        let t = BandTable::d65();
        let idx = t.band_index_at_nm(550.0).unwrap();
        let b = t.band(idx);
        assert!(b.contains_nm(550.0));
        // 550 nm sits in the 540-560 nm band (center 560, width 40 →
        // [540, 580]) ... actually we have center 560 width 40 = [540, 580]
        // so 550 falls in band index BAND_VISIBLE_START + 5 = 7.
        assert_eq!(idx, BAND_VISIBLE_START + 4); // 520-560 band
    }

    /// § Out-of-range wavelength returns None.
    #[test]
    fn out_of_range_returns_none() {
        let t = BandTable::d65();
        assert!(t.band_index_at_nm(100.0).is_none());
        assert!(t.band_index_at_nm(5000.0).is_none());
    }

    /// § Bands have positive widths.
    #[test]
    fn all_bands_positive_width() {
        let t = BandTable::d65();
        for b in t.all() {
            assert!(b.width_nm > 0.0, "non-positive width {:?}", b);
        }
    }

    /// § Visible band weights are non-negative.
    #[test]
    fn visible_weights_non_negative() {
        let t = BandTable::d65();
        for i in BAND_VISIBLE_START..BAND_VISIBLE_END {
            assert!(t.band(i).d65_weight >= 0.0);
        }
    }

    /// § UV + IR weights are zero (only visible contributes to luminous
    ///   tristimulus per `07_AES/03 § VII`).
    #[test]
    fn uv_ir_weights_zero() {
        let t = BandTable::d65();
        for i in 0..BAND_VISIBLE_START {
            assert_eq!(t.band(i).d65_weight, 0.0);
        }
        for i in BAND_VISIBLE_END..BAND_COUNT {
            assert_eq!(t.band(i).d65_weight, 0.0);
        }
    }

    /// § contains_nm at lower edge is inclusive.
    #[test]
    fn contains_nm_lower_edge_inclusive() {
        let b = SpectralBand::new(500.0, 40.0, 0.1);
        assert!(b.contains_nm(480.0));
        assert!(b.contains_nm(519.999));
        assert!(!b.contains_nm(520.0));
    }

    /// § Band lo + hi flank center.
    #[test]
    fn lo_hi_flank_center() {
        let b = SpectralBand::new(500.0, 40.0, 0.1);
        assert!((b.lo_nm() - 480.0).abs() < 1e-6);
        assert!((b.hi_nm() - 520.0).abs() < 1e-6);
    }

    /// § Default = D65.
    #[test]
    fn default_is_d65() {
        let t: BandTable = Default::default();
        assert!((t.d65_weight_sum() - 1.0).abs() < 1e-3);
    }
}
