//! § `WaveField<C>` — per-band complex amplitude field over Morton-keyed cells.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § PURPOSE
//!   The Wave-Unity solver carries one complex amplitude `ψ_b ∈ ℂ` per
//!   cell per band. The container is :
//!
//!     `WaveField<C>` ≡ `[BTreeMap<MortonKey, C32>; C]`
//!
//!   where `C` is the const-generic band count (default 5). Cells are
//!   keyed by `cssl_substrate_omega_field::MortonKey` so the on-disk
//!   bytes line up with the canonical Ω-field substrate.
//!
//!   Stage-0 uses `BTreeMap` rather than the SparseMortonGrid hash for
//!   two reasons :
//!     1. Iteration order is deterministic (Morton-key sort) without
//!        relying on the omega-field's internal probe-sequence.
//!     2. The wave-solver is the canonical replay-determinism citizen
//!        — every iteration MUST be byte-equal across hosts. BTreeMap
//!        gives us that for free.
//!
//!   The trade-off is O(log n) per-cell access vs O(1) amortized in
//!   the hash. At the active-region size (≤ 1 M cells) the constant
//!   factor difference is < 2× ; the determinism guarantee is worth it.
//!
//! § SURFACE
//!   ```text
//!   pub struct WaveField<const C: usize> {
//!     bands: [BTreeMap<MortonKey, C32>; C],
//!     dx_m:    [f64; C],     // recommended cell size per band
//!     dt_s:    [f64; C],     // recommended substep per band
//!     class:   [BandClass; C],
//!   }
//!   ```
//!
//! § REPLAY-DETERMINISM
//!   Iter helpers ([`WaveField::cells_in_band`], [`WaveField::all_cells`])
//!   walk the BTreeMap in sorted-key order, so two replays produce the
//!   same per-cell update sequence. The unit tests assert this on
//!   randomly-inserted keys.

use std::collections::BTreeMap;

use crate::band::{Band, BandClass, BAND_COUNT_DEFAULT, DEFAULT_BANDS};
use crate::complex::C32;

use cssl_substrate_omega_field::MortonKey;

/// § A per-cell pack — the complex amplitude across all `C` bands. Used
///   for serialization + replay snapshots.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PsiCell<const C: usize> {
    /// § Per-band amplitudes. Indexed by `band.index()`.
    pub psi: [C32; C],
}

impl<const C: usize> PsiCell<C> {
    /// § Construct a zero cell.
    #[must_use]
    pub fn zero() -> Self {
        Self {
            psi: [C32::ZERO; C],
        }
    }

    /// § Total |ψ|² summed across bands.
    #[must_use]
    pub fn total_norm_sqr(&self) -> f64 {
        let mut sum = 0.0_f64;
        for b in 0..C {
            sum += self.psi[b].norm_sqr() as f64;
        }
        sum
    }
}

impl<const C: usize> Default for PsiCell<C> {
    fn default() -> Self {
        Self::zero()
    }
}

/// § The canonical Wave-Unity field — per-band complex amplitudes.
#[derive(Debug, Clone)]
pub struct WaveField<const C: usize> {
    /// § Per-band sparse cell maps. Stage-0 uses BTreeMap for sorted iteration.
    bands: [BTreeMap<MortonKey, C32>; C],
    /// § Per-band recommended cell sizes (m).
    dx_m: [f64; C],
    /// § Per-band recommended substep sizes (s).
    dt_s: [f64; C],
    /// § Per-band classification.
    class: [BandClass; C],
}

impl<const C: usize> WaveField<C> {
    /// § Construct an empty field. The metadata arrays must be supplied
    ///   by the caller. For the default 5-band config use
    ///   [`WaveField::<5>::with_default_bands`].
    #[must_use]
    pub fn new(dx_m: [f64; C], dt_s: [f64; C], class: [BandClass; C]) -> Self {
        // Initialize each BTreeMap default. We can't `[BTreeMap::new(); C]`
        // because BTreeMap is not Copy. Use an array-init helper.
        let bands = std::array::from_fn(|_| BTreeMap::new());
        Self {
            bands,
            dx_m,
            dt_s,
            class,
        }
    }

    /// § Number of bands (compile-time const-generic).
    #[inline]
    #[must_use]
    pub const fn band_count(&self) -> usize {
        C
    }

    /// § Total cell count across all bands. Stage-0 sum is O(C). NOTE :
    ///   per-band cell maps are independent ; the same MortonKey can
    ///   appear in multiple bands (intentional — that's how cross-band
    ///   coupling reads from one band and writes to another).
    #[must_use]
    pub fn total_cell_count(&self) -> usize {
        let mut n = 0;
        for b in 0..C {
            n += self.bands[b].len();
        }
        n
    }

    /// § Cell count for a single band.
    #[inline]
    #[must_use]
    pub fn cell_count(&self, band_idx: usize) -> usize {
        if band_idx < C {
            self.bands[band_idx].len()
        } else {
            0
        }
    }

    /// § Read amplitude at `(band_idx, key)`. Defaults to zero if absent.
    #[inline]
    #[must_use]
    pub fn at(&self, band_idx: usize, key: MortonKey) -> C32 {
        if band_idx >= C {
            return C32::ZERO;
        }
        self.bands[band_idx].get(&key).copied().unwrap_or(C32::ZERO)
    }

    /// § Set amplitude at `(band_idx, key)`. Returns previous value.
    pub fn set(&mut self, band_idx: usize, key: MortonKey, val: C32) -> Option<C32> {
        if band_idx >= C {
            return None;
        }
        if val == C32::ZERO {
            self.bands[band_idx].remove(&key)
        } else {
            self.bands[band_idx].insert(key, val)
        }
    }

    /// § Add `delta` to amplitude at `(band_idx, key)`. Used by cross-band
    ///   coupling.
    pub fn add(&mut self, band_idx: usize, key: MortonKey, delta: C32) {
        if band_idx >= C {
            return;
        }
        let entry = self.bands[band_idx].entry(key).or_insert(C32::ZERO);
        *entry += delta;
        // Cull true-zero entries to keep the map compact.
        if *entry == C32::ZERO {
            self.bands[band_idx].remove(&key);
        }
    }

    /// § Iterate cells of one band in canonical (sorted Morton-key) order.
    pub fn cells_in_band(&self, band_idx: usize) -> impl Iterator<Item = (MortonKey, C32)> + '_ {
        let opt = if band_idx < C {
            Some(&self.bands[band_idx])
        } else {
            None
        };
        opt.into_iter()
            .flat_map(|m| m.iter().map(|(k, v)| (*k, *v)))
    }

    /// § Sum the |ψ|² norm across one band. Returns total energy in band.
    #[must_use]
    pub fn band_norm_sqr(&self, band_idx: usize) -> f64 {
        if band_idx >= C {
            return 0.0;
        }
        let mut sum = 0.0_f64;
        for v in self.bands[band_idx].values() {
            sum += v.norm_sqr() as f64;
        }
        sum
    }

    /// § Sum |ψ|² across all bands.
    #[must_use]
    pub fn total_norm_sqr(&self) -> f64 {
        let mut sum = 0.0_f64;
        for b in 0..C {
            sum += self.band_norm_sqr(b);
        }
        sum
    }

    /// § Per-band cell-size accessor.
    #[inline]
    #[must_use]
    pub fn dx_m(&self, band_idx: usize) -> f64 {
        self.dx_m[band_idx.min(C - 1)]
    }

    /// § Per-band substep size accessor.
    #[inline]
    #[must_use]
    pub fn dt_s(&self, band_idx: usize) -> f64 {
        self.dt_s[band_idx.min(C - 1)]
    }

    /// § Per-band class accessor.
    #[inline]
    #[must_use]
    pub fn class(&self, band_idx: usize) -> BandClass {
        self.class[band_idx.min(C - 1)]
    }

    /// § Read a [`PsiCell`] (all bands) at `key`.
    #[must_use]
    pub fn psi_cell(&self, key: MortonKey) -> PsiCell<C> {
        let mut psi = [C32::ZERO; C];
        for b in 0..C {
            psi[b] = self.at(b, key);
        }
        PsiCell { psi }
    }

    /// § Write a [`PsiCell`] (all bands) at `key`.
    pub fn set_psi_cell(&mut self, key: MortonKey, cell: PsiCell<C>) {
        for b in 0..C {
            self.set(b, key, cell.psi[b]);
        }
    }

    /// § Mean phase coherence across the band ; defined as
    ///   `|Σ ψ_i / |ψ_i| | / N` for non-zero amplitudes. A coherent
    ///   standing wave returns ≈ 1.0 ; uniform-random phase returns
    ///   ≈ 1/√N. Used by Phase-6 entropy-book to detect numerical
    ///   decoherence.
    #[must_use]
    pub fn phase_coherence(&self, band_idx: usize) -> f32 {
        if band_idx >= C {
            return 0.0;
        }
        let mut sum_re = 0.0_f64;
        let mut sum_im = 0.0_f64;
        let mut n = 0usize;
        for v in self.bands[band_idx].values() {
            let r = v.norm() as f64;
            if r > 1e-12 {
                sum_re += (v.re as f64) / r;
                sum_im += (v.im as f64) / r;
                n += 1;
            }
        }
        if n == 0 {
            return 0.0;
        }
        let mag = sum_re.hypot(sum_im);
        (mag / (n as f64)) as f32
    }
}

impl WaveField<BAND_COUNT_DEFAULT> {
    /// § Construct the default 5-band field with metadata derived from
    ///   [`Band::recommended_cell_size_m`] / [`Band::recommended_substep_s`].
    #[must_use]
    pub fn with_default_bands() -> Self {
        let mut dx_m = [0.0_f64; BAND_COUNT_DEFAULT];
        let mut dt_s = [0.0_f64; BAND_COUNT_DEFAULT];
        let mut class = [BandClass::FastDirect; BAND_COUNT_DEFAULT];
        for (i, b) in DEFAULT_BANDS.iter().enumerate() {
            dx_m[i] = b.recommended_cell_size_m();
            dt_s[i] = b.recommended_substep_s();
            class[i] = b.class();
        }
        WaveField::new(dx_m, dt_s, class)
    }

    /// § Read amplitude at the default-band position.
    #[inline]
    #[must_use]
    pub fn at_band(&self, band: Band, key: MortonKey) -> C32 {
        self.at(band.index(), key)
    }

    /// § Set amplitude at the default-band position.
    pub fn set_band(&mut self, band: Band, key: MortonKey, val: C32) -> Option<C32> {
        self.set(band.index(), key, val)
    }

    /// § Cell count for a default Band.
    #[inline]
    #[must_use]
    pub fn cell_count_band(&self, band: Band) -> usize {
        self.cell_count(band.index())
    }

    /// § |ψ|² norm for a default Band.
    #[inline]
    #[must_use]
    pub fn band_norm_sqr_band(&self, band: Band) -> f64 {
        self.band_norm_sqr(band.index())
    }
}

impl Default for WaveField<BAND_COUNT_DEFAULT> {
    fn default() -> Self {
        Self::with_default_bands()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(x: u64, y: u64, z: u64) -> MortonKey {
        MortonKey::encode(x, y, z).unwrap()
    }

    #[test]
    fn default_field_is_5_bands_empty() {
        let f = WaveField::<5>::with_default_bands();
        assert_eq!(f.band_count(), 5);
        assert_eq!(f.total_cell_count(), 0);
        for b in 0..5 {
            assert_eq!(f.cell_count(b), 0);
        }
    }

    #[test]
    fn set_and_read_round_trip() {
        let mut f = WaveField::<5>::with_default_bands();
        let k = key(1, 2, 3);
        let v = C32::new(0.5, 0.25);
        f.set(0, k, v);
        assert_eq!(f.at(0, k), v);
    }

    #[test]
    fn zero_set_removes_cell() {
        let mut f = WaveField::<5>::with_default_bands();
        let k = key(1, 2, 3);
        f.set(0, k, C32::new(0.5, 0.0));
        assert_eq!(f.cell_count(0), 1);
        f.set(0, k, C32::ZERO);
        assert_eq!(f.cell_count(0), 0);
    }

    #[test]
    fn add_accumulates_then_culls_zero() {
        let mut f = WaveField::<5>::with_default_bands();
        let k = key(0, 0, 0);
        f.add(0, k, C32::new(0.5, 0.0));
        f.add(0, k, C32::new(0.25, 0.0));
        assert!((f.at(0, k).re - 0.75).abs() < 1e-6);
        f.add(0, k, C32::new(-0.75, 0.0));
        assert_eq!(f.cell_count(0), 0);
    }

    #[test]
    fn cells_in_band_iter_in_morton_order() {
        let mut f = WaveField::<5>::with_default_bands();
        let keys = [key(2, 0, 0), key(0, 0, 0), key(1, 0, 0)];
        for (i, k) in keys.iter().enumerate() {
            f.set(0, *k, C32::new((i + 1) as f32, 0.0));
        }
        let collected: Vec<_> = f.cells_in_band(0).collect();
        assert_eq!(collected.len(), 3);
        let mut sorted = collected.clone();
        sorted.sort_by_key(|(k, _)| *k);
        assert_eq!(collected, sorted, "iter must be in Morton-sorted order");
    }

    #[test]
    fn band_norm_sqr_sums_correctly() {
        let mut f = WaveField::<5>::with_default_bands();
        f.set(0, key(0, 0, 0), C32::new(3.0, 0.0));
        f.set(0, key(1, 0, 0), C32::new(0.0, 4.0));
        let n = f.band_norm_sqr(0);
        // 9 + 16 = 25.
        assert!((n - 25.0).abs() < 1e-6);
    }

    #[test]
    fn total_norm_sqr_sums_across_bands() {
        let mut f = WaveField::<5>::with_default_bands();
        f.set(0, key(0, 0, 0), C32::new(3.0, 0.0));
        f.set(1, key(0, 0, 0), C32::new(0.0, 4.0));
        let total = f.total_norm_sqr();
        assert!((total - 25.0).abs() < 1e-6);
    }

    #[test]
    fn psi_cell_round_trip() {
        let mut f = WaveField::<5>::with_default_bands();
        let k = key(1, 1, 1);
        let mut cell = PsiCell::<5>::zero();
        cell.psi[0] = C32::new(1.0, 0.0);
        cell.psi[2] = C32::new(0.0, 0.5);
        f.set_psi_cell(k, cell);
        let read = f.psi_cell(k);
        assert_eq!(read, cell);
        // Zero bands should have been culled.
        assert_eq!(f.cell_count(1), 0);
        assert_eq!(f.cell_count(0), 1);
        assert_eq!(f.cell_count(2), 1);
    }

    #[test]
    fn psi_cell_total_norm_sqr_aggregates() {
        let mut cell = PsiCell::<5>::zero();
        cell.psi[0] = C32::new(3.0, 0.0);
        cell.psi[1] = C32::new(0.0, 4.0);
        let n = cell.total_norm_sqr();
        assert!((n - 25.0).abs() < 1e-9);
    }

    #[test]
    fn dx_dt_metadata_matches_band_defaults() {
        let f = WaveField::<5>::with_default_bands();
        assert!((f.dx_m(0) - Band::AudioSubKHz.recommended_cell_size_m()).abs() < 1e-9);
        assert!((f.dx_m(1) - Band::LightRed.recommended_cell_size_m()).abs() < 1e-9);
        assert!((f.dt_s(0) - Band::AudioSubKHz.recommended_substep_s()).abs() < 1e-9);
    }

    #[test]
    fn class_metadata_matches_band_defaults() {
        let f = WaveField::<5>::with_default_bands();
        assert_eq!(f.class(0), Band::AudioSubKHz.class());
        assert_eq!(f.class(1), Band::LightRed.class());
    }

    #[test]
    fn phase_coherence_unity_for_aligned_phases() {
        let mut f = WaveField::<5>::with_default_bands();
        for i in 0..10 {
            // All amplitudes have phase 0 ; coherence ≈ 1.
            f.set(0, key(i, 0, 0), C32::new(1.0, 0.0));
        }
        let c = f.phase_coherence(0);
        assert!((c - 1.0).abs() < 1e-3);
    }

    #[test]
    fn phase_coherence_low_for_random_phases() {
        let mut f = WaveField::<5>::with_default_bands();
        // Two anti-phase pairs : net = 0.
        f.set(0, key(0, 0, 0), C32::new(1.0, 0.0));
        f.set(0, key(1, 0, 0), C32::new(-1.0, 0.0));
        f.set(0, key(2, 0, 0), C32::new(0.0, 1.0));
        f.set(0, key(3, 0, 0), C32::new(0.0, -1.0));
        let c = f.phase_coherence(0);
        assert!(c < 0.01);
    }

    #[test]
    fn at_band_default_helper() {
        let mut f = WaveField::<5>::with_default_bands();
        let k = key(0, 0, 0);
        f.set_band(Band::AudioSubKHz, k, C32::new(1.0, 0.0));
        assert_eq!(f.at_band(Band::AudioSubKHz, k), C32::new(1.0, 0.0));
        assert_eq!(f.cell_count_band(Band::AudioSubKHz), 1);
        assert!((f.band_norm_sqr_band(Band::AudioSubKHz) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn at_returns_zero_for_missing_cell() {
        let f = WaveField::<5>::with_default_bands();
        assert_eq!(f.at(0, key(99, 99, 99)), C32::ZERO);
    }

    #[test]
    fn at_returns_zero_for_oob_band_index() {
        let f = WaveField::<5>::with_default_bands();
        assert_eq!(f.at(99, key(0, 0, 0)), C32::ZERO);
    }

    #[test]
    fn deterministic_iter_after_sequential_inserts() {
        let mut f1 = WaveField::<5>::with_default_bands();
        let mut f2 = WaveField::<5>::with_default_bands();
        let keys = [key(7, 5, 3), key(1, 2, 3), key(9, 0, 1), key(0, 0, 0)];
        for (i, k) in keys.iter().enumerate() {
            f1.set(0, *k, C32::new(i as f32, 0.0));
        }
        // Insert in different order ; iteration must agree.
        for k in keys.iter().rev() {
            let i = keys.iter().position(|x| x == k).unwrap();
            f2.set(0, *k, C32::new(i as f32, 0.0));
        }
        let it1: Vec<_> = f1.cells_in_band(0).collect();
        let it2: Vec<_> = f2.cells_in_band(0).collect();
        assert_eq!(it1, it2, "iter must be deterministic across insert order");
    }
}
