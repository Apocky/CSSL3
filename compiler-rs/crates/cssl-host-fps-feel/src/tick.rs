// § tick.rs — Master FpsFeelTick aggregator
// ════════════════════════════════════════════════════════════════════
// § I> per spec §I.role : "4 disjoint feel-surfaces wire to single FpsFeelTick"
//      — composes ads + recoil + bloom + crosshair into a single
//      pure-deterministic step() called once per frame from loa-host.
// § I> input layer (cssl-host-input W13-11) supplies FpsFeelInput per frame.
//      camera (W13-4) consumes ADS + recoil events.
//      weapons (W13-2) consume bloom-cone override + see recoil-events.
//      hud (W13-7) consumes CrosshairBloomPx.

use serde::{Deserialize, Serialize};

use crate::ads::{AdsState, AdsZoomFovOverride};
use crate::bloom::{AccuracyConeRadiansOverride, BloomState};
use crate::crosshair::{CrosshairBloomPx, CrosshairState, HitFlashKind};
use crate::recoil::{RecoilEvent, RecoilState, WeaponArchetype};
use crate::seed::DeterministicRng;

/// Per-frame input bundle from cssl-host-input (W13-11).
/// Stays Copy + serde for replay-manifest archival.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FpsFeelInput {
    /// True while the aim button is held (or toggle is active).
    pub aim_held: bool,
    /// True on the frame the player fired.
    pub firing: bool,
    /// Player-supplied downward camera-correction for skill-floor recoil-counter (radians).
    pub pull_down_input_rad: f32,
    /// True on the frame the player landed a hit.
    pub hit_landed: bool,
    /// Type of hit (body / head / kill) if `hit_landed`.
    pub hit_kind: HitFlashKind,
}

impl Default for FpsFeelInput {
    fn default() -> Self {
        Self {
            aim_held: false,
            firing: false,
            pull_down_input_rad: 0.0,
            hit_landed: false,
            hit_kind: HitFlashKind::Body,
        }
    }
}

/// Aggregator state — one per local-player. All four sub-states are public so
/// host code can introspect / serialize / debug-render them directly.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FpsFeelTick {
    pub ads: AdsState,
    pub recoil: RecoilState,
    pub bloom: BloomState,
    pub crosshair: CrosshairState,
    pub rng: DeterministicRng,
    /// Frame counter ; increments each step ; useful for replay-manifest tag.
    pub frame: u64,
}

impl FpsFeelTick {
    /// Construct a fresh tick state for `archetype` with deterministic RNG seed.
    #[must_use]
    pub const fn new(archetype: WeaponArchetype, rng_seed: u64) -> Self {
        Self {
            ads: AdsState::new(),
            recoil: RecoilState::new(archetype),
            bloom: BloomState::new(),
            crosshair: CrosshairState::new(),
            rng: DeterministicRng::new(rng_seed),
            frame: 0,
        }
    }

    /// Apply input + advance time. Returns nothing — host queries snapshot
    /// methods to build the per-frame events for camera / weapons / hud.
    pub fn step(&mut self, input: FpsFeelInput, dt_ms: f32) {
        let dt = dt_ms.max(0.0);

        // 1. ADS — drive zoom progress from input.aim_held.
        self.ads.step(dt, input.aim_held);

        // 2. Recoil — apply fire then advance recovery + skill-floor counter.
        if input.firing {
            self.recoil.on_fire(&mut self.rng);
        }
        self.recoil.step(dt, input.pull_down_input_rad);

        // 3. Bloom — apply fire then advance grace/decay.
        if input.firing {
            self.bloom.on_fire(self.ads.engaged);
        }
        self.bloom.step(dt);

        // 4. Crosshair — trigger flash then advance time.
        if input.hit_landed {
            self.crosshair.on_hit(input.hit_kind);
        }
        self.crosshair.step(dt);

        self.frame = self.frame.wrapping_add(1);
    }

    /// Snapshot the camera-host event (W13-4 consumes).
    #[must_use]
    pub fn ads_event(&self) -> AdsZoomFovOverride {
        AdsZoomFovOverride::from(&self.ads)
    }

    /// Snapshot the recoil event (W13-4 + W13-2 consume).
    #[must_use]
    pub fn recoil_event(&self) -> RecoilEvent {
        RecoilEvent::from(&self.recoil)
    }

    /// Snapshot the weapons-host bloom-cone override (W13-2 consumes).
    #[must_use]
    pub fn bloom_event(&self) -> AccuracyConeRadiansOverride {
        AccuracyConeRadiansOverride::snapshot(&self.bloom, self.ads.engaged)
    }

    /// Snapshot the HUD crosshair event (W13-7 consumes).
    /// `screen_height_px` and `fov_deg` are used for radian → pixel conversion ;
    /// pass the actual current eased-fov so the pip scales correctly with ADS.
    #[must_use]
    pub fn crosshair_event(&self, screen_height_px: f32) -> CrosshairBloomPx {
        CrosshairBloomPx::snapshot(
            &self.crosshair,
            self.bloom.current_rad,
            screen_height_px,
            self.ads.fov_deg(),
            self.ads.scope_overlay_visible(),
        )
    }

    /// Reset for weapon-swap / death.
    pub fn reset(&mut self, new_archetype: Option<WeaponArchetype>) {
        if let Some(a) = new_archetype {
            self.recoil = RecoilState::new(a);
        } else {
            self.recoil.reset();
        }
        self.ads.reset();
        self.bloom.reset();
        self.crosshair.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_tick_is_zeroed() {
        let t = FpsFeelTick::new(WeaponArchetype::Rifle, 0xABCD);
        assert_eq!(t.bloom.current_rad, 0.0);
        assert_eq!(t.recoil.pitch_rad, 0.0);
        assert_eq!(t.ads.progress, 0.0);
        assert!(!t.crosshair.flash_active());
        assert_eq!(t.frame, 0);
    }

    #[test]
    fn full_tick_integration_deterministic() {
        // Two parallel ticks driven by identical input + seed should
        // produce bit-equal state ; this is the master replay-bit-equal test.
        let mut a = FpsFeelTick::new(WeaponArchetype::Smg, 0xF00DBABE);
        let mut b = FpsFeelTick::new(WeaponArchetype::Smg, 0xF00DBABE);
        let inputs = [
            FpsFeelInput { aim_held: false, firing: true, ..FpsFeelInput::default() },
            FpsFeelInput { aim_held: true,  firing: true, ..FpsFeelInput::default() },
            FpsFeelInput { aim_held: true,  firing: true, ..FpsFeelInput::default() },
            FpsFeelInput { aim_held: true,  firing: false, hit_landed: true, hit_kind: HitFlashKind::Head, ..FpsFeelInput::default() },
            FpsFeelInput::default(),
            FpsFeelInput::default(),
        ];
        for inp in inputs {
            a.step(inp, 16.6);
            b.step(inp, 16.6);
        }
        assert_eq!(a, b);
    }

    #[test]
    fn integration_bloom_grows_during_sustained_fire() {
        let mut t = FpsFeelTick::new(WeaponArchetype::Rifle, 1);
        let fire = FpsFeelInput { aim_held: false, firing: true, ..FpsFeelInput::default() };
        for _ in 0..20 {
            t.step(fire, 10.0);
        }
        assert!(t.bloom.current_rad > 0.0);
    }

    #[test]
    fn integration_recovery_after_fire_stop() {
        let mut t = FpsFeelTick::new(WeaponArchetype::Lmg, 2);
        let fire = FpsFeelInput { aim_held: false, firing: true, ..FpsFeelInput::default() };
        for _ in 0..10 {
            t.step(fire, 10.0);
        }
        let pitch_after_fire = t.recoil.pitch_rad;
        let bloom_after_fire = t.bloom.current_rad;
        assert!(pitch_after_fire > 0.0);
        assert!(bloom_after_fire > 0.0);
        // 1 second of no input
        let idle = FpsFeelInput::default();
        for _ in 0..100 {
            t.step(idle, 10.0);
        }
        // Recoil should have fully recovered ; bloom should be ≪ initial.
        assert!(t.recoil.pitch_rad < pitch_after_fire * 0.05);
        assert!(t.bloom.current_rad < bloom_after_fire * 0.5);
    }

    #[test]
    fn reset_with_new_archetype() {
        let mut t = FpsFeelTick::new(WeaponArchetype::Pistol, 3);
        t.reset(Some(WeaponArchetype::Sniper));
        assert_eq!(t.recoil.archetype, WeaponArchetype::Sniper);
    }
}
