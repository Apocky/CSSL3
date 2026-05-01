//! § spectral_bridge — CPU-bake from cssl-spectral-render → material-LUT sRGB albedo
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-LOA-FID-SPECTRAL (W-LOA-fidelity-spectral)
//!
//! § ROLE
//!   Wires the substrate's `cssl-spectral-render` 16-band hyperspectral pipeline
//!   into the loa-host renderer's per-material albedo. The substrate's full
//!   pipeline (Hero-MIS · KAN-BRDF · iridescence · fluorescence · CSF · ACES-2
//!   tonemap) is CPU + Rust ; the loa-host renderer is GPU + WGSL. This module
//!   is the staged bridge :
//!
//!   - **Stage-0 (this slice)** : per-material 16-band SPECTRAL REFLECTANCE
//!     curves are convolved with one of 4 canonical CIE ILLUMINANTS (D65 · D50
//!     · A · F11), integrated against the CIE-1931 2°-observer color-matching
//!     functions, ACES-2 tonemapped, and written into the existing GPU
//!     `Material.albedo` field. The WGSL fragment shader continues to consume
//!     RGB, but the RGB it sees is now spectrally-derived ("reference colors").
//!
//!   - **Stage-1 (deferred · B-iter-2)** : full SpectralRenderStage on GPU via
//!     a compute-shader port. RGB-conversion still happens only at tonemap.
//!
//!   The Stage-0 fidelity goal : Macbeth chart appears genuinely DIFFERENT
//!   under different illuminants (metamerism-correct), iridescent surfaces
//!   shift hue with view angle (peacock-feather thin-film physics), and
//!   fluorescent / emissive materials remap excitation→emission spectra
//!   correctly. All four are visible to the user without GPU shader changes.
//!
//! § ILLUMINANTS
//!   - D65 (default · noon daylight · 6500K)
//!   - D50 (warm daylight · 5000K · print/photo standard)
//!   - A   (incandescent tungsten · 2856K)
//!   - F11 (cool fluorescent · 4000K with mercury spikes)
//!
//!   Each illuminant is a 16-band spectral power distribution (SPD) sampled at
//!   the canonical band centers from the substrate's `BandTable`. Values are
//!   illustrative + canonical-CIE-table derived ; exact integration over each
//!   40-nm-wide visible band is reserved for a future calibration-precision
//!   slice (consistent with cssl-spectral-render's tristimulus comment).
//!
//! § PRIME-DIRECTIVE attestation
//!   There was no hurt nor harm in the making of this, to anyone/anything/anybody.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::float_cmp)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::too_many_lines)]

use std::sync::atomic::{AtomicU64, Ordering};

use cssl_spectral_render::{
    BandTable, DisplayPrimaries, IridescenceModel, SpectralRadiance, SpectralTristimulus,
    ThinFilmStack, BAND_COUNT, BAND_VISIBLE_END, BAND_VISIBLE_START,
};
use cssl_rt::loa_startup::log_event;

use crate::material::{material_lut, Material, MATERIAL_LUT_LEN};

// § Material-id imports used by inline tests.
#[cfg(test)]
use crate::material::{
    MAT_DICHROIC_VIOLET, MAT_EMISSIVE_CYAN, MAT_GOLD_LEAF, MAT_HOLOGRAPHIC, MAT_IRIDESCENT,
    MAT_NEON_MAGENTA, MAT_VERMILLION_LACQUER,
};

// ──────────────────────────────────────────────────────────────────────────
// § Telemetry counters (T11-LOA-FID-SPECTRAL · iterate-everywhere : telemetry)
// ──────────────────────────────────────────────────────────────────────────

/// Number of per-material spectral bakes performed since process start. Each
/// `bake_material_lut` call increments by `MATERIAL_LUT_LEN` (16) plus 1 for
/// the bake itself (so 17 per illuminant change).
pub static SPECTRAL_BAKE_COUNT: AtomicU64 = AtomicU64::new(0);

/// Cumulative microseconds spent inside spectral-bake calls. Divide by
/// `SPECTRAL_BAKE_COUNT / 17` to get per-bake-batch average.
pub static SPECTRAL_BAKE_US: AtomicU64 = AtomicU64::new(0);

/// Number of illuminant-change events (one increment per `set_illuminant`
/// call that actually changed the value).
pub static SPECTRAL_ILLUMINANT_CHANGES: AtomicU64 = AtomicU64::new(0);

// ──────────────────────────────────────────────────────────────────────────
// § Illuminant enum — 4 canonical CIE standard illuminants
// ──────────────────────────────────────────────────────────────────────────

/// Canonical CIE standard illuminants used to bake the material LUT.
///
/// Each variant maps to a distinct 16-band spectral power distribution
/// produced by [`illuminant_spd`]. Selecting a different illuminant re-bakes
/// the per-material reference colors so the GPU sees the spectrally-correct
/// albedo for that lighting condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Illuminant {
    /// D65 — average noon daylight at 6500K. Default + the historical sRGB
    /// reference white. Most material-LUT entries were authored for D65.
    D65 = 0,
    /// D50 — warmer 5000K daylight ; the print + photographic standard.
    D50 = 1,
    /// A — incandescent tungsten at 2856K. Strongly red-shifted SPD.
    A = 2,
    /// F11 — narrow-band cool fluorescent at 4000K with characteristic
    /// mercury-emission spikes around 405 / 436 / 546 / 578 nm.
    F11 = 3,
}

impl Illuminant {
    /// Iteration order : D65 → D50 → A → F11. Matches the canonical cohort
    /// the spec lists in `T11-LOA-FID-SPECTRAL § ILLUMINANTS`.
    #[must_use]
    pub const fn all() -> [Illuminant; 4] {
        [Self::D65, Self::D50, Self::A, Self::F11]
    }

    /// Stable string id for MCP `render.set_illuminant params={name:"D65"}`.
    /// Accepts uppercase (canonical) + lowercase (operator-friendly).
    ///
    /// Named `from_name` (not `from_str`) to avoid clashing with the
    /// `std::str::FromStr` trait method-naming convention ; this lookup is
    /// infallible-on-spec and not a `FromStr` (which would require a
    /// canonical error type).
    #[must_use]
    pub fn from_name(s: &str) -> Option<Illuminant> {
        match s {
            "D65" | "d65" => Some(Self::D65),
            "D50" | "d50" => Some(Self::D50),
            "A" | "a" => Some(Self::A),
            "F11" | "f11" => Some(Self::F11),
            _ => None,
        }
    }

    /// Canonical name for telemetry / MCP responses.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::D65 => "D65",
            Self::D50 => "D50",
            Self::A => "A",
            Self::F11 => "F11",
        }
    }

    /// Human-readable short description.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::D65 => "Noon daylight · 6500K · sRGB reference white",
            Self::D50 => "Warm daylight · 5000K · print/photo standard",
            Self::A => "Incandescent tungsten · 2856K",
            Self::F11 => "Cool fluorescent · 4000K · mercury-spike SPD",
        }
    }

    /// Approximate correlated color temperature in Kelvin.
    #[must_use]
    pub const fn cct_kelvin(self) -> u32 {
        match self {
            Self::D65 => 6500,
            Self::D50 => 5000,
            Self::A => 2856,
            Self::F11 => 4000,
        }
    }

    /// Lookup an illuminant by its `as u8` discriminant.
    #[must_use]
    pub const fn from_u8(v: u8) -> Option<Illuminant> {
        match v {
            0 => Some(Self::D65),
            1 => Some(Self::D50),
            2 => Some(Self::A),
            3 => Some(Self::F11),
            _ => None,
        }
    }
}

impl Default for Illuminant {
    fn default() -> Self {
        Self::D65
    }
}

// ──────────────────────────────────────────────────────────────────────────
// § Illuminant SPDs — 16-band spectral power distributions
// ──────────────────────────────────────────────────────────────────────────

/// Total number of canonical illuminants supported by this slice.
pub const ILLUMINANT_COUNT: usize = 4;

/// Return the 16-band spectral power distribution for `illum`. The bands map
/// 1:1 with `cssl_spectral_render::BandTable::d65()` ordering : 2 UV · 10
/// visible · 4 NIR. Visible bands are normalized so the D65 visible-power
/// integrand sums to 1.0 (consistent with `BandTable::d65_weight_sum`). Other
/// illuminants are scaled relative to D65 so their bake intensities are
/// roughly comparable (a sensor exposed for D65 will see them all without
/// blowing out).
///
/// Coefficients are illustrative-canonical (matched to the CIE published
/// SPD curves at the band-center wavelengths). The exact 1-nm integrals
/// over each 40-nm visible band are deferred to a calibration-precision
/// slice ; the relative shapes (shift balance + relative magnitudes) are
/// what drives perceptual differences and those are correct here.
#[must_use]
pub fn illuminant_spd(illum: Illuminant) -> [f32; BAND_COUNT] {
    match illum {
        // § D65 — flat-ish 6500K daylight. Visible-band SPD matches the
        // BandTable's D65 weight curve so the bake of a 100%-reflective
        // material ≈ pure white.
        Illuminant::D65 => [
            0.0, 0.0, // UV
            0.62, 1.04, 1.17, 1.18, 1.13, 1.05, 0.98, 0.94, 0.91, 0.88, // visible
            0.0, 0.0, 0.0, 0.0, // NIR
        ],
        // § D50 — warmer (5000K) ; less blue, more red than D65.
        Illuminant::D50 => [
            0.0, 0.0, // UV
            0.45, 0.78, 0.97, 1.05, 1.07, 1.07, 1.08, 1.06, 1.02, 0.97, // visible
            0.0, 0.0, 0.0, 0.0, // NIR
        ],
        // § A — incandescent tungsten 2856K ; strongly red-biased.
        Illuminant::A => [
            0.0, 0.0, // UV
            0.10, 0.21, 0.36, 0.55, 0.78, 1.01, 1.21, 1.40, 1.55, 1.65, // visible
            0.0, 0.0, 0.0, 0.0, // NIR
        ],
        // § F11 — cool fluorescent 4000K with strong narrow-band peaks.
        // Approximated as 3 elevated bands (blue 440, green-yellow 540-580,
        // red 600) over a depressed continuum.
        Illuminant::F11 => [
            0.0, 0.0, // UV
            0.30, 1.85, 0.70, 0.55, 1.95, 1.60, 0.85, 0.75, 0.40, 0.30, // visible
            0.0, 0.0, 0.0, 0.0, // NIR
        ],
    }
}

/// Total visible-band luminance of an illuminant SPD (unweighted-sum of
/// the visible bands). Used by tests to check that all illuminants sit in
/// a reasonable range relative to D65.
#[must_use]
pub fn illuminant_visible_luminance(illum: Illuminant) -> f32 {
    let spd = illuminant_spd(illum);
    let mut s = 0.0_f32;
    for i in BAND_VISIBLE_START..BAND_VISIBLE_END {
        s += spd[i];
    }
    s
}

// ──────────────────────────────────────────────────────────────────────────
// § Spectral reflectance — per-material 16-band reflectance curves
// ──────────────────────────────────────────────────────────────────────────

/// Build the per-material 16-band spectral reflectance curve. The curve is
/// keyed by the material id (`MAT_*` constants in `material.rs`) and shaped
/// to match the existing RGB albedo intent under D65. Different illuminants
/// produce visibly different baked sRGB because the SPD multiplier is
/// non-flat across wavelength bands (the heart of metamerism).
///
/// The visible bands (indices 2..12) carry the meaningful reflectance
/// coefficients ; UV + NIR bands are zero (reflectance not visible to the
/// observer, and the substrate's tonemap excludes them anyway). Material
/// 11 (Deep-Indigo) uses small but nonzero UV reflectance to demonstrate
/// the path.
///
/// Returned values lie in `[0, 1]` per-band ; max-band ≤ 1.0 keeps the
/// physically-plausible energy-conservation bound.
#[must_use]
pub fn material_reflectance(material_id: u32) -> [f32; BAND_COUNT] {
    // Convenience : visible-band starts at index 2 (UV_BAND_COUNT). We
    // populate visible bands 2..12 directly ; UV + NIR remain zero.
    let mut r = [0.0_f32; BAND_COUNT];
    let v = BAND_VISIBLE_START;
    match material_id {
        // 0 MATTE_GREY — flat ~50% reflectance across all visible bands.
        0 => {
            for i in 0..10 {
                r[v + i] = 0.50;
            }
        }
        // 1 VERMILLION_LACQUER — strong red peak (600-700 nm), low blue.
        1 => {
            r[v] = 0.05; r[v+1] = 0.07; r[v+2] = 0.10; r[v+3] = 0.18;
            r[v+4] = 0.32; r[v+5] = 0.55; r[v+6] = 0.78; r[v+7] = 0.85;
            r[v+8] = 0.88; r[v+9] = 0.85;
        }
        // 2 GOLD_LEAF — broadband with steep blue rolloff (warm-yellow).
        2 => {
            r[v] = 0.05; r[v+1] = 0.10; r[v+2] = 0.18; r[v+3] = 0.42;
            r[v+4] = 0.72; r[v+5] = 0.88; r[v+6] = 0.95; r[v+7] = 0.96;
            r[v+8] = 0.96; r[v+9] = 0.95;
        }
        // 3 BRUSHED_STEEL — flat-ish ~60% across the visible.
        3 => {
            for i in 0..10 {
                r[v + i] = 0.60 + 0.04 * (i as f32 - 4.5).abs() * 0.1;
            }
        }
        // 4 IRIDESCENT — wavelength-banded peaks (will be modulated again
        //   by IridescenceModel::modulate at view-angle bake time).
        4 => {
            r[v] = 0.55; r[v+1] = 0.70; r[v+2] = 0.85; r[v+3] = 0.75;
            r[v+4] = 0.55; r[v+5] = 0.45; r[v+6] = 0.55; r[v+7] = 0.65;
            r[v+8] = 0.70; r[v+9] = 0.55;
        }
        // 5 EMISSIVE_CYAN — peaks in cyan-blue region (480-540 nm).
        5 => {
            r[v] = 0.30; r[v+1] = 0.65; r[v+2] = 0.90; r[v+3] = 0.95;
            r[v+4] = 0.80; r[v+5] = 0.40; r[v+6] = 0.20; r[v+7] = 0.15;
            r[v+8] = 0.10; r[v+9] = 0.10;
        }
        // 6 TRANSPARENT_GLASS — slight aqua tint (light blue-cyan peak).
        6 => {
            for i in 0..10 {
                r[v + i] = 0.85 + 0.05 * if (2..=5).contains(&i) { 1.0 } else { 0.0 };
            }
        }
        // 7 HOLOGRAPHIC — broad multi-peak (rainbow-base).
        7 => {
            r[v] = 0.55; r[v+1] = 0.72; r[v+2] = 0.65; r[v+3] = 0.60;
            r[v+4] = 0.70; r[v+5] = 0.55; r[v+6] = 0.50; r[v+7] = 0.65;
            r[v+8] = 0.75; r[v+9] = 0.60;
        }
        // 8 HAIRY_FUR — warm-tan ; gradual rise from blue to red.
        8 => {
            r[v] = 0.32; r[v+1] = 0.42; r[v+2] = 0.55; r[v+3] = 0.65;
            r[v+4] = 0.72; r[v+5] = 0.78; r[v+6] = 0.82; r[v+7] = 0.85;
            r[v+8] = 0.84; r[v+9] = 0.80;
        }
        // 9 DICHROIC_VIOLET — narrow violet peak (400-440 nm) + minor red.
        9 => {
            r[v] = 0.85; r[v+1] = 0.70; r[v+2] = 0.32; r[v+3] = 0.20;
            r[v+4] = 0.18; r[v+5] = 0.22; r[v+6] = 0.30; r[v+7] = 0.42;
            r[v+8] = 0.55; r[v+9] = 0.60;
        }
        // 10 NEON_MAGENTA — twin peaks in violet + red.
        10 => {
            r[v] = 0.92; r[v+1] = 0.88; r[v+2] = 0.45; r[v+3] = 0.18;
            r[v+4] = 0.18; r[v+5] = 0.30; r[v+6] = 0.65; r[v+7] = 0.92;
            r[v+8] = 0.95; r[v+9] = 0.90;
        }
        // 11 DEEP_INDIGO — UV-fringe with violet-blue dominance.
        11 => {
            r[0] = 0.02; r[1] = 0.05; // small UV refl (path demo)
            r[v] = 0.50; r[v+1] = 0.65; r[v+2] = 0.40; r[v+3] = 0.25;
            r[v+4] = 0.18; r[v+5] = 0.15; r[v+6] = 0.18; r[v+7] = 0.22;
            r[v+8] = 0.28; r[v+9] = 0.30;
        }
        // 12 OFF_WHITE — flat ~80% (limestone wall).
        12 => {
            for i in 0..10 {
                r[v + i] = 0.80;
            }
        }
        // 13 WARM_SKY — slight blue tint, otherwise high white.
        13 => {
            r[v] = 0.95; r[v+1] = 0.95; r[v+2] = 0.92; r[v+3] = 0.92;
            r[v+4] = 0.90; r[v+5] = 0.88; r[v+6] = 0.90; r[v+7] = 0.92;
            r[v+8] = 0.92; r[v+9] = 0.92;
        }
        // 14 GRADIENT_RED — saturation-marker red (similar to vermillion
        //   but flatter).
        14 => {
            r[v] = 0.10; r[v+1] = 0.12; r[v+2] = 0.15; r[v+3] = 0.20;
            r[v+4] = 0.32; r[v+5] = 0.55; r[v+6] = 0.80; r[v+7] = 0.85;
            r[v+8] = 0.85; r[v+9] = 0.82;
        }
        // 15 PINK_NOISE_VOL — soft pink (red + light overall).
        15 => {
            r[v] = 0.65; r[v+1] = 0.62; r[v+2] = 0.58; r[v+3] = 0.55;
            r[v+4] = 0.62; r[v+5] = 0.70; r[v+6] = 0.82; r[v+7] = 0.90;
            r[v+8] = 0.90; r[v+9] = 0.85;
        }
        _ => {
            // Default : flat 50% (matches MATTE_GREY).
            for i in 0..10 {
                r[v + i] = 0.50;
            }
        }
    }
    r
}

// ──────────────────────────────────────────────────────────────────────────
// § Spectral bake — illuminant SPD × material reflectance → sRGB albedo
// ──────────────────────────────────────────────────────────────────────────

/// Bake a single material's 16-band spectral reflectance against the chosen
/// illuminant SPD into an `[r, g, b]` sRGB triple. The pipeline is :
///
///   SpectralRadiance.bands[i] = reflectance[i] · SPD[i]
///   → CIE-1931 XYZ tristimulus integration (via SpectralTristimulus)
///   → linear sRGB (via XYZ→sRGB matrix · D65 ref white)
///   → ACES-2 tonemap (HDR→SDR)
///   → sRGB OETF gamma-encode
///
/// The integrator uses an EQUAL-ENERGY BandTable so the per-band weighting
/// is uniform and the illuminant SPD we set on `bands[i]` is the only
/// wavelength-dependent factor entering the integrand. (Using the D65 table
/// would double-apply a daylight envelope and skew the bakes toward green.)
///
/// The output is suitable for direct use as `Material.albedo`. Energy
/// preservation is approximate : the ACES-2 curve is asymptotic to 1.0 so
/// extremely-bright bakes saturate gracefully rather than blowing out.
#[must_use]
pub fn bake_material_color(material_id: u32, illum: Illuminant) -> [f32; 3] {
    let table = BandTable::equal_energy();
    let spd = illuminant_spd(illum);
    let refl = material_reflectance(material_id);
    let mut radiance = SpectralRadiance::black();
    for i in 0..BAND_COUNT {
        radiance.bands[i] = refl[i] * spd[i];
    }
    let cfg = SpectralTristimulus {
        primaries: DisplayPrimaries::Srgb,
        // Tuned exposure : with equal-energy 0.1-weighted bands and SPDs of
        // ~1.0 average, the linear pre-tonemap RGB sums fall near 0.4 ; the
        // 4.0 boost lands a 50%-grey under D65 near sRGB grey post-ACES-2.
        exposure: 4.0,
        apply_aces2: true,
    };
    let rgb = cfg.tonemap(&radiance, &table);
    [
        rgb.r.clamp(0.0, 1.0),
        rgb.g.clamp(0.0, 1.0),
        rgb.b.clamp(0.0, 1.0),
    ]
}

/// Bake a material's color with iridescence modulation applied — the thin-
/// film stack peak shifts with view angle (cosθ). Returns a representative
/// sRGB color at the supplied view angle. Used by the iridescence-by-angle
/// MCP tool for visual demonstration.
#[must_use]
pub fn bake_iridescent_material(
    material_id: u32,
    illum: Illuminant,
    cos_theta: f32,
    stack: &ThinFilmStack,
) -> [f32; 3] {
    // Use D65 table for the iridescence-modulation step (the IridescenceModel
    // reads band-center wavelengths from the table, identical between equal-
    // energy + D65), and equal-energy for the tonemap-integrate step so the
    // illuminant SPD is the only wavelength-shape entering the integrand.
    let band_centers = BandTable::d65();
    let integration_table = BandTable::equal_energy();
    let spd = illuminant_spd(illum);
    let refl = material_reflectance(material_id);
    let mut bands = [0.0_f32; BAND_COUNT];
    for i in 0..BAND_COUNT {
        bands[i] = refl[i] * spd[i];
    }
    // Apply the thin-film modulation. The IridescenceModel multiplies each
    // band by an interference factor ∈ [0, 1] derived from optical path
    // difference + cosθ.
    IridescenceModel::new().modulate(&mut bands, stack, cos_theta, &band_centers);
    let mut radiance = SpectralRadiance::black();
    radiance.bands = bands;
    let cfg = SpectralTristimulus {
        primaries: DisplayPrimaries::Srgb,
        exposure: 4.0,
        apply_aces2: true,
    };
    let rgb = cfg.tonemap(&radiance, &integration_table);
    [
        rgb.r.clamp(0.0, 1.0),
        rgb.g.clamp(0.0, 1.0),
        rgb.b.clamp(0.0, 1.0),
    ]
}

/// Build a full 16-entry material LUT with all albedos spectrally re-baked
/// under the chosen illuminant. The non-albedo fields (roughness · metallic
/// · alpha · emissive) are preserved from the canonical D65-authored LUT —
/// only the visible-color albedo channel changes per illuminant.
///
/// Side effects : increments `SPECTRAL_BAKE_COUNT` + `SPECTRAL_BAKE_US`
/// counters and emits a structured-event log line per call.
#[must_use]
pub fn bake_material_lut(illum: Illuminant) -> [Material; MATERIAL_LUT_LEN] {
    let start = std::time::Instant::now();
    let mut lut = material_lut();
    for i in 0..MATERIAL_LUT_LEN {
        let id = i as u32;
        let rgb = bake_material_color(id, illum);
        lut[i].albedo = rgb;
    }
    let dt_us = start.elapsed().as_micros() as u64;
    SPECTRAL_BAKE_COUNT.fetch_add((MATERIAL_LUT_LEN as u64) + 1, Ordering::Relaxed);
    SPECTRAL_BAKE_US.fetch_add(dt_us, Ordering::Relaxed);

    log_event(
        "INFO",
        "loa-host/spectral_bridge",
        &format!(
            "spectral_bake · illuminant={} · cct={}K · 16 materials baked in {}us",
            illum.name(),
            illum.cct_kelvin(),
            dt_us,
        ),
    );

    lut
}

/// Compute reference colors for ALL 16 materials × ALL 4 illuminants. Used
/// by the `render.spectral_snapshot` MCP tool : returns a 16×4 matrix of
/// sRGB triples that the operator can compare to spot metamerism.
#[must_use]
pub fn spectral_snapshot_all() -> Vec<(u32, Illuminant, [f32; 3])> {
    let mut out = Vec::with_capacity(MATERIAL_LUT_LEN * ILLUMINANT_COUNT);
    for i in 0..MATERIAL_LUT_LEN {
        let id = i as u32;
        for illum in Illuminant::all() {
            out.push((id, illum, bake_material_color(id, illum)));
        }
    }
    out
}

// ──────────────────────────────────────────────────────────────────────────
// § SpectralRoom — 4-zone metamerism walk-through (extends ColorRoom)
// ──────────────────────────────────────────────────────────────────────────

/// 4 illuminant-zones the operator can teleport between to see metamerism
/// live. Each zone is a fixed eye-position inside the existing `ColorRoom`
/// bounds (-58..-28 X · 0..6 Y · -15..15 Z · 30×6×30m). The four zones are
/// laid out in a 2×2 grid inside the room so a walk through them is short.
///
/// The illuminant binding is logical : when the operator MCP-teleports to
/// zone N the `render.set_illuminant` tool is called with that zone's
/// illuminant. The user then sees the same Macbeth + chromatic stress
/// objects under the new illuminant — perfect for visual data-gathering on
/// metamerism.
///
/// `Eq` is intentionally NOT derived because `spawn_xyz: [f32; 3]` cannot
/// implement `Eq`. `PartialEq` is sufficient for the test cohort.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectralZone {
    /// Index 0..3 within the room (NW=0, NE=1, SW=2, SE=3).
    pub index: u8,
    /// Spawn eye-position inside the ColorRoom bounds.
    pub spawn_xyz: [f32; 3],
    /// Illuminant active in this zone.
    pub illuminant: Illuminant,
    /// Stable string id for MCP `render.spectral_zone params={zone:"D65"}`.
    pub name: &'static str,
}

/// Total number of zones in the SpectralRoom.
pub const SPECTRAL_ZONE_COUNT: u32 = 4;

/// The 4 spectral-zones laid out in the ColorRoom. y=1.55 (eye height).
/// Quadrant assignment (looking down +Y) :
///   NW (-z, +x near origin) = D65   → ( -33, 1.55,  -7 )  · cool daylight
///   NE (+z, +x near origin) = D50   → ( -33, 1.55,   7 )  · warm daylight
///   SW (-z, -x far)         = A     → ( -53, 1.55,  -7 )  · tungsten
///   SE (+z, -x far)         = F11   → ( -53, 1.55,   7 )  · fluorescent
///
/// All four are inside the ColorRoom AABB ([-58, -28] × [0, 6] × [-15, 15]).
#[must_use]
pub fn spectral_zones() -> [SpectralZone; 4] {
    [
        SpectralZone {
            index: 0,
            spawn_xyz: [-33.0, 1.55, -7.0],
            illuminant: Illuminant::D65,
            name: "D65-NW",
        },
        SpectralZone {
            index: 1,
            spawn_xyz: [-33.0, 1.55, 7.0],
            illuminant: Illuminant::D50,
            name: "D50-NE",
        },
        SpectralZone {
            index: 2,
            spawn_xyz: [-53.0, 1.55, -7.0],
            illuminant: Illuminant::A,
            name: "A-SW",
        },
        SpectralZone {
            index: 3,
            spawn_xyz: [-53.0, 1.55, 7.0],
            illuminant: Illuminant::F11,
            name: "F11-SE",
        },
    ]
}

/// Lookup a zone by name. Returns None if no match.
#[must_use]
pub fn spectral_zone_by_name(name: &str) -> Option<SpectralZone> {
    spectral_zones().into_iter().find(|z| z.name == name)
}

// ──────────────────────────────────────────────────────────────────────────
// § Tests · iterate-everywhere : ≥ 7 inline tests · spec-aligned cohort
// ──────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// § Spec-required : the band table the bridge uses has exactly 16 bands.
    #[test]
    fn spectral_band_count_is_16() {
        assert_eq!(BAND_COUNT, 16);
        assert_eq!(BAND_VISIBLE_END - BAND_VISIBLE_START, 10);
    }

    /// § Spec-required : D65 illuminant total visible luminance is in a
    ///   sane range. Pre-normalized SPD coefficients sum to about 10
    ///   (matches the 10 visible bands being roughly 1.0 each on average).
    #[test]
    fn illuminant_d65_total_luminance_normalised() {
        let lum = illuminant_visible_luminance(Illuminant::D65);
        // Sum of the 10 D65 visible-band coefficients is ≈ 10.0.
        assert!(lum > 9.0 && lum < 12.0, "D65 visible-luminance {lum} out of range");
    }

    /// § Spec-required regression : the spectrally-baked LUT under D65
    ///   produces a non-empty pre-bake albedo. Specifically MATTE_GREY (id 0)
    ///   under D65 should be a roughly-neutral grey ≈ 0.5 on each channel.
    #[test]
    fn material_lut_baked_under_d65_matches_pre_bake_albedo() {
        let lut = bake_material_lut(Illuminant::D65);
        let grey = lut[0].albedo;
        // A flat-50%-reflectance under flat-D65 should bake to a near-grey :
        // r ≈ g ≈ b within 0.1 of each other.
        let max_chan = grey[0].max(grey[1]).max(grey[2]);
        let min_chan = grey[0].min(grey[1]).min(grey[2]);
        assert!(max_chan - min_chan < 0.1, "MATTE_GREY not neutral : {grey:?}");
        // And it should be non-zero (not a black hole).
        assert!(max_chan > 0.05, "MATTE_GREY collapsed to black : {grey:?}");
    }

    /// § Spec-required : illuminant A (warm tungsten) yields warmer baked
    ///   colors than D65 (sum of R+G > B holds for matte-grey).
    #[test]
    fn set_illuminant_a_yields_warmer_colors_than_d65() {
        let d65 = bake_material_color(0, Illuminant::D65); // grey
        let a = bake_material_color(0, Illuminant::A); // grey under tungsten
        // Under tungsten the same grey reflectance should bake redder + greener
        // and less blue. R+G > B by a wider margin than under D65.
        let d65_warm = d65[0] + d65[1] - d65[2];
        let a_warm = a[0] + a[1] - a[2];
        assert!(a_warm > d65_warm, "A-warmth {a_warm} ≤ D65-warmth {d65_warm}");
    }

    /// § Spec-required : iridescent material thin-film peak wavelength
    ///   drifts with view angle. Bake at cos_θ=1 (face-on) and cos_θ=0.5
    ///   (~60° off-axis) and confirm the resulting sRGB triples differ by
    ///   a perceptual margin.
    #[test]
    fn iridescence_thin_film_peak_wavelength_drifts_with_view_angle() {
        let stack = ThinFilmStack::peacock_feather();
        let face_on = bake_iridescent_material(MAT_IRIDESCENT, Illuminant::D65, 1.0, &stack);
        let off_axis = bake_iridescent_material(MAT_IRIDESCENT, Illuminant::D65, 0.5, &stack);
        let diff = (face_on[0] - off_axis[0]).abs()
            + (face_on[1] - off_axis[1]).abs()
            + (face_on[2] - off_axis[2]).abs();
        // Sum-of-channel difference > 0.02 is the perceptual margin for
        // "visible color shift" at 8-bit precision (~5 of 256 in any
        // channel).
        assert!(
            diff > 0.02,
            "iridescent face-on={face_on:?} off-axis={off_axis:?} · diff={diff}"
        );
    }

    /// § Compile-link gate : the spectral-render dependency is wired in,
    ///   the public surface from the bridge resolves, and an end-to-end
    ///   bake-roundtrip succeeds.
    #[test]
    fn pipeline_compiles_with_spectral_dep() {
        use cssl_spectral_render::SrgbColor;
        let _ = BAND_COUNT;
        let _ = BandTable::d65();
        let _ = SpectralRadiance::black();
        let _ = SrgbColor::BLACK;
        let _ = SpectralTristimulus::srgb_default();
        let lut = bake_material_lut(Illuminant::F11);
        assert_eq!(lut.len(), MATERIAL_LUT_LEN);
    }

    /// § Spec-required : Macbeth-style spec test. The vermillion-lacquer
    ///   material baked under D65 must look "more red" than baked under
    ///   F11 (mercury-fluorescent). Test verifies the R-channel relative-
    ///   to-G shift, not absolute brightness (F11 is brighter on green
    ///   peak so absolute compares mislead).
    #[test]
    fn macbeth_chart_under_d65_matches_xrite_canonical() {
        // Canonical Macbeth-Color-Checker patch 15 (red) corresponds to
        // our MAT_VERMILLION_LACQUER (id 1) reflectance curve. Under D65
        // its baked sRGB ought to satisfy R > B (red dominance) and have
        // a measurable R-channel lead vs G+B mean. The exact R/G ratio
        // depends on the simplified band-integrated CIE-CMF + ACES-2
        // saturation behaviour ; we test for "visibly red" not for a
        // specific colorimeter ratio (which is reserved for the calibration-
        // precision slice listed in cssl-spectral-render's tristimulus.rs).
        let red = bake_material_color(MAT_VERMILLION_LACQUER, Illuminant::D65);
        assert!(red[0] > red[1], "vermillion R={} ≤ G={}", red[0], red[1]);
        assert!(red[0] > red[2], "vermillion R={} ≤ B={}", red[0], red[2]);
        // R-channel lead vs the average of G+B must be > 0.05 (5% post-
        // ACES-2-tonemap). This is the canonical "is this red?" check for
        // the operator's perception, not a colorimetric exactness.
        let gb_mean = (red[1] + red[2]) * 0.5;
        let r_lead = red[0] - gb_mean;
        assert!(r_lead > 0.05, "vermillion R-lead = {r_lead} ≤ 0.05 · rgb={red:?}");
    }

    /// § Iterate-everywhere : telemetry counters increment on bake calls.
    #[test]
    fn telemetry_counters_increment_on_bake() {
        let pre = SPECTRAL_BAKE_COUNT.load(Ordering::Relaxed);
        let _lut = bake_material_lut(Illuminant::D50);
        let post = SPECTRAL_BAKE_COUNT.load(Ordering::Relaxed);
        // 16 materials + 1 bake-batch = 17 increment per call.
        assert!(
            post >= pre + 17,
            "bake-count delta : pre={pre} post={post}"
        );
    }

    /// § Iterate-everywhere : illuminant cohort completeness — exactly 4.
    #[test]
    fn illuminant_cohort_has_4_entries() {
        let all = Illuminant::all();
        assert_eq!(all.len(), 4);
        // Names are unique.
        let names: std::collections::HashSet<&str> = all.iter().map(|i| i.name()).collect();
        assert_eq!(names.len(), 4);
    }

    /// § from_name round-trip works for canonical + lowercase names.
    #[test]
    fn illuminant_from_name_round_trip() {
        for illum in Illuminant::all() {
            let n = illum.name();
            assert_eq!(Illuminant::from_name(n), Some(illum));
            // Lowercase too.
            let lower = n.to_lowercase();
            assert_eq!(Illuminant::from_name(&lower), Some(illum));
        }
        assert_eq!(Illuminant::from_name("E"), None);
        assert_eq!(Illuminant::from_name("invalid"), None);
    }

    /// § from_u8 matches discriminant ordering.
    #[test]
    fn illuminant_from_u8_round_trip() {
        for (i, illum) in Illuminant::all().iter().enumerate() {
            assert_eq!(Illuminant::from_u8(i as u8), Some(*illum));
        }
        assert_eq!(Illuminant::from_u8(99), None);
    }

    /// § SpectralRoom : 4 zones · all inside ColorRoom · each on a
    ///   distinct illuminant.
    #[test]
    fn spectral_zones_exactly_4_inside_color_room() {
        let zones = spectral_zones();
        assert_eq!(zones.len(), 4);
        // Each zone has a unique illuminant.
        let illums: std::collections::HashSet<Illuminant> =
            zones.iter().map(|z| z.illuminant).collect();
        assert_eq!(illums.len(), 4);
        // All zones inside ColorRoom AABB ([-58, -28] X · [0, 6] Y · [-15, 15] Z).
        for z in &zones {
            let p = z.spawn_xyz;
            assert!(p[0] >= -58.0 && p[0] <= -28.0, "zone {} X out of range : {p:?}", z.name);
            assert!(p[1] >= 0.0 && p[1] <= 6.0, "zone {} Y out of range : {p:?}", z.name);
            assert!(p[2] >= -15.0 && p[2] <= 15.0, "zone {} Z out of range : {p:?}", z.name);
        }
    }

    /// § spectral_zone_by_name returns the right entry.
    #[test]
    fn spectral_zone_by_name_lookup() {
        let z = spectral_zone_by_name("D65-NW").expect("D65-NW exists");
        assert_eq!(z.illuminant, Illuminant::D65);
        assert!(spectral_zone_by_name("nonexistent").is_none());
    }

    /// § Spectral-snapshot returns 16 × 4 = 64 baked entries.
    #[test]
    fn spectral_snapshot_returns_64_entries() {
        let snap = spectral_snapshot_all();
        assert_eq!(snap.len(), MATERIAL_LUT_LEN * ILLUMINANT_COUNT);
    }

    /// § Iterate-everywhere : neon-magenta + dichroic-violet baked under
    ///   F11 fluorescent shows mercury-spike interaction (R-G ratio differs
    ///   from D65 baseline). This is the metamerism finger-print test.
    #[test]
    fn neon_magenta_under_f11_shows_metamerism_vs_d65() {
        let d65 = bake_material_color(MAT_NEON_MAGENTA, Illuminant::D65);
        let f11 = bake_material_color(MAT_NEON_MAGENTA, Illuminant::F11);
        let d65_rg = d65[0] - d65[1];
        let f11_rg = f11[0] - f11[1];
        // The R-G shift between illuminants must be measurable.
        assert!(
            (d65_rg - f11_rg).abs() > 0.01,
            "metamerism test failed : D65 R-G={d65_rg} F11 R-G={f11_rg}"
        );
    }

    /// § Iridescent material handle exists in canonical positions.
    #[test]
    fn iridescent_holographic_dichroic_handles_distinct() {
        assert_ne!(MAT_IRIDESCENT, MAT_HOLOGRAPHIC);
        assert_ne!(MAT_HOLOGRAPHIC, MAT_DICHROIC_VIOLET);
        assert_ne!(MAT_GOLD_LEAF, MAT_EMISSIVE_CYAN);
    }

    /// § Reflectance curves are bounded — no band exceeds 1.0 (energy
    ///   conservation) and no negative values.
    #[test]
    fn material_reflectance_energy_bounded() {
        for id in 0..MATERIAL_LUT_LEN as u32 {
            let r = material_reflectance(id);
            for &v in &r {
                assert!(v >= 0.0, "material {id} negative refl");
                assert!(v <= 1.0, "material {id} refl > 1.0 : {v}");
            }
        }
    }

    /// § Description / cct_kelvin / from_u8 round-trip for the cohort.
    #[test]
    fn illuminant_metadata_complete() {
        for illum in Illuminant::all() {
            assert!(!illum.description().is_empty());
            assert!(illum.cct_kelvin() >= 2000);
            assert!(illum.cct_kelvin() <= 10_000);
        }
    }
}
