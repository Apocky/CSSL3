// § aug.rs — the movement-augmentation state machine.
// ════════════════════════════════════════════════════════════════════
// § I> Pure-deterministic ; fixed-step ; replay-bit-equal.
// § I> Accepts a per-frame `WorldHints` from the host (no path-dep on physics).
// § I> Returns a `ProposedMotion { delta, state_update }` ; host commits.
// § I> COSMETIC-ONLY-AXIOM enforced : skin/affix never reads or writes here.
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

use crate::intent::MovementIntent;
use crate::params::{MovementParams, StaminaPolicy};
use crate::state::{LocomotionPhase, MovementState, WallSide};

/// Hints from the host's spatial-query layer. We never call physics ourselves.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct WorldHints {
    /// True if the player capsule's feet touch ground.
    pub on_ground: bool,
    /// True if there's a runnable surface to the player's left within reach.
    pub wall_left: bool,
    /// True if there's a runnable surface to the player's right within reach.
    pub wall_right: bool,
    /// True if a low-ledge mantle target is within `mantle_reach_m` ahead.
    pub mantle_target_ahead: bool,
    /// World-space gravity (m/s²). Negative = down (e.g. -9.81).
    pub gravity: f32,
}

impl WorldHints {
    pub const fn ground() -> Self {
        Self {
            on_ground: true,
            wall_left: false,
            wall_right: false,
            mantle_target_ahead: false,
            gravity: -9.81,
        }
    }

    pub const fn airborne() -> Self {
        Self {
            on_ground: false,
            wall_left: false,
            wall_right: false,
            mantle_target_ahead: false,
            gravity: -9.81,
        }
    }
}

/// Output of `MovementAug::tick`. The host commits `delta` to its camera
/// + records the `phase` for renderer overlays + audio cues.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProposedMotion {
    /// World-space position-delta (m).
    pub delta: [f32; 3],
    /// Effective horizontal-speed multiplier applied this tick (for VFX).
    pub speed_mult: f32,
    /// Hitbox-Y offset (m ; negative = lower ; for slide hitbox-drop).
    pub hitbox_drop: f32,
    /// True if the renderer should emit a "boost spawned" event (FX/audio).
    pub boost_emit: bool,
}

impl Default for ProposedMotion {
    fn default() -> Self {
        Self {
            delta: [0.0, 0.0, 0.0],
            speed_mult: 1.0,
            hitbox_drop: 0.0,
            boost_emit: false,
        }
    }
}

/// The movement-augmentation engine. One per local player.
///
/// All timing is fixed-step ; the caller passes `dt` (seconds) on each tick.
/// The state machine is pure — no global state, no I/O, no `std::time`.
#[derive(Debug, Clone, Copy)]
pub struct MovementAug {
    pub state: MovementState,
    pub params: MovementParams,
    pub stamina_policy: StaminaPolicy,
    /// Walk-speed in m/s ; the host's canonical walk-speed (matches
    /// `loa-host::movement::SPEED_M_PER_S`). Local copy avoids path-dep.
    pub walk_speed: f32,
    /// True only on the FRAME of a slide-end ; consumed by combo logic.
    pub slide_just_ended: bool,
    /// Latches `true` when a slide times-out while crouch is still held.
    /// Forces caller to RELEASE crouch before another slide can begin —
    /// prevents the timer-cap oscillation seen pre-fix.
    pub slide_cooldown: bool,
    /// Latches `true` when a wall-run times-out. Cleared on grounding.
    pub wall_run_cooldown: bool,
}

impl MovementAug {
    pub const fn new(walk_speed: f32) -> Self {
        Self {
            state: MovementState::new(),
            params: MovementParams::CANONICAL,
            stamina_policy: StaminaPolicy::Bounded,
            walk_speed,
            slide_just_ended: false,
            slide_cooldown: false,
            wall_run_cooldown: false,
        }
    }

    /// Set stamina-policy (accessibility toggle).
    pub fn set_stamina_policy(&mut self, policy: StaminaPolicy) {
        self.stamina_policy = policy;
    }

    /// True if the player is in any "boosted" phase that the renderer should
    /// decorate with a trail/VFX.
    pub fn is_boosted(&self) -> bool {
        matches!(
            self.state.phase,
            LocomotionPhase::Sprinting
                | LocomotionPhase::Sliding
                | LocomotionPhase::WallRunning
                | LocomotionPhase::Mantling
        )
    }

    /// Effective sprint multiplier — gates on stamina & policy.
    pub fn effective_sprint_mult(&self, intent: &MovementIntent) -> f32 {
        if !intent.sprint_held {
            return 1.0;
        }
        match self.stamina_policy {
            StaminaPolicy::Sovereign => self.params.sprint_mult,
            StaminaPolicy::Bounded => {
                if self.state.stamina > 0.0 {
                    self.params.sprint_mult
                } else {
                    1.0
                }
            }
        }
    }

    /// Advance stamina based on phase + policy.
    fn tick_stamina(&mut self, intent: &MovementIntent, dt: f32) {
        if matches!(self.stamina_policy, StaminaPolicy::Sovereign) {
            self.state.stamina = 1.0;
            return;
        }
        let drain_rate = 1.0 / self.params.sprint_max_secs;
        let recover_rate = 1.0 / self.params.sprint_recover_secs;
        let sprint_drains = matches!(self.state.phase, LocomotionPhase::Sprinting)
            && intent.sprint_held
            && intent.has_horiz_input();
        if sprint_drains {
            self.state.stamina = (self.state.stamina - drain_rate * dt).max(0.0);
        } else {
            self.state.stamina = (self.state.stamina + recover_rate * dt).min(1.0);
        }
    }

    /// Determine the phase for THIS tick based on intent + hints + prev-state.
    fn pick_phase(
        &self,
        intent: &MovementIntent,
        hints: &WorldHints,
    ) -> (LocomotionPhase, Option<WallSide>) {
        // Mantle takes precedence (auto-vault).
        if hints.mantle_target_ahead && hints.on_ground && intent.forward > 0.1 {
            return (LocomotionPhase::Mantling, None);
        }

        // If currently mantling, finish the animation (fixed-step duration ≈ 0.4s).
        if matches!(self.state.phase, LocomotionPhase::Mantling) && self.state.phase_time < 0.4 {
            return (LocomotionPhase::Mantling, None);
        }

        // Slide-cap : if currently sliding AND timer expired → must exit ;
        // the cooldown (set in `tick()`) prevents immediate re-entry until
        // crouch is RELEASED. Jump-press also cancels the slide.
        let slide_timer_expired = matches!(self.state.phase, LocomotionPhase::Sliding)
            && self.state.phase_time >= self.params.slide_duration_secs;

        // If currently sliding and timer not yet expired, stay sliding.
        if matches!(self.state.phase, LocomotionPhase::Sliding)
            && !slide_timer_expired
            && !intent.jump_pressed
        {
            return (LocomotionPhase::Sliding, None);
        }

        // Wall-run : airborne + wall-side + below max-time + no jump-press.
        // Once the cap-cooldown is set, no further wall-run until grounding.
        if !hints.on_ground
            && (hints.wall_left || hints.wall_right)
            && intent.has_horiz_input()
            && !self.wall_run_cooldown
        {
            let curr_wall = matches!(self.state.phase, LocomotionPhase::WallRunning);
            let allowed = !curr_wall || self.state.phase_time < self.params.wall_run_max_secs;
            if allowed {
                let side = if hints.wall_left {
                    WallSide::Left
                } else {
                    WallSide::Right
                };
                return (LocomotionPhase::WallRunning, Some(side));
            }
        }

        // Airborne ?
        if !hints.on_ground {
            return (LocomotionPhase::Airborne, None);
        }

        // Slide entry : crouch-while-sprinting + on-ground + cooldown clear.
        // Only entered from a Sprinting prior phase (not from Sliding) so
        // hold-crouch-through-end-of-slide does NOT immediately re-enter.
        if intent.crouch_held
            && matches!(self.state.phase, LocomotionPhase::Sprinting)
            && hints.on_ground
            && !self.slide_cooldown
        {
            return (LocomotionPhase::Sliding, None);
        }

        // Sprint requires sprint_held + has-horiz-input + (stamina>0 OR Sovereign)
        // + slide-cooldown CLEAR (don't bounce out of an expired slide back into
        // a sprint that would re-trigger the slide).
        let stamina_ok = self.state.stamina > 0.0
            || matches!(self.stamina_policy, StaminaPolicy::Sovereign);
        if intent.sprint_held
            && intent.has_horiz_input()
            && stamina_ok
            && !self.slide_cooldown
        {
            return (LocomotionPhase::Sprinting, None);
        }

        (LocomotionPhase::Walking, None)
    }

    /// Advance the engine by `dt` seconds. Returns the proposed-motion delta
    /// for the host to commit (after physics-validation, if any).
    pub fn tick(
        &mut self,
        intent: &MovementIntent,
        camera_forward_xz: [f32; 2],
        camera_right_xz: [f32; 2],
        dt: f32,
        hints: &WorldHints,
    ) -> ProposedMotion {
        // 1. phase-decision
        let prev_phase = self.state.phase;
        // Detect timer-cap conditions BEFORE picking next phase so cooldown
        // latches see the timer-expired state.
        let slide_was_capped = matches!(prev_phase, LocomotionPhase::Sliding)
            && self.state.phase_time >= self.params.slide_duration_secs;
        let wall_was_capped = matches!(prev_phase, LocomotionPhase::WallRunning)
            && self.state.phase_time >= self.params.wall_run_max_secs;

        let (next_phase, wall_side) = self.pick_phase(intent, hints);

        // detect slide-end (for slide-jump combo boost).
        self.slide_just_ended = matches!(prev_phase, LocomotionPhase::Sliding)
            && !matches!(next_phase, LocomotionPhase::Sliding);

        // Cooldown latches : SET on cap-end ; CLEAR on input-release / ground.
        if slide_was_capped {
            self.slide_cooldown = true;
        }
        if !intent.crouch_held {
            self.slide_cooldown = false;
        }
        if wall_was_capped {
            self.wall_run_cooldown = true;
        }
        if hints.on_ground {
            self.wall_run_cooldown = false;
        }

        if next_phase == prev_phase {
            self.state.advance_phase_time(dt);
            if matches!(next_phase, LocomotionPhase::WallRunning) {
                self.state.wall_side = wall_side;
            }
        } else {
            self.state.enter_phase(next_phase);
            self.state.wall_side = wall_side;
        }

        // 2. ground-reset for jumps
        if hints.on_ground && !matches!(next_phase, LocomotionPhase::Airborne) {
            self.state.on_ground();
        }

        // 3. stamina
        self.tick_stamina(intent, dt);

        // 4. jump processing — applies on the JUMP edge regardless of phase
        let mut boost_emit = false;
        if intent.jump_pressed {
            if hints.on_ground || matches!(next_phase, LocomotionPhase::WallRunning) {
                // Ground-jump or wall-kick : free + reset jump-count.
                self.state.vy = self.params.jump_velocity;
                self.state.air_jumps_used = 0;
                self.state.enter_phase(LocomotionPhase::Airborne);
                boost_emit = true;
            } else if self.state.air_jumps_used < self.params.max_jumps_in_air {
                // Mid-air double-jump.
                self.state.vy = self.params.jump_velocity;
                self.state.air_jumps_used += 1;
                boost_emit = true;
            }

            // slide-jump combo : if we just-ended a slide, add momentum boost.
            if self.slide_just_ended {
                let mag = self.state.momentum_xz[0].hypot(self.state.momentum_xz[1]);
                let boost = self.params.slide_jump_boost;
                if mag > 1e-3 {
                    let scale = (mag + boost) / mag;
                    self.state.momentum_xz[0] *= scale;
                    self.state.momentum_xz[1] *= scale;
                } else if intent.has_horiz_input() {
                    // No prior momentum but had input — boost in input direction.
                    let f = intent.forward;
                    let r = intent.right;
                    let imag = f.hypot(r).max(1e-3);
                    let bx = (camera_forward_xz[0] * f + camera_right_xz[0] * r) / imag;
                    let bz = (camera_forward_xz[1] * f + camera_right_xz[1] * r) / imag;
                    self.state.momentum_xz[0] = bx * boost;
                    self.state.momentum_xz[1] = bz * boost;
                }
            }
        }

        // 5. compute horizontal delta
        let speed_mult = match next_phase {
            LocomotionPhase::Sprinting => self.effective_sprint_mult(intent),
            LocomotionPhase::Sliding => {
                // Speed decays linearly with phase_time.
                let decay = (1.0 - self.state.phase_time / self.params.slide_duration_secs)
                    .clamp(0.0, 1.0);
                self.params.sprint_mult * decay.max(0.6)
            }
            LocomotionPhase::WallRunning => self.params.sprint_mult * 0.9,
            LocomotionPhase::Mantling => 0.5,
            LocomotionPhase::Airborne => 1.0,
            LocomotionPhase::Walking => 1.0,
        };

        let air_factor = if matches!(next_phase, LocomotionPhase::Airborne) {
            self.params.air_control
        } else {
            1.0
        };

        // Horizontal input vector → world-space.
        let f_in = intent.forward;
        let r_in = intent.right;
        let mag = f_in.hypot(r_in).max(1e-6);
        let inv = if mag > 1.0 { 1.0 / mag } else { 1.0 };
        let dir_x = (camera_forward_xz[0] * f_in + camera_right_xz[0] * r_in) * inv;
        let dir_z = (camera_forward_xz[1] * f_in + camera_right_xz[1] * r_in) * inv;

        let speed = self.walk_speed * speed_mult * air_factor;
        let mut dx = dir_x * speed * dt;
        let mut dz = dir_z * speed * dt;

        // Slide momentum-preserve : while sliding we add stored momentum on top.
        if matches!(next_phase, LocomotionPhase::Sliding) {
            // store / decay momentum
            if matches!(prev_phase, LocomotionPhase::Sprinting) {
                // Just entered slide : capture momentum.
                self.state.momentum_xz[0] = dir_x * self.walk_speed * self.params.sprint_mult;
                self.state.momentum_xz[1] = dir_z * self.walk_speed * self.params.sprint_mult;
                boost_emit = true;
            }
            // Apply friction-decay.
            let decay_mult = (1.0 - self.params.slide_friction * dt).max(0.0);
            self.state.momentum_xz[0] *= decay_mult;
            self.state.momentum_xz[1] *= decay_mult;
            dx += self.state.momentum_xz[0] * dt;
            dz += self.state.momentum_xz[1] * dt;
        } else {
            // Decay any leftover momentum.
            let decay_mult = (1.0 - 2.0 * dt).max(0.0);
            self.state.momentum_xz[0] *= decay_mult;
            self.state.momentum_xz[1] *= decay_mult;
        }

        // 6. vertical : gravity + jumps + wall-run gravity-dampen
        let g = if matches!(next_phase, LocomotionPhase::WallRunning) {
            hints.gravity * self.params.wall_run_gravity_factor
        } else if matches!(next_phase, LocomotionPhase::Airborne) {
            hints.gravity
        } else {
            0.0
        };
        if !hints.on_ground {
            self.state.vy += g * dt;
        } else if !intent.jump_pressed {
            self.state.vy = 0.0;
        }
        let dy = self.state.vy * dt;

        // Mantle handles its own delta (fixed lift).
        let (dx, dy, dz) = if matches!(next_phase, LocomotionPhase::Mantling) {
            // Lift over ~0.4s : raise by mantle_max_height_m + small forward step.
            let lift = self.params.mantle_max_height_m / 0.4 * dt;
            let fwd_step = self.params.mantle_reach_m / 0.4 * dt;
            (
                camera_forward_xz[0] * fwd_step,
                lift,
                camera_forward_xz[1] * fwd_step,
            )
        } else {
            (dx, dy, dz)
        };

        // 7. hitbox drop while sliding
        let hitbox_drop = if matches!(next_phase, LocomotionPhase::Sliding) {
            -self.params.slide_hitbox_drop_m
        } else {
            0.0
        };

        ProposedMotion {
            delta: [dx, dy, dz],
            speed_mult,
            hitbox_drop,
            boost_emit,
        }
    }
}

impl Default for MovementAug {
    fn default() -> Self {
        // 5.0 m/s matches loa-host::movement::SPEED_M_PER_S canonical value.
        Self::new(5.0)
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    fn fwd_xz() -> ([f32; 2], [f32; 2]) {
        // yaw=0 : forward = (0, -1), right = (1, 0).
        ([0.0, -1.0], [1.0, 0.0])
    }

    fn intent_walk_forward() -> MovementIntent {
        MovementIntent {
            forward: 1.0,
            ..Default::default()
        }
    }

    #[test]
    fn sprint_doubles_speed_when_held() {
        let mut a = MovementAug::default();
        let (f, r) = fwd_xz();
        let mut i = intent_walk_forward();
        i.sprint_held = true;
        let m = a.tick(&i, f, r, 0.1, &WorldHints::ground());
        // walk_speed * 1.6 * 0.1 = 5.0 * 1.6 * 0.1 = 0.8 along -Z
        assert!((m.delta[2] - (-0.8)).abs() < 1e-4);
        assert_eq!(a.state.phase, LocomotionPhase::Sprinting);
    }

    #[test]
    fn stamina_drains_during_sprint_then_caps_speed() {
        let mut a = MovementAug::default();
        let (f, r) = fwd_xz();
        let mut i = intent_walk_forward();
        i.sprint_held = true;
        // Drain for 6 seconds (max is 5) — stamina hits 0.
        for _ in 0..600 {
            a.tick(&i, f, r, 0.01, &WorldHints::ground());
        }
        assert!((a.state.stamina - 0.0).abs() < 1e-3);
        // Now sprint should fall to walk-speed.
        let m = a.tick(&i, f, r, 0.1, &WorldHints::ground());
        assert!((m.speed_mult - 1.0).abs() < 1e-4);
    }

    #[test]
    fn stamina_recovers_when_idle() {
        let mut a = MovementAug::default();
        a.state.stamina = 0.0;
        let (f, r) = fwd_xz();
        let i = MovementIntent::default();
        // Recover for 3 seconds.
        for _ in 0..300 {
            a.tick(&i, f, r, 0.01, &WorldHints::ground());
        }
        assert!((a.state.stamina - 1.0).abs() < 1e-3);
    }

    #[test]
    fn sovereign_policy_never_drains() {
        let mut a = MovementAug::default();
        a.set_stamina_policy(StaminaPolicy::Sovereign);
        let (f, r) = fwd_xz();
        let mut i = intent_walk_forward();
        i.sprint_held = true;
        for _ in 0..1000 {
            a.tick(&i, f, r, 0.01, &WorldHints::ground());
        }
        assert!((a.state.stamina - 1.0).abs() < 1e-3);
    }

    #[test]
    fn slide_enters_from_sprint_with_crouch() {
        let mut a = MovementAug::default();
        let (f, r) = fwd_xz();
        let mut i = intent_walk_forward();
        i.sprint_held = true;
        // Get into sprint
        a.tick(&i, f, r, 0.05, &WorldHints::ground());
        assert_eq!(a.state.phase, LocomotionPhase::Sprinting);
        // Crouch press
        i.crouch_held = true;
        a.tick(&i, f, r, 0.05, &WorldHints::ground());
        assert_eq!(a.state.phase, LocomotionPhase::Sliding);
    }

    #[test]
    fn slide_lasts_one_second_then_exits() {
        let mut a = MovementAug::default();
        let (f, r) = fwd_xz();
        let mut i = intent_walk_forward();
        i.sprint_held = true;
        a.tick(&i, f, r, 0.05, &WorldHints::ground());
        i.crouch_held = true;
        // Tick 1.1s in 0.01s steps.
        for _ in 0..110 {
            a.tick(&i, f, r, 0.01, &WorldHints::ground());
        }
        assert_ne!(a.state.phase, LocomotionPhase::Sliding);
    }

    #[test]
    fn slide_drops_hitbox() {
        let mut a = MovementAug::default();
        let (f, r) = fwd_xz();
        let mut i = intent_walk_forward();
        i.sprint_held = true;
        a.tick(&i, f, r, 0.05, &WorldHints::ground());
        i.crouch_held = true;
        let m = a.tick(&i, f, r, 0.05, &WorldHints::ground());
        assert!(m.hitbox_drop < -0.5);
    }

    #[test]
    fn double_jump_consumes_air_jump() {
        let mut a = MovementAug::default();
        let (f, r) = fwd_xz();
        let mut i = MovementIntent {
            jump_pressed: true,
            ..Default::default()
        };
        // Ground jump.
        a.tick(&i, f, r, 0.016, &WorldHints::ground());
        assert_eq!(a.state.air_jumps_used, 0); // ground-jump doesn't count
        assert!(a.state.vy > 0.0);
        // Now in-air ; second jump.
        i.jump_pressed = true;
        a.tick(&i, f, r, 0.016, &WorldHints::airborne());
        assert_eq!(a.state.air_jumps_used, 1);
        // Third jump : should NOT increment past max=2 → first air consumes jump 1
        // already, so a third press uses jump 2 (cap).
        i.jump_pressed = true;
        a.tick(&i, f, r, 0.016, &WorldHints::airborne());
        assert_eq!(a.state.air_jumps_used, 2);
        // Fourth press : capped at max_jumps_in_air = 2 ; no further increment.
        i.jump_pressed = true;
        a.tick(&i, f, r, 0.016, &WorldHints::airborne());
        assert_eq!(a.state.air_jumps_used, 2);
    }

    #[test]
    fn ground_resets_air_jumps() {
        let mut a = MovementAug::default();
        let (f, r) = fwd_xz();
        let mut i = MovementIntent {
            jump_pressed: true,
            ..Default::default()
        };
        a.tick(&i, f, r, 0.016, &WorldHints::airborne());
        a.tick(&i, f, r, 0.016, &WorldHints::airborne());
        assert!(a.state.air_jumps_used > 0);
        // Ground.
        i.jump_pressed = false;
        a.tick(&i, f, r, 0.016, &WorldHints::ground());
        assert_eq!(a.state.air_jumps_used, 0);
    }

    #[test]
    fn air_control_reduced_to_30_percent() {
        let mut a = MovementAug::default();
        let (f, r) = fwd_xz();
        let i = intent_walk_forward();
        let m = a.tick(&i, f, r, 0.1, &WorldHints::airborne());
        // walk_speed * 1.0 * 0.3 * 0.1 = 0.15 (forward = -Z).
        assert!((m.delta[2] - (-0.15)).abs() < 1e-3);
    }

    #[test]
    fn wall_run_caps_at_two_seconds() {
        let mut a = MovementAug::default();
        let (f, r) = fwd_xz();
        let i = intent_walk_forward();
        let mut hints = WorldHints::airborne();
        hints.wall_left = true;
        // Tick 2.5s.
        let mut last_phase = LocomotionPhase::Walking;
        for n in 0..250 {
            a.tick(&i, f, r, 0.01, &hints);
            if n == 50 {
                assert_eq!(a.state.phase, LocomotionPhase::WallRunning);
            }
            last_phase = a.state.phase;
        }
        assert_ne!(last_phase, LocomotionPhase::WallRunning);
    }

    #[test]
    fn mantle_auto_triggers_on_ledge() {
        let mut a = MovementAug::default();
        let (f, r) = fwd_xz();
        let i = intent_walk_forward();
        let mut hints = WorldHints::ground();
        hints.mantle_target_ahead = true;
        let m = a.tick(&i, f, r, 0.05, &hints);
        assert_eq!(a.state.phase, LocomotionPhase::Mantling);
        // Should produce upward lift.
        assert!(m.delta[1] > 0.0);
    }

    #[test]
    fn slide_jump_boost_increases_momentum() {
        let mut a = MovementAug::default();
        let (f, r) = fwd_xz();
        let mut i = intent_walk_forward();
        i.sprint_held = true;
        a.tick(&i, f, r, 0.05, &WorldHints::ground());
        i.crouch_held = true;
        a.tick(&i, f, r, 0.05, &WorldHints::ground());
        let mom_before = a.state.momentum_xz[0].hypot(a.state.momentum_xz[1]);
        i.crouch_held = false;
        i.jump_pressed = true;
        a.tick(&i, f, r, 0.016, &WorldHints::ground());
        let mom_after = a.state.momentum_xz[0].hypot(a.state.momentum_xz[1]);
        assert!(
            mom_after > mom_before,
            "slide-jump should boost momentum (before={mom_before} after={mom_after})"
        );
    }

    #[test]
    fn cosmetic_only_axiom_distance_invariant_across_skins() {
        // Two augs with different skin-affixes should produce the SAME delta.
        // (`MovementAug::tick` doesn't accept a skin param → cosmetic-only by
        // construction. This test makes the invariant explicit.)
        let mut a1 = MovementAug::default();
        let mut a2 = MovementAug::default();
        let (f, r) = fwd_xz();
        let mut i = intent_walk_forward();
        i.sprint_held = true;
        let m1 = a1.tick(&i, f, r, 0.1, &WorldHints::ground());
        let m2 = a2.tick(&i, f, r, 0.1, &WorldHints::ground());
        assert_eq!(m1.delta, m2.delta);
        assert_eq!(m1.speed_mult, m2.speed_mult);
    }

    #[test]
    fn boosted_phases_emit_renderer_signal() {
        let mut a = MovementAug::default();
        let (f, r) = fwd_xz();
        let mut i = intent_walk_forward();
        i.sprint_held = true;
        a.tick(&i, f, r, 0.05, &WorldHints::ground());
        assert!(a.is_boosted());
        // Reset to walking.
        let i2 = MovementIntent::default();
        for _ in 0..40 {
            a.tick(&i2, f, r, 0.05, &WorldHints::ground());
        }
        assert!(!a.is_boosted());
    }

    #[test]
    fn snapshot_replay_bit_equal_for_same_inputs() {
        let mut a1 = MovementAug::default();
        let mut a2 = MovementAug::default();
        let (f, r) = fwd_xz();
        let mut i = intent_walk_forward();
        i.sprint_held = true;
        for n in 0..200 {
            if n == 100 {
                i.crouch_held = true;
            }
            if n == 150 {
                i.crouch_held = false;
                i.jump_pressed = true;
            } else {
                i.jump_pressed = false;
            }
            a1.tick(&i, f, r, 0.01, &WorldHints::ground());
            a2.tick(&i, f, r, 0.01, &WorldHints::ground());
        }
        assert_eq!(a1.state.snapshot_bytes(), a2.state.snapshot_bytes());
    }
}
