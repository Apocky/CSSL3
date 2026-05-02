// § ads.rs — Aim-Down-Sights state + zoom-FOV cubic-ease
// ════════════════════════════════════════════════════════════════════
// § I> per spec §I.1.ADS-zoom-curve :
//      · fov 90→55 over 150ms · cubic-ease (3t² - 2t³)
//      · press-hold-toggle
//      · movement-while-ADS reduces walk-speed × 0.6
//      · ADS-spread < hipfire-spread (handled by bloom.rs caps)
// § I> deterministic ; cosmetic-only-axiom : zoom-CURVE frozen ∀ player
//      visual-skin overrides (scope-reticle-art) handled by crosshair.rs
// § I> emits AdsZoomFovOverride consumed by cssl-host-camera (W13-4).

use serde::{Deserialize, Serialize};

/// Hipfire FOV in degrees — frozen mechanic.
pub const FOV_HIPFIRE_DEG: f32 = 90.0;
/// ADS FOV in degrees — frozen mechanic.
pub const FOV_ADS_DEG: f32 = 55.0;
/// ADS transition duration in milliseconds (both directions).
pub const ADS_TRANSITION_MS: f32 = 150.0;
/// Walk-speed multiplier while ADS-engaged (60% of base).
pub const ADS_WALK_SPEED_MULT: f32 = 0.6;

/// ADS state-machine. `progress` ∈ [0, 1] : 0 = full hipfire, 1 = full ADS.
/// Linear-interp time-driven, cubic-eased on read.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AdsState {
    /// Linear progress through transition ∈ [0, 1].
    pub progress: f32,
    /// Whether the ADS button is currently held (or toggle is active).
    pub engaged: bool,
}

impl Default for AdsState {
    fn default() -> Self {
        Self::new()
    }
}

impl AdsState {
    /// Construct a fresh hipfire-default ADS state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            progress: 0.0,
            engaged: false,
        }
    }

    /// Advance the linear progress one tick. `dt_ms` is the frame delta in ms.
    /// `engaged_input` is the resolved press/toggle state from input layer.
    pub fn step(&mut self, dt_ms: f32, engaged_input: bool) {
        self.engaged = engaged_input;
        let dt = dt_ms.max(0.0);
        let delta = dt / ADS_TRANSITION_MS;
        if engaged_input {
            self.progress = (self.progress + delta).min(1.0);
        } else {
            self.progress = (self.progress - delta).max(0.0);
        }
    }

    /// Cubic-ease applied to linear progress : `3t² - 2t³` (smoothstep).
    /// Bit-equal across hosts — pure-math, no FMA.
    #[must_use]
    pub fn eased(&self) -> f32 {
        let t = self.progress.max(0.0).min(1.0);
        (3.0 * t * t) - (2.0 * t * t * t)
    }

    /// Current FOV in degrees, eased.
    #[must_use]
    pub fn fov_deg(&self) -> f32 {
        let t = self.eased();
        FOV_HIPFIRE_DEG + (FOV_ADS_DEG - FOV_HIPFIRE_DEG) * t
    }

    /// Walk-speed multiplier ∈ [ADS_WALK_SPEED_MULT, 1.0], eased.
    #[must_use]
    pub fn walk_speed_mult(&self) -> f32 {
        let t = self.eased();
        1.0 + (ADS_WALK_SPEED_MULT - 1.0) * t
    }

    /// Whether the ADS overlay (scope-art) should be shown — true when
    /// transition is far enough that the crosshair pip would just overlap.
    #[must_use]
    pub fn scope_overlay_visible(&self) -> bool {
        self.progress >= 0.5
    }

    /// Reset to hipfire (used on death / weapon-swap).
    pub fn reset(&mut self) {
        self.progress = 0.0;
        self.engaged = false;
    }
}

/// Event emitted to camera-host (W13-4) per tick.
/// Camera consumes-only ; never writes back.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AdsZoomFovOverride {
    pub fov_deg: f32,
    pub walk_speed_mult: f32,
    pub scope_overlay_visible: bool,
}

impl From<&AdsState> for AdsZoomFovOverride {
    fn from(s: &AdsState) -> Self {
        Self {
            fov_deg: s.fov_deg(),
            walk_speed_mult: s.walk_speed_mult(),
            scope_overlay_visible: s.scope_overlay_visible(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_hipfire() {
        let s = AdsState::new();
        assert_eq!(s.progress, 0.0);
        assert!(!s.engaged);
        assert_eq!(s.fov_deg(), FOV_HIPFIRE_DEG);
        assert_eq!(s.walk_speed_mult(), 1.0);
    }

    #[test]
    fn ads_zoom_roundtrip_deterministic() {
        // 90 → 55 over 150ms, then back. Confirm bit-equal both directions.
        let mut s = AdsState::new();
        // engage : drive to full ADS
        for _ in 0..15 {
            s.step(10.0, true);
        }
        assert!((s.fov_deg() - FOV_ADS_DEG).abs() < 1e-5);
        // release : drive back to hipfire
        for _ in 0..15 {
            s.step(10.0, false);
        }
        assert!((s.fov_deg() - FOV_HIPFIRE_DEG).abs() < 1e-5);
    }

    #[test]
    fn cubic_ease_matches_smoothstep_endpoints() {
        let mut s = AdsState::new();
        s.progress = 0.0;
        assert_eq!(s.eased(), 0.0);
        s.progress = 1.0;
        assert_eq!(s.eased(), 1.0);
        s.progress = 0.5;
        // 3·0.25 - 2·0.125 = 0.75 - 0.25 = 0.5 (smoothstep midpoint)
        assert!((s.eased() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn walk_speed_drops_to_60pct_at_full_ads() {
        let mut s = AdsState::new();
        for _ in 0..30 {
            s.step(10.0, true);
        }
        assert!((s.walk_speed_mult() - ADS_WALK_SPEED_MULT).abs() < 1e-5);
    }

    #[test]
    fn scope_overlay_appears_past_half() {
        let mut s = AdsState::new();
        assert!(!s.scope_overlay_visible());
        s.progress = 0.49;
        assert!(!s.scope_overlay_visible());
        s.progress = 0.51;
        assert!(s.scope_overlay_visible());
    }

    #[test]
    fn reset_returns_to_hipfire() {
        let mut s = AdsState::new();
        for _ in 0..10 {
            s.step(10.0, true);
        }
        assert!(s.progress > 0.0);
        s.reset();
        assert_eq!(s.progress, 0.0);
        assert!(!s.engaged);
    }

    #[test]
    fn override_event_round_trip() {
        let mut s = AdsState::new();
        for _ in 0..5 {
            s.step(10.0, true);
        }
        let evt: AdsZoomFovOverride = (&s).into();
        assert_eq!(evt.fov_deg, s.fov_deg());
        assert_eq!(evt.walk_speed_mult, s.walk_speed_mult());
    }
}
