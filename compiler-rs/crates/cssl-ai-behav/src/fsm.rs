//! StateMachine — finite-state machine for NPC behavior.
//!
//! § THESIS
//!   The simplest decision-shape : a current state + a transition table
//!   mapping `(from-state, predicate)` → `to-state`. Transitions evaluate
//!   each tick ; the first matching predicate wins. Predicates are pure
//!   fns of `&BlackBoard` — no clock, no entropy, deterministic.
//!
//! § DESIGN
//!   - States are caller-defined (any type implementing [`FsmState`] +
//!     `Copy + Eq + Hash`). We use generic `S` rather than a fixed enum so
//!     this primitive composes with arbitrary game-AI state-sets.
//!   - Transitions stored as `Vec<Transition<S>>` ; the FSM iterates them
//!     in declared order each tick. **First predicate to return `true` wins.**
//!     Order-stability is the determinism guarantee.
//!   - Predicates are wrapped in [`FsmTransitionPredicate`] (a boxed `Fn`).
//!     Callers often write small `move` closures that read the BlackBoard.
//!
//! § DETERMINISM
//!   - First-match-wins iteration over a `Vec<Transition>` is order-stable.
//!   - Predicates take `&BlackBoard` — pure-fn-shape ; no mutation, no
//!     side-effects. The only legal way to capture mutable AI state is
//!     via the BlackBoard, which is itself deterministic.
//!   - No internal RNG. If a transition needs randomness, the caller seeds
//!     a `DetRng` upstream + writes its outputs into the BlackBoard for
//!     the predicate to read.
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - Every state carries a name() (via FsmState trait) — no hidden states.
//!   - Transitions are inspectable + auditable ; `transition_names()`
//!     returns the declared list for audit-log dump.
//!   - The Companion-archetype is rejected at the [`StateMachine::new`]
//!     entry — see [`crate::companion_guard`].

use std::fmt;
use std::marker::PhantomData;

use thiserror::Error;

use crate::blackboard::BlackBoard;
use crate::companion_guard::{assert_not_companion, ActorKind, CompanionGuardError};

/// Trait every FSM state-enum must implement. Stage-0 stable.
///
/// § REQUIREMENTS
///   - `Copy + Eq + Hash` — states compared by value, used as map keys.
///   - `name()` — debug + audit-log readable identifier ; should be a
///     short, unique-per-variant string (e.g. "patrol", "alert", "flee").
pub trait FsmState: Copy + Eq + std::hash::Hash + fmt::Debug + Send + Sync + 'static {
    /// Short identifier for audit-log + telemetry.
    fn name(&self) -> &'static str;
}

/// A transition predicate — pure fn of `&BlackBoard`. Return `true` to fire
/// the transition.
///
/// § BOXED-FN
///   We box rather than generic-bound because storing many transitions
///   (each with a different closure-type) in one `Vec` requires erasure.
///   The cost is one indirection per predicate-call ; trivial vs the
///   AI-tick budget.
pub struct FsmTransitionPredicate {
    name: String,
    predicate: Box<dyn Fn(&BlackBoard) -> bool + Send + Sync>,
}

impl FsmTransitionPredicate {
    /// Construct a named predicate from a closure.
    pub fn new<F>(name: impl Into<String>, predicate: F) -> Self
    where
        F: Fn(&BlackBoard) -> bool + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            predicate: Box::new(predicate),
        }
    }

    /// Evaluate the predicate.
    #[must_use]
    pub fn eval(&self, bb: &BlackBoard) -> bool {
        (self.predicate)(bb)
    }

    /// The predicate's name (audit-readable).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Debug for FsmTransitionPredicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FsmTransitionPredicate")
            .field("name", &self.name)
            .field("predicate", &"<closure>")
            .finish()
    }
}

/// A single transition declared on the FSM.
struct Transition<S: FsmState> {
    /// State this transition fires from.
    from: S,
    /// Target state if the predicate fires.
    to: S,
    /// The guarding predicate (named for audit).
    predicate: FsmTransitionPredicate,
}

impl<S: FsmState> fmt::Debug for Transition<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transition")
            .field("from", &self.from)
            .field("to", &self.to)
            .field("predicate", &self.predicate)
            .finish()
    }
}

/// Errors the StateMachine surfaces.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum StateMachineError {
    /// Caller attempted to drive a Companion via FSM.
    #[error("AIBEHAV0020 — StateMachine rejects Companion-archetype: {0}")]
    Companion(#[from] CompanionGuardError),

    /// `transition_to(target)` called with a target state not in the
    /// declared state-set.
    #[error("AIBEHAV0021 — target state '{name}' not registered with the StateMachine")]
    UnknownTargetState { name: &'static str },
}

impl StateMachineError {
    /// Stable diagnostic code prefix.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Companion(_) => "AIBEHAV0020",
            Self::UnknownTargetState { .. } => "AIBEHAV0021",
        }
    }
}

/// A finite-state machine for NPC behavior.
///
/// § INVARIANTS
///   - `current` is always a member of `states` (initial check at `new`).
///   - Transitions evaluate in **declared order** ; first match wins.
pub struct StateMachine<S: FsmState> {
    /// All known states. Stored for audit-log + debug round-trips.
    states: Vec<S>,
    /// Current state.
    current: S,
    /// Transitions, evaluated in declared order each tick.
    transitions: Vec<Transition<S>>,
    /// Bookkeeping : counts of how many ticks each state has held the
    /// current-slot (for replay-determinism testing).
    tick_count: u64,
    _phantom: PhantomData<S>,
}

impl<S: FsmState> StateMachine<S> {
    /// Construct a new FSM for an `Npc` actor.
    ///
    /// § GUARD
    ///   Companion-archetype is rejected per PRIME_DIRECTIVE §3.
    pub fn new(kind: ActorKind, states: Vec<S>, initial: S) -> Result<Self, StateMachineError> {
        assert_not_companion(kind)?;
        if !states.contains(&initial) {
            return Err(StateMachineError::UnknownTargetState {
                name: initial.name(),
            });
        }
        Ok(Self {
            states,
            current: initial,
            transitions: Vec::new(),
            tick_count: 0,
            _phantom: PhantomData,
        })
    }

    /// Add a transition to the FSM. Order is declaration-order ; first
    /// match wins each tick.
    pub fn add_transition(
        &mut self,
        from: S,
        to: S,
        predicate: FsmTransitionPredicate,
    ) -> Result<(), StateMachineError> {
        if !self.states.contains(&from) {
            return Err(StateMachineError::UnknownTargetState { name: from.name() });
        }
        if !self.states.contains(&to) {
            return Err(StateMachineError::UnknownTargetState { name: to.name() });
        }
        self.transitions.push(Transition {
            from,
            to,
            predicate,
        });
        Ok(())
    }

    /// Current state.
    #[must_use]
    pub fn current(&self) -> S {
        self.current
    }

    /// Number of ticks evaluated since construction.
    #[must_use]
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Number of declared transitions.
    #[must_use]
    pub fn transition_count(&self) -> usize {
        self.transitions.len()
    }

    /// Number of declared states.
    #[must_use]
    pub fn state_count(&self) -> usize {
        self.states.len()
    }

    /// All declared transition-names in declared order ; for audit-log dump.
    #[must_use]
    pub fn transition_names(&self) -> Vec<&str> {
        self.transitions
            .iter()
            .map(|t| t.predicate.name())
            .collect()
    }

    /// Evaluate transitions against the BlackBoard. The FIRST transition
    /// where `from == current && predicate(bb) == true` fires ; subsequent
    /// transitions are not evaluated this tick.
    ///
    /// § DETERMINISM
    ///   - Iteration order = declaration order ; bit-stable.
    ///   - Each predicate is a pure fn of `&BlackBoard` — no side effects.
    ///   - `tick_count` increments by 1 every call.
    ///
    /// § Returns the (possibly-unchanged) current state after the tick.
    pub fn tick(&mut self, bb: &BlackBoard) -> S {
        for t in &self.transitions {
            if t.from == self.current && t.predicate.eval(bb) {
                self.current = t.to;
                break;
            }
        }
        self.tick_count = self.tick_count.saturating_add(1);
        self.current
    }

    /// Force-set the current state. **Use sparingly** — bypasses transition
    /// guarding ; meant for replay-restore or scripted-event injection.
    pub fn force_state(&mut self, target: S) -> Result<(), StateMachineError> {
        if !self.states.contains(&target) {
            return Err(StateMachineError::UnknownTargetState {
                name: target.name(),
            });
        }
        self.current = target;
        Ok(())
    }
}

impl<S: FsmState> fmt::Debug for StateMachine<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateMachine")
            .field("current", &self.current)
            .field("state_count", &self.states.len())
            .field("transition_count", &self.transitions.len())
            .field("tick_count", &self.tick_count)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum GuardState {
        Patrol,
        Alert,
        Flee,
    }

    impl FsmState for GuardState {
        fn name(&self) -> &'static str {
            match self {
                Self::Patrol => "patrol",
                Self::Alert => "alert",
                Self::Flee => "flee",
            }
        }
    }

    fn fresh_fsm() -> StateMachine<GuardState> {
        StateMachine::new(
            ActorKind::Npc,
            vec![GuardState::Patrol, GuardState::Alert, GuardState::Flee],
            GuardState::Patrol,
        )
        .unwrap()
    }

    #[test]
    fn fsm_initial_state() {
        let f = fresh_fsm();
        assert_eq!(f.current(), GuardState::Patrol);
        assert_eq!(f.tick_count(), 0);
    }

    #[test]
    fn fsm_companion_rejected() {
        let err = StateMachine::<GuardState>::new(
            ActorKind::Companion,
            vec![GuardState::Patrol],
            GuardState::Patrol,
        )
        .unwrap_err();
        assert!(matches!(err, StateMachineError::Companion(_)));
        assert_eq!(err.code(), "AIBEHAV0020");
    }

    #[test]
    fn fsm_initial_state_must_be_in_set() {
        // Pass an initial state that's not in the states list.
        let err = StateMachine::new(
            ActorKind::Npc,
            vec![GuardState::Patrol], // missing Alert
            GuardState::Alert,
        )
        .unwrap_err();
        assert!(matches!(err, StateMachineError::UnknownTargetState { .. }));
    }

    #[test]
    fn fsm_no_transition_holds_state() {
        let mut f = fresh_fsm();
        let bb = BlackBoard::new();
        let s = f.tick(&bb);
        assert_eq!(s, GuardState::Patrol);
        assert_eq!(f.tick_count(), 1);
    }

    #[test]
    fn fsm_predicate_fires_transition() {
        let mut f = fresh_fsm();
        f.add_transition(
            GuardState::Patrol,
            GuardState::Alert,
            FsmTransitionPredicate::new("see-enemy", |bb: &BlackBoard| {
                bb.get_bool("enemy_visible").unwrap_or(false)
            }),
        )
        .unwrap();
        let mut bb = BlackBoard::new();
        bb.set_bool("enemy_visible", true);
        let s = f.tick(&bb);
        assert_eq!(s, GuardState::Alert);
    }

    #[test]
    fn fsm_first_match_wins() {
        let mut f = fresh_fsm();
        // Two predicates that would both fire ; declaration-order picks the first.
        f.add_transition(
            GuardState::Patrol,
            GuardState::Alert,
            FsmTransitionPredicate::new("first-true", |_| true),
        )
        .unwrap();
        f.add_transition(
            GuardState::Patrol,
            GuardState::Flee,
            FsmTransitionPredicate::new("second-true", |_| true),
        )
        .unwrap();
        let bb = BlackBoard::new();
        let s = f.tick(&bb);
        assert_eq!(s, GuardState::Alert, "first-match-wins discipline");
    }

    #[test]
    fn fsm_transitions_only_fire_from_current_state() {
        let mut f = fresh_fsm();
        f.add_transition(
            GuardState::Alert, // not the current state
            GuardState::Flee,
            FsmTransitionPredicate::new("alert-to-flee", |_| true),
        )
        .unwrap();
        let bb = BlackBoard::new();
        let s = f.tick(&bb);
        // Still Patrol — the Alert→Flee transition's `from` doesn't match.
        assert_eq!(s, GuardState::Patrol);
    }

    #[test]
    fn fsm_chain_two_ticks() {
        let mut f = fresh_fsm();
        f.add_transition(
            GuardState::Patrol,
            GuardState::Alert,
            FsmTransitionPredicate::new("p-to-a", |bb| bb.get_bool("ev").unwrap_or(false)),
        )
        .unwrap();
        f.add_transition(
            GuardState::Alert,
            GuardState::Flee,
            FsmTransitionPredicate::new("a-to-f", |bb| {
                bb.get_int("hp").map(|h| h < 10).unwrap_or(false)
            }),
        )
        .unwrap();
        let mut bb = BlackBoard::new();
        bb.set_bool("ev", true);
        bb.set_int("hp", 100);
        assert_eq!(f.tick(&bb), GuardState::Alert);
        bb.set_int("hp", 5);
        assert_eq!(f.tick(&bb), GuardState::Flee);
    }

    #[test]
    fn fsm_force_state_works() {
        let mut f = fresh_fsm();
        f.force_state(GuardState::Flee).unwrap();
        assert_eq!(f.current(), GuardState::Flee);
    }

    #[test]
    fn fsm_force_state_unknown_rejected() {
        let mut f = StateMachine::new(ActorKind::Npc, vec![GuardState::Patrol], GuardState::Patrol)
            .unwrap();
        let err = f.force_state(GuardState::Flee).unwrap_err();
        assert!(matches!(err, StateMachineError::UnknownTargetState { .. }));
    }

    #[test]
    fn fsm_add_transition_unknown_state_rejected() {
        let mut f = StateMachine::new(
            ActorKind::Npc,
            vec![GuardState::Patrol], // only Patrol declared
            GuardState::Patrol,
        )
        .unwrap();
        let err = f
            .add_transition(
                GuardState::Patrol,
                GuardState::Flee, // not in states list
                FsmTransitionPredicate::new("p-to-f", |_| true),
            )
            .unwrap_err();
        assert!(matches!(err, StateMachineError::UnknownTargetState { .. }));
    }

    #[test]
    fn fsm_transition_names_lists_in_order() {
        let mut f = fresh_fsm();
        f.add_transition(
            GuardState::Patrol,
            GuardState::Alert,
            FsmTransitionPredicate::new("first", |_| false),
        )
        .unwrap();
        f.add_transition(
            GuardState::Alert,
            GuardState::Flee,
            FsmTransitionPredicate::new("second", |_| false),
        )
        .unwrap();
        let names = f.transition_names();
        assert_eq!(names, vec!["first", "second"]);
    }

    #[test]
    fn fsm_determinism_across_runs() {
        // Two FSMs with identical declarations + identical BB should
        // produce identical tick-sequences.
        let make = || -> StateMachine<GuardState> {
            let mut f = fresh_fsm();
            f.add_transition(
                GuardState::Patrol,
                GuardState::Alert,
                FsmTransitionPredicate::new("ev", |bb| bb.get_bool("ev").unwrap_or(false)),
            )
            .unwrap();
            f.add_transition(
                GuardState::Alert,
                GuardState::Flee,
                FsmTransitionPredicate::new("low-hp", |bb| {
                    bb.get_int("hp").map(|h| h < 10).unwrap_or(false)
                }),
            )
            .unwrap();
            f
        };
        let mut a = make();
        let mut b = make();
        let mut bb = BlackBoard::new();
        bb.set_bool("ev", true);
        bb.set_int("hp", 100);
        for _ in 0..3 {
            assert_eq!(a.tick(&bb), b.tick(&bb));
        }
        bb.set_int("hp", 5);
        for _ in 0..3 {
            assert_eq!(a.tick(&bb), b.tick(&bb));
        }
        assert_eq!(a.tick_count(), b.tick_count());
        assert_eq!(a.current(), b.current());
    }

    #[test]
    fn fsm_state_count_and_transition_count() {
        let mut f = fresh_fsm();
        assert_eq!(f.state_count(), 3);
        assert_eq!(f.transition_count(), 0);
        f.add_transition(
            GuardState::Patrol,
            GuardState::Alert,
            FsmTransitionPredicate::new("t", |_| false),
        )
        .unwrap();
        assert_eq!(f.transition_count(), 1);
    }

    #[test]
    fn fsm_predicate_named() {
        let p = FsmTransitionPredicate::new("test-name", |_| true);
        assert_eq!(p.name(), "test-name");
        assert!(p.eval(&BlackBoard::new()));
    }
}
