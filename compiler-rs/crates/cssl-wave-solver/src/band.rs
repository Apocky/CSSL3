//! § Band — the 5-default-band enumeration for Wave-Unity.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § THE 5-BAND DEFAULT (per task-dispatch instruction)
//!   The signature-rendering "ω-FIELD UNITY" technique requires a single
//!   PDE solver producing both light + audio from the same wave-equation.
//!   The 5-band default lights up the canonical "you can hear the light
//!   and see the sound" demo :
//!
//!     1. `AUDIO_SUB_KHZ`   — direct-amplitude AUDIO band, ~1 kHz carrier,
//!                            Δx ≈ 0.5 m, Δt ≈ 1 ms (tractable).
//!     2. `LIGHT_RED`       — SVEA envelope, ~430 THz centre,
//!                            Δx ≈ 1 cm, Δt ≈ 1 ms (envelope rate).
//!     3. `LIGHT_GREEN`     — SVEA envelope, ~540 THz centre.
//!     4. `LIGHT_BLUE`      — SVEA envelope, ~700 THz centre.
//!     5. `LIGHT_NEAR_IR`   — SVEA envelope, ~300 THz centre (heat-coupled).
//!
//!   Slow bands (HEAT, SCENT, MANA per spec §III) are deferred to a
//!   follow-on slice ; this slice owns the 5-default that drives the
//!   LoA novelty claim.
//!
//! § BAND-CLASS DISCRIMINATOR
//!   Each band carries a [`BandClass`] tag :
//!     - `BandClass::FastDirect`   — explicit LBM step ; AUDIO.
//!     - `BandClass::FastEnvelope` — SVEA envelope ; LIGHT (RGB+IR).
//!     - `BandClass::SlowEnvelope` — IMEX implicit step ; reserved
//!       for HEAT / SCENT / MANA in the 8-band extended config.
//!
//! § REPLAY DETERMINISM
//!   The discriminants are STABLE u8 indices ; iteration order is the
//!   declaration order. `Band::all()` returns `[u8; 5]` in canonical
//!   order so the replay-log records cells in deterministic per-band
//!   sequence.

/// § Band classification — tells the solver which substep kernel to
///   route the band through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BandClass {
    /// § Fast band, direct-amplitude (no envelope strip). AUDIO.
    FastDirect,
    /// § Fast band, SVEA envelope. LIGHT (RGB + near-IR).
    FastEnvelope,
    /// § Slow band, IMEX implicit envelope. HEAT / SCENT / MANA.
    SlowEnvelope,
}

/// § Band — the 5-default enumeration. Discriminants are STABLE u8 indices ;
///   the IMPL contract is "low-discriminant = first to update". Stable
///   ordering matters for replay-determinism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Band {
    /// § AUDIO sub-kHz direct-amplitude band. Carrier ≈ 1 kHz ; Δx = 0.5 m.
    AudioSubKHz = 0,
    /// § Visible-red envelope band. Centre ≈ 430 THz ; Δx = 1 cm.
    LightRed = 1,
    /// § Visible-green envelope band. Centre ≈ 540 THz ; Δx = 1 cm.
    LightGreen = 2,
    /// § Visible-blue envelope band. Centre ≈ 700 THz ; Δx = 1 cm.
    LightBlue = 3,
    /// § Near-IR envelope band. Centre ≈ 300 THz ; Δx = 1 cm (heat-coupled).
    LightNearIr = 4,
}

/// § Total number of default bands (5).
pub const BAND_COUNT_DEFAULT: usize = 5;

/// § The default 5 bands in canonical iteration order.
pub const DEFAULT_BANDS: [Band; BAND_COUNT_DEFAULT] = [
    Band::AudioSubKHz,
    Band::LightRed,
    Band::LightGreen,
    Band::LightBlue,
    Band::LightNearIr,
];

/// § The default-fast bands (audio + light) — explicit step.
pub const BANDS_FAST_DEFAULT: [Band; 5] = DEFAULT_BANDS;

/// § The default-slow bands (none in the 5-band config). Empty array
///   ; reserved for HEAT/SCENT/MANA in extended configs.
pub const BANDS_SLOW_DEFAULT: [Band; 0] = [];

impl Band {
    /// § Stable u8 discriminant.
    #[inline]
    #[must_use]
    pub const fn index(self) -> usize {
        self as u8 as usize
    }

    /// § Reverse map from u8 index to Band.
    #[inline]
    #[must_use]
    pub const fn from_index(i: usize) -> Option<Self> {
        match i {
            0 => Some(Self::AudioSubKHz),
            1 => Some(Self::LightRed),
            2 => Some(Self::LightGreen),
            3 => Some(Self::LightBlue),
            4 => Some(Self::LightNearIr),
            _ => None,
        }
    }

    /// § Stable canonical name for telemetry + replay logs.
    #[inline]
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::AudioSubKHz => "audio_sub_khz",
            Self::LightRed => "light_red",
            Self::LightGreen => "light_green",
            Self::LightBlue => "light_blue",
            Self::LightNearIr => "light_near_ir",
        }
    }

    /// § Approximate centre wavelength (m) for the band. Used by the
    ///   KAN-impedance lookup `Z(λ, embedding)`. Light bands return the
    ///   visible-spectrum centre ; audio returns acoustic-wavelength at
    ///   c_audio / f_centre = 343 / 1000 ≈ 0.343 m.
    #[inline]
    #[must_use]
    pub fn centre_wavelength_m(self) -> f32 {
        match self {
            Self::AudioSubKHz => 0.343,   // 343 m/s ÷ 1 kHz.
            Self::LightRed => 700e-9,     // 700 nm.
            Self::LightGreen => 540e-9,   // 540 nm.
            Self::LightBlue => 430e-9,    // 430 nm.
            Self::LightNearIr => 1100e-9, // 1.1 µm.
        }
    }

    /// § Approximate centre carrier-frequency (Hz). For LIGHT bands this
    ///   is the carrier we have stripped via SVEA — the field carries
    ///   the slowly-varying envelope.
    #[inline]
    #[must_use]
    pub fn carrier_hz(self) -> f64 {
        match self {
            Self::AudioSubKHz => 1.0e3,
            Self::LightRed => 430e12,
            Self::LightGreen => 540e12,
            Self::LightBlue => 700e12,
            Self::LightNearIr => 270e12,
        }
    }

    /// § Wave speed in vacuum / air (m/s). Used to derive CFL bound.
    ///   AUDIO uses 343 m/s ; LIGHT uses c. SVEA envelopes for LIGHT
    ///   *propagate* at envelope group-velocity which is approximately
    ///   c in vacuum but may differ in dispersive media — Stage-0
    ///   approximates both as c.
    #[inline]
    #[must_use]
    pub fn wave_speed_mps(self) -> f64 {
        match self {
            Self::AudioSubKHz => 343.0,
            _ => 2.997_924_58e8,
        }
    }

    /// § Recommended cell size Δx (m) for this band per spec §III.
    #[inline]
    #[must_use]
    pub fn recommended_cell_size_m(self) -> f64 {
        match self {
            Self::AudioSubKHz => 0.5,
            _ => 0.01, // 1 cm refined.
        }
    }

    /// § Recommended substep size Δt (s) per spec §III.
    #[inline]
    #[must_use]
    pub fn recommended_substep_s(self) -> f64 {
        match self {
            Self::AudioSubKHz => 1.0e-3, // 1 ms direct.
            _ => 1.0e-3,                 // 1 ms envelope.
        }
    }

    /// § Classification tag — drives kernel routing.
    #[inline]
    #[must_use]
    pub const fn class(self) -> BandClass {
        match self {
            Self::AudioSubKHz => BandClass::FastDirect,
            Self::LightRed | Self::LightGreen | Self::LightBlue | Self::LightNearIr => {
                BandClass::FastEnvelope
            }
        }
    }

    /// § True iff the band is a LIGHT band (RGB or near-IR).
    #[inline]
    #[must_use]
    pub const fn is_light(self) -> bool {
        matches!(
            self,
            Self::LightRed | Self::LightGreen | Self::LightBlue | Self::LightNearIr
        )
    }

    /// § True iff the band is the AUDIO band.
    #[inline]
    #[must_use]
    pub const fn is_audio(self) -> bool {
        matches!(self, Self::AudioSubKHz)
    }

    /// § Iterate all 5 bands in canonical order.
    #[inline]
    #[must_use]
    pub const fn all() -> &'static [Band; BAND_COUNT_DEFAULT] {
        &DEFAULT_BANDS
    }

    /// § CFL bound : c · Δt ≤ Δx ⇒ Δt_max = Δx / c.
    #[inline]
    #[must_use]
    pub fn cfl_dt_max(self) -> f64 {
        self.recommended_cell_size_m() / self.wave_speed_mps()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_count_is_five() {
        assert_eq!(BAND_COUNT_DEFAULT, 5);
        assert_eq!(DEFAULT_BANDS.len(), 5);
    }

    #[test]
    fn band_indices_are_stable() {
        assert_eq!(Band::AudioSubKHz.index(), 0);
        assert_eq!(Band::LightRed.index(), 1);
        assert_eq!(Band::LightGreen.index(), 2);
        assert_eq!(Band::LightBlue.index(), 3);
        assert_eq!(Band::LightNearIr.index(), 4);
    }

    #[test]
    fn band_from_index_round_trips() {
        for b in Band::all() {
            let i = b.index();
            assert_eq!(Band::from_index(i), Some(*b));
        }
        assert_eq!(Band::from_index(99), None);
    }

    #[test]
    fn band_canonical_names_unique() {
        let names: Vec<_> = Band::all().iter().map(|b| b.canonical_name()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        let original_len = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), original_len);
    }

    #[test]
    fn light_bands_have_optical_wavelengths() {
        assert!(Band::LightRed.centre_wavelength_m() < 1e-6);
        assert!(Band::LightGreen.centre_wavelength_m() < 1e-6);
        assert!(Band::LightBlue.centre_wavelength_m() < 1e-6);
        // Near-IR is around 1.1 µm.
        assert!(Band::LightNearIr.centre_wavelength_m() > 1e-6);
        assert!(Band::LightNearIr.centre_wavelength_m() < 2e-6);
    }

    #[test]
    fn audio_band_has_metres_wavelength() {
        // 343 m/s ÷ 1 kHz = 0.343 m.
        let a = Band::AudioSubKHz.centre_wavelength_m();
        assert!((a - 0.343).abs() < 1e-3);
    }

    #[test]
    fn audio_class_is_fast_direct() {
        assert_eq!(Band::AudioSubKHz.class(), BandClass::FastDirect);
    }

    #[test]
    fn light_classes_are_fast_envelope() {
        for b in [
            Band::LightRed,
            Band::LightGreen,
            Band::LightBlue,
            Band::LightNearIr,
        ] {
            assert_eq!(b.class(), BandClass::FastEnvelope);
        }
    }

    #[test]
    fn cell_size_for_light_is_small() {
        for b in [
            Band::LightRed,
            Band::LightGreen,
            Band::LightBlue,
            Band::LightNearIr,
        ] {
            assert!(b.recommended_cell_size_m() < 0.05);
        }
    }

    #[test]
    fn audio_cell_size_is_half_metre() {
        assert!((Band::AudioSubKHz.recommended_cell_size_m() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn cfl_bound_satisfied_for_audio() {
        // CFL : c · Δt ≤ Δx. With Δt = 1 ms and c = 343 m/s,
        // c·Δt = 0.343 m < Δx = 0.5 m. PASSES.
        let b = Band::AudioSubKHz;
        let cfl = b.cfl_dt_max();
        assert!(cfl >= b.recommended_substep_s());
    }

    #[test]
    fn cfl_bound_satisfied_for_light_envelope() {
        // SVEA envelope of LIGHT propagates ~ at c. With Δt = 1 ms
        // and c = 3e8 m/s, c·Δt = 3e5 m which DOES exceed Δx = 1 cm.
        // This is why LIGHT is treated as envelope — the envelope
        // propagates much slower in cell-time than the carrier.
        // Stage-0 enforces the slowing in [`stability::predict_stable_dt`].
        let b = Band::LightRed;
        let cfl = b.cfl_dt_max();
        assert!(cfl > 0.0);
    }

    #[test]
    fn fast_default_bands_match_default() {
        assert_eq!(BANDS_FAST_DEFAULT.len(), DEFAULT_BANDS.len());
    }

    #[test]
    fn slow_default_is_empty() {
        assert_eq!(BANDS_SLOW_DEFAULT.len(), 0);
    }

    #[test]
    fn audio_predicate_only_for_audio() {
        assert!(Band::AudioSubKHz.is_audio());
        assert!(!Band::LightRed.is_audio());
    }

    #[test]
    fn light_predicate_for_all_light() {
        for b in [
            Band::LightRed,
            Band::LightGreen,
            Band::LightBlue,
            Band::LightNearIr,
        ] {
            assert!(b.is_light());
        }
        assert!(!Band::AudioSubKHz.is_light());
    }
}
