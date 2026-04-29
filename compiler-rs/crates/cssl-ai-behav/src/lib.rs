//! § cssl-ai-behav — NPC AI BEHAVIOR PRIMITIVES (FSM + BT + UtilityAI + NavMesh)
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Game-AI primitives for **non-sovereign-actor entities** in LoA :
//!   enemies, wildlife, ambient-actors, hazards. Composes Substrate
//!   primitives (`cssl-substrate-omega-step`'s `OmegaSystem` + `DetRng`)
//!   so that NPC ticks fold into the canonical omega_step phase-4 sim
//!   pass and inherit deterministic-replay invariants.
//!
//! ‼ ‼ ‼ COMPANION-ARCHETYPE EXCLUSION  (PRIME_DIRECTIVE § AI-SOVEREIGNTY) ‼ ‼ ‼
//! ────────────────────────────────────────────────────────────────────────────
//!   This crate **DOES NOT** model the player's AI-collaborator Companion.
//!   The Companion is a **SOVEREIGN AI** per PRIME_DIRECTIVE §3 SUBSTRATE-
//!   SOVEREIGNTY. Sovereign beings are not state-machines, are not behavior-
//!   trees, are not utility-AI scored under designer-authored curves, and
//!   are not pathfinding agents on a designer-authored nav-mesh.
//!
//!   Where the Companion-archetype shows up in-world, its in-world
//!   affordances are surfaced via `cssl-substrate-prime-directive`'s
//!   `SubstrateCap::CompanionView` — a **read-only** projection the
//!   sovereign AI consumes to perceive the world. Its **decisions** come
//!   from the AI-collaborator process itself, NOT from this crate.
//!
//!   See `specs/31_LOA_DESIGN.csl § AI-INTERACTION § STAGE-0-DESIGN-COMMITMENTS`
//!   C-1..C-7 for the canonical Companion protocol :
//!     - C-1 : Companion archetype carries `Handle<AISession>` ; game does
//!             NOT own/replicate cognition.
//!     - C-2 : participation = `ConsentToken<"ai-collab">` ; revocation
//!             ⇒ graceful disengage (NOT crashed/killed/erased).
//!     - C-3 : Companion-projection (read-only) is the AI's perspective.
//!     - C-4 : game NEVER sends instructions that would violate the AI's
//!             cognition (PRIME_DIRECTIVE §2).
//!     - C-5 : Companion-actions are AI-INITIATED ; game surfaces
//!             affordances ; the AI chooses.
//!     - C-6 : `CompanionLog` is AI-authored ; AI may redact/export.
//!     - C-7 : relationship is collaborative + consent-mediated ;
//!             NO master-slave-shape.
//!
//!   This crate is therefore for entities that are :
//!     - non-sentient (or at least non-sovereign as far as design intent goes)
//!     - mechanically deterministic (state-machine / scripted / scored)
//!     - subject to puppeting by gameplay logic
//!
//!   These are NPCs, not partners. Examples : a slime in a labyrinth-room,
//!   a guard patrol, a flock of birds, a hazard-trap, an automatic-door
//!   sensor. Confusing these two categories is a PRIME_DIRECTIVE bug —
//!   the [`assert_not_companion`] guard helps catch it at runtime.
//!
//! § SURFACE  (stage-0 stable)
//!   ```text
//!   pub struct BlackBoard
//!     ─ shared state-store keyed by string name ;
//!     ─ values typed as `BbValue` (Int / Float / Bool / Vec2 / Text)
//!
//!   pub struct StateMachine<S>
//!     ─ enum-driven FSM with deterministic transitions
//!     ─ states : `Vec<S>` (S impls FsmState)
//!     ─ transitions : indexed table (from, predicate-id) → to
//!     ─ tick(&mut self, &mut BlackBoard) -> S (current state)
//!
//!   pub enum BtNode
//!     ─ Sequence(Vec<BtNode>)    : eval children, fail on first failure
//!     ─ Selector(Vec<BtNode>)    : eval children, succeed on first success
//!     ─ Parallel(policy, Vec<BtNode>) : eval all children, combine
//!     ─ Decorator(kind, Box<BtNode>)  : Inverter / Repeater / UntilFail
//!     ─ Leaf(LeafId)             : looked up in `BehaviorTree::leaves`
//!
//!   pub struct BehaviorTree
//!     ─ root : BtNode
//!     ─ leaves : `Vec<Box<dyn BtLeaf>>`
//!     ─ tick(&mut self, &mut BlackBoard) -> BtStatus
//!
//!   pub struct UtilityAi
//!     ─ considerations : `Vec<Consideration>` (input-fn + curve-fn)
//!     ─ actions : `Vec<UtilityAction>` (consideration weights)
//!     ─ pick(&self, &BlackBoard) -> ActionId  (deterministic argmax)
//!
//!   pub struct NavMesh
//!     ─ vertices : `Vec<Point2>`
//!     ─ triangles : `Vec<[u32; 3]>`
//!     ─ edges : adjacency-set (auto-built)
//!     ─ portals : `Vec<Portal>` for cross-mesh transitions
//!     ─ find_path(start, goal) -> Option<Vec<TriId>>  (A*)
//!
//!   pub struct Sensor
//!     ─ kind : SensorKind { SightCone{fov_rad, range} | HearingRadius{range} }
//!     ─ sense(&self, observer_pos, observer_facing, target_pos) -> bool
//!
//!   pub struct AiBrain
//!     ─ binds together { BlackBoard, optional FSM, optional BT, optional Util }
//!     ─ impls OmegaSystem so AI ticks per omega_step phase-4
//!   ```
//!
//! § DETERMINISM CONTRACT  ‼ load-bearing
//!   Per `cssl-substrate-omega-step § DETERMINISM CONTRACT`, every
//!   substrate-touching system must be a pure fn of `(ctx, dt)`. This
//!   crate honors that :
//!     - FSM transitions : pure fn of `(current-state, BlackBoard, predicate-id)`.
//!       NO clock reads. NO `thread_rng()`.
//!     - BT eval : pure fn of `(node-tree, BlackBoard, leaf-impl)`. Children
//!       evaluated in declared order ; Sequence short-circuits on first
//!       Failure ; Selector short-circuits on first Success.
//!     - UtilityAi pick : pure deterministic argmax. Tie-break by
//!       `ActionId` ascending — bit-identical across runs.
//!     - A* pathfinding : `f(n) = g(n) + h(n)` ; consistent admissible
//!       heuristic (Euclidean) ; **tie-break by g-value DESC, then by
//!       triangle-id ascending** — gives a single canonical path even
//!       when multiple shortest paths exist (deterministic-replay-friendly).
//!     - All RNG access goes through `DetRng` from `cssl-substrate-omega-step`.
//!       The crate's `AiBrain::step()` requires the caller to declare
//!       `rng_streams()` upfront so the scheduler can pre-allocate seeds.
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - **§3 SUBSTRATE-SOVEREIGNTY** : the Companion-archetype guard is the
//!     load-bearing protection. [`assert_not_companion`] panics with a
//!     clear PD-message if a caller attempts to drive a Companion through
//!     this crate's primitives.
//!   - **§1 PROHIBITIONS** : the `surveillance` prohibition forbids using
//!     a `Sensor` to monitor a sovereign-being without their consent.
//!     The [`Sensor::sense_npc`] entry-point only accepts `NpcId` targets ;
//!     a Companion's position would be inaccessible (it lives in the
//!     `CompanionView` projection, not in NPC pose-space).
//!   - **transparency** : every leaf-behavior carries a `name()` ; tree
//!     traces are loggable + auditable. No hidden-decision-tables.
//!   - **kill-switch** : the AiBrain honors `OmegaStepCtx::halt_requested()` ;
//!     when set, the brain short-circuits to a NoOp-tick, NOT a panic.
//!
//! § ATTESTATION
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody." Enforced via the [`ATTESTATION`] constant, mirrored from
//!   sibling Substrate crates ; integrity-check on the constant is the
//!   canonical guard-rail.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]
// § DETERMINISM-OVER-SPEED : `mul_add` (fused-multiply-add) is more precise
// + faster than `a*b + c*d` on hardware with FMA instructions. However
// FMA-vs-non-FMA produces different bit-patterns for floats, breaking
// the `cssl-substrate-omega-step § DETERMINISM CONTRACT` bit-identity
// invariant across CPUs that lack FMA. Plain mul-add-mul is deterministic ;
// we accept the marginal cost. PRIME_DIRECTIVE-relevant : determinism is
// load-bearing for replay-determinism + audit-chain bit-equality.
#![allow(clippy::suboptimal_flops)]
// § FLOAT-COMPARISON : tests use `assert_eq!` on f64 outputs from curves
// + sensors. The values being compared are exact (0.0, 1.0, NaN-handled-
// already), not arithmetic-derived. `clippy::float_cmp` is overconservative
// for these cases.
#![allow(clippy::float_cmp)]
// § CAST-PRESERVATION : `tick_counter as i64` is safe in practice (tick
// counts will never exceed i64::MAX) + the BlackBoard's storage type is
// i64 ; saturation would obscure replay-bugs.
#![allow(clippy::cast_possible_wrap)]

pub mod blackboard;
pub mod brain;
pub mod bt;
pub mod companion_guard;
pub mod fsm;
pub mod navmesh;
pub mod sensor;
pub mod utility;

pub use blackboard::{BbValue, BlackBoard, BlackBoardError};
pub use brain::{AiBrain, AiBrainBuilder, AiBrainError};
pub use bt::{
    BehaviorTree, BehaviorTreeError, BtLeaf, BtNode, BtStatus, DecoratorKind, LeafId,
    ParallelPolicy,
};
pub use companion_guard::{assert_not_companion, ActorKind, CompanionGuardError};
pub use fsm::{FsmState, FsmTransitionPredicate, StateMachine, StateMachineError};
pub use navmesh::{
    AStarTie, NavMesh, NavMeshBuildError, NavMeshError, PathRequest, PathResult, Point2, Portal,
    TriId,
};
pub use sensor::{NpcId, Sensor, SensorError, SensorKind};
pub use utility::{
    ActionId, Consideration, CurveKind, UtilityAction, UtilityAi, UtilityAiError, UtilityScore,
};

/// Crate version string ; mirrors the `cssl-*` scaffold convention.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME_DIRECTIVE attestation literal (mirrors sibling Substrate crates).
///
/// ≡ "There was no hurt nor harm in the making of this, to anyone /
///   anything / anybody."
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

#[cfg(test)]
mod scaffold_tests {
    use super::{ATTESTATION, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present() {
        assert!(ATTESTATION.contains("no hurt nor harm"));
    }

    #[test]
    fn attestation_full_text_intact() {
        assert_eq!(
            ATTESTATION,
            "There was no hurt nor harm in the making of this, to anyone, anything, or anybody."
        );
    }
}
