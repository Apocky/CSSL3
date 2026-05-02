// § crosshair.rs — Crosshair state : 4-way-pip + hit-flash + cosmetic-skin
// ════════════════════════════════════════════════════════════════════
// § I> per spec §I.4.crosshair-state :
//      · 4-way-pip expand-with-bloom · pip-px = bloom-rad × screen-px
//      · hit-feedback : color → red 80ms → revert
//      · during-ADS : hidden ; replaced-by-scope-overlay (camera-host)
//      · cosmetic-only customization : pip-color · pip-thickness · center-dot
//        ¬ pay-to-aim-better ; pip-radius ALWAYS = bloom-radius regardless of skin
// § I> emits CrosshairBloomPx consumed by cssl-host-hud (W13-7) ; HUD renders-only.

use serde::{Deserialize, Serialize};

/// Hit-feedback flash duration in milliseconds.
pub const HIT_FLASH_DURATION_MS: f32 = 80.0;

/// RGBA color packed [r,g,b,a] ∈ [0,1].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const WHITE: Rgba = Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const RED:   Rgba = Rgba { r: 1.0, g: 0.15, b: 0.15, a: 1.0 };

    #[must_use]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

/// Type of hit-feedback flash. Different events can drive distinct visual
/// reactions even though the timing is shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HitFlashKind {
    /// Body-shot — standard red flash.
    Body,
    /// Headshot — same timing, but distinct event so HUD can swap art.
    Head,
    /// Killshot — same timing, distinct event for kill-confirm sound trigger.
    Kill,
}

/// Cosmetic customization. NEVER affects mechanics ; spec invariant.
/// `pip_color`/`pip_thickness_px`/`center_dot` are visual-only knobs.
/// `pip_radius` is computed from bloom and is NOT skin-overridable.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CrosshairSkin {
    pub pip_color: Rgba,
    pub pip_thickness_px: f32,
    pub center_dot: bool,
}

impl Default for CrosshairSkin {
    fn default() -> Self {
        Self {
            pip_color: Rgba::WHITE,
            pip_thickness_px: 1.5,
            center_dot: true,
        }
    }
}

/// Crosshair state. Tracks hit-flash timing + last-flash-kind + skin.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CrosshairState {
    pub skin: CrosshairSkin,
    /// Time-since-hit in ms. ≥ HIT_FLASH_DURATION_MS = no flash active.
    pub time_since_hit_ms: f32,
    pub last_flash_kind: HitFlashKind,
}

impl Default for CrosshairState {
    fn default() -> Self {
        Self::new()
    }
}

impl CrosshairState {
    /// New default-skin crosshair, no active flash.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            skin: CrosshairSkin {
                pip_color: Rgba::WHITE,
                pip_thickness_px: 1.5,
                center_dot: true,
            },
            time_since_hit_ms: HIT_FLASH_DURATION_MS,
            last_flash_kind: HitFlashKind::Body,
        }
    }

    /// Construct with a player-chosen cosmetic skin.
    #[must_use]
    pub const fn with_skin(skin: CrosshairSkin) -> Self {
        Self {
            skin,
            time_since_hit_ms: HIT_FLASH_DURATION_MS,
            last_flash_kind: HitFlashKind::Body,
        }
    }

    /// Trigger a hit-feedback flash (body / head / kill).
    pub fn on_hit(&mut self, kind: HitFlashKind) {
        self.time_since_hit_ms = 0.0;
        self.last_flash_kind = kind;
    }

    /// Advance time.
    pub fn step(&mut self, dt_ms: f32) {
        let dt = dt_ms.max(0.0);
        self.time_since_hit_ms = (self.time_since_hit_ms + dt).min(HIT_FLASH_DURATION_MS * 4.0);
    }

    /// Whether a flash is currently active.
    #[must_use]
    pub fn flash_active(&self) -> bool {
        self.time_since_hit_ms < HIT_FLASH_DURATION_MS
    }

    /// Effective pip color this frame — RED during flash, skin-color otherwise.
    /// COSMETIC-ONLY-AXIOM : the flash is mechanical (not customizable) ;
    /// only the resting-color is skin-overridable.
    #[must_use]
    pub fn current_pip_color(&self) -> Rgba {
        if self.flash_active() {
            Rgba::RED
        } else {
            self.skin.pip_color
        }
    }

    /// Compute pip-radius in screen-pixels from bloom-rad + screen-height.
    /// This is the FROZEN mechanic — NOT skin-overridable. Cosmetic-only-axiom.
    #[must_use]
    pub fn pip_radius_px(bloom_rad: f32, screen_height_px: f32, fov_deg: f32) -> f32 {
        // Convert bloom-radians to vertical-screen-pixels :
        //   pixels-per-radian = screen_height_px / fov_radians
        //   so : pip_px = bloom_rad × (screen_height_px / fov_rad)
        // Use a small minimum so the pip never collapses below 4px.
        let fov_rad = (fov_deg.max(1.0)) * std::f32::consts::PI / 180.0;
        let px_per_rad = screen_height_px / fov_rad;
        let computed = bloom_rad * px_per_rad;
        computed.max(4.0)
    }

    /// Reset to fresh state (keeps skin, clears flash).
    pub fn reset(&mut self) {
        self.time_since_hit_ms = HIT_FLASH_DURATION_MS;
        self.last_flash_kind = HitFlashKind::Body;
    }
}

/// Event emitted to HUD (W13-7). Hidden when ADS scope-overlay is active.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CrosshairBloomPx {
    pub pip_radius_px: f32,
    pub pip_color: Rgba,
    pub pip_thickness_px: f32,
    pub center_dot: bool,
    pub flash_kind: HitFlashKind,
    pub flash_active: bool,
    pub hidden_for_ads: bool,
}

impl CrosshairBloomPx {
    pub fn snapshot(
        cross: &CrosshairState,
        bloom_rad: f32,
        screen_height_px: f32,
        fov_deg: f32,
        scope_overlay_visible: bool,
    ) -> Self {
        Self {
            pip_radius_px: CrosshairState::pip_radius_px(bloom_rad, screen_height_px, fov_deg),
            pip_color: cross.current_pip_color(),
            pip_thickness_px: cross.skin.pip_thickness_px,
            center_dot: cross.skin.center_dot,
            flash_kind: cross.last_flash_kind,
            flash_active: cross.flash_active(),
            hidden_for_ads: scope_overlay_visible,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_state_no_flash() {
        let s = CrosshairState::new();
        assert!(!s.flash_active());
        assert_eq!(s.current_pip_color(), Rgba::WHITE);
    }

    #[test]
    fn hit_feedback_flash_80ms() {
        // Spec invariant : red @ t=0..80ms, revert @ t=80ms+.
        let mut s = CrosshairState::new();
        s.on_hit(HitFlashKind::Body);
        assert!(s.flash_active());
        assert_eq!(s.current_pip_color(), Rgba::RED);
        // step 79ms : still active
        s.step(79.0);
        assert!(s.flash_active());
        // step 2 more ms (total 81) : expired
        s.step(2.0);
        assert!(!s.flash_active());
        assert_eq!(s.current_pip_color(), Rgba::WHITE);
    }

    #[test]
    fn crosshair_bloom_tracks_cone() {
        // Spec invariant : pip-radius IS bloom-radius (frozen mechanic).
        // Two different blooms => different pip radii.
        let small = CrosshairState::pip_radius_px(0.005, 1080.0, 90.0);
        let large = CrosshairState::pip_radius_px(0.040, 1080.0, 90.0);
        assert!(large > small);
    }

    #[test]
    fn pip_radius_minimum_floor() {
        // Zero bloom should still give a non-zero pip (visible center-dot).
        let r = CrosshairState::pip_radius_px(0.0, 1080.0, 90.0);
        assert!(r >= 4.0);
    }

    #[test]
    fn cosmetic_skin_changes_color_not_radius() {
        // Spec invariant : pip-radius FROZEN ; only color/thickness skinnable.
        let custom_skin = CrosshairSkin {
            pip_color: Rgba::new(0.0, 1.0, 0.0, 1.0), // green
            pip_thickness_px: 3.0,
            center_dot: false,
        };
        let s = CrosshairState::with_skin(custom_skin);
        assert_eq!(s.current_pip_color(), Rgba::new(0.0, 1.0, 0.0, 1.0));
        assert_eq!(s.skin.pip_thickness_px, 3.0);
        // Radius computation is identical regardless of skin :
        let r_default = CrosshairState::pip_radius_px(0.020, 1080.0, 90.0);
        let r_skinned = CrosshairState::pip_radius_px(0.020, 1080.0, 90.0);
        assert_eq!(r_default, r_skinned);
    }

    #[test]
    fn flash_kind_persists_until_next_hit() {
        let mut s = CrosshairState::new();
        s.on_hit(HitFlashKind::Head);
        assert_eq!(s.last_flash_kind, HitFlashKind::Head);
        s.step(200.0); // flash expired but kind stays
        assert_eq!(s.last_flash_kind, HitFlashKind::Head);
        s.on_hit(HitFlashKind::Kill);
        assert_eq!(s.last_flash_kind, HitFlashKind::Kill);
    }

    #[test]
    fn hidden_during_ads_overlay() {
        let cross = CrosshairState::new();
        let evt = CrosshairBloomPx::snapshot(&cross, 0.01, 1080.0, 70.0, true);
        assert!(evt.hidden_for_ads);
    }

    #[test]
    fn snapshot_reflects_current_color_during_flash() {
        let mut cross = CrosshairState::new();
        cross.on_hit(HitFlashKind::Body);
        let evt = CrosshairBloomPx::snapshot(&cross, 0.01, 1080.0, 90.0, false);
        assert_eq!(evt.pip_color, Rgba::RED);
        assert!(evt.flash_active);
    }

    #[test]
    fn reset_clears_flash_keeps_skin() {
        let custom_skin = CrosshairSkin {
            pip_color: Rgba::new(0.5, 0.5, 1.0, 1.0),
            pip_thickness_px: 2.0,
            center_dot: false,
        };
        let mut s = CrosshairState::with_skin(custom_skin);
        s.on_hit(HitFlashKind::Body);
        s.reset();
        assert!(!s.flash_active());
        assert_eq!(s.skin.pip_thickness_px, 2.0);
        assert!(!s.skin.center_dot);
    }
}
