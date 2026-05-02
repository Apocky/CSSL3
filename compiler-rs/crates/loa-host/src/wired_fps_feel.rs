//! § wired_fps_feel — per-frame FPS-feel tick wired into the loa-host event-loop.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W16-WIREUP-FPS-FEEL (W13-5 → loa-host event-loop)
//!
//! § ROLE
//!   Pre-allocates an `FpsFeelTick` (ADS + recoil + bloom + crosshair) per
//!   local-player. Per-frame `tick(state, dt_ms, frame_input)` advances the
//!   four sub-states. Cap-gated : every input bit (aim_held / firing) is
//!   ignored unless the corresponding cap is granted (default-deny).
//!
//! § Σ-CAP-GATE attestation
//!   - default-deny : `FpsFeelInputCapped { allow_aim: false, allow_fire:
//!     false, .. }` is a no-op tick.
//!   - cosmetic-only-axiom : recoil-PATTERN + zoom-CURVE + bloom-MAX are
//!     FROZEN per archetype ; this slice surfaces NONE of those for runtime
//!     mutation (cap-gating just throttles WHEN the FPS-feel tick fires).
//!
//! § ATTESTATION
//!   ¬ harm · ¬ control · ¬ surveillance.
//!   There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody.

#![allow(clippy::module_name_repetitions)]

pub use cssl_host_fps_feel::{
    AdsState, BloomState, CrosshairSkin, CrosshairState, FpsFeelInput, FpsFeelTick,
    HitFlashKind, RecoilState, WeaponArchetype,
};

/// § Per-frame fps-feel input bundle from the host's `InputFrame`.
/// `allow_*` are cap-gates ; default-deny.
#[derive(Debug, Clone, Copy, Default)]
pub struct FpsFeelInputCapped {
    pub aim_held: bool,
    pub firing: bool,
    pub pull_down_input_rad: f32,
    pub hit_landed: bool,
    pub hit_kind_byte: u8, // 0=Body 1=Head 2=Kill (HitFlashKind discriminants)
    /// Σ-cap-gate : aim transitions ONLY if true. Default-deny.
    pub allow_aim: bool,
    /// Σ-cap-gate : fire transitions ONLY if true. Default-deny.
    pub allow_fire: bool,
}

/// § Persistent FPS-feel state (one per local-player, swappable on weapon-change).
pub struct FpsFeelState {
    pub feel: FpsFeelTick,
    /// Tick-counter for telemetry / replay-manifest tagging.
    pub frame_count: u64,
}

impl Default for FpsFeelState {
    fn default() -> Self {
        Self::new(WeaponArchetype::Rifle, 0xC0FFEE)
    }
}

impl FpsFeelState {
    /// Construct fresh state for `archetype` with deterministic RNG seed.
    #[must_use]
    pub fn new(archetype: WeaponArchetype, rng_seed: u64) -> Self {
        Self {
            feel: FpsFeelTick::new(archetype, rng_seed),
            frame_count: 0,
        }
    }

    /// § Σ-cap-gated weapon-swap (resets recoil pattern to new archetype).
    pub fn swap_archetype(&mut self, a: WeaponArchetype, allow: bool) -> bool {
        if !allow {
            return false;
        }
        self.feel.reset(Some(a));
        true
    }
}

fn hit_kind_from_byte(b: u8) -> HitFlashKind {
    match b {
        1 => HitFlashKind::Head,
        2 => HitFlashKind::Kill,
        _ => HitFlashKind::Body,
    }
}

/// Per-frame tick — advance ADS / recoil / bloom / crosshair sub-states.
///
/// Cap-gating : input bits are AND'd with their `allow_*` gate so a
/// no-cap caller produces a NEUTRAL input even when the player is mashing
/// the fire button (defense-against-stuck-input + sovereign-revoke flow).
pub fn tick(state: &mut FpsFeelState, dt_ms: f32, input: FpsFeelInputCapped) {
    let dt = dt_ms.max(0.0).min(100.0);
    let gated = FpsFeelInput {
        aim_held: input.aim_held && input.allow_aim,
        firing: input.firing && input.allow_fire,
        pull_down_input_rad: input.pull_down_input_rad,
        hit_landed: input.hit_landed,
        hit_kind: hit_kind_from_byte(input.hit_kind_byte),
    };
    state.feel.step(gated, dt);
    state.frame_count = state.frame_count.wrapping_add(1);
}

// ─── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_zeroed() {
        let s = FpsFeelState::default();
        assert_eq!(s.frame_count, 0);
        assert_eq!(s.feel.bloom.current_rad, 0.0);
        assert_eq!(s.feel.recoil.pitch_rad, 0.0);
    }

    #[test]
    fn tick_with_caps_off_is_neutral() {
        let mut s = FpsFeelState::default();
        let inp = FpsFeelInputCapped {
            firing: true,
            aim_held: true,
            allow_aim: false,
            allow_fire: false,
            ..Default::default()
        };
        for _ in 0..10 {
            tick(&mut s, 16.6, inp);
        }
        // No fire → bloom stays zero ; ads stays at 0.0 (no zoom).
        assert!(s.feel.bloom.current_rad < 1e-6);
    }

    #[test]
    fn tick_with_fire_cap_grows_bloom() {
        let mut s = FpsFeelState::default();
        let inp = FpsFeelInputCapped {
            firing: true,
            allow_fire: true,
            ..Default::default()
        };
        for _ in 0..10 {
            tick(&mut s, 16.6, inp);
        }
        assert!(s.feel.bloom.current_rad > 0.0);
    }

    #[test]
    fn swap_archetype_default_deny() {
        let mut s = FpsFeelState::default();
        let applied = s.swap_archetype(WeaponArchetype::Sniper, false);
        assert!(!applied);
        assert_eq!(s.feel.recoil.archetype, WeaponArchetype::Rifle);
    }

    #[test]
    fn swap_archetype_with_cap() {
        let mut s = FpsFeelState::default();
        let applied = s.swap_archetype(WeaponArchetype::Sniper, true);
        assert!(applied);
        assert_eq!(s.feel.recoil.archetype, WeaponArchetype::Sniper);
    }

    #[test]
    fn frame_counter_increments() {
        let mut s = FpsFeelState::default();
        for _ in 0..7 {
            tick(&mut s, 16.6, FpsFeelInputCapped::default());
        }
        assert_eq!(s.frame_count, 7);
    }

    #[test]
    fn hit_kind_byte_decode_table() {
        assert!(matches!(hit_kind_from_byte(0), HitFlashKind::Body));
        assert!(matches!(hit_kind_from_byte(1), HitFlashKind::Head));
        assert!(matches!(hit_kind_from_byte(2), HitFlashKind::Kill));
        assert!(matches!(hit_kind_from_byte(99), HitFlashKind::Body));
    }
}
