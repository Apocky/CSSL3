// § tick.rs — master per-actor combat-tick orchestrator
// ════════════════════════════════════════════════════════════════════
// § I> consumes : (CombatInput, dt) → CombatOutput{events, damage_dealt}
// § I> deterministic : same (state, input, dt, rng-seed) ⇒ bit-equal output
// § I> integrates : state-machine + stamina + statuses + RNG
// § I> hit-detection here uses caller-supplied samples + target ; tick-fn does
//      not own hit-geometry computation (that's a host-render concern) — it
//      only consumes the boolean-hit + damage-roll the caller provides.
// § I> ¬ panic ; ¬ unwrap ; saturating arithmetic
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

use crate::damage_types::{ArmorClass, DamageRoll};
use crate::seed::DeterministicRng;
use crate::stamina::{StaminaAction, StaminaPool};
use crate::state_machine::{CombatInput, CombatState, CombatTransition};
use crate::status_effects::{tick_status, StatusInstance};
use crate::weapons::{stats_for, WeaponArchetype, WeaponStats};

/// Events emitted per-tick ; replay-anchor when audited at phase-6.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CombatEvent {
    /// State changed ; carries (prev, next).
    StateChanged(CombatState, CombatState),
    /// Action consumed N stamina (or 0 if rejected).
    StaminaConsumed { action: StaminaAction, ok: bool },
    /// Hit landed against target ; carries final damage applied.
    HitLanded { damage: f32, glance: bool },
    /// Stamina-underflow audit row (clamp-to-zero).
    StaminaUnderflow,
    /// Forced-Idle due to starvation.
    ForcedIdleStarved,
    /// Rejected action due to invalid transition.
    InvalidTransition,
}

/// Output of one tick.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CombatOutput {
    /// Ordered events emitted this tick.
    pub events: Vec<CombatEvent>,
    /// Total damage dealt this tick (post-affinity).
    pub damage_dealt: f32,
    /// Damage taken this tick (post-affinity).
    pub damage_taken: f32,
}

/// Per-actor combat tick state. FFI-friendly aside from `Vec<StatusInstance>`
/// (which mirrors as a fixed-cap array on the .csl side).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatTick {
    pub state: CombatState,
    pub stamina: StaminaPool,
    pub equipped_weapon: WeaponArchetype,
    /// Actor's own armor class for incoming-damage affinity application.
    pub armor: ArmorClass,
    pub statuses: Vec<StatusInstance>,
    pub rng: DeterministicRng,
    /// Per-state-frame counter (windup / active / recovery elapsed in seconds).
    pub state_elapsed_secs: f32,
    /// Last weapon-stats snapshot (cached for FFI mirroring).
    pub weapon_stats: WeaponStats,
}

impl CombatTick {
    /// Construct a new actor with default stamina + Idle state.
    #[must_use]
    pub fn new(weapon: WeaponArchetype, armor: ArmorClass, rng_seed: u64) -> Self {
        Self {
            state: CombatState::Idle,
            stamina: StaminaPool::default(),
            equipped_weapon: weapon,
            armor,
            statuses: Vec::new(),
            rng: DeterministicRng::new(rng_seed),
            state_elapsed_secs: 0.0,
            weapon_stats: stats_for(weapon),
        }
    }

    /// Master per-tick fn : consumes input + dt ; returns events + damage.
    /// Pure-deterministic given identical inputs (incl. seeded RNG state).
    pub fn tick(&mut self, input: CombatInput, dt: f32, hit_target: Option<DamageRoll>, target_armor: Option<ArmorClass>) -> CombatOutput {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let mut out = CombatOutput::default();
        let prev_state = self.state;

        // 1) Stamina + status decay
        self.stamina.tick(dt);
        tick_status(&mut self.statuses, dt);

        // 2) Action-cost gating before transition
        let action_cost = match (self.state, input) {
            (CombatState::Idle, CombatInput::AttackPress)
            | (CombatState::Recovery, CombatInput::AttackPress) => Some(StaminaAction::LightAttack),
            (CombatState::Idle, CombatInput::DodgePress)
            | (CombatState::WindupAttack, CombatInput::DodgePress)
            | (CombatState::Recovery, CombatInput::DodgePress) => Some(StaminaAction::DodgeRoll),
            (CombatState::Idle, CombatInput::ParryPress) => Some(StaminaAction::ParryAttempt),
            _ => None,
        };
        let allowed = match action_cost {
            Some(a) => {
                let ok = self.stamina.try_consume(a);
                out.events.push(CombatEvent::StaminaConsumed { action: a, ok });
                if !ok {
                    out.events.push(CombatEvent::InvalidTransition);
                }
                ok
            }
            None => true,
        };

        // 3) Stamina-starvation forces Idle (forced-Idle per GDD)
        if self.stamina.is_starved() && self.state != CombatState::Idle && self.state != CombatState::Stagger {
            out.events.push(CombatEvent::ForcedIdleStarved);
            self.state = CombatState::Idle;
            self.state_elapsed_secs = 0.0;
        }

        // 4) Apply transition iff stamina was sufficient
        let next = if allowed {
            CombatTransition::step(self.state, input, dt)
        } else {
            self.state
        };
        self.state_elapsed_secs += dt;
        if next != self.state {
            out.events.push(CombatEvent::StateChanged(prev_state, next));
            self.state = next;
            self.state_elapsed_secs = 0.0;
        }

        // 5) Hit-event resolution (caller pre-resolves SDF math + supplies roll)
        if matches!(self.state, CombatState::Active) {
            if let (Some(roll), Some(tgt)) = (hit_target.as_ref(), target_armor) {
                let dmg = roll.apply(tgt);
                // Glance-flag : driven externally by ε ; default false here
                out.events.push(CombatEvent::HitLanded {
                    damage: dmg,
                    glance: false,
                });
                out.damage_dealt += dmg;
            }
        }

        // 6) Incoming hit application (when state shows Block / Hit-flagged)
        if matches!(input, CombatInput::IncomingHit) {
            if let (Some(roll), Some(_)) = (hit_target.as_ref(), target_armor) {
                let dmg_in = roll.apply(self.armor);
                out.damage_taken += dmg_in;
            }
        }

        // 7) Stamina-underflow audit (caller-driven invariant ; emit if pool clamped)
        if self.stamina.current <= 0.0 && action_cost.is_some() && !allowed {
            out.events.push(CombatEvent::StaminaUnderflow);
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_tick_idle_with_no_input_no_state_change() {
        let mut t = CombatTick::new(WeaponArchetype::Sword, ArmorClass::FleshBeast, 1);
        let out = t.tick(CombatInput::None, 0.016, None, None);
        assert_eq!(t.state, CombatState::Idle);
        // No stamina-action ⇒ no state-change event
        assert!(!out
            .events
            .iter()
            .any(|e| matches!(e, CombatEvent::StateChanged(_, _))));
    }

    #[test]
    fn attack_press_transitions_windup_and_drains_stamina() {
        let mut t = CombatTick::new(WeaponArchetype::Sword, ArmorClass::FleshBeast, 1);
        let before = t.stamina.current;
        let out = t.tick(CombatInput::AttackPress, 0.016, None, None);
        assert_eq!(t.state, CombatState::WindupAttack);
        assert!(t.stamina.current < before);
        assert!(out.events.iter().any(
            |e| matches!(e, CombatEvent::StaminaConsumed { action: StaminaAction::LightAttack, ok: true })
        ));
    }
}
