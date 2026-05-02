//! § wired_weapons — per-frame weapons-tick wired into the loa-host event-loop.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W16-WIREUP-WEAPONS (W13-2 → loa-host event-loop)
//!
//! § ROLE
//!   Pre-allocates a `WeaponState` (current WeaponKind + AccuracyState +
//!   ProjectilePool) on App-start. Per-frame `tick(state, dt_ms, frame_input)`
//!   advances accuracy-recovery + steps any in-flight projectiles in the
//!   pool. Cap-gated : every mutation is rejected unless `allow_fire` is true
//!   (default-deny ; the engine integrator sets this from SOVEREIGN_CAP).
//!
//! § Σ-CAP-GATE attestation
//!   - default-deny : `WeaponInput { allow_fire: false, .. }` is a no-op.
//!   - cap-bit alignment : ¬ pay-for-power · ¬ surveillance ; the cosmetic
//!     channel is OFF in this slice (cosmetic-skin selection lives MCP-side).
//!
//! § ATTESTATION
//!   ¬ harm · ¬ control · ¬ exploitation. Hitscans / projectiles are pure
//!   data-events ; no global mutation. There was no hurt nor harm in the
//!   making of this, to anyone, anything, or anybody.

#![allow(clippy::module_name_repetitions)]

pub use cssl_host_weapons::{
    AccuracyParams, AccuracyState, ProjectilePool, TrajectoryEnv, WeaponKind, WeaponTier,
    MAX_PROJECTILES,
};
use cssl_host_weapons::HitscanTarget;
use cssl_host_weapons::projectile::ProjectileImpact;

/// § Per-frame weapon-input bundle the host fills from `InputFrame`.
/// `allow_fire` is the cap-gate — default-deny.
#[derive(Debug, Clone, Copy, Default)]
pub struct WeaponInput {
    /// True the frame the trigger fires.
    pub fired_this_frame: bool,
    /// Σ-cap-gate : weapons mutate ONLY if true. Default-deny.
    pub allow_fire: bool,
    /// Σ-cap-gate : projectile-step mutates ONLY if true. Default-allow
    /// (stepping in-flight bullets is a passive simulation, not a new
    /// effect on the world ; the cap-gate above already prevented spawn).
    pub allow_step: bool,
}

/// § Persistent weapon-state held by the host (one per local player).
/// Pre-allocated at App-construct time ; zero per-frame heap.
pub struct WeaponsState {
    /// Currently-selected weapon kind. Cycled by F1-F6 (sibling input wiring).
    pub current_kind: WeaponKind,
    /// Currently-selected weapon tier (Common..Mythic).
    pub current_tier: WeaponTier,
    /// Bloom-state (cone-of-fire growth + recovery).
    pub accuracy: AccuracyState,
    /// Pre-allocated projectile pool (256 cap · ring + free-stack).
    pub pool: ProjectilePool,
    /// Trajectory environment (gravity · drag) ; updated when scene changes.
    pub env: TrajectoryEnv,
    /// Monotonic shots-fired counter ; useful for HUD + replay manifests.
    pub shots_fired: u64,
    /// Caller-owned scratch-buffer for projectile-impact records (64 entries).
    impacts_scratch: [ProjectileImpact; 64],
}

impl Default for WeaponsState {
    fn default() -> Self {
        Self::new()
    }
}

impl WeaponsState {
    /// Construct a fresh state with default kind=Rifle · tier=Common.
    #[must_use]
    pub fn new() -> Self {
        let zero_impact = ProjectileImpact {
            projectile_id: 0,
            target_id: 0,
            impact_pos: [0.0; 3],
            damage: 0.0,
            damage_type: cssl_host_weapons::DamageType::Kinetic,
        };
        Self {
            current_kind: WeaponKind::Rifle,
            current_tier: WeaponTier::Common,
            accuracy: AccuracyState::new(AccuracyParams::PISTOL_DEFAULT),
            pool: ProjectilePool::new(),
            env: TrajectoryEnv::EARTHLIKE,
            shots_fired: 0,
            impacts_scratch: [zero_impact; 64],
        }
    }

    /// § Σ-cap-gated cycle to the next WeaponKind (F-key / MCP control).
    /// Returns true iff the cycle was applied.
    pub fn cycle_kind(&mut self, allow: bool) -> bool {
        if !allow {
            return false;
        }
        let next_u32 = (self.current_kind.as_u32() + 1) % 16;
        if let Some(k) = WeaponKind::from_u32(next_u32) {
            self.current_kind = k;
            true
        } else {
            false
        }
    }

    /// § Set a specific WeaponKind by index 0..=15 ; cap-gated.
    pub fn set_kind(&mut self, idx: u32, allow: bool) -> bool {
        if !allow {
            return false;
        }
        if let Some(k) = WeaponKind::from_u32(idx) {
            self.current_kind = k;
            true
        } else {
            false
        }
    }
}

/// § Per-frame tick — advance accuracy-recovery + step in-flight projectiles.
///
/// `dt_ms` is the wall-clock delta (1ms..100ms typical) ; we convert to
/// seconds for the inner accuracy-tick.
///
/// Σ-cap-gating : `input.allow_fire` gates the bloom-growth path. Pool-step
/// runs unconditionally because it's pure-passive (gravity + drag on
/// already-spawned bullets ; no NEW world-effect).
pub fn tick(state: &mut WeaponsState, dt_ms: f32, input: WeaponInput) {
    let dt_secs = (dt_ms.max(0.0) / 1000.0).min(0.1);

    // § Bloom growth (only if allowed AND fired-this-frame).
    if input.allow_fire && input.fired_this_frame {
        state.accuracy.on_shot();
        state.shots_fired = state.shots_fired.saturating_add(1);
    }

    // § Accuracy recovery (always tick — it's a passive decay).
    state.accuracy.tick(dt_secs);

    // § Step the projectile pool (gravity + drag on in-flight bullets).
    // Empty target-list this frame ; cssl-host-weapons sweep_collision
    // remains pure-passive (no spawns). Sibling wave wires actual targets.
    if input.allow_step {
        let empty_targets: [HitscanTarget; 0] = [];
        let _impacts =
            state
                .pool
                .step_all(state.env, dt_secs, &empty_targets, &mut state.impacts_scratch);
    }
}

// ─── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weapons_state_default_kind_is_rifle() {
        let s = WeaponsState::new();
        assert_eq!(s.current_kind, WeaponKind::Rifle);
        assert_eq!(s.shots_fired, 0);
    }

    #[test]
    fn cycle_kind_default_deny_when_cap_off() {
        let mut s = WeaponsState::new();
        let before = s.current_kind;
        let applied = s.cycle_kind(false);
        assert!(!applied);
        assert_eq!(s.current_kind, before);
    }

    #[test]
    fn cycle_kind_advances_with_cap() {
        let mut s = WeaponsState::new();
        let before = s.current_kind;
        let applied = s.cycle_kind(true);
        assert!(applied);
        assert_ne!(s.current_kind, before);
    }

    #[test]
    fn cycle_kind_wraps_after_16() {
        let mut s = WeaponsState::new();
        // Rifle = 1 ; cycle 16 times must return to Rifle.
        for _ in 0..16 {
            s.cycle_kind(true);
        }
        assert_eq!(s.current_kind, WeaponKind::Rifle);
    }

    #[test]
    fn set_kind_default_deny() {
        let mut s = WeaponsState::new();
        let applied = s.set_kind(5, false);
        assert!(!applied);
    }

    #[test]
    fn set_kind_with_cap() {
        let mut s = WeaponsState::new();
        let applied = s.set_kind(5, true);
        assert!(applied);
        assert_eq!(s.current_kind, WeaponKind::SniperProjectile);
    }

    #[test]
    fn tick_no_fire_no_shot_increment() {
        let mut s = WeaponsState::new();
        let inp = WeaponInput {
            fired_this_frame: false,
            allow_fire: true,
            allow_step: true,
        };
        tick(&mut s, 16.6, inp);
        assert_eq!(s.shots_fired, 0);
    }

    #[test]
    fn tick_fire_with_cap_increments_shot() {
        let mut s = WeaponsState::new();
        let inp = WeaponInput {
            fired_this_frame: true,
            allow_fire: true,
            allow_step: true,
        };
        tick(&mut s, 16.6, inp);
        assert_eq!(s.shots_fired, 1);
    }

    #[test]
    fn tick_fire_without_cap_is_noop() {
        let mut s = WeaponsState::new();
        let inp = WeaponInput {
            fired_this_frame: true,
            allow_fire: false, // cap denied
            allow_step: false,
        };
        tick(&mut s, 16.6, inp);
        assert_eq!(s.shots_fired, 0);
    }

    #[test]
    fn tick_clamps_dt_to_100ms() {
        let mut s = WeaponsState::new();
        let inp = WeaponInput::default();
        // 5-second debugger pause ; should NOT explode.
        tick(&mut s, 5_000.0, inp);
        // No fire ; counter unchanged.
        assert_eq!(s.shots_fired, 0);
    }

    #[test]
    fn tick_clamps_negative_dt() {
        let mut s = WeaponsState::new();
        let inp = WeaponInput::default();
        tick(&mut s, -10.0, inp);
        assert_eq!(s.shots_fired, 0);
    }
}
