//! § PsiAudioField — sparse Morton-keyed ψ-AUDIO band overlay.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The wave-unity AUDIO-band ψ-field as a sparse-Morton-keyed overlay.
//!   Per `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl § VIII.1` :
//!
//!   ```text
//!   AUDIO-band : Complex<f32> = 8B   (direct amplitude, full-precision)
//!   ψ-state @ active-region cells ⊗ separate sparse-hash-grid (Morton-keyed)
//!   similar-pattern to Λ + Ψ overlays
//!   ```
//!
//!   This crate IMPLEMENTS that overlay specifically for the AUDIO band —
//!   the LIGHT / HEAT / SCENT / MANA bands live in the upstream
//!   `cssl-wave-solver` crate (T11-D114, deferred). When D114 lands its
//!   full multi-band ψ-field container the cross-band-coupler in this
//!   crate hooks into it ; until then `PsiAudioField` is the canonical
//!   AUDIO-band ψ-store + `CrossBandCoupler` carries a stub-matrix that
//!   matches the spec's § XI cross-band-coupling-table on the AUDIO-row.
//!
//! § STORAGE
//!   - `PsiAudioCell` : one `Complex<f32>` (8 B + 8 B padding for align(16)
//!     std430 compat = 16 B per-cell). The padding is zero-initialized
//!     and reserved for future expansion (e.g. a per-cell impedance
//!     handle when the D114 KAN-impedance lands).
//!   - `PsiAudioField` : wraps `SparseMortonGrid<PsiAudioCell>` with the
//!     AUDIO-band conventions baked in (default = silence ; norm-conserve
//!     check ; per-cell ψ-injection through the Σ-mask consent gate).
//!
//! § CONSERVATION (spec § XII.1 + § XII.3)
//!   ∫|ψ|² dV ⊗ conserved-modulo-impedance-absorption + cross-band-transfer
//!   `total_energy` returns the L2 norm² over all cells ; the LBM solver
//!   is expected to drive `total_energy` monotonically-decreasing (modulo
//!   active sources injecting new energy, whose net contribution is
//!   accounted by the source-injection bookkeeping).
//!
//! § PRIME-DIRECTIVE
//!   Per spec § XVII.1 : "ψ-modifications @ Sovereign-domain-cells ⊗ R!
//!   check Σ.consent_bits BEFORE-write". The `inject` method on this
//!   field consults a Σ-overlay before each write ; if the cell refuses
//!   the modify-bit the injection is rejected with
//!   `WaveAudioError::ConsentDenied`.

use crate::complex::Complex;
use crate::error::{Result, WaveAudioError};
use cssl_substrate_omega_field::morton::MortonKey;
use cssl_substrate_omega_field::sigma_overlay::SigmaOverlay;
use cssl_substrate_omega_field::sparse_grid::{OmegaCellLayout, SparseMortonGrid};
use cssl_substrate_prime_directive::sigma::ConsentBit;

/// Per-cell ψ-AUDIO storage : one `Complex<f32>` plus 8 B reserved
/// padding for `align(16)` std430 compatibility (matches the cssl-pga
/// canonical std430 alignment + leaves room for a 32-bit impedance
/// handle + 32-bit substep counter when the D114 wave-solver lands).
///
/// § FIELDS
///   - `amplitude` : the complex AUDIO-band amplitude `ψ_AUDIO(x,t) ∈ ℂ`.
///   - `_pad` : zero-initialized reserved padding ; do not interpret.
///
/// § DEFAULT
///   `Default = silence` : `amplitude = 0+0i`. The wave-unity § VII.3
///   "inactive regions" semantic : cells without explicit injection
///   are silent by convention.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C, align(16))]
pub struct PsiAudioCell {
    /// Complex amplitude `ψ_AUDIO(x,t) ∈ ℂ`. Real = pressure, imag =
    /// Hilbert-conjugate phase.
    pub amplitude: Complex,
    /// Reserved padding ; zero-initialized + ignored by audio code.
    _pad: [u32; 2],
}

impl PsiAudioCell {
    /// Construct a cell with a given complex amplitude.
    #[must_use]
    pub const fn from_amplitude(amp: Complex) -> PsiAudioCell {
        PsiAudioCell {
            amplitude: amp,
            _pad: [0; 2],
        }
    }

    /// "Silence" — `amplitude = 0+0i`.
    pub const SILENCE: PsiAudioCell = PsiAudioCell {
        amplitude: Complex::ZERO,
        _pad: [0; 2],
    };

    /// Acoustic pressure (real part). Units : whatever the source
    /// synthesizer chose (typically normalized to dimensionless `[-1, 1]`
    /// after the WaveAudioProjector's per-listener gain).
    #[inline]
    #[must_use]
    pub fn pressure(self) -> f32 {
        self.amplitude.re
    }

    /// Energy density `|ψ|²`. Used by the conservation-check + the
    /// cross-band-coupling threshold.
    #[inline]
    #[must_use]
    pub fn energy(self) -> f32 {
        self.amplitude.norm_sq()
    }
}

impl OmegaCellLayout for PsiAudioCell {
    fn omega_cell_size() -> usize {
        16
    }
    fn omega_cell_align() -> usize {
        16
    }
    fn omega_cell_layout_tag() -> &'static str {
        "PsiAudioCell"
    }
}

/// Sparse Morton-keyed ψ-AUDIO band overlay.
///
/// § STORAGE
///   The underlying grid is `SparseMortonGrid<PsiAudioCell>`. This is
///   the same canonical grid the LIGHT/HEAT/SCENT/MANA overlays will
///   use when D114 lands them ; cssl-wave-audio is the AUDIO-band
///   "first occupant" of the per-band overlay pattern.
#[derive(Debug, Clone, Default)]
pub struct PsiAudioField {
    grid: SparseMortonGrid<PsiAudioCell>,
}

impl PsiAudioField {
    /// Construct an empty AUDIO-band ψ-field.
    #[must_use]
    pub fn new() -> PsiAudioField {
        PsiAudioField {
            grid: SparseMortonGrid::with_capacity(64),
        }
    }

    /// Construct with at-least the given number of slots reserved.
    /// Useful when sizing for a known active-region size.
    #[must_use]
    pub fn with_capacity(min_capacity: usize) -> PsiAudioField {
        PsiAudioField {
            grid: SparseMortonGrid::with_capacity(min_capacity),
        }
    }

    /// Number of explicitly-stored cells (ψ ≠ 0).
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.grid.len()
    }

    /// True iff the field has no explicit cells.
    #[must_use]
    pub fn is_silent(&self) -> bool {
        self.grid.is_empty()
    }

    /// Read the complex amplitude at `key`. Returns `Complex::ZERO` for
    /// missing cells (silence-default per spec § VII.3 inactive-regions).
    #[must_use]
    pub fn at(&self, key: MortonKey) -> Complex {
        self.grid
            .at_const(key)
            .map(|c| c.amplitude)
            .unwrap_or(Complex::ZERO)
    }

    /// Set the complex amplitude at `key`. Returns the prior value
    /// (or `None`).
    ///
    /// § PRIME-DIRECTIVE
    ///   This is the **unguarded** writer — used by the LBM kernel
    ///   internal stream-collide step where the Σ-mask check has
    ///   already been performed at the active-region admission gate.
    ///   For Sovereign-domain ψ-injection use [`Self::inject`] which
    ///   threads the Σ check.
    pub fn set(&mut self, key: MortonKey, amp: Complex) -> Result<Option<Complex>> {
        let cell = PsiAudioCell::from_amplitude(amp);
        match self.grid.insert(key, cell) {
            Ok(prev) => Ok(prev.map(|p| p.amplitude)),
            Err(e) => Err(WaveAudioError::Storage(format!("{e}"))),
        }
    }

    /// Remove a cell (return its prior amplitude, if any).
    pub fn remove(&mut self, key: MortonKey) -> Option<Complex> {
        self.grid.remove(key).map(|c| c.amplitude)
    }

    /// Add `delta` to the existing amplitude at `key` (creates the cell
    /// if missing). Used by the LBM stream-step to accumulate per-
    /// substep contributions and by the cross-band coupler to add a
    /// transferred-amplitude delta.
    pub fn add_at(&mut self, key: MortonKey, delta: Complex) -> Result<()> {
        let prev = self.at(key);
        let next = prev.add(delta);
        self.set(key, next)?;
        Ok(())
    }

    /// Σ-gated ψ-injection — the canonical Sovereign-domain mutation
    /// surface. Per spec § XVII.1 every ψ-write at a consent-protected
    /// cell goes through this method.
    ///
    /// § ERRORS
    ///   - [`WaveAudioError::ConsentDenied`] when the Σ-overlay refuses
    ///     the `Modify` op-class for `key`.
    pub fn inject(
        &mut self,
        key: MortonKey,
        amp: Complex,
        sigma: &SigmaOverlay,
    ) -> Result<Option<Complex>> {
        // Σ-mask hot-path : look up the full mask + check Modify-bit.
        // Per the OmegaField::set_cell precedent (cssl-substrate-omega-
        // field § II.set_cell) the DEFAULT mask is DefaultPrivate which
        // does NOT include Modify ; consent must be explicitly granted
        // before a ψ-injection succeeds. This matches spec § XVII.1
        // verbatim : "ψ-modifications @ Sovereign-domain-cells ⊗ R!
        // check Σ.consent_bits BEFORE-write".
        let mask = sigma.at(key);
        if !mask.permits(ConsentBit::Modify) {
            return Err(WaveAudioError::ConsentDenied {
                key: key.to_u64(),
                requested: "modify",
            });
        }
        self.set(key, amp)
    }

    /// Total ψ-energy across all explicit cells : `Σ |ψ|²`. The
    /// conservation-law check per spec § XII.1 reads this before + after
    /// each LBM substep ; the delta should be ≤ ε beyond what
    /// impedance-absorption + cross-band-transfer account for.
    #[must_use]
    pub fn total_energy(&self) -> f32 {
        self.grid.iter_unordered().map(|(_, c)| c.energy()).sum()
    }

    /// L1 amplitude-magnitude norm `Σ |ψ|`. Used by the soft-source-
    /// detection threshold inside the cross-band coupler — when L1 is
    /// below `COUPLING_EPSILON` the band is effectively silent and we
    /// skip the coupling-application pass entirely.
    #[must_use]
    pub fn l1_amplitude(&self) -> f32 {
        self.grid
            .iter_unordered()
            .map(|(_, c)| c.amplitude.norm())
            .sum()
    }

    /// Sample the field at a continuous world-position via trilinear
    /// interpolation across the 8 surrounding Morton-keyed cells. The
    /// projector uses this to read the ψ-field at the per-ear sample
    /// positions which generally do not coincide with cell-centers.
    ///
    /// `voxel_size` is in metres ; the world-to-cell mapping is
    /// `cell = floor(world / voxel_size)`.
    #[must_use]
    pub fn sample_world(&self, world_pos: [f32; 3], voxel_size: f32) -> Complex {
        if voxel_size <= 0.0 {
            return Complex::ZERO;
        }
        let inv = 1.0 / voxel_size;
        let fx = world_pos[0] * inv;
        let fy = world_pos[1] * inv;
        let fz = world_pos[2] * inv;
        let ix = fx.floor() as i64;
        let iy = fy.floor() as i64;
        let iz = fz.floor() as i64;
        let tx = fx - fx.floor();
        let ty = fy - fy.floor();
        let tz = fz - fz.floor();

        let mut acc = Complex::ZERO;
        for dz in 0..2_i64 {
            for dy in 0..2_i64 {
                for dx in 0..2_i64 {
                    let cx = ix + dx;
                    let cy = iy + dy;
                    let cz = iz + dz;
                    if cx < 0 || cy < 0 || cz < 0 {
                        continue; // negative coords clip to 0 amplitude
                    }
                    let key = match MortonKey::encode(cx as u64, cy as u64, cz as u64) {
                        Ok(k) => k,
                        Err(_) => continue,
                    };
                    let amp = self.at(key);
                    let wx = if dx == 0 { 1.0 - tx } else { tx };
                    let wy = if dy == 0 { 1.0 - ty } else { ty };
                    let wz = if dz == 0 { 1.0 - tz } else { tz };
                    let w = wx * wy * wz;
                    acc = acc.add(amp.scale(w));
                }
            }
        }
        acc
    }

    /// Iterate cells in MortonKey-ascending order. Replay-stable.
    pub fn iter(&self) -> impl Iterator<Item = (MortonKey, &PsiAudioCell)> {
        self.grid.iter()
    }

    /// Clear the entire ψ-field back to silence. Used between solver
    /// frames when the active-region resets.
    pub fn clear(&mut self) {
        self.grid = SparseMortonGrid::with_capacity(64);
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::{PsiAudioCell, PsiAudioField};
    use crate::complex::Complex;
    use cssl_substrate_omega_field::morton::MortonKey;
    use cssl_substrate_omega_field::sigma_overlay::SigmaOverlay;
    use cssl_substrate_omega_field::sparse_grid::OmegaCellLayout;
    use cssl_substrate_prime_directive::sigma::{ConsentBit, SigmaMaskPacked};

    #[test]
    fn psi_audio_cell_size_is_16_bytes() {
        assert_eq!(<PsiAudioCell as OmegaCellLayout>::omega_cell_size(), 16);
        assert_eq!(core::mem::size_of::<PsiAudioCell>(), 16);
    }

    #[test]
    fn psi_audio_cell_align_is_16() {
        assert_eq!(<PsiAudioCell as OmegaCellLayout>::omega_cell_align(), 16);
        assert_eq!(core::mem::align_of::<PsiAudioCell>(), 16);
    }

    #[test]
    fn psi_audio_cell_default_is_silence() {
        let c = PsiAudioCell::default();
        assert_eq!(c, PsiAudioCell::SILENCE);
        assert_eq!(c.energy(), 0.0);
    }

    #[test]
    fn psi_audio_cell_pressure_is_real() {
        let c = PsiAudioCell::from_amplitude(Complex::new(0.5, 0.0));
        assert_eq!(c.pressure(), 0.5);
    }

    #[test]
    fn psi_audio_cell_energy_is_norm_sq() {
        let c = PsiAudioCell::from_amplitude(Complex::new(3.0, 4.0));
        assert_eq!(c.energy(), 25.0);
    }

    #[test]
    fn psi_audio_field_default_is_silent() {
        let f = PsiAudioField::new();
        assert!(f.is_silent());
        assert_eq!(f.cell_count(), 0);
        assert_eq!(f.total_energy(), 0.0);
    }

    #[test]
    fn psi_audio_field_default_at_unset_is_zero() {
        let f = PsiAudioField::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        assert_eq!(f.at(k), Complex::ZERO);
    }

    #[test]
    fn psi_audio_field_set_and_read() {
        let mut f = PsiAudioField::new();
        let k = MortonKey::encode(1, 2, 3).unwrap();
        f.set(k, Complex::new(0.7, -0.2)).unwrap();
        assert_eq!(f.at(k), Complex::new(0.7, -0.2));
        assert_eq!(f.cell_count(), 1);
    }

    #[test]
    fn psi_audio_field_set_replaces_returns_prior() {
        let mut f = PsiAudioField::new();
        let k = MortonKey::encode(0, 0, 0).unwrap();
        let prior = f.set(k, Complex::new(1.0, 0.0)).unwrap();
        assert!(prior.is_none());
        let prior = f.set(k, Complex::new(2.0, 0.0)).unwrap();
        assert_eq!(prior, Some(Complex::new(1.0, 0.0)));
    }

    #[test]
    fn psi_audio_field_remove_restores_default() {
        let mut f = PsiAudioField::new();
        let k = MortonKey::encode(2, 0, 0).unwrap();
        f.set(k, Complex::new(0.5, 0.0)).unwrap();
        let removed = f.remove(k);
        assert_eq!(removed, Some(Complex::new(0.5, 0.0)));
        assert_eq!(f.at(k), Complex::ZERO);
    }

    #[test]
    fn psi_audio_field_add_at_creates_then_accumulates() {
        let mut f = PsiAudioField::new();
        let k = MortonKey::encode(1, 0, 0).unwrap();
        f.add_at(k, Complex::new(0.3, 0.0)).unwrap();
        f.add_at(k, Complex::new(0.4, 0.1)).unwrap();
        // f32 addition has tiny rounding ; check within 1e-5.
        let got = f.at(k);
        assert!((got.re - 0.7).abs() < 1e-5, "re = {}", got.re);
        assert!((got.im - 0.1).abs() < 1e-6, "im = {}", got.im);
    }

    #[test]
    fn psi_audio_field_total_energy_sums_cells() {
        let mut f = PsiAudioField::new();
        f.set(MortonKey::encode(0, 0, 0).unwrap(), Complex::new(3.0, 4.0))
            .unwrap();
        f.set(MortonKey::encode(1, 0, 0).unwrap(), Complex::new(1.0, 0.0))
            .unwrap();
        assert!((f.total_energy() - 26.0).abs() < 1e-5);
    }

    #[test]
    fn psi_audio_field_l1_amplitude_sums_norms() {
        let mut f = PsiAudioField::new();
        f.set(MortonKey::encode(0, 0, 0).unwrap(), Complex::new(3.0, 4.0))
            .unwrap();
        f.set(MortonKey::encode(1, 0, 0).unwrap(), Complex::new(0.0, 1.0))
            .unwrap();
        // |3+4i| + |0+1i| = 5 + 1 = 6
        assert!((f.l1_amplitude() - 6.0).abs() < 1e-5);
    }

    #[test]
    fn psi_audio_field_iter_is_morton_sorted() {
        let mut f = PsiAudioField::new();
        f.set(MortonKey::encode(2, 0, 0).unwrap(), Complex::new(1.0, 0.0))
            .unwrap();
        f.set(MortonKey::encode(0, 0, 0).unwrap(), Complex::new(2.0, 0.0))
            .unwrap();
        f.set(MortonKey::encode(1, 0, 0).unwrap(), Complex::new(3.0, 0.0))
            .unwrap();
        let keys: Vec<u64> = f.iter().map(|(k, _)| k.to_u64()).collect();
        // Morton-sorted : strictly ascending u64s.
        for w in keys.windows(2) {
            assert!(w[0] < w[1], "expected ascending : {keys:?}");
        }
    }

    #[test]
    fn psi_audio_field_inject_consent_granted_writes() {
        let mut f = PsiAudioField::new();
        let mut sigma = SigmaOverlay::new();
        let k = MortonKey::encode(1, 0, 0).unwrap();
        let mask = SigmaMaskPacked::default_mask()
            .with_consent(ConsentBit::Modify.bits() | ConsentBit::Observe.bits());
        sigma.set(k, mask);

        let prev = f.inject(k, Complex::new(0.5, 0.0), &sigma).unwrap();
        assert!(prev.is_none());
        assert_eq!(f.at(k), Complex::new(0.5, 0.0));
    }

    #[test]
    fn psi_audio_field_inject_consent_denied_refuses() {
        let mut f = PsiAudioField::new();
        let mut sigma = SigmaOverlay::new();
        let k = MortonKey::encode(2, 0, 0).unwrap();
        // Set Σ to OBSERVE-only (no Modify-bit).
        let mask = SigmaMaskPacked::default_mask().with_consent(ConsentBit::Observe.bits());
        sigma.set(k, mask);

        let result = f.inject(k, Complex::new(0.5, 0.0), &sigma);
        assert!(result.is_err());
        // Field must be unchanged.
        assert_eq!(f.at(k), Complex::ZERO);
    }

    #[test]
    fn psi_audio_field_clear_resets() {
        let mut f = PsiAudioField::new();
        f.set(MortonKey::encode(1, 0, 0).unwrap(), Complex::new(1.0, 0.0))
            .unwrap();
        f.clear();
        assert!(f.is_silent());
        assert_eq!(f.cell_count(), 0);
    }

    #[test]
    fn psi_audio_field_sample_world_at_cell_center() {
        let mut f = PsiAudioField::new();
        // voxel_size = 0.5m ; cell (0,0,0) covers world [0,0.5)³ ;
        // world (0.0, 0.0, 0.0) is the cell-corner — trilinear sample
        // there is just the cell's amplitude.
        let k = MortonKey::encode(0, 0, 0).unwrap();
        f.set(k, Complex::new(1.0, 0.0)).unwrap();
        let s = f.sample_world([0.0, 0.0, 0.0], 0.5);
        assert!((s.re - 1.0).abs() < 1e-5);
    }

    #[test]
    fn psi_audio_field_sample_world_midpoint_averages() {
        // Two cells (0,0,0) and (1,0,0) at amplitudes 1.0 and 2.0 ; sample
        // halfway between them should give ≈ 1.5.
        let mut f = PsiAudioField::new();
        f.set(MortonKey::encode(0, 0, 0).unwrap(), Complex::new(1.0, 0.0))
            .unwrap();
        f.set(MortonKey::encode(1, 0, 0).unwrap(), Complex::new(2.0, 0.0))
            .unwrap();
        let voxel = 0.5_f32;
        // Midpoint world coords: x = 0.5 (between cell 0 and cell 1) ;
        // tx = 0.0 means we land EXACTLY on cell (1,0,0)'s start. To get
        // the trilinear average we offset 0.25m → tx = 0.5.
        let s = f.sample_world([0.25, 0.0, 0.0], voxel);
        assert!((s.re - 1.5).abs() < 1e-4);
    }

    #[test]
    fn psi_audio_field_sample_world_zero_voxel_is_zero() {
        let f = PsiAudioField::new();
        let s = f.sample_world([0.0, 0.0, 0.0], 0.0);
        assert_eq!(s, Complex::ZERO);
    }
}
