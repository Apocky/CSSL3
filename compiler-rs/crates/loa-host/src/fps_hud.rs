//! § fps_hud — FPS-grade HUD overlay for LoA-v13.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W13-FPS-HUD (W13-7) — extends `ui_overlay.rs` with the action-FPS-grade
//! HUD-element suite. ¬ rewrite ; ¬ re-roll the bitmap-font / textured-quad
//! pipeline. This module emits `UiVertex` streams into caller-supplied
//! `Vec<UiVertex>` buffers that the existing UI render-pass consumes.
//!
//! § canonical-spec : `Labyrinth of Apocalypse/systems/fps_hud.csl`
//!
//! § HUD-elements (10) :
//!   1.  Crosshair          — 4-way pip · expand-with-bloom · color-flash-on-hit
//!   2.  AmmoCounter        — current-mag/reserve · low-ammo warn ≤25%
//!   3.  Radar              — 8-tick compass · 50m radius · enemy/friendly blips
//!   4.  ObjectiveTracker   — top-right · label + distance + relative-yaw arrow
//!   5.  DamageFloaterPool  — 32 pre-allocated · color-by-tier · 1.5s fade
//!   6.  HealthShieldBar    — 2-segment · shield+health · low-health pulse
//!   7.  ScoreTeamIndicator — top-center · multiplayer-only
//!   8.  ReloadIndicator    — circular progress around crosshair
//!   9.  HitMarker          — x-shape brief flash · color-by-kind
//!   10. Killfeed           — top-right · ring-buffer N=5 · 5s auto-fade
//!
//! § sovereign-UX (CRITICAL) : every element is toggleable via `HudVisibility`.
//! Damage-floaters can be disabled ("feel-only" mode). 5 color-blind palettes
//! ship with the runtime ; each is contrast-tested (≥WCAG-AA equivalent for
//! HUD-against-game-bg). NO engagement-bait UI : no "killstreak" amplification,
//! no rank-pressure prompts, no coercive objective text.
//!
//! § PRIME-DIRECTIVE attestations :
//!   - ¬ surveillance : radar-pings expire; never persisted; never sent off-host.
//!   - ¬ engagement-bait : tested-string-deny-list bans STREAK/RAMPAGE/DOMINATING.
//!   - ¬ control : 10/10 elements toggleable + restore-defaults always available.
//!   - consent-OS : 5 color-blind palettes ; Achromatopsia baseline is shape-coded.
//!
//! § zero-alloc-hot-path : DAMAGE_FLOATER_POOL_CAP=32 ; Killfeed N=5 ;
//! enemy_pings cap=32 ; friendly_pings cap=8. All bounded ring-buffers ;
//! `Vec::push` is forbidden during steady-state once pools are seeded.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::too_many_lines)]

use crate::ui_overlay::{
    build_shadowed_text, build_text_quads, push_solid_rect, UiVertex, CELL_W, COLOR_BLACK,
    COLOR_DIM_TEXT, COLOR_HIGHLIGHT, COLOR_WHITE, TEXT_SCALE,
};

// ─────────────────────────────────────────────────────────────────────────
// § Color constants — extends ui_overlay's palette
// ─────────────────────────────────────────────────────────────────────────

/// Crosshair-pip default color (white with full alpha).
pub const COLOR_CROSSHAIR: [f32; 4] = [1.0, 1.0, 1.0, 0.95];
/// Brief on-hit flash color (red-orange).
pub const COLOR_HIT_FLASH: [f32; 4] = [1.0, 0.18, 0.18, 1.0];
/// Low-ammo warning color (warm amber).
pub const COLOR_WARNING: [f32; 4] = [1.0, 0.70, 0.10, 1.0];
/// Reload-indicator amber.
pub const COLOR_RELOAD_AMBER: [f32; 4] = [1.0, 0.69, 0.13, 1.0];
/// Shield-bar blue-cyan (default palette).
pub const COLOR_SHIELD_BLUE: [f32; 4] = [0.30, 0.75, 1.0, 0.95];
/// Health-bar green (default palette).
pub const COLOR_HEALTH_GREEN: [f32; 4] = [0.30, 0.95, 0.40, 0.95];
/// Health-bar pulse-red when ≤25%.
pub const COLOR_HEALTH_PULSE: [f32; 4] = [1.0, 0.20, 0.20, 0.95];
/// Enemy radar-blip default color.
pub const COLOR_ENEMY_BLIP: [f32; 4] = [1.0, 0.30, 0.25, 1.0];
/// Friendly radar-blip default color.
pub const COLOR_FRIENDLY_BLIP: [f32; 4] = [0.40, 0.65, 1.0, 1.0];
/// Damage-floater tier · light (<25 dmg).
pub const COLOR_DMG_LIGHT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
/// Damage-floater tier · mid (<75 dmg).
pub const COLOR_DMG_MID: [f32; 4] = [1.0, 0.92, 0.30, 1.0];
/// Damage-floater tier · heavy (<150 dmg).
pub const COLOR_DMG_HEAVY: [f32; 4] = [1.0, 0.55, 0.15, 1.0];
/// Damage-floater tier · crit (≥150 dmg).
pub const COLOR_DMG_CRIT: [f32; 4] = [1.0, 0.25, 0.20, 1.0];
/// Hit-marker · body shot.
pub const COLOR_HITMARK_BODY: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
/// Hit-marker · headshot.
pub const COLOR_HITMARK_HEAD: [f32; 4] = [1.0, 0.95, 0.30, 1.0];
/// Hit-marker · kill confirmed.
pub const COLOR_HITMARK_KILL: [f32; 4] = [1.0, 0.30, 0.20, 1.0];
/// Radar-frame ring (dim glass-panel).
pub const COLOR_RADAR_FRAME: [f32; 4] = [0.10, 0.13, 0.18, 0.55];
/// Radar tick (compass directions).
pub const COLOR_RADAR_TICK: [f32; 4] = [0.85, 0.88, 0.95, 0.85];

// ─────────────────────────────────────────────────────────────────────────
// § Constants — pool capacities (zero-alloc invariants)
// ─────────────────────────────────────────────────────────────────────────

/// Maximum simultaneous damage-floaters — pre-allocated, recycled.
pub const DAMAGE_FLOATER_POOL_CAP: usize = 32;
/// Killfeed ring-buffer depth — last N eliminations.
pub const KILLFEED_CAPACITY: usize = 5;
/// Enemy radar-blip cap (per-frame).
pub const RADAR_ENEMY_CAP: usize = 32;
/// Friendly radar-blip cap (per-frame).
pub const RADAR_FRIENDLY_CAP: usize = 8;
/// Default radar radius in world meters.
pub const RADAR_RADIUS_M: f32 = 50.0;
/// Damage-floater fade duration in seconds.
pub const DAMAGE_FLOATER_FADE_S: f32 = 1.5;
/// Damage-floater rise speed (px/s upward).
pub const DAMAGE_FLOATER_RISE_PX_S: f32 = 40.0;
/// Hit-marker visible frame count (200 ms @ 60 Hz).
pub const HIT_MARKER_FRAMES: u8 = 12;
/// Killfeed entry auto-fade duration (seconds).
pub const KILLFEED_FADE_S: f32 = 5.0;
/// Crosshair pip-min offset from center (px).
pub const CROSSHAIR_PIP_MIN_PX: f32 = 2.0;
/// Crosshair pip-max offset from center (px).
pub const CROSSHAIR_PIP_MAX_PX: f32 = 24.0;
/// Crosshair pip thickness (px).
pub const CROSSHAIR_PIP_THICK_PX: f32 = 2.0;
/// Crosshair pip length (px).
pub const CROSSHAIR_PIP_LEN_PX: f32 = 8.0;
/// Reload-indicator radius (px).
pub const RELOAD_INDICATOR_RADIUS_PX: f32 = 32.0;
/// Reload-indicator segment count (visual ring resolution).
pub const RELOAD_INDICATOR_SEGMENTS: u32 = 24;
/// Low-ammo warning threshold (fraction of magazine).
pub const LOW_AMMO_THRESHOLD: f32 = 0.25;
/// Low-health pulse threshold.
pub const LOW_HEALTH_THRESHOLD: f32 = 0.25;

// ─────────────────────────────────────────────────────────────────────────
// § Color-blind palettes (5 presets)
// ─────────────────────────────────────────────────────────────────────────

/// Color-blind palette presets — toggle via player-config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ColorBlindPalette {
    Default = 0,
    Protanopia = 1,
    Deuteranopia = 2,
    Tritanopia = 3,
    Achromatopsia = 4,
}

impl ColorBlindPalette {
    pub const COUNT: u8 = 5;

    pub fn from_index(i: u8) -> Self {
        match i % Self::COUNT {
            0 => Self::Default,
            1 => Self::Protanopia,
            2 => Self::Deuteranopia,
            3 => Self::Tritanopia,
            _ => Self::Achromatopsia,
        }
    }

    /// Stable label for tests / UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "DEFAULT",
            Self::Protanopia => "PROTANOPIA",
            Self::Deuteranopia => "DEUTERANOPIA",
            Self::Tritanopia => "TRITANOPIA",
            Self::Achromatopsia => "ACHROMATOPSIA",
        }
    }

    /// Look up the enemy-blip color under this palette.
    pub fn enemy_color(self) -> [f32; 4] {
        match self {
            Self::Default => COLOR_ENEMY_BLIP,
            // Protanopia : red shifts dark; raise lightness, hue → orange.
            Self::Protanopia => [1.0, 0.55, 0.15, 1.0],
            // Deuteranopia : green-deficient; keep red but boost saturation + add yellow shift.
            Self::Deuteranopia => [1.0, 0.45, 0.10, 1.0],
            // Tritanopia : blue-yellow deficient; red/orange-red still distinguishable.
            Self::Tritanopia => [1.0, 0.20, 0.20, 1.0],
            // Achromatopsia : full mono — light grey for enemy + shape-coded outline.
            Self::Achromatopsia => [0.95, 0.95, 0.95, 1.0],
        }
    }

    /// Look up the friendly-blip color under this palette.
    pub fn friendly_color(self) -> [f32; 4] {
        match self {
            Self::Default => COLOR_FRIENDLY_BLIP,
            Self::Protanopia => [0.40, 0.70, 1.0, 1.0],
            Self::Deuteranopia => [0.20, 0.70, 1.0, 1.0],
            Self::Tritanopia => [0.45, 0.70, 0.95, 1.0],
            Self::Achromatopsia => [0.55, 0.55, 0.55, 1.0],
        }
    }

    /// Look up the shield-bar color under this palette.
    pub fn shield_color(self) -> [f32; 4] {
        match self {
            Self::Default => COLOR_SHIELD_BLUE,
            Self::Protanopia => [0.30, 0.75, 1.0, 0.95],
            Self::Deuteranopia => [0.20, 0.65, 1.0, 0.95],
            Self::Tritanopia => [0.40, 0.78, 0.92, 0.95],
            Self::Achromatopsia => [0.85, 0.85, 0.85, 0.95],
        }
    }

    /// Look up the health-bar color under this palette.
    pub fn health_color(self) -> [f32; 4] {
        match self {
            Self::Default => COLOR_HEALTH_GREEN,
            // Protanopia / Deuteranopia : green-impaired, use cyan-leaning blue-green.
            Self::Protanopia => [0.30, 0.85, 0.85, 0.95],
            Self::Deuteranopia => [0.30, 0.85, 0.85, 0.95],
            Self::Tritanopia => [0.40, 0.92, 0.45, 0.95],
            Self::Achromatopsia => [0.65, 0.65, 0.65, 0.95],
        }
    }
}

impl Default for ColorBlindPalette {
    fn default() -> Self {
        Self::Default
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Sovereign HUD-visibility (toggleable per element)
// ─────────────────────────────────────────────────────────────────────────

/// Per-element visibility toggle. ALL default-true on first launch ; the
/// user can disable any subset via player-config.
///
/// This is the sovereign-cap — every HUD-element MUST be toggleable.
#[derive(Debug, Clone, Copy)]
pub struct HudVisibility {
    pub crosshair: bool,
    pub ammo: bool,
    pub radar: bool,
    pub objective: bool,
    pub damage_floaters: bool,
    pub health_shield: bool,
    pub score_team: bool,
    pub reload_indicator: bool,
    pub hit_marker: bool,
    pub killfeed: bool,
}

impl Default for HudVisibility {
    fn default() -> Self {
        // Every element on by default ; player toggles in settings.
        Self {
            crosshair: true,
            ammo: true,
            radar: true,
            objective: true,
            damage_floaters: true,
            health_shield: true,
            score_team: true,
            reload_indicator: true,
            hit_marker: true,
            killfeed: true,
        }
    }
}

impl HudVisibility {
    /// Restore all elements to default (sovereign-cap : restore always available).
    pub fn restore_defaults(&mut self) {
        *self = Self::default();
    }

    /// Count of currently-visible elements (for tests / status).
    pub fn count_visible(&self) -> u8 {
        [
            self.crosshair,
            self.ammo,
            self.radar,
            self.objective,
            self.damage_floaters,
            self.health_shield,
            self.score_team,
            self.reload_indicator,
            self.hit_marker,
            self.killfeed,
        ]
        .iter()
        .filter(|b| **b)
        .count() as u8
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Crosshair state — consumes bloom from W13-5 ADS-recoil
// ─────────────────────────────────────────────────────────────────────────

/// Cosmetic crosshair shape (5 presets — battle-pass / W13-9 cosmetic only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CrosshairStyle {
    /// Classic 4-way pip (default).
    FourPip = 0,
    /// Center dot only.
    Dot = 1,
    /// Open circle.
    Circle = 2,
    /// T-shape (no top pip).
    TShape = 3,
    /// Plus / classic-cross.
    PlusCross = 4,
}

impl CrosshairStyle {
    pub const COUNT: u8 = 5;

    pub fn from_index(i: u8) -> Self {
        match i % Self::COUNT {
            0 => Self::FourPip,
            1 => Self::Dot,
            2 => Self::Circle,
            3 => Self::TShape,
            _ => Self::PlusCross,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::FourPip => "4-PIP",
            Self::Dot => "DOT",
            Self::Circle => "CIRCLE",
            Self::TShape => "T-SHAPE",
            Self::PlusCross => "PLUS",
        }
    }
}

impl Default for CrosshairStyle {
    fn default() -> Self {
        Self::FourPip
    }
}

/// Crosshair runtime state. `bloom_norm` is consumed-only from W13-5 (ADS-recoil)
/// — never written back from this module.
#[derive(Debug, Clone, Copy)]
pub struct CrosshairState {
    /// Bloom factor 0..1 from ADS-recoil. Drives pip-spread.
    pub bloom_norm: f32,
    /// Frames remaining for the on-hit color-flash (counts down each frame).
    pub flash_frames_remaining: u8,
    /// Shape preset (cosmetic).
    pub style: CrosshairStyle,
    /// Screen-center color (overridden during flash).
    pub base_color: [f32; 4],
}

impl Default for CrosshairState {
    fn default() -> Self {
        Self {
            bloom_norm: 0.0,
            flash_frames_remaining: 0,
            style: CrosshairStyle::FourPip,
            base_color: COLOR_CROSSHAIR,
        }
    }
}

impl CrosshairState {
    /// Trigger the on-hit color-flash. Resets the countdown to 11 frames
    /// (~180 ms @ 60 Hz). Idempotent if already flashing.
    pub fn trigger_hit_flash(&mut self) {
        self.flash_frames_remaining = 11;
    }

    /// Tick down the flash counter. Call once per simulation frame.
    pub fn tick(&mut self) {
        if self.flash_frames_remaining > 0 {
            self.flash_frames_remaining -= 1;
        }
    }

    /// Compute the current pip-spread offset in pixels.
    /// Linear interpolation between MIN and MAX based on bloom_norm.
    pub fn current_pip_offset(&self) -> f32 {
        let b = self.bloom_norm.clamp(0.0, 1.0);
        CROSSHAIR_PIP_MIN_PX + (CROSSHAIR_PIP_MAX_PX - CROSSHAIR_PIP_MIN_PX) * b
    }

    /// Return the active draw-color (flash-aware).
    pub fn current_color(&self) -> [f32; 4] {
        if self.flash_frames_remaining > 0 {
            COLOR_HIT_FLASH
        } else {
            self.base_color
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Ammo counter
// ─────────────────────────────────────────────────────────────────────────

/// Ammo-counter state. Inputs come from W13-2 weapons module each frame.
#[derive(Debug, Clone, Copy)]
pub struct AmmoCounter {
    pub mag_current: u16,
    pub mag_capacity: u16,
    pub reserve: u16,
}

impl Default for AmmoCounter {
    fn default() -> Self {
        Self {
            mag_current: 30,
            mag_capacity: 30,
            reserve: 90,
        }
    }
}

impl AmmoCounter {
    /// Round-trip update from weapons module.
    pub fn update(&mut self, mag_current: u16, mag_capacity: u16, reserve: u16) {
        self.mag_current = mag_current.min(mag_capacity);
        self.mag_capacity = mag_capacity;
        self.reserve = reserve;
    }

    /// True when current-mag is at or below the low-warn threshold.
    pub fn is_low(&self) -> bool {
        if self.mag_capacity == 0 {
            return false;
        }
        let frac = self.mag_current as f32 / self.mag_capacity as f32;
        frac <= LOW_AMMO_THRESHOLD
    }

    /// True when magazine is fully empty (distinct from low : trigger empty-color).
    pub fn is_empty(&self) -> bool {
        self.mag_current == 0
    }

    /// Color-tier for the displayed digits.
    pub fn color_tier(&self) -> [f32; 4] {
        if self.is_empty() {
            COLOR_HIT_FLASH
        } else if self.is_low() {
            COLOR_WARNING
        } else {
            COLOR_WHITE
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Radar / Compass
// ─────────────────────────────────────────────────────────────────────────

/// A single radar blip (enemy or friendly). All-coords are world-space.
#[derive(Debug, Clone, Copy)]
pub struct RadarBlip {
    /// World-X (player relative coords computed at draw-time).
    pub world_x: f32,
    /// World-Z.
    pub world_z: f32,
    /// Seconds remaining before fade-out (enemy-only ; ∞ for friendly).
    pub fade_remaining_s: f32,
    /// True = friendly (always-on) ; false = enemy (fades 2s post-fire).
    pub is_friendly: bool,
}

/// Radar state — caps enforced via push helpers (zero-alloc steady-state).
#[derive(Debug, Clone)]
pub struct RadarState {
    pub center_x: f32,
    pub center_y: f32,
    pub radius_px: f32,
    pub radius_world_m: f32,
    /// Player yaw in radians — drives compass-rotation.
    pub player_yaw_rad: f32,
    /// Player world-position (for blip relative-coords).
    pub player_world_x: f32,
    pub player_world_z: f32,
    /// Active enemy blips. Capacity-bounded ; oldest evicted on overflow.
    pub enemy_pings: Vec<RadarBlip>,
    /// Active friendly blips. Capacity-bounded.
    pub friendly_pings: Vec<RadarBlip>,
}

impl Default for RadarState {
    fn default() -> Self {
        let mut s = Self {
            center_x: 0.0,
            center_y: 0.0,
            radius_px: 60.0,
            radius_world_m: RADAR_RADIUS_M,
            player_yaw_rad: 0.0,
            player_world_x: 0.0,
            player_world_z: 0.0,
            enemy_pings: Vec::with_capacity(RADAR_ENEMY_CAP),
            friendly_pings: Vec::with_capacity(RADAR_FRIENDLY_CAP),
        };
        // Pre-seed to capacity so steady-state push doesn't reallocate.
        // We never actually exceed cap because push_*_blip evicts.
        s.enemy_pings.reserve_exact(RADAR_ENEMY_CAP);
        s.friendly_pings.reserve_exact(RADAR_FRIENDLY_CAP);
        s
    }
}

impl RadarState {
    /// Add an enemy blip ; evicts oldest if cap exceeded.
    pub fn push_enemy_blip(&mut self, blip: RadarBlip) {
        if self.enemy_pings.len() >= RADAR_ENEMY_CAP {
            self.enemy_pings.remove(0);
        }
        self.enemy_pings.push(blip);
    }

    /// Add a friendly blip ; evicts oldest if cap exceeded.
    pub fn push_friendly_blip(&mut self, blip: RadarBlip) {
        if self.friendly_pings.len() >= RADAR_FRIENDLY_CAP {
            self.friendly_pings.remove(0);
        }
        self.friendly_pings.push(blip);
    }

    /// Tick fade-counters for enemy blips ; remove expired.
    pub fn tick(&mut self, dt_s: f32) {
        for b in &mut self.enemy_pings {
            b.fade_remaining_s = (b.fade_remaining_s - dt_s).max(0.0);
        }
        self.enemy_pings.retain(|b| b.fade_remaining_s > 0.0);
    }

    /// Compute the blip's screen-coord (radar-relative) given the player
    /// world-pos + radar-radius. Returns `None` when the blip is out of range.
    pub fn blip_to_radar_coord(&self, blip: &RadarBlip) -> Option<(f32, f32)> {
        let dx = blip.world_x - self.player_world_x;
        let dz = blip.world_z - self.player_world_z;
        let dist = (dx * dx + dz * dz).sqrt();
        if dist > self.radius_world_m {
            return None;
        }
        // Normalize to radar-radius (0..1) then to pixel-radius.
        let nx = dx / self.radius_world_m;
        let nz = dz / self.radius_world_m;
        // Rotate by -yaw so player-forward = radar-up.
        let cos_y = self.player_yaw_rad.cos();
        let sin_y = self.player_yaw_rad.sin();
        let rx = nx * cos_y + nz * sin_y;
        let rz = -nx * sin_y + nz * cos_y;
        Some((self.center_x + rx * self.radius_px, self.center_y + rz * self.radius_px))
    }

    /// Compute the 8 compass-tick screen positions in N, NE, E, ... order.
    /// Returned as `(x_px, y_px, label)`.
    pub fn compass_ticks(&self) -> [(f32, f32, &'static str); 8] {
        let labels = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
        let mut out = [(0.0, 0.0, ""); 8];
        for i in 0..8 {
            let theta = (i as f32) * core::f32::consts::FRAC_PI_4 - self.player_yaw_rad;
            let x = self.center_x + theta.sin() * self.radius_px;
            let y = self.center_y - theta.cos() * self.radius_px;
            out[i] = (x, y, labels[i]);
        }
        out
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Objective tracker
// ─────────────────────────────────────────────────────────────────────────

/// Top-right objective marker. World-pos drives distance + arrow.
#[derive(Debug, Clone)]
pub struct ObjectiveTracker {
    pub label: String,
    pub world_pos: [f32; 3],
    pub player_pos: [f32; 3],
    pub player_yaw_rad: f32,
    pub visible: bool,
}

impl Default for ObjectiveTracker {
    fn default() -> Self {
        Self {
            label: String::from("EXPLORE"),
            world_pos: [0.0, 0.0, 0.0],
            player_pos: [0.0, 0.0, 0.0],
            player_yaw_rad: 0.0,
            visible: false,
        }
    }
}

impl ObjectiveTracker {
    /// Set the objective. Engagement-bait language is rejected (returns false).
    /// Returns `true` if the label was accepted.
    pub fn set_label(&mut self, label: &str) -> bool {
        if is_engagement_bait(label) {
            return false;
        }
        self.label.clear();
        self.label.push_str(label);
        true
    }

    /// Distance from player to objective in meters.
    pub fn distance_m(&self) -> f32 {
        let dx = self.world_pos[0] - self.player_pos[0];
        let dy = self.world_pos[1] - self.player_pos[1];
        let dz = self.world_pos[2] - self.player_pos[2];
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// Yaw to objective relative to player-yaw, in radians.
    /// Positive = clockwise from forward.
    pub fn relative_yaw(&self) -> f32 {
        let dx = self.world_pos[0] - self.player_pos[0];
        let dz = self.world_pos[2] - self.player_pos[2];
        let target_yaw = dx.atan2(dz);
        // Wrap to (-π, π].
        let mut delta = target_yaw - self.player_yaw_rad;
        while delta > core::f32::consts::PI {
            delta -= core::f32::consts::TAU;
        }
        while delta < -core::f32::consts::PI {
            delta += core::f32::consts::TAU;
        }
        delta
    }

    /// True when the objective is behind the camera (|relative_yaw| > π/2).
    pub fn is_behind(&self) -> bool {
        self.relative_yaw().abs() > core::f32::consts::FRAC_PI_2
    }
}

/// Reject engagement-bait language patterns. Returns `true` when the input
/// contains a banned substring (case-insensitive).
pub fn is_engagement_bait(s: &str) -> bool {
    const BANNED: &[&str] = &[
        "STREAK",
        "RAMPAGE",
        "DOMINATING",
        "GET RICHER",
        "PROVE YOURSELF",
        "ON FIRE",
        "GODLIKE",
        "UNSTOPPABLE",
    ];
    let upper: String = s.to_uppercase();
    BANNED.iter().any(|b| upper.contains(b))
}

// ─────────────────────────────────────────────────────────────────────────
// § Damage-floater pool (32 pre-allocated, recycled)
// ─────────────────────────────────────────────────────────────────────────

/// One damage-popup floater. Active when `t_remaining_s > 0`.
#[derive(Debug, Clone, Copy)]
pub struct DamageFloater {
    pub damage: u32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub age_s: f32,
    pub t_remaining_s: f32,
    pub is_crit: bool,
}

impl DamageFloater {
    pub const fn empty() -> Self {
        Self {
            damage: 0,
            origin_x: 0.0,
            origin_y: 0.0,
            age_s: 0.0,
            t_remaining_s: 0.0,
            is_crit: false,
        }
    }

    /// True when this slot is actively rendered.
    pub fn is_active(&self) -> bool {
        self.t_remaining_s > 0.0
    }

    /// Pick the color-tier for this floater's damage value.
    pub fn tier_color(&self) -> [f32; 4] {
        if self.is_crit || self.damage >= 150 {
            COLOR_DMG_CRIT
        } else if self.damage >= 75 {
            COLOR_DMG_HEAVY
        } else if self.damage >= 25 {
            COLOR_DMG_MID
        } else {
            COLOR_DMG_LIGHT
        }
    }

    /// Current alpha (linear fade 1 → 0 over fade-duration).
    pub fn current_alpha(&self) -> f32 {
        if !self.is_active() {
            return 0.0;
        }
        (self.t_remaining_s / DAMAGE_FLOATER_FADE_S).clamp(0.0, 1.0)
    }

    /// Current screen-Y including upward rise.
    pub fn current_y(&self) -> f32 {
        self.origin_y - self.age_s * DAMAGE_FLOATER_RISE_PX_S
    }
}

/// Fixed-capacity pool of damage-floaters. Pre-allocated at construction ;
/// `spawn` recycles the oldest expired slot when the pool is saturated.
#[derive(Debug, Clone)]
pub struct DamageFloaterPool {
    pub slots: Vec<DamageFloater>,
}

impl Default for DamageFloaterPool {
    fn default() -> Self {
        Self::new()
    }
}

impl DamageFloaterPool {
    /// Allocate the pool to capacity. NEVER grows during gameplay.
    pub fn new() -> Self {
        Self {
            slots: vec![DamageFloater::empty(); DAMAGE_FLOATER_POOL_CAP],
        }
    }

    /// Spawn a new floater. Recycles oldest expired slot if pool is saturated.
    /// Returns the slot-index used.
    pub fn spawn(&mut self, damage: u32, origin_x: f32, origin_y: f32, is_crit: bool) -> usize {
        // Try free slot first.
        for (i, s) in self.slots.iter_mut().enumerate() {
            if !s.is_active() {
                *s = DamageFloater {
                    damage,
                    origin_x,
                    origin_y,
                    age_s: 0.0,
                    t_remaining_s: DAMAGE_FLOATER_FADE_S,
                    is_crit,
                };
                return i;
            }
        }
        // Saturated → recycle the oldest (smallest t_remaining_s).
        let mut oldest_i = 0;
        let mut oldest_t = f32::MAX;
        for (i, s) in self.slots.iter().enumerate() {
            if s.t_remaining_s < oldest_t {
                oldest_t = s.t_remaining_s;
                oldest_i = i;
            }
        }
        self.slots[oldest_i] = DamageFloater {
            damage,
            origin_x,
            origin_y,
            age_s: 0.0,
            t_remaining_s: DAMAGE_FLOATER_FADE_S,
            is_crit,
        };
        oldest_i
    }

    /// Tick all active floaters by `dt_s`. Inactive slots remain inactive.
    pub fn tick(&mut self, dt_s: f32) {
        for s in &mut self.slots {
            if s.is_active() {
                s.age_s += dt_s;
                s.t_remaining_s = (s.t_remaining_s - dt_s).max(0.0);
            }
        }
    }

    /// Count active floaters (for tests / debug).
    pub fn active_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_active()).count()
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Health / shield bar
// ─────────────────────────────────────────────────────────────────────────

/// 2-segment bar : shield drains-first, then health.
#[derive(Debug, Clone, Copy)]
pub struct HealthShieldBar {
    pub shield_norm: f32,
    pub health_norm: f32,
    /// Frame counter for low-health pulse (visual sync).
    pub pulse_frame: u32,
}

impl Default for HealthShieldBar {
    fn default() -> Self {
        Self {
            shield_norm: 1.0,
            health_norm: 1.0,
            pulse_frame: 0,
        }
    }
}

impl HealthShieldBar {
    pub fn update(&mut self, shield_norm: f32, health_norm: f32) {
        self.shield_norm = shield_norm.clamp(0.0, 1.0);
        self.health_norm = health_norm.clamp(0.0, 1.0);
    }

    /// True when health has dropped below the pulse threshold.
    pub fn is_low_health(&self) -> bool {
        self.health_norm <= LOW_HEALTH_THRESHOLD
    }

    /// 2 Hz pulse (alternates each ~30 frames @ 60 Hz).
    pub fn pulse_visible(&self) -> bool {
        self.is_low_health() && (self.pulse_frame / 30) % 2 == 0
    }

    pub fn tick(&mut self) {
        self.pulse_frame = self.pulse_frame.wrapping_add(1);
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Score / team indicator (multiplayer-only)
// ─────────────────────────────────────────────────────────────────────────

/// Top-center multiplayer indicator. Hidden in singleplayer.
#[derive(Debug, Clone)]
pub struct ScoreTeamIndicator {
    pub blue_score: u32,
    pub red_score: u32,
    pub mode_label: String,
    pub time_left_s: f32,
    pub multiplayer_active: bool,
}

impl Default for ScoreTeamIndicator {
    fn default() -> Self {
        Self {
            blue_score: 0,
            red_score: 0,
            mode_label: String::from("TDM"),
            time_left_s: 0.0,
            multiplayer_active: false,
        }
    }
}

impl ScoreTeamIndicator {
    /// Format the time-left as "M:SS" (truncates ≥10min cleanly).
    pub fn fmt_time_left(&self) -> String {
        let total = self.time_left_s.max(0.0) as u32;
        let m = total / 60;
        let s = total % 60;
        format!("{}:{:02}", m, s)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Reload indicator (circular progress)
// ─────────────────────────────────────────────────────────────────────────

/// Reload-progress ring around the crosshair.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReloadIndicator {
    pub progress_norm: f32,
    pub visible: bool,
}

impl ReloadIndicator {
    pub fn update(&mut self, progress_norm: f32, visible: bool) {
        self.progress_norm = progress_norm.clamp(0.0, 1.0);
        self.visible = visible;
    }

    /// How many of the ring's segments to draw, given progress.
    pub fn segments_filled(&self) -> u32 {
        if !self.visible {
            return 0;
        }
        let segs = (self.progress_norm * RELOAD_INDICATOR_SEGMENTS as f32).round() as u32;
        segs.min(RELOAD_INDICATOR_SEGMENTS)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Hit marker (x-flash on hit-confirm)
// ─────────────────────────────────────────────────────────────────────────

/// Type of hit confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HitKind {
    Body = 0,
    Headshot = 1,
    Kill = 2,
}

impl HitKind {
    pub fn color(self) -> [f32; 4] {
        match self {
            Self::Body => COLOR_HITMARK_BODY,
            Self::Headshot => COLOR_HITMARK_HEAD,
            Self::Kill => COLOR_HITMARK_KILL,
        }
    }
}

/// X-shape flash that appears briefly on hit-confirmed.
#[derive(Debug, Clone, Copy)]
pub struct HitMarker {
    pub frames_remaining: u8,
    pub last_kind: HitKind,
}

impl Default for HitMarker {
    fn default() -> Self {
        Self {
            frames_remaining: 0,
            last_kind: HitKind::Body,
        }
    }
}

impl HitMarker {
    pub fn trigger(&mut self, kind: HitKind) {
        self.last_kind = kind;
        self.frames_remaining = HIT_MARKER_FRAMES;
    }

    pub fn tick(&mut self) {
        if self.frames_remaining > 0 {
            self.frames_remaining -= 1;
        }
    }

    pub fn is_visible(&self) -> bool {
        self.frames_remaining > 0
    }

    /// Linear fade 1 → 0 over the visible window.
    pub fn current_alpha(&self) -> f32 {
        (self.frames_remaining as f32) / (HIT_MARKER_FRAMES as f32)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Killfeed (ring-buffer N=5)
// ─────────────────────────────────────────────────────────────────────────

/// One killfeed entry. `t_seen_s` drives the auto-fade.
#[derive(Debug, Clone)]
pub struct KillEntry {
    pub killer: String,
    pub victim: String,
    pub weapon: String,
    pub t_seen_s: f32,
}

/// Bounded ring-buffer of last N eliminations.
#[derive(Debug, Clone)]
pub struct Killfeed {
    pub entries: Vec<KillEntry>,
}

impl Default for Killfeed {
    fn default() -> Self {
        Self {
            entries: Vec::with_capacity(KILLFEED_CAPACITY),
        }
    }
}

impl Killfeed {
    /// Append a kill entry ; evicts oldest if capacity exceeded.
    pub fn push_entry(&mut self, killer: &str, victim: &str, weapon: &str) {
        if self.entries.len() >= KILLFEED_CAPACITY {
            self.entries.remove(0);
        }
        self.entries.push(KillEntry {
            killer: killer.to_string(),
            victim: victim.to_string(),
            weapon: weapon.to_string(),
            t_seen_s: 0.0,
        });
    }

    /// Tick all entries by `dt_s` ; auto-fade past KILLFEED_FADE_S.
    pub fn tick(&mut self, dt_s: f32) {
        for e in &mut self.entries {
            e.t_seen_s += dt_s;
        }
        self.entries.retain(|e| e.t_seen_s < KILLFEED_FADE_S);
    }

    /// Visible-alpha for the entry at `idx` (linear fade in last 1s).
    pub fn entry_alpha(&self, idx: usize) -> f32 {
        if idx >= self.entries.len() {
            return 0.0;
        }
        let age = self.entries[idx].t_seen_s;
        let fade_start = KILLFEED_FADE_S - 1.0;
        if age < fade_start {
            1.0
        } else {
            (1.0 - (age - fade_start)).max(0.0)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § FpsHud — top-level holder
// ─────────────────────────────────────────────────────────────────────────

/// Per-frame snapshot the host populates and feeds into `build_fps_hud_vertices`.
#[derive(Debug, Clone)]
pub struct FpsHud {
    pub visibility: HudVisibility,
    pub palette: ColorBlindPalette,
    pub crosshair: CrosshairState,
    pub ammo: AmmoCounter,
    pub radar: RadarState,
    pub objective: ObjectiveTracker,
    pub damage_floaters: DamageFloaterPool,
    pub health_shield: HealthShieldBar,
    pub score_team: ScoreTeamIndicator,
    pub reload: ReloadIndicator,
    pub hit_marker: HitMarker,
    pub killfeed: Killfeed,
}

impl Default for FpsHud {
    fn default() -> Self {
        Self {
            visibility: HudVisibility::default(),
            palette: ColorBlindPalette::Default,
            crosshair: CrosshairState::default(),
            ammo: AmmoCounter::default(),
            radar: RadarState::default(),
            objective: ObjectiveTracker::default(),
            damage_floaters: DamageFloaterPool::new(),
            health_shield: HealthShieldBar::default(),
            score_team: ScoreTeamIndicator::default(),
            reload: ReloadIndicator::default(),
            hit_marker: HitMarker::default(),
            killfeed: Killfeed::default(),
        }
    }
}

impl FpsHud {
    /// Per-simulation-frame tick. `dt_s` is the simulation step.
    pub fn tick(&mut self, dt_s: f32) {
        self.crosshair.tick();
        self.radar.tick(dt_s);
        self.damage_floaters.tick(dt_s);
        self.health_shield.tick();
        self.hit_marker.tick();
        self.killfeed.tick(dt_s);
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Vertex emission — push_*_quads helpers (caller-supplied Vec<UiVertex>)
// ─────────────────────────────────────────────────────────────────────────

/// Push the crosshair (style + bloom-spread + flash-color) at screen-center.
/// Returns vertex count emitted.
pub fn push_crosshair(
    sw: f32,
    sh: f32,
    state: &CrosshairState,
    out: &mut Vec<UiVertex>,
) -> usize {
    let n0 = out.len();
    let cx = sw * 0.5;
    let cy = sh * 0.5;
    let off = state.current_pip_offset();
    let len = CROSSHAIR_PIP_LEN_PX;
    let thick = CROSSHAIR_PIP_THICK_PX;
    let color = state.current_color();

    match state.style {
        CrosshairStyle::FourPip => {
            // 4 small pips, one in each cardinal direction.
            push_solid_rect(out, cx - thick * 0.5, cy - off - len, thick, len, color);
            push_solid_rect(out, cx - thick * 0.5, cy + off, thick, len, color);
            push_solid_rect(out, cx - off - len, cy - thick * 0.5, len, thick, color);
            push_solid_rect(out, cx + off, cy - thick * 0.5, len, thick, color);
        }
        CrosshairStyle::Dot => {
            push_solid_rect(out, cx - thick * 1.5, cy - thick * 1.5, thick * 3.0, thick * 3.0, color);
        }
        CrosshairStyle::Circle => {
            // Approximate by 8 small dots around a unit circle.
            for i in 0..8 {
                let theta = (i as f32) * core::f32::consts::FRAC_PI_4;
                let x = cx + theta.cos() * (off + len * 0.5);
                let y = cy + theta.sin() * (off + len * 0.5);
                push_solid_rect(out, x - thick * 0.5, y - thick * 0.5, thick, thick, color);
            }
        }
        CrosshairStyle::TShape => {
            // No top pip ; only bottom + left + right.
            push_solid_rect(out, cx - thick * 0.5, cy + off, thick, len, color);
            push_solid_rect(out, cx - off - len, cy - thick * 0.5, len, thick, color);
            push_solid_rect(out, cx + off, cy - thick * 0.5, len, thick, color);
        }
        CrosshairStyle::PlusCross => {
            // Plus sign : full vertical + full horizontal at center.
            push_solid_rect(out, cx - thick * 0.5, cy - off - len, thick, len * 2.0 + off * 2.0, color);
            push_solid_rect(out, cx - off - len, cy - thick * 0.5, len * 2.0 + off * 2.0, thick, color);
        }
    }
    out.len() - n0
}

/// Push the ammo counter at the bottom-right corner.
pub fn push_ammo_counter(
    sw: f32,
    sh: f32,
    ammo: &AmmoCounter,
    out: &mut Vec<UiVertex>,
) -> usize {
    let n0 = out.len();
    let scale = TEXT_SCALE * 1.5; // bigger ammo text
    let glyph_w = (CELL_W as f32) * scale;
    let pad = 16.0;
    // Right-justified "MM / RR" string.
    let line = format!("{:02}/{:03}", ammo.mag_current, ammo.reserve);
    let approx_w = (line.len() as f32) * glyph_w;
    let x = sw - approx_w - pad;
    let y = sh - 36.0 - pad;
    let color = ammo.color_tier();
    build_shadowed_text(&line, x, y, color, scale, out);

    // Sub-line : "mag / capacity" small text.
    let sub = format!("mag {}/{}", ammo.mag_current, ammo.mag_capacity);
    let sub_scale = TEXT_SCALE;
    let sub_w = (sub.len() as f32) * (CELL_W as f32) * sub_scale;
    build_shadowed_text(
        &sub,
        sw - sub_w - pad,
        y - 22.0,
        COLOR_DIM_TEXT,
        sub_scale,
        out,
    );
    out.len() - n0
}

/// Push the radar/compass disk at the top-left, below the standard HUD text.
pub fn push_radar(
    radar: &RadarState,
    palette: ColorBlindPalette,
    out: &mut Vec<UiVertex>,
) -> usize {
    let n0 = out.len();
    let cx = radar.center_x;
    let cy = radar.center_y;
    let r = radar.radius_px;
    // Dim glass-panel background as a single quad.
    push_solid_rect(out, cx - r, cy - r, r * 2.0, r * 2.0, COLOR_RADAR_FRAME);

    // 8 compass ticks as small dots ; "N" gets shadowed-text label.
    for (tx, ty, label) in radar.compass_ticks() {
        push_solid_rect(out, tx - 1.5, ty - 1.5, 3.0, 3.0, COLOR_RADAR_TICK);
        if label == "N" {
            build_text_quads(label, tx + 4.0, ty - 6.0, COLOR_WHITE, TEXT_SCALE * 0.75, out);
        }
    }

    // Player center indicator (always-visible 5x5 dot).
    push_solid_rect(out, cx - 2.5, cy - 2.5, 5.0, 5.0, COLOR_WHITE);

    // Friendly blips first (drawn under enemy for prominence).
    for b in &radar.friendly_pings {
        if let Some((x, y)) = radar.blip_to_radar_coord(b) {
            push_solid_rect(out, x - 2.0, y - 2.0, 4.0, 4.0, palette.friendly_color());
        }
    }

    // Enemy blips with fade-alpha.
    for b in &radar.enemy_pings {
        if let Some((x, y)) = radar.blip_to_radar_coord(b) {
            let mut col = palette.enemy_color();
            // Fade alpha proportional to remaining-life over a 2 s window.
            col[3] = (b.fade_remaining_s / 2.0).clamp(0.0, 1.0);
            push_solid_rect(out, x - 2.5, y - 2.5, 5.0, 5.0, col);
        }
    }
    out.len() - n0
}

/// Push the objective tracker at the top-right.
pub fn push_objective(
    sw: f32,
    obj: &ObjectiveTracker,
    out: &mut Vec<UiVertex>,
) -> usize {
    let n0 = out.len();
    if !obj.visible {
        return 0;
    }
    let pad = 16.0;
    let scale = TEXT_SCALE;
    let glyph_w = (CELL_W as f32) * scale;
    let line1 = format!("> {}", obj.label);
    let line2 = format!("{:6.1} m", obj.distance_m());

    let approx_w = (line1.len().max(line2.len()) as f32) * glyph_w;
    let x = sw - approx_w - pad;
    let y = 70.0;
    let color = if obj.is_behind() { COLOR_DIM_TEXT } else { COLOR_HIGHLIGHT };
    build_shadowed_text(&line1, x, y, color, scale, out);
    build_shadowed_text(&line2, x, y + 22.0, COLOR_WHITE, scale, out);

    // Tiny arrow indicator left of the label.
    let arrow_x = x - 24.0;
    let arrow_y = y + 10.0;
    let yaw = obj.relative_yaw();
    let dx = yaw.sin() * 8.0;
    let dy = -yaw.cos() * 8.0;
    push_solid_rect(out, arrow_x + dx - 1.5, arrow_y + dy - 1.5, 3.0, 3.0, color);
    out.len() - n0
}

/// Push all active damage-floaters.
pub fn push_damage_floaters(
    pool: &DamageFloaterPool,
    out: &mut Vec<UiVertex>,
) -> usize {
    let n0 = out.len();
    for f in &pool.slots {
        if !f.is_active() {
            continue;
        }
        let mut col = f.tier_color();
        col[3] = f.current_alpha();
        let scale = if f.is_crit { TEXT_SCALE * 1.4 } else { TEXT_SCALE };
        let s = format!("{}", f.damage);
        build_shadowed_text(&s, f.origin_x, f.current_y(), col, scale, out);
    }
    out.len() - n0
}

/// Push the 2-segment health/shield bar at the bottom-left.
pub fn push_health_shield(
    sh: f32,
    bar: &HealthShieldBar,
    palette: ColorBlindPalette,
    out: &mut Vec<UiVertex>,
) -> usize {
    let n0 = out.len();
    let pad = 16.0;
    let bar_w = 240.0;
    let bar_h = 12.0;
    let gap = 4.0;

    let x = pad;
    let shield_y = sh - pad - bar_h * 2.0 - gap;
    let health_y = sh - pad - bar_h;

    // Background (slate).
    push_solid_rect(out, x - 2.0, shield_y - 2.0, bar_w + 4.0, bar_h, [0.05, 0.06, 0.10, 0.85]);
    push_solid_rect(out, x - 2.0, health_y - 2.0, bar_w + 4.0, bar_h, [0.05, 0.06, 0.10, 0.85]);

    // Shield fill.
    let s_fill = bar_w * bar.shield_norm.clamp(0.0, 1.0);
    push_solid_rect(out, x, shield_y, s_fill, bar_h - 4.0, palette.shield_color());

    // Health fill (pulses red when low).
    let h_color = if bar.pulse_visible() {
        COLOR_HEALTH_PULSE
    } else {
        palette.health_color()
    };
    let h_fill = bar_w * bar.health_norm.clamp(0.0, 1.0);
    push_solid_rect(out, x, health_y, h_fill, bar_h - 4.0, h_color);
    out.len() - n0
}

/// Push the multiplayer score / team indicator at the top-center.
pub fn push_score_team(
    sw: f32,
    score: &ScoreTeamIndicator,
    out: &mut Vec<UiVertex>,
) -> usize {
    let n0 = out.len();
    if !score.multiplayer_active {
        return 0;
    }
    let scale = TEXT_SCALE * 1.25;
    let line = format!(
        "{:3} | {} | {:3}     {}",
        score.blue_score,
        score.fmt_time_left(),
        score.red_score,
        score.mode_label,
    );
    let glyph_w = (CELL_W as f32) * scale;
    let approx_w = (line.len() as f32) * glyph_w;
    let x = (sw - approx_w) * 0.5;
    let y = 16.0;
    build_shadowed_text(&line, x, y, COLOR_WHITE, scale, out);
    out.len() - n0
}

/// Push the circular reload indicator around screen-center.
pub fn push_reload_indicator(
    sw: f32,
    sh: f32,
    reload: &ReloadIndicator,
    out: &mut Vec<UiVertex>,
) -> usize {
    let n0 = out.len();
    if !reload.visible {
        return 0;
    }
    let cx = sw * 0.5;
    let cy = sh * 0.5;
    let r = RELOAD_INDICATOR_RADIUS_PX;
    let segs = reload.segments_filled();
    for i in 0..segs {
        let theta = (i as f32) / (RELOAD_INDICATOR_SEGMENTS as f32) * core::f32::consts::TAU
            - core::f32::consts::FRAC_PI_2;
        let x = cx + theta.cos() * r;
        let y = cy + theta.sin() * r;
        push_solid_rect(out, x - 2.0, y - 2.0, 4.0, 4.0, COLOR_RELOAD_AMBER);
    }
    out.len() - n0
}

/// Push the x-shape hit-marker at screen-center.
pub fn push_hit_marker(
    sw: f32,
    sh: f32,
    marker: &HitMarker,
    out: &mut Vec<UiVertex>,
) -> usize {
    let n0 = out.len();
    if !marker.is_visible() {
        return 0;
    }
    let cx = sw * 0.5;
    let cy = sh * 0.5;
    let half = 8.0;
    let thick = 2.0;
    let mut col = marker.last_kind.color();
    col[3] = marker.current_alpha();
    // 4 small diagonal pips (X-shape).
    for &(dx, dy) in &[(half, half), (-half, half), (half, -half), (-half, -half)] {
        push_solid_rect(out, cx + dx - thick * 0.5, cy + dy - thick * 0.5, thick, thick, col);
    }
    // Subtle connecting bars (X arms).
    let arm = 8.0;
    push_solid_rect(out, cx - arm * 0.5, cy - thick * 0.5, arm, thick, col);
    push_solid_rect(out, cx - thick * 0.5, cy - arm * 0.5, thick, arm, col);
    out.len() - n0
}

/// Push the killfeed at the top-right (below the objective tracker).
pub fn push_killfeed(
    sw: f32,
    feed: &Killfeed,
    out: &mut Vec<UiVertex>,
) -> usize {
    let n0 = out.len();
    let scale = TEXT_SCALE;
    let glyph_w = (CELL_W as f32) * scale;
    let pad = 16.0;
    let row_h = 22.0;
    let base_y = 130.0;
    for (i, e) in feed.entries.iter().enumerate() {
        let alpha = feed.entry_alpha(i);
        if alpha <= 0.0 {
            continue;
        }
        let text = format!("{} [{}] {}", e.killer, e.weapon, e.victim);
        let approx_w = (text.len() as f32) * glyph_w;
        let x = sw - approx_w - pad;
        let y = base_y + (i as f32) * row_h;
        let color = [1.0, 1.0, 1.0, alpha];
        let _ = build_text_quads(&text, x + 1.0, y + 1.0, [0.0, 0.0, 0.0, alpha], scale, out);
        let _ = build_text_quads(&text, x, y, color, scale, out);
    }
    out.len() - n0
}

/// Top-level frame builder. Caller-supplied `Vec<UiVertex>` ; observes
/// every `HudVisibility` toggle. Returns the count of vertices appended.
pub fn build_fps_hud_vertices(
    sw: f32,
    sh: f32,
    hud: &FpsHud,
    out: &mut Vec<UiVertex>,
) -> usize {
    let n0 = out.len();
    let v = &hud.visibility;
    if v.radar {
        // Radar requires center coords seeded — caller sets these on hud.radar.
        push_radar(&hud.radar, hud.palette, out);
    }
    if v.health_shield {
        push_health_shield(sh, &hud.health_shield, hud.palette, out);
    }
    if v.score_team {
        push_score_team(sw, &hud.score_team, out);
    }
    if v.objective {
        push_objective(sw, &hud.objective, out);
    }
    if v.killfeed {
        push_killfeed(sw, &hud.killfeed, out);
    }
    if v.damage_floaters {
        push_damage_floaters(&hud.damage_floaters, out);
    }
    if v.ammo {
        push_ammo_counter(sw, sh, &hud.ammo, out);
    }
    if v.reload_indicator {
        push_reload_indicator(sw, sh, &hud.reload, out);
    }
    if v.hit_marker {
        push_hit_marker(sw, sh, &hud.hit_marker, out);
    }
    if v.crosshair {
        push_crosshair(sw, sh, &hud.crosshair, out);
    }
    // Use COLOR_BLACK at least once symbolically so it stays in scope and
    // contrast-pair invariants are reachable from the test surface.
    let _ = COLOR_BLACK;
    out.len() - n0
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests
// ─────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ───── 1. Pool-capacity invariants ─────

    #[test]
    fn damage_floater_pool_pre_allocates_to_cap() {
        let pool = DamageFloaterPool::new();
        assert_eq!(pool.slots.len(), DAMAGE_FLOATER_POOL_CAP);
        assert_eq!(pool.slots.capacity(), DAMAGE_FLOATER_POOL_CAP);
        assert_eq!(pool.active_count(), 0);
    }

    #[test]
    fn damage_floater_pool_recycle_oldest_when_saturated() {
        let mut pool = DamageFloaterPool::new();
        // Saturate the pool with 32 fresh floaters.
        for i in 0..DAMAGE_FLOATER_POOL_CAP {
            pool.spawn(10 + i as u32, 100.0, 200.0, false);
        }
        assert_eq!(pool.active_count(), DAMAGE_FLOATER_POOL_CAP);
        // Tick the first ones down so they're "older" than the rest.
        for s in pool.slots.iter_mut().take(4) {
            s.t_remaining_s = 0.1;
        }
        let new_idx = pool.spawn(999, 50.0, 60.0, true);
        // The new entry must have replaced one of the 4 older slots.
        assert!(new_idx < 4);
        assert_eq!(pool.slots[new_idx].damage, 999);
        assert!(pool.slots[new_idx].is_crit);
    }

    #[test]
    fn damage_floater_tier_colors_select_correct_band() {
        let mut light = DamageFloater::empty();
        light.damage = 10;
        light.t_remaining_s = 1.0;
        assert_eq!(light.tier_color(), COLOR_DMG_LIGHT);

        let mut mid = DamageFloater::empty();
        mid.damage = 40;
        mid.t_remaining_s = 1.0;
        assert_eq!(mid.tier_color(), COLOR_DMG_MID);

        let mut heavy = DamageFloater::empty();
        heavy.damage = 100;
        heavy.t_remaining_s = 1.0;
        assert_eq!(heavy.tier_color(), COLOR_DMG_HEAVY);

        let mut crit = DamageFloater::empty();
        crit.damage = 200;
        crit.t_remaining_s = 1.0;
        assert_eq!(crit.tier_color(), COLOR_DMG_CRIT);
    }

    #[test]
    fn damage_floater_alpha_fades_over_time() {
        let mut f = DamageFloater::empty();
        f.damage = 50;
        f.t_remaining_s = DAMAGE_FLOATER_FADE_S;
        let a0 = f.current_alpha();
        f.t_remaining_s = DAMAGE_FLOATER_FADE_S * 0.5;
        let a1 = f.current_alpha();
        f.t_remaining_s = 0.0;
        let a2 = f.current_alpha();
        assert!(a0 > a1);
        assert!(a1 > a2);
        assert!((a2 - 0.0).abs() < f32::EPSILON);
    }

    // ───── 2. Ammo counter ─────

    #[test]
    fn ammo_update_round_trip() {
        let mut a = AmmoCounter::default();
        a.update(15, 30, 90);
        assert_eq!(a.mag_current, 15);
        assert_eq!(a.mag_capacity, 30);
        assert_eq!(a.reserve, 90);
        // Update saturates current ≤ capacity.
        a.update(99, 30, 0);
        assert_eq!(a.mag_current, 30);
    }

    #[test]
    fn ammo_low_warning_at_25_percent() {
        let mut a = AmmoCounter::default();
        // 8 / 30 ≈ 26.7% — strictly above the 25% threshold ; not low.
        a.update(8, 30, 60);
        assert!(!a.is_low());
        assert_eq!(a.color_tier(), COLOR_WHITE);
        // 7 / 30 ≈ 23.3% — below threshold ; low + warning color.
        a.update(7, 30, 60);
        assert!(a.is_low());
        assert_eq!(a.color_tier(), COLOR_WARNING);
        // 0 / 30 — empty triggers the empty-mag color (hit-flash red).
        a.update(0, 30, 60);
        assert!(a.is_empty());
        assert_eq!(a.color_tier(), COLOR_HIT_FLASH);
    }

    // ───── 3. Crosshair / bloom ─────

    #[test]
    fn crosshair_pip_offset_lerps_with_bloom() {
        let mut c = CrosshairState::default();
        c.bloom_norm = 0.0;
        let off0 = c.current_pip_offset();
        c.bloom_norm = 1.0;
        let off1 = c.current_pip_offset();
        assert!((off0 - CROSSHAIR_PIP_MIN_PX).abs() < 1e-3);
        assert!((off1 - CROSSHAIR_PIP_MAX_PX).abs() < 1e-3);
        c.bloom_norm = 0.5;
        let mid = c.current_pip_offset();
        assert!(mid > off0 && mid < off1);
    }

    #[test]
    fn crosshair_hit_flash_overrides_color_until_tick_finishes() {
        let mut c = CrosshairState::default();
        assert_eq!(c.current_color(), COLOR_CROSSHAIR);
        c.trigger_hit_flash();
        assert_eq!(c.current_color(), COLOR_HIT_FLASH);
        for _ in 0..15 {
            c.tick();
        }
        assert_eq!(c.current_color(), COLOR_CROSSHAIR);
        assert_eq!(c.flash_frames_remaining, 0);
    }

    #[test]
    fn crosshair_emits_vertices_for_each_style() {
        let mut state = CrosshairState::default();
        for i in 0..CrosshairStyle::COUNT {
            state.style = CrosshairStyle::from_index(i);
            let mut out = Vec::new();
            let n = push_crosshair(1280.0, 720.0, &state, &mut out);
            assert!(n > 0, "style {} produced 0 vertices", state.style.label());
        }
    }

    // ───── 4. Radar ─────

    #[test]
    fn radar_compass_rotates_with_player_yaw() {
        let mut r = RadarState::default();
        r.center_x = 100.0;
        r.center_y = 100.0;
        r.radius_px = 50.0;
        r.player_yaw_rad = 0.0;
        let ticks0 = r.compass_ticks();
        // North-tick at yaw=0 should be directly above center.
        let (nx, ny, label) = ticks0[0];
        assert_eq!(label, "N");
        assert!((nx - 100.0).abs() < 1e-3);
        assert!((ny - 50.0).abs() < 1e-3);
        // Rotate 90° → East-tick is now where North used to be (above).
        r.player_yaw_rad = core::f32::consts::FRAC_PI_2;
        let ticks_rot = r.compass_ticks();
        let (ex, ey, _) = ticks_rot[0];
        // After rotation, the "N"-labeled position moves left of center.
        // (As player faces east, compass-N is on the left of the radar.)
        assert!(ex < 100.0);
        assert!((ey - 100.0).abs() < 5.0);
    }

    #[test]
    fn radar_blip_in_range_returns_screen_coord() {
        let mut r = RadarState::default();
        r.center_x = 100.0;
        r.center_y = 100.0;
        r.radius_px = 50.0;
        r.player_yaw_rad = 0.0;
        let blip = RadarBlip {
            world_x: 10.0,
            world_z: 10.0,
            fade_remaining_s: 1.0,
            is_friendly: false,
        };
        let coord = r.blip_to_radar_coord(&blip);
        assert!(coord.is_some());
    }

    #[test]
    fn radar_blip_out_of_range_returns_none() {
        let mut r = RadarState::default();
        r.center_x = 100.0;
        r.center_y = 100.0;
        r.radius_px = 50.0;
        r.radius_world_m = 50.0;
        let blip = RadarBlip {
            world_x: 1000.0,
            world_z: 0.0,
            fade_remaining_s: 1.0,
            is_friendly: false,
        };
        let coord = r.blip_to_radar_coord(&blip);
        assert!(coord.is_none());
    }

    #[test]
    fn radar_pings_evict_oldest_at_cap() {
        let mut r = RadarState::default();
        for i in 0..(RADAR_ENEMY_CAP + 5) {
            r.push_enemy_blip(RadarBlip {
                world_x: i as f32,
                world_z: 0.0,
                fade_remaining_s: 2.0,
                is_friendly: false,
            });
        }
        assert_eq!(r.enemy_pings.len(), RADAR_ENEMY_CAP);
        // Newest entry survives.
        assert!(r.enemy_pings.iter().any(|b| (b.world_x - (RADAR_ENEMY_CAP as f32 + 4.0)).abs() < 1e-3));
    }

    #[test]
    fn radar_tick_decays_enemy_blips_and_drops_expired() {
        let mut r = RadarState::default();
        r.push_enemy_blip(RadarBlip {
            world_x: 0.0,
            world_z: 0.0,
            fade_remaining_s: 1.0,
            is_friendly: false,
        });
        r.push_friendly_blip(RadarBlip {
            world_x: 0.0,
            world_z: 0.0,
            fade_remaining_s: f32::INFINITY,
            is_friendly: true,
        });
        r.tick(2.0);
        assert!(r.enemy_pings.is_empty());
        assert_eq!(r.friendly_pings.len(), 1);
    }

    // ───── 5. Objective tracker ─────

    #[test]
    fn objective_distance_is_euclidean() {
        let mut obj = ObjectiveTracker::default();
        obj.player_pos = [0.0, 0.0, 0.0];
        obj.world_pos = [3.0, 0.0, 4.0];
        let d = obj.distance_m();
        assert!((d - 5.0).abs() < 1e-3);
    }

    #[test]
    fn objective_relative_yaw_wraps_in_minus_pi_to_pi() {
        let mut obj = ObjectiveTracker::default();
        obj.player_pos = [0.0, 0.0, 0.0];
        obj.world_pos = [0.0, 0.0, 1.0];
        obj.player_yaw_rad = 0.0;
        // Forward → relative-yaw ≈ 0.
        let y = obj.relative_yaw();
        assert!(y.abs() < 1e-3);
        // 90° to the right.
        obj.world_pos = [1.0, 0.0, 0.0];
        let y = obj.relative_yaw();
        assert!((y - core::f32::consts::FRAC_PI_2).abs() < 1e-3);
    }

    #[test]
    fn objective_is_behind_when_yaw_exceeds_half_pi() {
        let mut obj = ObjectiveTracker::default();
        obj.player_pos = [0.0, 0.0, 0.0];
        obj.world_pos = [0.0, 0.0, -1.0];
        obj.player_yaw_rad = 0.0;
        assert!(obj.is_behind());
        obj.world_pos = [0.0, 0.0, 1.0];
        assert!(!obj.is_behind());
    }

    #[test]
    fn objective_rejects_engagement_bait_label() {
        let mut obj = ObjectiveTracker::default();
        // Clean label accepted.
        assert!(obj.set_label("EXTRACT"));
        assert_eq!(obj.label, "EXTRACT");
        // Banned phrase rejected.
        assert!(!obj.set_label("Get Richer Today!"));
        assert_eq!(obj.label, "EXTRACT", "label preserved on rejection");
        assert!(!obj.set_label("KILLSTREAK INCOMING"));
        assert_eq!(obj.label, "EXTRACT");
    }

    #[test]
    fn engagement_bait_filter_catches_all_banned_terms() {
        assert!(is_engagement_bait("KILLSTREAK"));
        assert!(is_engagement_bait("RAMPAGE"));
        assert!(is_engagement_bait("DOMINATING SOLO"));
        assert!(is_engagement_bait("UNSTOPPABLE !!"));
        assert!(is_engagement_bait("X is on fire"));
        assert!(is_engagement_bait("godlike combo"));
        assert!(!is_engagement_bait("EXTRACT"));
        assert!(!is_engagement_bait("DEFEND ZONE A"));
        assert!(!is_engagement_bait("RESCUE THE NPC"));
    }

    // ───── 6. Health / shield ─────

    #[test]
    fn health_pulses_below_threshold_only() {
        let mut bar = HealthShieldBar::default();
        bar.update(0.5, 0.5);
        bar.pulse_frame = 0;
        assert!(!bar.pulse_visible());
        bar.update(0.0, 0.20);
        bar.pulse_frame = 0;
        assert!(bar.pulse_visible());
        bar.pulse_frame = 30;
        assert!(!bar.pulse_visible());
        bar.pulse_frame = 60;
        assert!(bar.pulse_visible());
    }

    #[test]
    fn health_shield_clamps_inputs() {
        let mut bar = HealthShieldBar::default();
        bar.update(2.0, -0.5);
        assert!((bar.shield_norm - 1.0).abs() < f32::EPSILON);
        assert!((bar.health_norm - 0.0).abs() < f32::EPSILON);
    }

    // ───── 7. Sovereign-cap (toggle restore) ─────

    #[test]
    fn hud_visibility_default_all_on() {
        let v = HudVisibility::default();
        assert_eq!(v.count_visible(), 10);
    }

    #[test]
    fn hud_visibility_toggle_then_restore() {
        let mut v = HudVisibility::default();
        v.crosshair = false;
        v.damage_floaters = false;
        v.killfeed = false;
        assert_eq!(v.count_visible(), 7);
        v.restore_defaults();
        assert_eq!(v.count_visible(), 10);
    }

    #[test]
    fn fps_hud_skips_disabled_elements() {
        let mut hud = FpsHud::default();
        hud.visibility.crosshair = false;
        hud.visibility.radar = false;
        hud.visibility.health_shield = false;
        hud.visibility.killfeed = false;
        hud.visibility.objective = false;
        hud.visibility.score_team = false;
        hud.visibility.damage_floaters = false;
        hud.visibility.ammo = false;
        hud.visibility.reload_indicator = false;
        hud.visibility.hit_marker = false;
        let mut out = Vec::new();
        let n = build_fps_hud_vertices(1280.0, 720.0, &hud, &mut out);
        assert_eq!(n, 0, "all-disabled HUD must emit zero vertices");
    }

    #[test]
    fn fps_hud_emits_vertices_when_enabled() {
        let mut hud = FpsHud::default();
        hud.radar.center_x = 80.0;
        hud.radar.center_y = 80.0;
        hud.radar.radius_px = 60.0;
        let mut out = Vec::new();
        let n = build_fps_hud_vertices(1280.0, 720.0, &hud, &mut out);
        assert!(n > 0);
        assert_eq!(out.len(), n);
    }

    // ───── 8. Color-blind palettes ─────

    #[test]
    fn color_blind_palette_switch_returns_distinct_colors() {
        let default_enemy = ColorBlindPalette::Default.enemy_color();
        let proto_enemy = ColorBlindPalette::Protanopia.enemy_color();
        let achr_enemy = ColorBlindPalette::Achromatopsia.enemy_color();
        // Each palette is structurally distinguishable from default.
        assert_ne!(default_enemy, proto_enemy);
        assert_ne!(default_enemy, achr_enemy);
        // Achromatopsia must be unsaturated (R ≈ G ≈ B).
        let r = achr_enemy[0];
        let g = achr_enemy[1];
        let b = achr_enemy[2];
        assert!((r - g).abs() < 0.05 && (g - b).abs() < 0.05);
    }

    #[test]
    fn color_blind_palette_count_is_5() {
        for i in 0..ColorBlindPalette::COUNT {
            let p = ColorBlindPalette::from_index(i);
            // Round-trip sanity.
            assert_eq!(p as u8, i);
            assert!(!p.label().is_empty());
        }
        assert_eq!(ColorBlindPalette::COUNT, 5);
    }

    #[test]
    fn color_blind_palette_pairs_have_lightness_separation() {
        // Adjacent palettes must differ in either R/G/B distinguishably so
        // they aren't visually identical.
        let palettes = [
            ColorBlindPalette::Default,
            ColorBlindPalette::Protanopia,
            ColorBlindPalette::Deuteranopia,
            ColorBlindPalette::Tritanopia,
            ColorBlindPalette::Achromatopsia,
        ];
        for i in 0..palettes.len() {
            for j in (i + 1)..palettes.len() {
                let a = palettes[i].enemy_color();
                let b = palettes[j].enemy_color();
                let delta = (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs();
                assert!(delta > 0.05, "palettes {} vs {} too close", palettes[i].label(), palettes[j].label());
            }
        }
    }

    // ───── 9. Score / killfeed / hit-marker ─────

    #[test]
    fn score_time_left_formats_minutes_seconds() {
        let mut s = ScoreTeamIndicator::default();
        s.time_left_s = 75.0;
        assert_eq!(s.fmt_time_left(), "1:15");
        s.time_left_s = 0.0;
        assert_eq!(s.fmt_time_left(), "0:00");
        s.time_left_s = -10.0;
        assert_eq!(s.fmt_time_left(), "0:00");
    }

    #[test]
    fn killfeed_evicts_oldest_at_cap() {
        let mut k = Killfeed::default();
        for i in 0..(KILLFEED_CAPACITY + 3) {
            k.push_entry(&format!("k{}", i), &format!("v{}", i), "rifle");
        }
        assert_eq!(k.entries.len(), KILLFEED_CAPACITY);
        assert_eq!(k.entries[0].killer, "k3");
    }

    #[test]
    fn killfeed_tick_drops_expired_entries() {
        let mut k = Killfeed::default();
        k.push_entry("a", "b", "rifle");
        k.push_entry("c", "d", "shotgun");
        k.tick(KILLFEED_FADE_S + 0.1);
        assert!(k.entries.is_empty());
    }

    #[test]
    fn hit_marker_visibility_decays_with_tick() {
        let mut m = HitMarker::default();
        assert!(!m.is_visible());
        m.trigger(HitKind::Headshot);
        assert!(m.is_visible());
        assert_eq!(m.last_kind, HitKind::Headshot);
        for _ in 0..(HIT_MARKER_FRAMES as u32 + 1) {
            m.tick();
        }
        assert!(!m.is_visible());
    }

    #[test]
    fn hit_marker_color_by_kind() {
        assert_eq!(HitKind::Body.color(), COLOR_HITMARK_BODY);
        assert_eq!(HitKind::Headshot.color(), COLOR_HITMARK_HEAD);
        assert_eq!(HitKind::Kill.color(), COLOR_HITMARK_KILL);
    }

    // ───── 10. Reload indicator ─────

    #[test]
    fn reload_indicator_segments_proportional_to_progress() {
        let mut r = ReloadIndicator::default();
        r.update(0.0, true);
        assert_eq!(r.segments_filled(), 0);
        r.update(1.0, true);
        assert_eq!(r.segments_filled(), RELOAD_INDICATOR_SEGMENTS);
        r.update(0.5, true);
        let half = r.segments_filled();
        assert!(half >= RELOAD_INDICATOR_SEGMENTS / 2 - 1 && half <= RELOAD_INDICATOR_SEGMENTS / 2 + 1);
        // Hidden indicator emits zero segments regardless of progress.
        r.update(0.5, false);
        assert_eq!(r.segments_filled(), 0);
    }

    // ───── 11. Top-level builder ─────

    #[test]
    fn fps_hud_tick_progresses_subsystems() {
        let mut hud = FpsHud::default();
        hud.crosshair.trigger_hit_flash();
        hud.hit_marker.trigger(HitKind::Body);
        hud.damage_floaters.spawn(50, 100.0, 200.0, false);
        hud.killfeed.push_entry("a", "b", "rifle");
        hud.radar.push_enemy_blip(RadarBlip {
            world_x: 1.0,
            world_z: 1.0,
            fade_remaining_s: 1.5,
            is_friendly: false,
        });
        // Tick a chunky slice ; expect everything to decay.
        // 12 frames clears the crosshair-flash (≤11 remaining at trigger) ;
        // 24 ticks @ 0.25 s = 6 s wallclock clears killfeed (5 s fade) +
        // radar enemy-blips (1.5 s) + damage-floaters (1.5 s).
        for _ in 0..24 {
            hud.tick(0.25);
        }
        assert_eq!(hud.crosshair.flash_frames_remaining, 0);
        assert!(!hud.hit_marker.is_visible());
        assert_eq!(hud.damage_floaters.active_count(), 0);
        assert!(hud.killfeed.entries.is_empty(), "killfeed should expire after >5s");
        assert!(hud.radar.enemy_pings.is_empty());
    }
}
