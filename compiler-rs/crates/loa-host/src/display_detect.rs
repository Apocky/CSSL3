//! § display_detect — auto-detect monitor characteristics + map → DisplayProfile.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-DISPLAY · Apocky-directive (2026-05-02)
//!   "Resolution and contrast should be dynamic · deeply support amoled and
//!    other pitch-black display types."
//!
//! § ROLE
//!   Reads winit `MonitorHandle` characteristics (panel pixel-size +
//!   refresh-rate-millihertz) + an optional env-var override, returns the
//!   `DisplayProfile` discriminant the substrate-compose pass should adopt.
//!   Pure-CPU · zero wgpu dep · zero winit-runtime requirement at type level
//!   so the catalog-mode build still exercises the heuristic.
//!
//! § DETECTION HEURISTIC
//!   1. `LOA_DISPLAY_PROFILE` env-var override : amoled/oled/ips/va/hdr
//!      (case-insensitive · hyphen/underscore tolerant).
//!   2. ≥ 1000 nit peak from monitor (winit-0.30 does NOT expose this) +
//!      env-var hint `LOA_DISPLAY_HDR_NITS` ≥ 1000 → HdrExt.
//!   3. High-refresh (≥ 120 Hz) + low-DPI panel (< 1080p × 100 dpi)
//!      → likely-OLED-laptop → Oled.
//!   4. Default → Amoled (pitch-black-friendly · most-conservative for
//!      OLED/AMOLED · degrades-gracefully on IPS via runtime-tweak).
//!
//! § AUTO-RESIZE COMPANION
//!   Sister-fn `compute_substrate_dims` clamps the GPU compute-resolution
//!   to the panel resolution but caps the CPU pixel-field at 512×512 so the
//!   per-frame ray-walk stays inside the 8.33 ms / 120 Hz CPU budget on the
//!   default Apocky-box rayon-pool.
//!
//! § PRIME-DIRECTIVE attestation
//!   No hurt nor harm in the making of this, to anyone/anything/anybody.
//!   Detection is read-only · zero telemetry exfiltration · sovereign-
//!   revocable via env-var override.

use crate::substrate_compose::DisplayProfile;

/// Maximum CPU-pixel-field width. Beyond this the per-frame rayon walk
/// blows the 120 Hz budget on Apocky's box ; the GPU compute path runs at
/// native panel resolution instead.
pub const MAX_CPU_SUBSTRATE_W: u32 = 512;
/// Maximum CPU-pixel-field height (matches MAX_CPU_SUBSTRATE_W).
pub const MAX_CPU_SUBSTRATE_H: u32 = 512;

/// HDR threshold (nits). Monitors reporting ≥ this peak luminance via the
/// `LOA_DISPLAY_HDR_NITS` env-var hint are treated as `HdrExt`.
pub const HDR_NIT_THRESHOLD: u32 = 1000;

/// Refresh-rate threshold (Hz). Panels reporting ≥ this in
/// `MonitorHandle::refresh_rate_millihertz / 1000` are considered
/// "high-refresh" for the OLED-laptop heuristic.
pub const HIGH_REFRESH_HZ: u32 = 120;

/// Inputs for the display-detection heuristic. All fields are optional so
/// the function can be exercised in catalog-mode tests + driven from a real
/// `MonitorHandle` at runtime.
#[derive(Debug, Clone, Copy, Default)]
pub struct MonitorInfo {
    /// Panel native pixel-width.
    pub width: u32,
    /// Panel native pixel-height.
    pub height: u32,
    /// Refresh-rate in Hz (e.g., 60 · 120 · 144 · 240).
    pub refresh_hz: u32,
    /// HDR peak luminance hint (nits). 0 = unknown.
    pub hdr_peak_nits: u32,
}

impl MonitorInfo {
    /// Build a MonitorInfo from explicit fields (test-friendly).
    #[must_use]
    pub const fn new(width: u32, height: u32, refresh_hz: u32, hdr_peak_nits: u32) -> Self {
        Self {
            width,
            height,
            refresh_hz,
            hdr_peak_nits,
        }
    }
}

/// § T11-W18-DISPLAY — Detect a `DisplayProfile` from monitor characteristics
/// + the `LOA_DISPLAY_PROFILE` env-var override. Pure function · easy to
/// unit-test.
///
/// Override-priority (highest → lowest) :
///   1. `LOA_DISPLAY_PROFILE` env-var (amoled · oled · ips · va · hdr)
///   2. HDR peak ≥ HDR_NIT_THRESHOLD nits → HdrExt
///   3. high-refresh (≥ 120 Hz) + sub-1080p → Oled (likely-OLED-laptop)
///   4. fallthrough → Amoled (pitch-black-friendly default)
#[must_use]
pub fn detect_profile(info: MonitorInfo) -> DisplayProfile {
    if let Some(env_profile) = read_env_profile_override() {
        return env_profile;
    }
    if info.hdr_peak_nits >= HDR_NIT_THRESHOLD {
        return DisplayProfile::HdrExt;
    }
    if info.refresh_hz >= HIGH_REFRESH_HZ
        && info.width > 0
        && info.width < 1920
        && info.height > 0
        && info.height < 1080
    {
        return DisplayProfile::Oled;
    }
    DisplayProfile::Amoled
}

/// § T11-W18-DISPLAY — Read `LOA_DISPLAY_PROFILE` env-var (if set) and parse
/// it to a `DisplayProfile`. Tolerant of case + hyphens/underscores.
/// Returns `None` for unset/empty/unrecognized values so the heuristic
/// fall-through path runs.
#[must_use]
pub fn read_env_profile_override() -> Option<DisplayProfile> {
    let raw = std::env::var("LOA_DISPLAY_PROFILE").ok()?;
    parse_profile_token(&raw)
}

/// § T11-W18-DISPLAY — Parse a profile token. Pure-fn so tests can exercise
/// it without mucking with process env-vars.
#[must_use]
pub fn parse_profile_token(raw: &str) -> Option<DisplayProfile> {
    let normalized: String = raw
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | ' '))
        .collect();
    match normalized.as_str() {
        "amoled" => Some(DisplayProfile::Amoled),
        "oled" => Some(DisplayProfile::Oled),
        "ips" | "ipslcd" | "lcd" => Some(DisplayProfile::IpsLcd),
        "va" | "valcd" => Some(DisplayProfile::VaLcd),
        "hdr" | "hdrext" | "hdr10" | "hdr1000" => Some(DisplayProfile::HdrExt),
        _ => None,
    }
}

/// § T11-W18-DISPLAY — Optional HDR-nits hint env-var. The `LOA_DISPLAY_HDR_NITS`
/// var lets users (or auto-config scripts) declare a peak-luminance hint
/// when winit cannot supply one. Returns 0 on unset/invalid.
#[must_use]
pub fn read_env_hdr_nits() -> u32 {
    std::env::var("LOA_DISPLAY_HDR_NITS")
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0)
}

/// § T11-W18-DISPLAY — Compute the `(cpu_w, cpu_h, gpu_w, gpu_h)` substrate
/// dims given a panel resolution.
///
/// CPU is capped at MAX_CPU_SUBSTRATE_* for perf safety ; GPU runs at native
/// panel resolution but never under 256×256 (so the dispatch stays
/// well-formed on tiny windows during resize). All values are at least 1.
#[must_use]
pub fn compute_substrate_dims(panel_w: u32, panel_h: u32) -> (u32, u32, u32, u32) {
    let cpu_w = panel_w.clamp(64, MAX_CPU_SUBSTRATE_W);
    let cpu_h = panel_h.clamp(64, MAX_CPU_SUBSTRATE_H);
    let gpu_w = panel_w.max(256);
    let gpu_h = panel_h.max(256);
    (cpu_w, cpu_h, gpu_w, gpu_h)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Process-wide env-var lock so concurrent #[test]s don't see each
    /// other's overrides. std::env::set_var is safe-but-non-atomic across
    /// threads ; tests that set the override hold this lock.
    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        use std::sync::{Mutex, OnceLock};
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn parse_profile_token_handles_all_variants() {
        assert_eq!(parse_profile_token("amoled"), Some(DisplayProfile::Amoled));
        assert_eq!(parse_profile_token("AMOLED"), Some(DisplayProfile::Amoled));
        assert_eq!(parse_profile_token("oled"), Some(DisplayProfile::Oled));
        assert_eq!(parse_profile_token("ips"), Some(DisplayProfile::IpsLcd));
        assert_eq!(parse_profile_token("ips-lcd"), Some(DisplayProfile::IpsLcd));
        assert_eq!(parse_profile_token("IPS_LCD"), Some(DisplayProfile::IpsLcd));
        assert_eq!(parse_profile_token("va"), Some(DisplayProfile::VaLcd));
        assert_eq!(parse_profile_token("va-lcd"), Some(DisplayProfile::VaLcd));
        assert_eq!(parse_profile_token("hdr"), Some(DisplayProfile::HdrExt));
        assert_eq!(parse_profile_token("HDR-EXT"), Some(DisplayProfile::HdrExt));
        assert_eq!(parse_profile_token("hdr10"), Some(DisplayProfile::HdrExt));
        assert_eq!(parse_profile_token("hdr1000"), Some(DisplayProfile::HdrExt));
        assert_eq!(parse_profile_token("garbage"), None);
        assert_eq!(parse_profile_token(""), None);
        assert_eq!(parse_profile_token("   "), None);
    }

    #[test]
    fn detect_profile_default_is_amoled() {
        let _g = env_guard();
        // Ensure no environment override leaks into this test.
        std::env::remove_var("LOA_DISPLAY_PROFILE");
        // Conservative "unknown" panel : no info → Amoled.
        let info = MonitorInfo::default();
        assert_eq!(detect_profile(info), DisplayProfile::Amoled);
    }

    #[test]
    fn detect_profile_env_override_beats_heuristic() {
        let _g = env_guard();
        // Even on a clearly-HDR panel, env-var wins.
        std::env::set_var("LOA_DISPLAY_PROFILE", "ips");
        let info = MonitorInfo::new(3840, 2160, 144, 1500);
        assert_eq!(detect_profile(info), DisplayProfile::IpsLcd);
        std::env::remove_var("LOA_DISPLAY_PROFILE");
    }

    #[test]
    fn detect_profile_1080p_60hz_default_amoled() {
        let _g = env_guard();
        std::env::remove_var("LOA_DISPLAY_PROFILE");
        // Standard 1080p 60Hz IPS desktop monitor — the heuristic plays
        // safe + falls through to Amoled (pitch-black friendly default).
        let info = MonitorInfo::new(1920, 1080, 60, 0);
        assert_eq!(detect_profile(info), DisplayProfile::Amoled);
    }

    #[test]
    fn detect_profile_4k_panel_falls_through_to_amoled() {
        let _g = env_guard();
        std::env::remove_var("LOA_DISPLAY_PROFILE");
        // 4k panel at 60 Hz with no HDR hint → Amoled default.
        let info = MonitorInfo::new(3840, 2160, 60, 0);
        assert_eq!(detect_profile(info), DisplayProfile::Amoled);
    }

    #[test]
    fn detect_profile_hdr_nits_triggers_hdrext() {
        let _g = env_guard();
        std::env::remove_var("LOA_DISPLAY_PROFILE");
        // 4k panel reporting 1500 nit peak → HdrExt.
        let info = MonitorInfo::new(3840, 2160, 60, 1500);
        assert_eq!(detect_profile(info), DisplayProfile::HdrExt);
    }

    #[test]
    fn detect_profile_high_refresh_low_dpi_oled_laptop() {
        let _g = env_guard();
        std::env::remove_var("LOA_DISPLAY_PROFILE");
        // 1366×768 / 120 Hz panel = likely-OLED-laptop heuristic → Oled.
        let info = MonitorInfo::new(1366, 768, 120, 0);
        assert_eq!(detect_profile(info), DisplayProfile::Oled);
    }

    #[test]
    fn compute_substrate_dims_caps_cpu_at_512() {
        // 4K panel — CPU clamps to 512×512 ; GPU passes through native.
        let (cpu_w, cpu_h, gpu_w, gpu_h) = compute_substrate_dims(3840, 2160);
        assert_eq!(cpu_w, MAX_CPU_SUBSTRATE_W);
        assert_eq!(cpu_h, MAX_CPU_SUBSTRATE_H);
        assert_eq!(gpu_w, 3840);
        assert_eq!(gpu_h, 2160);
    }

    #[test]
    fn compute_substrate_dims_floors_gpu_at_256() {
        // Very tiny window during resize — GPU clamps up to 256.
        let (_cpu_w, _cpu_h, gpu_w, gpu_h) = compute_substrate_dims(80, 60);
        assert_eq!(gpu_w, 256);
        assert_eq!(gpu_h, 256);
    }

    #[test]
    fn compute_substrate_dims_1080p_passthrough() {
        // 1080p panel — CPU clamps to 512×512 (still under panel) ;
        // GPU passes through.
        let (cpu_w, cpu_h, gpu_w, gpu_h) = compute_substrate_dims(1920, 1080);
        assert_eq!(cpu_w, 512);
        assert_eq!(cpu_h, 512);
        assert_eq!(gpu_w, 1920);
        assert_eq!(gpu_h, 1080);
    }

    #[test]
    fn read_env_hdr_nits_default_zero() {
        let _g = env_guard();
        std::env::remove_var("LOA_DISPLAY_HDR_NITS");
        assert_eq!(read_env_hdr_nits(), 0);
    }

    #[test]
    fn read_env_hdr_nits_parses_valid() {
        let _g = env_guard();
        std::env::set_var("LOA_DISPLAY_HDR_NITS", "1500");
        assert_eq!(read_env_hdr_nits(), 1500);
        std::env::remove_var("LOA_DISPLAY_HDR_NITS");
    }
}
