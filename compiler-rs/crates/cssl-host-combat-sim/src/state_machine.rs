// § state_machine.rs — combat-loop state-machine (per GDD § COMBAT-LOOP)
// ════════════════════════════════════════════════════════════════════
// § I> 12 enumerated states matching the GDD set ; pure-fn step
// § I> table-driven transition ; ¬ if/else thicket ; deterministic
// § I> tick-elapsed countdown stored in caller (CombatTick) ; this module
//      only encodes the (state × input) → next-state dispatch
// § I> ¬ panics ; invalid transition = stay-in-state ; caller emits audit
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Combat-loop state ; matches GDD § COMBAT-LOOP § STATES.
///
/// Note · `Held` / `Released` / `Hit` / `Downed` / `Dead` from the GDD list
/// are caller-orchestrated meta-states (e.g. HP-driven) ; this enum encodes
/// the 12 mechanic-load-bearing states from the brief :
/// {Idle, WindupAttack, Active, Recovery, Dodge, IFrames, Parry, ParryWindow,
///  Block, Stun, Stagger, Counter}.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CombatState {
    Idle,
    WindupAttack,
    Active,
    Recovery,
    Dodge,
    IFrames,
    Parry,
    ParryWindow,
    Block,
    Stun,
    Stagger,
    Counter,
}

impl CombatState {
    /// Returns true iff the actor is currently invulnerable (i-frame window).
    #[must_use]
    pub const fn is_invulnerable(self) -> bool {
        matches!(self, Self::IFrames)
    }

    /// Returns true iff the actor can receive new input transitions.
    #[must_use]
    pub const fn accepts_input(self) -> bool {
        matches!(
            self,
            Self::Idle | Self::WindupAttack | Self::Recovery | Self::Block
        )
    }

    /// Returns true iff the state cannot be cancelled even by valid input.
    #[must_use]
    pub const fn is_locked(self) -> bool {
        matches!(
            self,
            Self::Active | Self::Stun | Self::Stagger | Self::Counter | Self::IFrames
        )
    }
}

/// Input events that drive transitions. Caller decides which input maps to
/// which player keypress / AI-decision ; this enum is the abstraction layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CombatInput {
    /// No input this tick — tick-elapsed transitions still fire.
    None,
    AttackPress,
    DodgePress,
    ParryPress,
    BlockHold,
    BlockRelease,
    /// Caller-injected hit event (target was hit).
    IncomingHit,
    /// Tick-elapsed (e.g. windup→active) — caller supplies based on
    /// per-state-frame counter (held in `CombatTick`).
    TickElapsed,
    /// Stamina-broken guard (caller checks pool before injecting).
    GuardBroken,
    /// HP≤0 — caller checks before injecting.
    Downed,
}

/// Pure-fn state-transition dispatcher. Wraps a single static `step`.
#[derive(Debug, Default, Clone, Copy)]
pub struct CombatTransition;

impl CombatTransition {
    /// Pure-deterministic step : (state, input) → next-state.
    /// Invalid transitions stay-in-state ; caller emits audit row.
    /// `_dt` is reserved for future timed-window mechanics ; not used now.
    #[must_use]
    pub fn step(state: CombatState, input: CombatInput, _dt: f32) -> CombatState {
        use CombatInput as I;
        use CombatState as S;
        match (state, input) {
            // Idle —————————————————————————————————————————————————
            (S::Idle, I::AttackPress) => S::WindupAttack,
            (S::Idle, I::DodgePress) => S::Dodge,
            (S::Idle, I::ParryPress) => S::Parry,
            (S::Idle, I::BlockHold) => S::Block,
            // Windup ——————————————————————————————————————————————
            (S::WindupAttack, I::DodgePress) => S::Dodge, // anim-cancel
            (S::WindupAttack, I::TickElapsed) => S::Active,
            // Active ——————————————————————————————————————————————
            (S::Active, I::TickElapsed) => S::Recovery,
            // Recovery ————————————————————————————————————————————
            (S::Recovery, I::AttackPress) => S::WindupAttack, // combo-chain
            (S::Recovery, I::DodgePress) => S::Dodge,         // recovery-cancel
            (S::Recovery, I::TickElapsed) => S::Idle,
            // Dodge ———————————————————————————————————————————————
            (S::Dodge, I::TickElapsed) => S::IFrames,
            (S::IFrames, I::TickElapsed) => S::Recovery,
            // Parry ———————————————————————————————————————————————
            (S::Parry, I::TickElapsed) => S::ParryWindow,
            (S::ParryWindow, I::IncomingHit) => S::Counter, // perfect-window
            (S::ParryWindow, I::TickElapsed) => S::Recovery, // whiffed
            (S::Counter, I::TickElapsed) => S::Recovery,
            // Block ———————————————————————————————————————————————
            (S::Block, I::IncomingHit) => S::Block, // costs stamina externally
            (S::Block, I::GuardBroken) => S::Stagger,
            (S::Block, I::BlockRelease) => S::Idle,
            // Stagger / Stun ——————————————————————————————————————
            (S::Stagger, I::TickElapsed) => S::Idle,
            (S::Stun, I::TickElapsed) => S::Idle,
            // catch-all : stay-in-state (caller may emit INVALID_TRANS audit)
            _ => state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_attack_press_goes_windup() {
        let next = CombatTransition::step(CombatState::Idle, CombatInput::AttackPress, 0.016);
        assert_eq!(next, CombatState::WindupAttack);
    }

    #[test]
    fn windup_dodge_anim_cancel() {
        let next =
            CombatTransition::step(CombatState::WindupAttack, CombatInput::DodgePress, 0.016);
        assert_eq!(next, CombatState::Dodge);
    }

    #[test]
    fn invalid_transition_stays_in_state() {
        let next = CombatTransition::step(CombatState::Active, CombatInput::AttackPress, 0.016);
        assert_eq!(next, CombatState::Active);
    }
}
