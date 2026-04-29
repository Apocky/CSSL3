//! Integration tests for `cssl-ai-behav`.
//!
//! § ROUND-TRIP CHECK (mandatory per dispatch-prompt)
//!   FSM + BT + A*-pathfind round-trip end-to-end : an NPC that uses
//!   the FSM to gate which behavior to run (patrol / chase), the BT to
//!   sequence sub-behaviors (search-then-attack), and the NavMesh to
//!   find a path between rooms. Demonstrates the surface composes
//!   coherently + remains deterministic across runs.

use cssl_ai_behav::{
    assert_not_companion, ActorKind, AiBrain, AiBrainBuilder, BbValue, BehaviorTree, BlackBoard,
    BtLeaf, BtNode, BtStatus, CompanionGuardError, CurveKind, Consideration, FsmState,
    FsmTransitionPredicate, LeafId, NavMesh, PathRequest, Point2, StateMachine, TriId,
    UtilityAction, UtilityAi,
};
use cssl_substrate_omega_step::{OmegaScheduler, SchedulerConfig};

/// Top-level FSM states for an integration NPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum NpcState {
    Patrol,
    Chase,
    Attack,
}
impl FsmState for NpcState {
    fn name(&self) -> &'static str {
        match self {
            Self::Patrol => "patrol",
            Self::Chase => "chase",
            Self::Attack => "attack",
        }
    }
}

/// A leaf that records its name into the BB so the integration test can
/// trace the BT's exec sequence.
struct NamedLeaf {
    name: String,
    status: BtStatus,
}
impl BtLeaf for NamedLeaf {
    fn tick(&mut self, bb: &mut BlackBoard) -> BtStatus {
        // Append a token to the trace.
        let prev = bb.get_text("trace").unwrap_or("").to_string();
        let next = if prev.is_empty() {
            self.name.clone()
        } else {
            format!("{prev},{}", self.name)
        };
        bb.set_text("trace", next);
        self.status
    }
    fn name(&self) -> &str {
        &self.name
    }
}

#[test]
fn companion_guard_runtime_blocks_all_entries() {
    // A direct check that every public-entry-with-an-actor-kind argument
    // rejects Companion. This is the "load-bearing test" that auditors
    // will look for first when reviewing this crate against PRIME_DIRECTIVE §3.
    assert!(matches!(
        assert_not_companion(ActorKind::Companion),
        Err(CompanionGuardError::CompanionNotPermitted)
    ));
    assert!(StateMachine::<NpcState>::new(
        ActorKind::Companion,
        vec![NpcState::Patrol],
        NpcState::Patrol,
    )
    .is_err());
    assert!(BehaviorTree::new(
        ActorKind::Companion,
        BtNode::Sequence(vec![]),
        vec![]
    )
    .is_err());
    assert!(UtilityAi::new(ActorKind::Companion).is_err());
    assert!(AiBrainBuilder::<NpcState>::new("c", ActorKind::Companion)
        .build()
        .is_err());
}

#[test]
fn fsm_bt_navmesh_round_trip() {
    // ‼ Top-line round-trip test : assemble FSM + BT + NavMesh, run a
    // sequence of ticks, observe deterministic outputs end-to-end.

    // 1. NavMesh : a 5-triangle corridor.
    let v = vec![
        Point2::new(0.0, 0.0),
        Point2::new(0.0, 1.0),
        Point2::new(1.0, 0.0),
        Point2::new(1.0, 1.0),
        Point2::new(2.0, 0.0),
        Point2::new(2.0, 1.0),
        Point2::new(3.0, 0.0),
        Point2::new(3.0, 1.0),
    ];
    let t = vec![
        [0, 2, 1],
        [2, 3, 1],
        [2, 4, 3],
        [4, 5, 3],
        [4, 6, 5],
    ];
    let mesh = NavMesh::build(v, t).unwrap();

    // Pathfind across the corridor.
    let path = mesh
        .find_path(PathRequest::new(TriId(0), TriId(4)))
        .unwrap();
    assert!(!path.path.is_empty());
    assert_eq!(*path.path.first().unwrap(), TriId(0));
    assert_eq!(*path.path.last().unwrap(), TriId(4));
    assert!(path.cost > 0.0);

    // 2. FSM : Patrol → Chase → Attack.
    let mut fsm = StateMachine::new(
        ActorKind::Npc,
        vec![NpcState::Patrol, NpcState::Chase, NpcState::Attack],
        NpcState::Patrol,
    )
    .unwrap();
    fsm.add_transition(
        NpcState::Patrol,
        NpcState::Chase,
        FsmTransitionPredicate::new("see-target", |bb| bb.get_bool("see_target").unwrap_or(false)),
    )
    .unwrap();
    fsm.add_transition(
        NpcState::Chase,
        NpcState::Attack,
        FsmTransitionPredicate::new("in-range", |bb| {
            bb.get_float("dist").map(|d| d < 1.0).unwrap_or(false)
        }),
    )
    .unwrap();
    fsm.add_transition(
        NpcState::Attack,
        NpcState::Patrol,
        FsmTransitionPredicate::new("target-gone", |bb| {
            !bb.get_bool("see_target").unwrap_or(false)
        }),
    )
    .unwrap();

    // 3. BT : Sequence{search, attack} ; both Success.
    let bt = BehaviorTree::new(
        ActorKind::Npc,
        BtNode::Sequence(vec![BtNode::Leaf(LeafId(0)), BtNode::Leaf(LeafId(1))]),
        vec![
            Box::new(NamedLeaf {
                name: "search".into(),
                status: BtStatus::Success,
            }),
            Box::new(NamedLeaf {
                name: "attack".into(),
                status: BtStatus::Success,
            }),
        ],
    )
    .unwrap();

    // 4. UtilityAi : pick action by current dist.
    let mut util = UtilityAi::new(ActorKind::Npc).unwrap();
    let c = util.add_consideration(Consideration::new(
        "want-attack",
        CurveKind::Inverse, // closer = more want
        |bb| bb.get_float("dist").unwrap_or(1.0).clamp(0.0, 1.0),
    ));
    let _action_attack = util.add_action(UtilityAction::new("attack", vec![c])).unwrap();
    let _action_idle = util.add_action(UtilityAction::new("idle", vec![])).unwrap();

    // 5. Assemble brain ; tick a sequence.
    let mut brain: AiBrain<NpcState> = AiBrainBuilder::new("npc-1", ActorKind::Npc)
        .with_fsm(fsm)
        .with_bt(bt)
        .with_util(util)
        .build()
        .unwrap();

    // Initial tick : nothing seen, FSM stays Patrol.
    brain.tick();
    assert_eq!(brain.blackboard().get_text("_brain_fsm_state").unwrap(), "patrol");
    assert_eq!(brain.blackboard().get_int("_brain_bt_status").unwrap(), 1); // success

    // Tick 2 : enemy spotted, dist far.
    brain.blackboard_mut().set_bool("see_target", true);
    brain.blackboard_mut().set_float("dist", 5.0);
    brain.tick();
    assert_eq!(brain.blackboard().get_text("_brain_fsm_state").unwrap(), "chase");

    // Tick 3 : closing in, dist < 1.
    brain.blackboard_mut().set_float("dist", 0.5);
    brain.tick();
    assert_eq!(brain.blackboard().get_text("_brain_fsm_state").unwrap(), "attack");

    // Tick 4 : target gone.
    brain.blackboard_mut().set_bool("see_target", false);
    brain.tick();
    assert_eq!(brain.blackboard().get_text("_brain_fsm_state").unwrap(), "patrol");

    // Tick counter has advanced to 4.
    assert_eq!(brain.tick_count(), 4);

    // BT trace : sequence ran twice per tick (search,attack), 4 ticks ;
    // the trace contains 8 tokens.
    let trace = brain.blackboard().get_text("trace").unwrap();
    let count = trace.split(',').count();
    assert_eq!(count, 8);
}

#[test]
fn brain_via_omega_scheduler_integration() {
    // Demonstrate the brain integrates cleanly with the canonical
    // OmegaScheduler via the OmegaSystem trait.
    let mut sched = OmegaScheduler::new(SchedulerConfig::default());
    let mut fsm = StateMachine::<NpcState>::new(
        ActorKind::Npc,
        vec![NpcState::Patrol, NpcState::Chase],
        NpcState::Patrol,
    )
    .unwrap();
    fsm.add_transition(
        NpcState::Patrol,
        NpcState::Chase,
        FsmTransitionPredicate::new("trigger", |bb| {
            bb.get_bool("trigger").unwrap_or(false)
        }),
    )
    .unwrap();
    let brain: AiBrain<NpcState> = AiBrainBuilder::new("scheduled-brain", ActorKind::Npc)
        .with_fsm(fsm)
        .build()
        .unwrap();
    let grant = cssl_substrate_omega_step::caps_grant(
        cssl_substrate_omega_step::OmegaCapability::OmegaRegister,
    );
    let _id = sched.register(brain, &grant).unwrap();
    sched.step(0.016).unwrap();
    sched.step(0.016).unwrap();
}

#[test]
fn determinism_two_brains_identical_input_identical_output() {
    // Two brains built from identical specs + given identical inputs
    // must produce bit-equal BlackBoard state.
    let make = || -> AiBrain<NpcState> {
        let mut fsm = StateMachine::new(
            ActorKind::Npc,
            vec![NpcState::Patrol, NpcState::Chase, NpcState::Attack],
            NpcState::Patrol,
        )
        .unwrap();
        fsm.add_transition(
            NpcState::Patrol,
            NpcState::Chase,
            FsmTransitionPredicate::new("see", |bb| bb.get_bool("see").unwrap_or(false)),
        )
        .unwrap();
        let bt = BehaviorTree::new(
            ActorKind::Npc,
            BtNode::Selector(vec![BtNode::Leaf(LeafId(0)), BtNode::Leaf(LeafId(1))]),
            vec![
                Box::new(NamedLeaf {
                    name: "a".into(),
                    status: BtStatus::Failure,
                }),
                Box::new(NamedLeaf {
                    name: "b".into(),
                    status: BtStatus::Success,
                }),
            ],
        )
        .unwrap();
        let mut util = UtilityAi::new(ActorKind::Npc).unwrap();
        let c = util.add_consideration(Consideration::new(
            "x",
            CurveKind::Quadratic,
            |bb| bb.get_float("x").unwrap_or(0.0),
        ));
        let _ = util.add_action(UtilityAction::new("a", vec![c])).unwrap();
        AiBrainBuilder::<NpcState>::new("b", ActorKind::Npc)
            .with_fsm(fsm)
            .with_bt(bt)
            .with_util(util)
            .with_bb_entry("x", BbValue::Float(0.7))
            .build()
            .unwrap()
    };

    let mut a = make();
    let mut b = make();
    a.tick();
    b.tick();
    a.tick();
    b.tick();
    a.blackboard_mut().set_bool("see", true);
    b.blackboard_mut().set_bool("see", true);
    a.tick();
    b.tick();

    // BB bit-equality.
    assert!(a.blackboard().bit_eq(b.blackboard()));
    assert_eq!(a.tick_count(), b.tick_count());
}

#[test]
fn navmesh_path_determinism_round_trip() {
    // Same nav-mesh + same request ⇒ bit-identical path twice in a row.
    let v = vec![
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(0.0, 1.0),
        Point2::new(1.0, 1.0),
        Point2::new(2.0, 0.0),
        Point2::new(2.0, 1.0),
    ];
    let t = vec![[0, 1, 2], [1, 3, 2], [1, 4, 3], [4, 5, 3]];
    let mesh = NavMesh::build(v, t).unwrap();
    let r1 = mesh
        .find_path(PathRequest::new(TriId(0), TriId(3)))
        .unwrap();
    let r2 = mesh
        .find_path(PathRequest::new(TriId(0), TriId(3)))
        .unwrap();
    assert_eq!(r1.path, r2.path);
    assert_eq!(r1.cost.to_bits(), r2.cost.to_bits());
}
