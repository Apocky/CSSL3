//! § Fluorescence + Phosphorescence — excitation→emission spectral remap
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per `07_AES/03 § IV` :
//!     "fluorescence + phosphorescence supported :
//!       @ certain-Λ-token-types (mana, blessing) ⊗ have-fluorescence-curves
//!       @ shifted-emission @ wavelength ⊗ from-absorption @ different-wavelength"
//!
//!   The fluorescence model is a per-band excitation→emission remap : light
//!   absorbed at one band is re-emitted at a longer-wavelength band (red-
//!   shift = Stokes shift). For phosphorescence we add a per-frame decay
//!   factor so the emission persists across multiple frames.
//!
//!   This module is intentionally physical (Stokes-shift + quantum-yield)
//!   rather than KAN-driven, since the canonical fluorescence-variant KAN
//!   shape is `17 → 16` (cosθ + 16-band absorbed) and a runtime evaluator
//!   for that variant is owned by the deferred KAN-runtime slice. The
//!   physical formula is the perceptual baseline.

use crate::band::{BandTable, BAND_COUNT};
use crate::radiance::SpectralRadiance;

/// § The Stokes-shift descriptor — how far the emitted-band shifts from
///   the absorbed band, plus the quantum yield (fraction of absorbed
///   photons re-emitted) and the conversion efficiency.
#[derive(Debug, Clone, Copy)]
pub struct StokesShift {
    /// § Absolute wavelength shift in nm. Positive = red-shift (longer
    ///   wavelength out than in). For UV→visible fluorescence this is
    ///   typically ~50-150 nm.
    pub shift_nm: f32,
    /// § Quantum yield in `[0, 1]`. The fraction of absorbed photons that
    ///   are re-emitted (vs lost to thermal decay).
    pub quantum_yield: f32,
}

impl StokesShift {
    /// § Construct a Stokes-shift descriptor.
    #[must_use]
    pub const fn new(shift_nm: f32, quantum_yield: f32) -> Self {
        Self {
            shift_nm,
            quantum_yield,
        }
    }

    /// § Canonical "white-paper" optical brightener : ~80 nm shift, ~0.85
    ///   yield. Used as the default fluorescence fixture.
    #[must_use]
    pub const fn optical_brightener() -> Self {
        Self::new(80.0, 0.85)
    }

    /// § Canonical "ruby" : ~110 nm shift, ~0.50 yield.
    #[must_use]
    pub const fn ruby() -> Self {
        Self::new(110.0, 0.50)
    }

    /// § Canonical "Λ-token mana" : ~140 nm shift, ~0.95 yield. The
    ///   high-yield variant for sentient-pattern emissive surfaces.
    #[must_use]
    pub const fn lambda_mana() -> Self {
        Self::new(140.0, 0.95)
    }
}

/// § The fluorescence model. Stateless ; carries the Stokes-shift descriptor.
#[derive(Debug, Clone, Copy)]
pub struct Fluorescence {
    /// § The Stokes-shift descriptor.
    pub stokes: StokesShift,
    /// § Whether to keep the original (un-shifted) absorbed component as
    ///   pass-through. If false, the absorbed light is fully consumed
    ///   regardless of yield.
    pub passthrough_absorbed: bool,
}

impl Fluorescence {
    /// § Construct a fluorescence model.
    #[must_use]
    pub const fn new(stokes: StokesShift) -> Self {
        Self {
            stokes,
            passthrough_absorbed: true,
        }
    }

    /// § Default optical-brightener fixture.
    #[must_use]
    pub const fn optical_brightener() -> Self {
        Self::new(StokesShift::optical_brightener())
    }

    /// § Apply the excitation→emission remap to a 16-band reflectance
    ///   buffer. The mapping is `emit[band(lambda + shift)] += yield *
    ///   absorb[band(lambda)]` plus an optional pass-through term
    ///   `pass[band(lambda)] = absorb[band(lambda)] - yield *
    ///   absorb[band(lambda)]`. The band the emitted light lands in is
    ///   found via `BandTable::band_index_at_nm` — if no band contains
    ///   `lambda + shift`, the emission is dropped (no photon escapes).
    pub fn remap(&self, bands: &mut [f32; BAND_COUNT], table: &BandTable) {
        let mut emit = [0.0_f32; BAND_COUNT];
        let mut pass = [0.0_f32; BAND_COUNT];

        let yield_ = self.stokes.quantum_yield.max(0.0).min(1.0);
        let shift = self.stokes.shift_nm;

        for i in 0..BAND_COUNT {
            let absorbed = bands[i];
            let lambda = table.band(i).center_nm;
            let lambda_emit = lambda + shift;
            if let Some(j) = table.band_index_at_nm(lambda_emit) {
                emit[j] += yield_ * absorbed;
            }
            // Pass-through component.
            if self.passthrough_absorbed {
                pass[i] = absorbed - yield_ * absorbed;
            }
        }

        for i in 0..BAND_COUNT {
            bands[i] = emit[i] + pass[i];
        }
    }

    /// § Apply the remap to a `SpectralRadiance` (in-place). The hero +
    ///   accompaniment view is rebuilt after the remap.
    pub fn remap_radiance(&self, r: &mut SpectralRadiance, table: &BandTable) {
        self.remap(&mut r.bands, table);
        r.accumulate_from_bands(4, table);
    }
}

/// § Phosphorescence : fluorescence with a per-frame persistence factor.
///   The emission accumulates across frames + decays exponentially, so
///   a phosphor-like material continues to glow after the excitation
///   source is removed.
#[derive(Debug, Clone, Copy)]
pub struct Phosphorescence {
    /// § The underlying fluorescence (Stokes shift + quantum yield).
    pub fluorescence: Fluorescence,
    /// § Per-frame retention factor in `[0, 1]`. 0.95 means 5% of the
    ///   accumulated emission decays per frame.
    pub retention_per_frame: f32,
}

impl Phosphorescence {
    /// § Construct.
    #[must_use]
    pub fn new(fluorescence: Fluorescence, retention_per_frame: f32) -> Self {
        Self {
            fluorescence,
            retention_per_frame: retention_per_frame.max(0.0).min(1.0),
        }
    }

    /// § Default short-persistence phosphor : ~0.88 retention, optical-
    ///   brightener Stokes-shift.
    #[must_use]
    pub fn short_persistence() -> Self {
        Self::new(Fluorescence::optical_brightener(), 0.88)
    }

    /// § Default long-persistence phosphor : ~0.98 retention.
    #[must_use]
    pub fn long_persistence() -> Self {
        Self::new(Fluorescence::optical_brightener(), 0.98)
    }

    /// § Step the phosphor by one frame. The `accumulator` carries the
    ///   per-band emission state across frames ; this function adds the
    ///   new fluorescence emission + decays the existing accumulator.
    pub fn step_frame(
        &self,
        absorbed_bands: [f32; BAND_COUNT],
        accumulator: &mut [f32; BAND_COUNT],
        table: &BandTable,
    ) {
        // Decay the existing accumulator first.
        for v in accumulator.iter_mut() {
            *v *= self.retention_per_frame;
        }
        // Compute the new emission (fluorescence remap of absorbed_bands)
        // and add it to the accumulator.
        let mut emit = absorbed_bands;
        self.fluorescence.remap(&mut emit, table);
        for i in 0..BAND_COUNT {
            // The "emit" buffer contains absorbed-pass-through + Stokes-
            // shifted emission ; we want to keep only the SHIFTED
            // component. So we subtract the absorbed-passthrough.
            let pass = if self.fluorescence.passthrough_absorbed {
                absorbed_bands[i] * (1.0 - self.fluorescence.stokes.quantum_yield)
            } else {
                0.0
            };
            let shifted = emit[i] - pass;
            accumulator[i] += shifted.max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::band::BAND_VISIBLE_START;

    /// § StokesShift constructors are sane.
    #[test]
    fn stokes_constructors() {
        assert!(StokesShift::optical_brightener().shift_nm > 0.0);
        assert!(StokesShift::ruby().quantum_yield > 0.0);
        assert!(StokesShift::lambda_mana().quantum_yield > 0.0);
    }

    /// § Fluorescence::remap shifts emission to longer wavelength.
    #[test]
    fn remap_shifts_red() {
        let t = BandTable::d65();
        let mut bands = [0.0_f32; BAND_COUNT];
        // Excite at 400 nm (band 2 = 380-420 nm).
        bands[BAND_VISIBLE_START] = 1.0;
        Fluorescence::optical_brightener().remap(&mut bands, &t);
        // Some red-shifted band must have non-zero emission ; the
        // original band's intensity is reduced.
        assert!(bands[BAND_VISIBLE_START] < 1.0);
        let any_red = bands.iter().skip(BAND_VISIBLE_START + 1).any(|&v| v > 0.0);
        assert!(any_red, "no red-shifted emission");
    }

    /// § Quantum yield = 0 leaves bands unchanged.
    #[test]
    fn yield_zero_no_emission() {
        let t = BandTable::d65();
        let mut bands = [0.5_f32; BAND_COUNT];
        let pre = bands;
        let f = Fluorescence::new(StokesShift::new(80.0, 0.0));
        f.remap(&mut bands, &t);
        // With yield 0 + passthrough true, output equals input.
        for i in 0..BAND_COUNT {
            assert!((bands[i] - pre[i]).abs() < 1e-6);
        }
    }

    /// § Quantum yield = 1 + passthrough = false consumes all input.
    #[test]
    fn yield_one_no_passthrough_consumes_input() {
        let t = BandTable::d65();
        let mut bands = [0.0_f32; BAND_COUNT];
        bands[BAND_VISIBLE_START] = 1.0;
        let mut f = Fluorescence::new(StokesShift::new(80.0, 1.0));
        f.passthrough_absorbed = false;
        f.remap(&mut bands, &t);
        // Source band is now 0.
        assert_eq!(bands[BAND_VISIBLE_START], 0.0);
        // Some red-shifted band has the full energy.
        let total: f32 = bands.iter().sum();
        assert!((total - 1.0).abs() < 1e-3);
    }

    /// § remap_radiance refreshes hero/accompaniment.
    #[test]
    fn remap_radiance_refreshes_hero() {
        let t = BandTable::d65();
        let mut r = SpectralRadiance::black();
        r.bands[BAND_VISIBLE_START] = 1.0;
        Fluorescence::optical_brightener().remap_radiance(&mut r, &t);
        assert!(r.hero_intensity > 0.0);
    }

    /// § Phosphorescence::short / long persistence retention values.
    #[test]
    fn phosphor_persistence() {
        assert!(Phosphorescence::short_persistence().retention_per_frame > 0.8);
        assert!(Phosphorescence::long_persistence().retention_per_frame > 0.95);
    }

    /// § Phosphorescence::step_frame decays without input.
    #[test]
    fn phosphor_decays_without_input() {
        let t = BandTable::d65();
        let mut acc = [0.0_f32; BAND_COUNT];
        // Seed accumulator.
        acc[BAND_VISIBLE_START + 3] = 1.0;
        let p = Phosphorescence::short_persistence();
        let zero = [0.0_f32; BAND_COUNT];
        p.step_frame(zero, &mut acc, &t);
        // Retention < 1.0 ⇒ accumulator decreased.
        assert!(acc[BAND_VISIBLE_START + 3] < 1.0);
        assert!(acc[BAND_VISIBLE_START + 3] > 0.0);
    }

    /// § Phosphorescence::step_frame accumulates emission.
    #[test]
    fn phosphor_accumulates_emission() {
        let t = BandTable::d65();
        let mut acc = [0.0_f32; BAND_COUNT];
        let mut absorbed = [0.0_f32; BAND_COUNT];
        absorbed[BAND_VISIBLE_START] = 1.0;
        let p = Phosphorescence::long_persistence();
        // First step accumulates emission.
        p.step_frame(absorbed, &mut acc, &t);
        let any_red = acc.iter().skip(BAND_VISIBLE_START + 1).any(|&v| v > 1e-3);
        assert!(any_red, "no accumulated emission");
        // Second step (no input) decays slightly.
        let acc_pre = acc;
        absorbed = [0.0_f32; BAND_COUNT];
        p.step_frame(absorbed, &mut acc, &t);
        let total_post: f32 = acc.iter().sum();
        let total_pre: f32 = acc_pre.iter().sum();
        assert!(total_post < total_pre);
    }

    /// § StokesShift::new clamp range.
    #[test]
    fn stokes_new() {
        let s = StokesShift::new(50.0, 0.5);
        assert_eq!(s.shift_nm, 50.0);
        assert!((s.quantum_yield - 0.5).abs() < 1e-6);
    }

    /// § Fluorescence with no shift maps to itself.
    #[test]
    fn no_shift_maps_to_self_band() {
        let t = BandTable::d65();
        let mut bands = [0.0_f32; BAND_COUNT];
        bands[BAND_VISIBLE_START + 4] = 1.0;
        let f = Fluorescence::new(StokesShift::new(0.0, 0.5));
        f.remap(&mut bands, &t);
        assert!((bands[BAND_VISIBLE_START + 4] - 1.0).abs() < 1e-6);
    }
}
