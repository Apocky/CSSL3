//! § damage — HP-tracking + damage-resolution + death detection
//! ══════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!
//! Per `combat_loop.csl` § PIPELINE STEP-6.5 and STEP-7 :
//!   t∞: damage = base × falloff × headshot-multiplier
//!   t∞: death-event = HP ≤ 0 transition ⇒ DeathEvent + LootDrop
//!   t∞: friendly-fire = configurable-per-mode (default-OFF · sovereign-toggle)
//!
//! HP is tracked per-crystal-handle in a `HashMap<u32, u32>`. A new crystal
//! that has never been damaged is implicitly at "max HP". Death is the
//! transition `pre_hp > 0 → post_hp = 0` and is idempotent.
//!
//! § ATTESTATION
//! There was no hurt nor harm in the making of this, to anyone, anything, or anybody.

use std::collections::HashMap;

pub const DEFAULT_NPC_HP: u32 = 100;
pub const FALLOFF_DEFAULT_START_M: f32 = 25.0;
pub const FALLOFF_MIN_FACTOR: f32 = 0.15;

/// Tracker for per-crystal HP. Implicitly initializes new crystals at
/// `DEFAULT_NPC_HP` on first damage.
#[derive(Debug, Default, Clone)]
pub struct HpTracker {
    hp: HashMap<u32, u32>,
}

impl HpTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_seeded_hp(seed: &[(u32, u32)]) -> Self {
        let mut hp = HashMap::with_capacity(seed.len());
        for &(handle, h) in seed {
            hp.insert(handle, h);
        }
        Self { hp }
    }

    pub fn hp(&self, handle: u32) -> u32 {
        self.hp.get(&handle).copied().unwrap_or(DEFAULT_NPC_HP)
    }

    pub fn apply(&mut self, handle: u32, damage: f32) -> (u32, u32) {
        let pre = self.hp(handle);
        if pre == 0 {
            return (0, 0);
        }
        let dmg_int = if damage.is_finite() && damage > 0.0 {
            damage.round() as u32
        } else {
            0
        };
        let post = pre.saturating_sub(dmg_int);
        self.hp.insert(handle, post);
        (pre, post)
    }

    pub fn reset(&mut self) {
        self.hp.clear();
    }
}

/// Apply static damage-falloff envelope.
pub fn apply_falloff(base: f32, range_m: f32, falloff_coef: f32, max_range_m: f32) -> f32 {
    if range_m <= FALLOFF_DEFAULT_START_M {
        return base;
    }
    if range_m >= max_range_m {
        return base * FALLOFF_MIN_FACTOR;
    }
    let span = max_range_m - FALLOFF_DEFAULT_START_M;
    let into = range_m - FALLOFF_DEFAULT_START_M;
    let mut t = into / span;
    if t < 0.0 {
        t = 0.0;
    }
    if t > 1.0 {
        t = 1.0;
    }
    let factor = 1.0 - t * (1.0 - FALLOFF_MIN_FACTOR);
    let coef = if falloff_coef <= 0.0 { 1.0 } else { falloff_coef };
    base * factor * coef
}

/// Apply headshot multiplier respecting `crit_cap_granted`.
pub fn apply_headshot(damage: f32, is_headshot: bool, crit_cap_granted: bool, crit_mult: f32) -> f32 {
    if !is_headshot {
        return damage;
    }
    if !crit_cap_granted {
        return damage;
    }
    let m = if crit_mult <= 0.0 { 2.0 } else { crit_mult };
    damage * m
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FriendlyFireMode {
    Off = 0,
    Partial = 1,
    Full = 2,
}

impl FriendlyFireMode {
    pub fn coef(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Partial => 0.25,
            Self::Full => 1.0,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § TESTS
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_handle_reports_default_hp() {
        let t = HpTracker::new();
        assert_eq!(t.hp(0xCAFE), DEFAULT_NPC_HP);
    }

    #[test]
    fn apply_decrements_hp() {
        let mut t = HpTracker::new();
        let (pre, post) = t.apply(7, 30.0);
        assert_eq!(pre, DEFAULT_NPC_HP);
        assert_eq!(post, DEFAULT_NPC_HP - 30);
    }

    #[test]
    fn apply_saturates_at_zero() {
        let mut t = HpTracker::with_seeded_hp(&[(7, 10)]);
        let (pre, post) = t.apply(7, 50.0);
        assert_eq!(pre, 10);
        assert_eq!(post, 0);
    }

    #[test]
    fn apply_to_dead_is_idempotent() {
        let mut t = HpTracker::with_seeded_hp(&[(7, 0)]);
        let (pre, post) = t.apply(7, 30.0);
        assert_eq!(pre, 0);
        assert_eq!(post, 0);
    }

    #[test]
    fn apply_zero_damage_is_noop() {
        let mut t = HpTracker::with_seeded_hp(&[(1, 50)]);
        let (pre, post) = t.apply(1, 0.0);
        assert_eq!(pre, 50);
        assert_eq!(post, 50);
    }

    #[test]
    fn falloff_at_close_range_is_full() {
        let dmg = apply_falloff(100.0, 10.0, 1.0, 80.0);
        assert!((dmg - 100.0).abs() < 0.01);
    }

    #[test]
    fn falloff_at_max_range_is_min_factor() {
        let dmg = apply_falloff(100.0, 200.0, 1.0, 80.0);
        assert!((dmg - 15.0).abs() < 0.5);
    }

    #[test]
    fn headshot_capped_no_buff() {
        let dmg = apply_headshot(50.0, true, false, 2.0);
        assert!((dmg - 50.0).abs() < 0.01);
    }

    #[test]
    fn headshot_active_uses_procgen_mult() {
        let dmg = apply_headshot(50.0, true, true, 2.5);
        assert!((dmg - 125.0).abs() < 0.01);
    }

    #[test]
    fn ff_modes_have_canonical_coefs() {
        assert_eq!(FriendlyFireMode::Off.coef(), 0.0);
        assert!((FriendlyFireMode::Partial.coef() - 0.25).abs() < 0.001);
        assert_eq!(FriendlyFireMode::Full.coef(), 1.0);
    }
}
