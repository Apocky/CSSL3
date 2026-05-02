// § params.rs — frozen mechanical tunables (canonical · skin-invariant).
// ════════════════════════════════════════════════════════════════════
// § I> All numeric tunables live here · cosmetic-only-axiom enforced
//      by the type system : `BoostAffix` (skin) cannot mutate `MovementParams`.
// § I> Stamina-economy mirrors GDD § Stamina-Budget : 5s drain / 3s recover.
// § I> Sovereign-cap toggle `infinite_sprint` is accessibility ; not payable.
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Canonical mechanical tunables for the movement-augmentation suite.
///
/// FROZEN across skins — see crate doc § COSMETIC-ONLY-AXIOM.
/// `Default` returns the published values ; tests assert that no skin-affix
/// can mutate any field here (enforced by construction in `skin.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MovementParams {
    /// Sprint multiplier on top of walk-speed.  Apex/Titanfall ≈ 1.5–1.8 ;
    /// LoA-canonical = 1.6 (per movement_aug.csl).
    pub sprint_mult: f32,
    /// Maximum sprint duration in seconds before stamina drains to 0.
    pub sprint_max_secs: f32,
    /// Time in seconds to fully recover stamina from 0 → 1.0.
    pub sprint_recover_secs: f32,
    /// Slide duration in seconds (friction-decel curve below).
    pub slide_duration_secs: f32,
    /// Slide friction coefficient (linear-decel applied per second).
    pub slide_friction: f32,
    /// Hitbox-Y reduction during slide (meters).
    pub slide_hitbox_drop_m: f32,
    /// Maximum airborne jump-count (2 = double-jump) — free for-everyone.
    pub max_jumps_in_air: u8,
    /// Jump initial vertical velocity (m/s).
    pub jump_velocity: f32,
    /// Air-control multiplier (% of ground-control). Apex ≈ 0.3.
    pub air_control: f32,
    /// Maximum wall-run duration in seconds (Apex/Titanfall feel).
    pub wall_run_max_secs: f32,
    /// Wall-run gravity dampening (0.0 = no gravity ; 1.0 = full gravity).
    pub wall_run_gravity_factor: f32,
    /// Mantle reach distance (meters) — auto-vault triggers within this radius.
    pub mantle_reach_m: f32,
    /// Mantle max ledge-height (meters) — taller ledges require a jump.
    pub mantle_max_height_m: f32,
    /// Slide-jump combo : extra horizontal velocity-boost (m/s) when jumping
    /// in the final 250ms of a slide.
    pub slide_jump_boost: f32,
}

impl MovementParams {
    /// Canonical published values — DO NOT vary by skin.
    pub const CANONICAL: Self = Self {
        sprint_mult: 1.6,
        sprint_max_secs: 5.0,
        sprint_recover_secs: 3.0,
        slide_duration_secs: 1.0,
        slide_friction: 4.0,
        slide_hitbox_drop_m: 0.6,
        max_jumps_in_air: 2,
        jump_velocity: 6.5,
        air_control: 0.30,
        wall_run_max_secs: 2.0,
        wall_run_gravity_factor: 0.15,
        mantle_reach_m: 1.0,
        mantle_max_height_m: 1.2,
        slide_jump_boost: 3.0,
    };
}

impl Default for MovementParams {
    fn default() -> Self {
        Self::CANONICAL
    }
}

/// Stamina-policy : selects between `Bounded` (default · skill-cap) and
/// `Sovereign` (accessibility toggle ; infinite-sprint).
///
/// Critically : `Sovereign` is NEVER a paid upgrade. Surfaced as an
/// accessibility setting per PRIME-DIRECTIVE consent-as-OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StaminaPolicy {
    /// Default 5s/3s drain/recover budget.
    Bounded,
    /// Infinite-sprint accessibility toggle. Stamina-bar still renders
    /// (visual continuity) but stamina never depletes.
    Sovereign,
}

impl Default for StaminaPolicy {
    fn default() -> Self {
        Self::Bounded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_values_match_spec() {
        let p = MovementParams::CANONICAL;
        assert!((p.sprint_mult - 1.6).abs() < 1e-6);
        assert!((p.sprint_max_secs - 5.0).abs() < 1e-6);
        assert!((p.sprint_recover_secs - 3.0).abs() < 1e-6);
        assert!((p.slide_duration_secs - 1.0).abs() < 1e-6);
        assert_eq!(p.max_jumps_in_air, 2);
        assert!((p.air_control - 0.30).abs() < 1e-6);
        assert!((p.wall_run_max_secs - 2.0).abs() < 1e-6);
    }

    #[test]
    fn default_policy_is_bounded() {
        assert_eq!(StaminaPolicy::default(), StaminaPolicy::Bounded);
    }
}
