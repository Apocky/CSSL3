//! AiBrain — orchestrator that binds {BlackBoard, FSM, BT, UtilityAi} together
//! and exposes the result as an `OmegaSystem` for omega_step phase-4 sim.
//!
//! § THESIS
//!   An NPC's "brain" is the composition of (some subset of) FSM + BT +
//!   UtilityAi backed by a shared BlackBoard. This struct is the binding ;
//!   it ticks the constituent decision-systems in a defined order each
//!   omega_step and writes the resulting "intent" back to the BlackBoard
//!   for downstream physics/animation systems to consume.
//!
//! § ORDER-OF-EVALUATION  (deterministic + load-bearing)
//!   Each tick :
//!     1. **Sensor refresh phase** — caller (or scheduler) populates the
//!        BlackBoard with sensor outputs BEFORE calling `step()`. Stage-0
//!        keeps this caller-driven so the brain stays portable.
//!     2. **FSM tick** (if present) — updates `current_state` ; writes
//!        its name to BlackBoard key `"_brain_fsm_state"`.
//!     3. **BehaviorTree tick** (if present) — runs ; writes its outcome
//!        to BlackBoard key `"_brain_bt_status"` as Int (0=Failure / 1=Success / 2=Running).
//!     4. **UtilityAi pick** (if present) — picks ActionId ; writes to
//!        BlackBoard key `"_brain_action_id"`.
//!     5. **Tick counter** increments ; written to `"_brain_tick"`.
//!
//!   Order is fixed at construction time. Subsystems may read each
//!   other's outputs via the BlackBoard keys above (FSM-state fed into
//!   BT predicates, BT-outcome fed into UtilityAi gating, etc.).
//!
//! § OMEGA-SYSTEM IMPLEMENTATION
//!   `AiBrain` impls [`cssl_substrate_omega_step::OmegaSystem`] so it
//!   participates in the canonical sim tick. Its `effect_row()` is `{Sim}` ;
//!   it requests no `RngStreamId`s by default (callers add streams via
//!   `AiBrainBuilder::with_rng_stream`).
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - Companion-archetype rejected at `AiBrainBuilder::build()`.
//!   - `halt_requested()` short-circuits to a NoOp tick — graceful halt,
//!     not panic.
//!   - All decision-system outputs land on the BlackBoard with a
//!     `_brain_*` prefix, which is auditable.
//!
//! § LIMITATIONS  (stage-0)
//!   - FSM state-type `S` is generic on the brain ; a brain that uses
//!     an FSM picks one S at construction. Multi-FSM brains (rare in
//!     game-AI literature) are deferred ; one FSM per brain is standard.
//!   - The brain does NOT directly mutate `OmegaSnapshot` ; it writes
//!     to the BlackBoard only. Translating BB-intent → world-effect is
//!     the job of a sibling system (e.g. `cssl-physics`), which reads
//!     from the brain's BB via the brain's NpcId.

use std::fmt;

use cssl_substrate_omega_step::{
    EffectRow, OmegaError, OmegaStepCtx, OmegaSystem, RngStreamId,
};
use thiserror::Error;

use crate::blackboard::BlackBoard;
use crate::bt::{BehaviorTree, BtStatus};
use crate::companion_guard::{assert_not_companion, ActorKind, CompanionGuardError};
use crate::fsm::{FsmState, StateMachine};
use crate::utility::UtilityAi;

/// Errors the AiBrain surfaces.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum AiBrainError {
    /// Caller attempted to construct a brain for a Companion-archetype.
    #[error("AIBEHAV0080 — AiBrain rejects Companion-archetype: {0}")]
    Companion(#[from] CompanionGuardError),
}

impl AiBrainError {
    /// Stable diagnostic code prefix.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Companion(_) => "AIBEHAV0080",
        }
    }
}

/// BlackBoard key written with the FSM's current state's name.
pub const BB_KEY_FSM_STATE: &str = "_brain_fsm_state";
/// BlackBoard key written with the BT's most recent tick status (Int 0/1/2).
pub const BB_KEY_BT_STATUS: &str = "_brain_bt_status";
/// BlackBoard key written with the UtilityAi's most recent pick (Int).
pub const BB_KEY_ACTION_ID: &str = "_brain_action_id";
/// BlackBoard key written with the brain's monotonic tick counter (Int).
pub const BB_KEY_TICK: &str = "_brain_tick";

/// The brain itself. Generic on FSM state type `S`.
///
/// § DESIGN-NOTE
///   We avoid trait-object'ing the FSM because the state-type is part
///   of the FSM's type signature ; using `Box<dyn ...>` would erase
///   `S` and force an enum-shape on every brain. The trade-off : you
///   get one `S` per brain. That's fine for stage-0 — most NPC brains
///   have a single state-set.
pub struct AiBrain<S: FsmState> {
    name: String,
    bb: BlackBoard,
    fsm: Option<StateMachine<S>>,
    bt: Option<BehaviorTree>,
    util: Option<UtilityAi>,
    rng_streams: Vec<RngStreamId>,
    tick_counter: u64,
}

impl<S: FsmState> AiBrain<S> {
    /// Read-only access to the BlackBoard (audit + tests).
    #[must_use]
    pub fn blackboard(&self) -> &BlackBoard {
        &self.bb
    }

    /// Mutable access to the BlackBoard (sensor-refresh upstream of step).
    pub fn blackboard_mut(&mut self) -> &mut BlackBoard {
        &mut self.bb
    }

    /// Read-only access to the FSM, if any.
    #[must_use]
    pub fn fsm(&self) -> Option<&StateMachine<S>> {
        self.fsm.as_ref()
    }

    /// Read-only access to the BehaviorTree, if any.
    #[must_use]
    pub fn bt(&self) -> Option<&BehaviorTree> {
        self.bt.as_ref()
    }

    /// Read-only access to the UtilityAi, if any.
    #[must_use]
    pub fn util(&self) -> Option<&UtilityAi> {
        self.util.as_ref()
    }

    /// Tick counter — number of `step()` calls observed so far.
    #[must_use]
    pub fn tick_count(&self) -> u64 {
        self.tick_counter
    }

    /// Internal tick body — runs FSM → BT → UtilityAi in order, writing
    /// outputs to the BlackBoard. Used by the `OmegaSystem` impl + by
    /// the convenience `tick()` method (for tests that don't drive a full
    /// scheduler).
    fn tick_internal(&mut self, halt_requested: bool) {
        if halt_requested {
            // Short-circuit ; no-op tick. Counter still increments so
            // replay-tests can see we entered the brain.
            self.tick_counter = self.tick_counter.saturating_add(1);
            self.bb.set_int(BB_KEY_TICK, self.tick_counter as i64);
            return;
        }

        // FSM tick.
        if let Some(fsm) = self.fsm.as_mut() {
            let new_state = fsm.tick(&self.bb);
            self.bb.set_text(BB_KEY_FSM_STATE, new_state.name());
        }

        // BehaviorTree tick.
        if let Some(bt) = self.bt.as_mut() {
            let status = bt.tick(&mut self.bb);
            self.bb.set_int(
                BB_KEY_BT_STATUS,
                match status {
                    BtStatus::Failure => 0,
                    BtStatus::Success => 1,
                    BtStatus::Running => 2,
                },
            );
        }

        // UtilityAi pick.
        if let Some(util) = self.util.as_ref() {
            if let Ok(pick) = util.pick(&self.bb) {
                self.bb.set_int(BB_KEY_ACTION_ID, pick.0 as i64);
            }
            // If pick errors (e.g. NoActions), the BB key is left unchanged ;
            // downstream systems read the absence as "no action".
        }

        self.tick_counter = self.tick_counter.saturating_add(1);
        self.bb.set_int(BB_KEY_TICK, self.tick_counter as i64);
    }

    /// Convenience tick — call directly when you don't have an
    /// `OmegaScheduler` driving the brain (tests, ad-hoc usage).
    pub fn tick(&mut self) {
        self.tick_internal(false);
    }
}

impl<S: FsmState> fmt::Debug for AiBrain<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiBrain")
            .field("name", &self.name)
            .field("has_fsm", &self.fsm.is_some())
            .field("has_bt", &self.bt.is_some())
            .field("has_util", &self.util.is_some())
            .field("rng_streams", &self.rng_streams.len())
            .field("tick_counter", &self.tick_counter)
            .finish()
    }
}

impl<S: FsmState> OmegaSystem for AiBrain<S> {
    fn step(&mut self, ctx: &mut OmegaStepCtx<'_>, _dt: f64) -> Result<(), OmegaError> {
        let halt_requested = ctx.halt_requested();
        self.tick_internal(halt_requested);
        ctx.telemetry().count("ai_behav_brain_ticks");
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn effect_row(&self) -> EffectRow {
        EffectRow::sim()
    }

    fn rng_streams(&self) -> &[RngStreamId] {
        &self.rng_streams
    }
}

/// Builder for `AiBrain<S>`. Stage-0 fluent-API ; constructed via
/// `AiBrainBuilder::new`.
pub struct AiBrainBuilder<S: FsmState> {
    kind: ActorKind,
    name: String,
    fsm: Option<StateMachine<S>>,
    bt: Option<BehaviorTree>,
    util: Option<UtilityAi>,
    rng_streams: Vec<RngStreamId>,
    initial_bb: BlackBoard,
}

impl<S: FsmState> AiBrainBuilder<S> {
    /// Begin building a brain with a name + actor-kind. Companion is
    /// rejected here at the gate.
    #[must_use]
    pub fn new(name: impl Into<String>, kind: ActorKind) -> Self {
        Self {
            kind,
            name: name.into(),
            fsm: None,
            bt: None,
            util: None,
            rng_streams: Vec::new(),
            initial_bb: BlackBoard::new(),
        }
    }

    /// Attach an FSM.
    #[must_use]
    pub fn with_fsm(mut self, fsm: StateMachine<S>) -> Self {
        self.fsm = Some(fsm);
        self
    }

    /// Attach a BehaviorTree.
    #[must_use]
    pub fn with_bt(mut self, bt: BehaviorTree) -> Self {
        self.bt = Some(bt);
        self
    }

    /// Attach a UtilityAi.
    #[must_use]
    pub fn with_util(mut self, util: UtilityAi) -> Self {
        self.util = Some(util);
        self
    }

    /// Declare an RNG stream the brain will use. The OmegaScheduler
    /// pre-allocates per-stream PRNG state from this declaration.
    #[must_use]
    pub fn with_rng_stream(mut self, stream: RngStreamId) -> Self {
        self.rng_streams.push(stream);
        self
    }

    /// Seed the BlackBoard with an initial entry. Useful for setting
    /// the starting position / initial state-flags.
    #[must_use]
    pub fn with_bb_entry(mut self, key: impl Into<String>, value: crate::blackboard::BbValue) -> Self {
        self.initial_bb.set(key, value);
        self
    }

    /// Build the brain. Companion-archetype is rejected here.
    pub fn build(self) -> Result<AiBrain<S>, AiBrainError> {
        assert_not_companion(self.kind)?;
        Ok(AiBrain {
            name: self.name,
            bb: self.initial_bb,
            fsm: self.fsm,
            bt: self.bt,
            util: self.util,
            rng_streams: self.rng_streams,
            tick_counter: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blackboard::BbValue;
    use crate::bt::{BtLeaf, BtNode, LeafId};
    use crate::fsm::FsmTransitionPredicate;
    use crate::utility::{Consideration, ConsiderationId, CurveKind, UtilityAction};
    use cssl_substrate_omega_step::{DetRng, OmegaSnapshot, OmegaStepCtx, TelemetryHook};
    use std::collections::BTreeMap;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum SimpleState {
        Idle,
        Active,
    }
    impl FsmState for SimpleState {
        fn name(&self) -> &'static str {
            match self {
                Self::Idle => "idle",
                Self::Active => "active",
            }
        }
    }

    struct AlwaysSucceedLeaf;
    impl BtLeaf for AlwaysSucceedLeaf {
        fn tick(&mut self, _bb: &mut BlackBoard) -> BtStatus {
            BtStatus::Success
        }
        fn name(&self) -> &'static str {
            "always-succeed"
        }
    }

    fn make_brain() -> AiBrain<SimpleState> {
        let mut fsm = StateMachine::new(
            ActorKind::Npc,
            vec![SimpleState::Idle, SimpleState::Active],
            SimpleState::Idle,
        )
        .unwrap();
        fsm.add_transition(
            SimpleState::Idle,
            SimpleState::Active,
            FsmTransitionPredicate::new("activate", |bb| {
                bb.get_bool("trigger").unwrap_or(false)
            }),
        )
        .unwrap();
        let bt = BehaviorTree::new(
            ActorKind::Npc,
            BtNode::Leaf(LeafId(0)),
            vec![Box::new(AlwaysSucceedLeaf)],
        )
        .unwrap();
        let mut util = UtilityAi::new(ActorKind::Npc).unwrap();
        let c = util.add_consideration(Consideration::new(
            "score",
            CurveKind::Linear,
            |bb| bb.get_float("desire").unwrap_or(0.5),
        ));
        let _ = util.add_action(UtilityAction::new("act", vec![c])).unwrap();

        AiBrainBuilder::new("test-brain", ActorKind::Npc)
            .with_fsm(fsm)
            .with_bt(bt)
            .with_util(util)
            .build()
            .unwrap()
    }

    #[test]
    fn brain_companion_rejected() {
        let err = AiBrainBuilder::<SimpleState>::new("c", ActorKind::Companion)
            .build()
            .unwrap_err();
        assert!(matches!(err, AiBrainError::Companion(_)));
        assert_eq!(err.code(), "AIBEHAV0080");
    }

    #[test]
    fn brain_basic_construction() {
        let b = make_brain();
        assert!(b.fsm().is_some());
        assert!(b.bt().is_some());
        assert!(b.util().is_some());
        assert_eq!(b.tick_count(), 0);
    }

    #[test]
    fn brain_tick_writes_bb_keys() {
        let mut b = make_brain();
        b.tick();
        assert_eq!(b.blackboard().get_text(BB_KEY_FSM_STATE).unwrap(), "idle");
        assert_eq!(b.blackboard().get_int(BB_KEY_BT_STATUS).unwrap(), 1); // Success
        assert_eq!(b.blackboard().get_int(BB_KEY_ACTION_ID).unwrap(), 0);
        assert_eq!(b.blackboard().get_int(BB_KEY_TICK).unwrap(), 1);
    }

    #[test]
    fn brain_fsm_state_transitions_via_bb() {
        let mut b = make_brain();
        b.blackboard_mut().set_bool("trigger", true);
        b.tick();
        assert_eq!(b.blackboard().get_text(BB_KEY_FSM_STATE).unwrap(), "active");
    }

    #[test]
    fn brain_tick_count_advances() {
        let mut b = make_brain();
        b.tick();
        b.tick();
        b.tick();
        assert_eq!(b.tick_count(), 3);
    }

    #[test]
    fn brain_no_subsystems_still_ticks() {
        let mut b = AiBrainBuilder::<SimpleState>::new("empty", ActorKind::Npc)
            .build()
            .unwrap();
        b.tick();
        assert_eq!(b.tick_count(), 1);
        // Only the tick counter is written.
        assert_eq!(b.blackboard().get_int(BB_KEY_TICK).unwrap(), 1);
        assert!(b.blackboard().get_text(BB_KEY_FSM_STATE).is_err());
    }

    #[test]
    fn brain_with_initial_bb_entry() {
        let b = AiBrainBuilder::<SimpleState>::new("w", ActorKind::Npc)
            .with_bb_entry("initial-x", BbValue::Int(42))
            .build()
            .unwrap();
        assert_eq!(b.blackboard().get_int("initial-x").unwrap(), 42);
    }

    #[test]
    fn brain_rng_streams_propagate() {
        let b = AiBrainBuilder::<SimpleState>::new("w", ActorKind::Npc)
            .with_rng_stream(RngStreamId(7))
            .with_rng_stream(RngStreamId(8))
            .build()
            .unwrap();
        let streams = b.rng_streams();
        assert_eq!(streams, &[RngStreamId(7), RngStreamId(8)]);
    }

    #[test]
    fn brain_omega_system_step() {
        // Build OmegaStepCtx skeleton.
        let mut omega = OmegaSnapshot::new();
        let mut rngs: BTreeMap<RngStreamId, DetRng> = BTreeMap::new();
        let mut telem = TelemetryHook::new();
        let inputs: BTreeMap<RngStreamId, _> = BTreeMap::new();
        let mut b = make_brain();
        {
            let mut ctx = OmegaStepCtx::new(&mut omega, &mut rngs, &mut telem, 1, false, &inputs);
            let r = b.step(&mut ctx, 0.016);
            assert!(r.is_ok());
        }
        assert_eq!(telem.read_counter("ai_behav_brain_ticks"), 1);
        assert_eq!(b.tick_count(), 1);
    }

    #[test]
    fn brain_omega_system_halt_no_op() {
        let mut omega = OmegaSnapshot::new();
        let mut rngs: BTreeMap<RngStreamId, DetRng> = BTreeMap::new();
        let mut telem = TelemetryHook::new();
        let inputs: BTreeMap<RngStreamId, _> = BTreeMap::new();
        let mut b = make_brain();
        {
            let mut ctx = OmegaStepCtx::new(&mut omega, &mut rngs, &mut telem, 0, true, &inputs);
            b.step(&mut ctx, 0.016).unwrap();
        }
        // Halt path : tick counter still advanced ; FSM/BT/Util NOT executed.
        assert_eq!(b.tick_count(), 1);
        assert_eq!(b.blackboard().get_int(BB_KEY_TICK).unwrap(), 1);
        // FSM didn't tick — its key wasn't written.
        assert!(b.blackboard().get_text(BB_KEY_FSM_STATE).is_err());
    }

    #[test]
    fn brain_effect_row_is_sim() {
        let b = make_brain();
        let row = b.effect_row();
        assert!(row.contains(cssl_substrate_omega_step::SubstrateEffect::Sim));
    }

    #[test]
    fn brain_omega_name() {
        let b = make_brain();
        assert_eq!(OmegaSystem::name(&b), "test-brain");
    }

    #[test]
    fn brain_determinism_two_brains_same_inputs() {
        let mut a = make_brain();
        let mut b = make_brain();
        a.blackboard_mut().set_float("desire", 0.7);
        b.blackboard_mut().set_float("desire", 0.7);
        a.tick();
        b.tick();
        assert_eq!(
            a.blackboard().get_int(BB_KEY_ACTION_ID).unwrap(),
            b.blackboard().get_int(BB_KEY_ACTION_ID).unwrap()
        );
        assert_eq!(
            a.blackboard().get_text(BB_KEY_FSM_STATE).unwrap(),
            b.blackboard().get_text(BB_KEY_FSM_STATE).unwrap()
        );
        assert_eq!(a.tick_count(), b.tick_count());
    }

    #[test]
    fn brain_util_no_actions_no_action_id() {
        let util = UtilityAi::new(ActorKind::Npc).unwrap(); // no actions
        let mut b = AiBrainBuilder::<SimpleState>::new("w", ActorKind::Npc)
            .with_util(util)
            .build()
            .unwrap();
        b.tick();
        // BB_KEY_ACTION_ID never set because pick() errored.
        assert!(b.blackboard().get_int(BB_KEY_ACTION_ID).is_err());
        // But the brain still ticked.
        assert_eq!(b.tick_count(), 1);
    }

    #[test]
    fn brain_only_fsm_ticks_fsm() {
        let fsm = StateMachine::new(
            ActorKind::Npc,
            vec![SimpleState::Idle, SimpleState::Active],
            SimpleState::Idle,
        )
        .unwrap();
        let mut b = AiBrainBuilder::<SimpleState>::new("f-only", ActorKind::Npc)
            .with_fsm(fsm)
            .build()
            .unwrap();
        b.tick();
        assert_eq!(b.blackboard().get_text(BB_KEY_FSM_STATE).unwrap(), "idle");
        assert!(b.blackboard().get_int(BB_KEY_BT_STATUS).is_err());
    }

    #[test]
    fn brain_only_bt_ticks_bt() {
        let bt = BehaviorTree::new(
            ActorKind::Npc,
            BtNode::Leaf(LeafId(0)),
            vec![Box::new(AlwaysSucceedLeaf)],
        )
        .unwrap();
        let mut b = AiBrainBuilder::<SimpleState>::new("b-only", ActorKind::Npc)
            .with_bt(bt)
            .build()
            .unwrap();
        b.tick();
        assert_eq!(b.blackboard().get_int(BB_KEY_BT_STATUS).unwrap(), 1);
        assert!(b.blackboard().get_text(BB_KEY_FSM_STATE).is_err());
    }

    #[test]
    fn brain_consideration_id_unused_in_test() {
        // Tag the unused-import lint without touching the production module.
        let _ = ConsiderationId(0);
    }
}
