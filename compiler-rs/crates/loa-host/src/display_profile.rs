//! § display_profile — deep-AMOLED + HDR + auto-detect for the substrate-compose pass.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-L9-AMOLED-DEEP · Apocky-directive (2026-05-02)
//!   "Deeply support amoled and other pitch black display types · resolution
//!    and contrast should be dynamic."
//!
//! § ROLE
//!   Sister-module to `display_detect`. Where `display_detect` parses winit
//!   `MonitorInfo` + the `LOA_DISPLAY_PROFILE` env-var, this module owns the
//!   PER-PROFILE COLOR-TRANSFORM ATTRIBUTES (saturation-boost · snap-to-zero
//!   threshold · peak-nits) the WGSL compose shader needs to render a true-
//!   black AMOLED · saturate-an-OLED · lift-blacks-on-IPS · or PQ-encode for
//!   HdrExt. Also carries the deeper Win32/DXGI auto-detect path that queries
//!   the actual color-space + EDID manufacturer when available.
//!
//! § DETECTION-LAYERS
//!   1. `LOA_DISPLAY_PROFILE` env-var override (delegated to display_detect)
//!   2. `LOA_DISPLAY_HDR_NITS` env-var override (delegated to display_detect)
//!   3. Win32/DXGI Output color-space query (compile-time-gated to the
//!      `runtime` feature so catalog-mode tests still compile/run).
//!      `DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020` → HdrExt
//!   4. Win32 EDID manufacturer hint (LG / Samsung known-OLED panels) →
//!      Amoled · Oled (compile-gated identical reasons).
//!   5. Fall through to `display_detect::detect_profile` (winit-driven
//!      MonitorInfo heuristic).
//!
//! § PER-PROFILE ATTRIBUTES (Apocky-tuned)
//!   ┌────────┬─────────┬──────────────┬─────────┬──────────────┐
//!   │profile │ blk-thr │ contrast-S   │ sat-✕   │ peak-nits    │
//!   ├────────┼─────────┼──────────────┼─────────┼──────────────┤
//!   │Amoled  │ 0.003   │ 0.40         │ 1.15    │ 800          │
//!   │Oled    │ 0.008   │ 0.30         │ 1.08    │ 600          │
//!   │IpsLcd  │ 0.020   │ 0.15         │ 1.00    │ 400          │
//!   │VaLcd   │ 0.012   │ 0.25         │ 1.05    │ 500          │
//!   │HdrExt  │ 0.0001  │ 0.45         │ 1.20    │ 1000         │
//!   └────────┴─────────┴──────────────┴─────────┴──────────────┘
//!
//!   - blk-thr  : sub-threshold alpha → emit (0,0,0,0)
//!   - contrast : S-curve strength (0 = linear · 1 = strong)
//!   - sat-✕   : per-pixel saturation boost (HSV-S * sat-boost)
//!   - peak-nits: HDR-only · PQ-encode target (Rec.2020 → 10000-nit PQ)
//!
//! § PRIME-DIRECTIVE attestation
//!   No hurt nor harm in the making of this, to anyone/anything/anybody.
//!   Detection is read-only · zero telemetry exfiltration · sovereign-
//!   revocable via env-var override.

#![allow(clippy::module_name_repetitions)]

use crate::display_detect::{
    detect_profile, read_env_hdr_nits, read_env_profile_override, MonitorInfo, HDR_NIT_THRESHOLD,
};
use crate::substrate_compose::DisplayProfile;

// ───────────────────────────────────────────────────────────────────────────
// § PER-PROFILE EXTENDED-ATTRIBUTE TABLE
// ───────────────────────────────────────────────────────────────────────────

/// AMOLED snap-to-zero threshold. Pixels with luminance below this clamp to
/// pure (0,0,0) → AMOLED preserves the unlit-pixel = zero-power physics.
pub const AMOLED_SNAP_TO_ZERO: f32 = 0.003;
/// AMOLED saturation boost (HSV-S × this). 1.15 = +15% punch ·
/// substrate-emitted resonance "pops" against pitch-black.
pub const AMOLED_SAT_BOOST: f32 = 1.15;
/// AMOLED implied peak nits (used by analytics · PQ-encode is HDR-only).
pub const AMOLED_PEAK_NITS: f32 = 800.0;

/// OLED snap-to-zero threshold. Slightly higher than AMOLED — most OLED
/// panels still leak ~1-3 nits of black-frame insertion light.
pub const OLED_SNAP_TO_ZERO: f32 = 0.008;
/// OLED saturation boost (lower than AMOLED — OLED's already saturated).
pub const OLED_SAT_BOOST: f32 = 1.08;
/// OLED implied peak nits.
pub const OLED_PEAK_NITS: f32 = 600.0;

/// IPS-LCD snap-to-zero threshold. LCDs have a backlight floor we cannot
/// crush below — `0.020` matches a typical IPS panel's contrast ratio.
pub const IPS_SNAP_TO_ZERO: f32 = 0.020;
/// IPS-LCD saturation boost (1.0 = identity · IPS already accurate).
pub const IPS_SAT_BOOST: f32 = 1.00;
/// IPS implied peak nits.
pub const IPS_PEAK_NITS: f32 = 400.0;

/// VA-LCD snap-to-zero threshold. Better than IPS · worse than OLED.
pub const VA_SNAP_TO_ZERO: f32 = 0.012;
/// VA-LCD saturation boost. Slight cool-tilt baked into the shader instead.
pub const VA_SAT_BOOST: f32 = 1.05;
/// VA implied peak nits.
pub const VA_PEAK_NITS: f32 = 500.0;

/// HdrExt snap-to-zero threshold. HDR panels have near-perfect blacks · the
/// PQ EOTF maps below 0.0001 to zero anyway.
pub const HDR_SNAP_TO_ZERO: f32 = 0.0001;
/// HdrExt saturation boost. Wide-gamut Rec.2020 lets us push saturation
/// without clipping into out-of-gamut regions.
pub const HDR_SAT_BOOST: f32 = 1.20;
/// HdrExt peak nits (HDR10-canon · PQ-encode target).
pub const HDR_PEAK_NITS: f32 = 1000.0;

// ───────────────────────────────────────────────────────────────────────────
// § DisplayProfile EXTENDED ACCESSORS
// ───────────────────────────────────────────────────────────────────────────

/// Per-profile EXTENDED attributes (snap-to-zero · saturation-boost · peak-
/// nits) consumed by the substrate-compose WGSL shader. Sister to
/// `DisplayProfile::defaults` (which carries the existing black-threshold +
/// contrast-S-curve pair) — additive · ¬ replaces.
pub trait DisplayProfileDeep {
    /// Snap-to-zero threshold : sub-threshold luminance → pure (0,0,0).
    fn snap_to_zero(self) -> f32;
    /// Saturation boost (HSV-S × this · clamped at 2.0 to avoid explosions).
    fn saturation_boost(self) -> f32;
    /// Implied peak nits (analytics + HDR-PQ encoding).
    fn peak_nits(self) -> f32;
    /// True iff this profile uses Rec.2020 + PQ encoding in the shader.
    fn is_hdr(self) -> bool;
}

impl DisplayProfileDeep for DisplayProfile {
    fn snap_to_zero(self) -> f32 {
        match self {
            Self::Amoled => AMOLED_SNAP_TO_ZERO,
            Self::Oled => OLED_SNAP_TO_ZERO,
            Self::IpsLcd => IPS_SNAP_TO_ZERO,
            Self::VaLcd => VA_SNAP_TO_ZERO,
            Self::HdrExt => HDR_SNAP_TO_ZERO,
        }
    }

    fn saturation_boost(self) -> f32 {
        match self {
            Self::Amoled => AMOLED_SAT_BOOST,
            Self::Oled => OLED_SAT_BOOST,
            Self::IpsLcd => IPS_SAT_BOOST,
            Self::VaLcd => VA_SAT_BOOST,
            Self::HdrExt => HDR_SAT_BOOST,
        }
    }

    fn peak_nits(self) -> f32 {
        match self {
            Self::Amoled => AMOLED_PEAK_NITS,
            Self::Oled => OLED_PEAK_NITS,
            Self::IpsLcd => IPS_PEAK_NITS,
            Self::VaLcd => VA_PEAK_NITS,
            Self::HdrExt => HDR_PEAK_NITS,
        }
    }

    fn is_hdr(self) -> bool {
        matches!(self, Self::HdrExt)
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § AutoDetect — deep platform-aware profile detection
// ───────────────────────────────────────────────────────────────────────────

/// EDID manufacturer-id mapping → likely panel-tech. Used by the deep-detect
/// path when winit cannot tell us the panel kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdidMfgHint {
    /// LG OLED / WOLED panels.
    LgOled,
    /// Samsung QD-OLED · AMOLED-tech.
    SamsungAmoled,
    /// Samsung Display PLS · IPS-class.
    SamsungIps,
    /// AU Optronics · LG Display IPS.
    Ips,
    /// VA-LCD (BOE · CSOT · AUO VA panels).
    Va,
    /// Apple Retina (mini-LED via VA · OLED on iPhone). Treat as Oled.
    AppleOled,
    /// Unknown manufacturer.
    Unknown,
}

impl EdidMfgHint {
    /// Map the 3-char EDID manufacturer-id (e.g., `"GSM"` = LG Goldstar) to
    /// a likely panel-tech. Public so unit-tests can exercise it.
    #[must_use]
    pub fn from_mfg_id(id: &str) -> Self {
        let normalized = id.trim().to_ascii_uppercase();
        match normalized.as_str() {
            // LG (GSM = Goldstar = LG · LGD = LG Display)
            "GSM" | "LGD" => Self::LgOled,
            // Samsung Display (SDC = Samsung Display Corp · SAM = Samsung · SEC = Samsung Electronics)
            "SDC" | "SAM" | "SEC" => Self::SamsungAmoled,
            // AU Optronics (AUO = AU Optronics IPS · CMN = Chimei Innolux)
            "AUO" | "CMN" => Self::Ips,
            // BOE · CSOT (typically VA on TVs · IPS on monitors · default to Ips)
            "BOE" | "CSO" => Self::Ips,
            // Apple
            "APP" | "AAP" => Self::AppleOled,
            "" => Self::Unknown,
            _ => Self::Unknown,
        }
    }

    /// Map mfg-hint → DisplayProfile fallback.
    #[must_use]
    pub fn implied_profile(self) -> DisplayProfile {
        match self {
            Self::LgOled => DisplayProfile::Oled,
            Self::SamsungAmoled => DisplayProfile::Amoled,
            Self::AppleOled => DisplayProfile::Oled,
            Self::SamsungIps | Self::Ips => DisplayProfile::IpsLcd,
            Self::Va => DisplayProfile::VaLcd,
            Self::Unknown => DisplayProfile::IpsLcd, // safest non-AMOLED fallback
        }
    }
}

/// DXGI color-space hint (subset · only the ones we mode-switch on).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DxgiColorSpaceHint {
    /// `DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020` — Rec.2020 + PQ EOTF · HDR10.
    Hdr10Pq,
    /// `DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709` — sRGB Rec.709.
    SdrSrgb,
    /// Unknown / not yet queried.
    Unknown,
}

impl DxgiColorSpaceHint {
    /// Map a DXGI color-space numeric value to the hint enum. The numeric
    /// constants come from `DXGI_COLOR_SPACE_TYPE` in `dxgicommon.h`.
    /// Public so unit-tests can exercise it without pulling windows-rs.
    #[must_use]
    pub fn from_dxgi_value(v: u32) -> Self {
        match v {
            // DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 = 12
            12 => Self::Hdr10Pq,
            // DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709 = 0
            0 => Self::SdrSrgb,
            _ => Self::Unknown,
        }
    }

    /// Map → DisplayProfile.
    #[must_use]
    pub fn implied_profile(self) -> Option<DisplayProfile> {
        match self {
            Self::Hdr10Pq => Some(DisplayProfile::HdrExt),
            Self::SdrSrgb => None, // need the EDID hint to disambiguate
            Self::Unknown => None,
        }
    }
}

/// Result of the deep auto-detect path.
#[derive(Debug, Clone, Copy)]
pub struct AutoDetectReport {
    /// The selected profile.
    pub profile: DisplayProfile,
    /// Layer that won. 0 = env · 1 = HDR-nits-env · 2 = DXGI · 3 = EDID · 4 = winit-heuristic · 5 = default.
    pub source_layer: u8,
    /// Optional human-readable annotation for logs.
    pub note_kind: AutoDetectNote,
}

/// Discriminant for the human-readable note (Pod-friendly · no String).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoDetectNote {
    /// Env-var `LOA_DISPLAY_PROFILE` won.
    EnvOverride,
    /// Env-var `LOA_DISPLAY_HDR_NITS` ≥ 1000 won.
    EnvHdrNits,
    /// DXGI color-space query returned HDR10/PQ.
    DxgiHdr,
    /// EDID manufacturer-id matched a known OLED/AMOLED vendor.
    EdidVendor,
    /// winit-MonitorInfo heuristic chose the profile.
    WinitHeuristic,
    /// Default fallback — Apocky-tuned safest pitch-black-friendly.
    DefaultFallback,
}

/// Inputs for `auto_detect_with_inputs`. Pure-function-friendly so unit-tests
/// exercise every code-path WITHOUT actually calling Win32/DXGI.
#[derive(Debug, Clone, Copy, Default)]
pub struct AutoDetectInputs {
    /// winit-driven MonitorInfo (delegate to display_detect on miss).
    pub monitor: MonitorInfo,
    /// DXGI color-space hint (None = not queried).
    pub dxgi: Option<DxgiColorSpaceHint>,
    /// EDID manufacturer hint (None = not queried).
    pub edid: Option<EdidMfgHint>,
}

/// § T11-W18-L9-AMOLED-DEEP — DEEP auto-detect : env > DXGI > EDID > winit-heuristic > default.
///
/// Pure-function · zero side-effects · zero direct OS calls. Caller is
/// responsible for populating `dxgi` / `edid` from the appropriate Win32
/// query (compile-gated to the `runtime` feature in `runtime_query`). Tests
/// in this module exercise every code-path with synthesized inputs.
#[must_use]
pub fn auto_detect_with_inputs(inputs: AutoDetectInputs) -> AutoDetectReport {
    // Layer 0 : LOA_DISPLAY_PROFILE env-var (highest precedence).
    if let Some(profile) = read_env_profile_override() {
        return AutoDetectReport {
            profile,
            source_layer: 0,
            note_kind: AutoDetectNote::EnvOverride,
        };
    }

    // Layer 1 : LOA_DISPLAY_HDR_NITS env-var (when ≥ HDR_NIT_THRESHOLD).
    let env_hdr_nits = read_env_hdr_nits();
    if env_hdr_nits >= HDR_NIT_THRESHOLD {
        return AutoDetectReport {
            profile: DisplayProfile::HdrExt,
            source_layer: 1,
            note_kind: AutoDetectNote::EnvHdrNits,
        };
    }

    // Layer 2 : DXGI color-space query (HDR10/PQ).
    if let Some(dxgi) = inputs.dxgi {
        if let Some(profile) = dxgi.implied_profile() {
            return AutoDetectReport {
                profile,
                source_layer: 2,
                note_kind: AutoDetectNote::DxgiHdr,
            };
        }
    }

    // Layer 3 : EDID manufacturer-id hint.
    if let Some(edid) = inputs.edid {
        // Only act on KNOWN vendors. Unknown falls through to the heuristic.
        if !matches!(edid, EdidMfgHint::Unknown) {
            return AutoDetectReport {
                profile: edid.implied_profile(),
                source_layer: 3,
                note_kind: AutoDetectNote::EdidVendor,
            };
        }
    }

    // Layer 4 : winit-driven MonitorInfo heuristic (display_detect).
    let by_heuristic = detect_profile(inputs.monitor);
    // The heuristic always returns SOMETHING — `display_detect::detect_profile`
    // falls through to Amoled. Distinguish "heuristic-found-something-non-
    // default" from "default-fallback" by checking whether the monitor info
    // is fully default (all zeros).
    let monitor_is_default = inputs.monitor.width == 0
        && inputs.monitor.height == 0
        && inputs.monitor.refresh_hz == 0
        && inputs.monitor.hdr_peak_nits == 0;
    let note_kind = if monitor_is_default {
        AutoDetectNote::DefaultFallback
    } else {
        AutoDetectNote::WinitHeuristic
    };
    AutoDetectReport {
        profile: by_heuristic,
        source_layer: if monitor_is_default { 5 } else { 4 },
        note_kind,
    }
}

/// Convenience : auto-detect with no DXGI / EDID hints (pure winit fallback).
/// Equivalent to `auto_detect_with_inputs(AutoDetectInputs { monitor, ..Default::default() })`.
#[must_use]
pub fn auto_detect_winit_only(monitor: MonitorInfo) -> AutoDetectReport {
    auto_detect_with_inputs(AutoDetectInputs {
        monitor,
        dxgi: None,
        edid: None,
    })
}

// ───────────────────────────────────────────────────────────────────────────
// § runtime_query (compile-gated · Win32/DXGI calls live here)
// ───────────────────────────────────────────────────────────────────────────

/// Runtime DXGI / EDID query helpers. Only compiled when the `runtime`
/// feature is on (which pulls wgpu's windows transitive deps). When OFF,
/// callers should use `auto_detect_with_inputs` with hand-supplied inputs
/// — typically `None` for both `dxgi` + `edid` so the winit-heuristic wins.
#[cfg(feature = "runtime")]
pub mod runtime_query {
    use super::{DxgiColorSpaceHint, EdidMfgHint};

    /// Query the DXGI Output's color-space hint. Best-effort · returns
    /// `Unknown` on any failure (no panics escape).
    ///
    /// § T11-W18-L9 · This is intentionally a SOFT-PROBE : the underlying
    /// IDXGIOutput6::GetDesc1 + DXGI_OUTPUT_DESC1.ColorSpace pair lives in
    /// the windows-rs crate which is NOT a default loa-host dep. Until the
    /// pull is wired, the function returns `Unknown` and the auto-detect
    /// path falls through to the next layer. Callers can override via
    /// `LOA_DISPLAY_PROFILE` / `LOA_DISPLAY_HDR_NITS` env-vars.
    #[must_use]
    pub fn query_dxgi_color_space() -> DxgiColorSpaceHint {
        // § T11-W18-L9-PHASE-2 : windows-rs IDXGIFactory6 + Output6 chain
        // wired-up in a follow-on patch (gated on a NEW `dxgi-deep` feature
        // to avoid pulling windows-rs into the default catalog build). For
        // now, soft-probe `LOA_DISPLAY_HDR_NITS` env-var is the path.
        DxgiColorSpaceHint::Unknown
    }

    /// Query the primary monitor's EDID manufacturer-id. Returns `Unknown`
    /// on any failure. Same soft-probe rationale as `query_dxgi_color_space`.
    #[must_use]
    pub fn query_primary_edid_mfg() -> EdidMfgHint {
        EdidMfgHint::Unknown
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Process-wide env-var lock so concurrent #[test]s don't see each
    /// other's overrides. Mirrors the lock in `display_detect::tests`.
    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        use std::sync::{Mutex, OnceLock};
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    // ──────────────────────────────────────────────────────────────────────
    // § DisplayProfileDeep — extended-attribute table
    // ──────────────────────────────────────────────────────────────────────

    #[test]
    fn deep_attributes_amoled_pitch_black_strong_punch() {
        let p = DisplayProfile::Amoled;
        assert!((p.snap_to_zero() - AMOLED_SNAP_TO_ZERO).abs() < f32::EPSILON);
        assert!((p.saturation_boost() - 1.15).abs() < f32::EPSILON);
        assert!((p.peak_nits() - 800.0).abs() < f32::EPSILON);
        assert!(!p.is_hdr());
    }

    #[test]
    fn deep_attributes_oled_below_amoled_punch() {
        let p = DisplayProfile::Oled;
        // OLED snap higher than AMOLED · saturation lower than AMOLED.
        assert!(p.snap_to_zero() > DisplayProfile::Amoled.snap_to_zero());
        assert!(p.saturation_boost() < DisplayProfile::Amoled.saturation_boost());
        assert!(!p.is_hdr());
    }

    #[test]
    fn deep_attributes_ips_no_crush_no_boost() {
        let p = DisplayProfile::IpsLcd;
        assert!((p.saturation_boost() - 1.0).abs() < f32::EPSILON);
        // IPS snap floor ≥ both OLED variants (cannot crush below LCD-floor).
        assert!(p.snap_to_zero() > DisplayProfile::Amoled.snap_to_zero());
        assert!(p.snap_to_zero() > DisplayProfile::Oled.snap_to_zero());
    }

    #[test]
    fn deep_attributes_va_between_ips_and_oled() {
        let p = DisplayProfile::VaLcd;
        assert!(p.snap_to_zero() < DisplayProfile::IpsLcd.snap_to_zero());
        assert!(p.snap_to_zero() > DisplayProfile::Oled.snap_to_zero());
        assert!(p.saturation_boost() > DisplayProfile::IpsLcd.saturation_boost());
        assert!(p.saturation_boost() < DisplayProfile::Amoled.saturation_boost());
    }

    #[test]
    fn deep_attributes_hdr_max_punch_pq_gated() {
        let p = DisplayProfile::HdrExt;
        assert!(p.is_hdr(), "HDR-ext is the only PQ-gated profile");
        // HDR has the highest saturation boost · the lowest snap-to-zero ·
        // the highest peak-nits.
        for other in [
            DisplayProfile::Amoled,
            DisplayProfile::Oled,
            DisplayProfile::IpsLcd,
            DisplayProfile::VaLcd,
        ] {
            assert!(p.saturation_boost() >= other.saturation_boost());
            assert!(p.snap_to_zero() <= other.snap_to_zero());
            assert!(p.peak_nits() >= other.peak_nits());
        }
        assert!((p.peak_nits() - 1000.0).abs() < f32::EPSILON);
    }

    // ──────────────────────────────────────────────────────────────────────
    // § EdidMfgHint
    // ──────────────────────────────────────────────────────────────────────

    #[test]
    fn edid_mfg_lg_samsung_ips_va_apple() {
        assert_eq!(EdidMfgHint::from_mfg_id("GSM"), EdidMfgHint::LgOled);
        assert_eq!(EdidMfgHint::from_mfg_id("LGD"), EdidMfgHint::LgOled);
        assert_eq!(EdidMfgHint::from_mfg_id("SDC"), EdidMfgHint::SamsungAmoled);
        assert_eq!(EdidMfgHint::from_mfg_id("SAM"), EdidMfgHint::SamsungAmoled);
        assert_eq!(EdidMfgHint::from_mfg_id("AUO"), EdidMfgHint::Ips);
        assert_eq!(EdidMfgHint::from_mfg_id("APP"), EdidMfgHint::AppleOled);
        assert_eq!(EdidMfgHint::from_mfg_id("ZZZ"), EdidMfgHint::Unknown);
        assert_eq!(EdidMfgHint::from_mfg_id(""), EdidMfgHint::Unknown);
        // Case-insensitive
        assert_eq!(EdidMfgHint::from_mfg_id("gsm"), EdidMfgHint::LgOled);
        assert_eq!(EdidMfgHint::from_mfg_id("  sdc "), EdidMfgHint::SamsungAmoled);
    }

    #[test]
    fn edid_mfg_implied_profile_mapping() {
        assert_eq!(
            EdidMfgHint::LgOled.implied_profile(),
            DisplayProfile::Oled
        );
        assert_eq!(
            EdidMfgHint::SamsungAmoled.implied_profile(),
            DisplayProfile::Amoled
        );
        assert_eq!(
            EdidMfgHint::AppleOled.implied_profile(),
            DisplayProfile::Oled
        );
        assert_eq!(EdidMfgHint::Ips.implied_profile(), DisplayProfile::IpsLcd);
        assert_eq!(EdidMfgHint::Va.implied_profile(), DisplayProfile::VaLcd);
        assert_eq!(
            EdidMfgHint::Unknown.implied_profile(),
            DisplayProfile::IpsLcd
        );
    }

    // ──────────────────────────────────────────────────────────────────────
    // § DxgiColorSpaceHint
    // ──────────────────────────────────────────────────────────────────────

    #[test]
    fn dxgi_color_space_known_values() {
        assert_eq!(
            DxgiColorSpaceHint::from_dxgi_value(12),
            DxgiColorSpaceHint::Hdr10Pq,
            "DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 = 12 → HDR10 PQ"
        );
        assert_eq!(
            DxgiColorSpaceHint::from_dxgi_value(0),
            DxgiColorSpaceHint::SdrSrgb,
            "DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709 = 0 → SDR sRGB"
        );
        assert_eq!(
            DxgiColorSpaceHint::from_dxgi_value(99),
            DxgiColorSpaceHint::Unknown
        );
        assert_eq!(
            DxgiColorSpaceHint::Hdr10Pq.implied_profile(),
            Some(DisplayProfile::HdrExt)
        );
        // SDR-sRGB does NOT imply a profile by itself · need EDID/heuristic.
        assert_eq!(DxgiColorSpaceHint::SdrSrgb.implied_profile(), None);
    }

    // ──────────────────────────────────────────────────────────────────────
    // § auto_detect_with_inputs — layered priority table
    // ──────────────────────────────────────────────────────────────────────

    #[test]
    fn auto_detect_env_override_wins_over_dxgi_and_edid() {
        let _g = env_guard();
        std::env::remove_var("LOA_DISPLAY_HDR_NITS");
        std::env::set_var("LOA_DISPLAY_PROFILE", "ips");
        let report = auto_detect_with_inputs(AutoDetectInputs {
            monitor: MonitorInfo::new(3840, 2160, 144, 1500),
            dxgi: Some(DxgiColorSpaceHint::Hdr10Pq),
            edid: Some(EdidMfgHint::SamsungAmoled),
        });
        assert_eq!(report.profile, DisplayProfile::IpsLcd);
        assert_eq!(report.source_layer, 0);
        assert_eq!(report.note_kind, AutoDetectNote::EnvOverride);
        std::env::remove_var("LOA_DISPLAY_PROFILE");
    }

    #[test]
    fn auto_detect_env_hdr_nits_wins_over_dxgi_sdr() {
        let _g = env_guard();
        std::env::remove_var("LOA_DISPLAY_PROFILE");
        std::env::set_var("LOA_DISPLAY_HDR_NITS", "1500");
        let report = auto_detect_with_inputs(AutoDetectInputs {
            monitor: MonitorInfo::default(),
            dxgi: Some(DxgiColorSpaceHint::SdrSrgb),
            edid: Some(EdidMfgHint::Ips),
        });
        assert_eq!(report.profile, DisplayProfile::HdrExt);
        assert_eq!(report.source_layer, 1);
        assert_eq!(report.note_kind, AutoDetectNote::EnvHdrNits);
        std::env::remove_var("LOA_DISPLAY_HDR_NITS");
    }

    #[test]
    fn auto_detect_dxgi_hdr_beats_edid_when_env_unset() {
        let _g = env_guard();
        std::env::remove_var("LOA_DISPLAY_PROFILE");
        std::env::remove_var("LOA_DISPLAY_HDR_NITS");
        let report = auto_detect_with_inputs(AutoDetectInputs {
            monitor: MonitorInfo::default(),
            dxgi: Some(DxgiColorSpaceHint::Hdr10Pq),
            edid: Some(EdidMfgHint::Ips), // would normally win at layer 3
        });
        assert_eq!(report.profile, DisplayProfile::HdrExt);
        assert_eq!(report.source_layer, 2);
        assert_eq!(report.note_kind, AutoDetectNote::DxgiHdr);
    }

    #[test]
    fn auto_detect_edid_lg_oled_wins_over_winit_heuristic() {
        let _g = env_guard();
        std::env::remove_var("LOA_DISPLAY_PROFILE");
        std::env::remove_var("LOA_DISPLAY_HDR_NITS");
        let report = auto_detect_with_inputs(AutoDetectInputs {
            // 4K 60 Hz panel (winit-heuristic would pick Amoled default)
            monitor: MonitorInfo::new(3840, 2160, 60, 0),
            dxgi: Some(DxgiColorSpaceHint::SdrSrgb),
            edid: Some(EdidMfgHint::LgOled),
        });
        assert_eq!(report.profile, DisplayProfile::Oled);
        assert_eq!(report.source_layer, 3);
        assert_eq!(report.note_kind, AutoDetectNote::EdidVendor);
    }

    #[test]
    fn auto_detect_unknown_edid_falls_through_to_winit() {
        let _g = env_guard();
        std::env::remove_var("LOA_DISPLAY_PROFILE");
        std::env::remove_var("LOA_DISPLAY_HDR_NITS");
        // 1366×768 / 120 Hz panel = winit-heuristic picks Oled (likely-OLED-laptop).
        let report = auto_detect_with_inputs(AutoDetectInputs {
            monitor: MonitorInfo::new(1366, 768, 120, 0),
            dxgi: Some(DxgiColorSpaceHint::SdrSrgb),
            edid: Some(EdidMfgHint::Unknown),
        });
        assert_eq!(report.profile, DisplayProfile::Oled);
        assert_eq!(report.source_layer, 4);
        assert_eq!(report.note_kind, AutoDetectNote::WinitHeuristic);
    }

    #[test]
    fn auto_detect_no_info_default_amoled_pitch_black_friendly() {
        let _g = env_guard();
        std::env::remove_var("LOA_DISPLAY_PROFILE");
        std::env::remove_var("LOA_DISPLAY_HDR_NITS");
        let report = auto_detect_with_inputs(AutoDetectInputs::default());
        // No info anywhere → the safest pitch-black-friendly default · Amoled.
        assert_eq!(report.profile, DisplayProfile::Amoled);
        assert_eq!(report.source_layer, 5);
        assert_eq!(report.note_kind, AutoDetectNote::DefaultFallback);
    }

    #[test]
    fn auto_detect_winit_only_helper_matches_full_path_no_dxgi_no_edid() {
        let _g = env_guard();
        std::env::remove_var("LOA_DISPLAY_PROFILE");
        std::env::remove_var("LOA_DISPLAY_HDR_NITS");
        let info = MonitorInfo::new(3840, 2160, 60, 1500);
        let helper = auto_detect_winit_only(info);
        let full = auto_detect_with_inputs(AutoDetectInputs {
            monitor: info,
            dxgi: None,
            edid: None,
        });
        assert_eq!(helper.profile, full.profile);
        assert_eq!(helper.source_layer, full.source_layer);
        assert_eq!(helper.note_kind, full.note_kind);
        // 1500 nit panel → HdrExt via display_detect heuristic (layer-4).
        assert_eq!(helper.profile, DisplayProfile::HdrExt);
    }

    #[test]
    fn auto_detect_env_profile_takes_precedence_over_env_hdr_nits() {
        let _g = env_guard();
        std::env::set_var("LOA_DISPLAY_PROFILE", "amoled");
        std::env::set_var("LOA_DISPLAY_HDR_NITS", "1500");
        let report = auto_detect_with_inputs(AutoDetectInputs::default());
        assert_eq!(report.profile, DisplayProfile::Amoled);
        assert_eq!(report.source_layer, 0);
        std::env::remove_var("LOA_DISPLAY_PROFILE");
        std::env::remove_var("LOA_DISPLAY_HDR_NITS");
    }

    #[test]
    fn deep_attribute_table_total_ordering_amoled_strongest() {
        // ∀ non-HDR profiles : AMOLED has highest sat-boost + lowest snap-thr.
        // (HDR has higher sat + lower snap, but that's the PQ-gated path.)
        let amoled = DisplayProfile::Amoled;
        for other in [
            DisplayProfile::Oled,
            DisplayProfile::IpsLcd,
            DisplayProfile::VaLcd,
        ] {
            assert!(
                amoled.saturation_boost() >= other.saturation_boost(),
                "AMOLED saturation_boost ≥ {other:?}"
            );
            assert!(
                amoled.snap_to_zero() <= other.snap_to_zero(),
                "AMOLED snap_to_zero ≤ {other:?}"
            );
        }
    }
}
