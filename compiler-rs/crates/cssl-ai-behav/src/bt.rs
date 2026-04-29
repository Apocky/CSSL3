//! BehaviorTree — composite decision-tree for NPC behavior.
//!
//! § THESIS
//!   Behavior trees layer atop simple primitives via composite nodes :
//!     - **Sequence** : run children left-to-right ; FAIL on first failure ;
//!                       SUCCEED only when all succeed ; RUNNING propagates.
//!     - **Selector** : run children left-to-right ; SUCCEED on first success ;
//!                       FAIL only when all fail ; RUNNING propagates.
//!     - **Parallel** : run all children regardless ; combine results via
//!                       a `ParallelPolicy` (RequireAll / RequireOne / Tally).
//!     - **Decorator** : Inverter / Repeater / UntilFail / etc.
//!     - **Leaf**     : the unit-of-work — looked up via `LeafId` in the
//!                       tree's `leaves: Vec<Box<dyn BtLeaf>>`.
//!
//! § DETERMINISM (‼ load-bearing)
//!   - Children evaluated in **declared order** ; Sequence + Selector
//!     **short-circuit** at first matching outcome (per spec landmines).
//!   - Leaf-tick is a pure fn of `(LeafId, &mut BlackBoard)` ;
//!     each leaf's `tick()` may mutate the BlackBoard but MUST NOT
//!     read clocks or entropy.
//!   - Parallel-tally uses BTreeMap accumulators so cumulative iteration
//!     is order-stable.
//!   - No internal RNG — all randomness comes from the brain's pre-seeded
//!     `DetRng` written into the BlackBoard.
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - Every leaf has a `name()` ; the tree is fully traceable.
//!   - The Companion-archetype is rejected at `BehaviorTree::new`.
//!   - Tree depth is bounded by the caller — no unbounded-recursion shape.

use std::fmt;

use thiserror::Error;

use crate::blackboard::BlackBoard;
use crate::companion_guard::{assert_not_companion, ActorKind, CompanionGuardError};

/// Behavior-tree tick status — three-valued logic.
///
/// § STAGE-0 STABLE
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BtStatus {
    /// The leaf/composite finished successfully this tick.
    Success,
    /// The leaf/composite failed this tick.
    Failure,
    /// The leaf/composite is mid-execution — needs more ticks.
    Running,
}

/// Decorator behaviors. Each wraps exactly one child.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecoratorKind {
    /// Invert child : Success ↔ Failure ; Running passes through.
    Inverter,
    /// Always succeed regardless of child outcome (Running passes through).
    AlwaysSuccess,
    /// Always fail regardless of child outcome (Running passes through).
    AlwaysFailure,
    /// Repeat child up to `n` times. After `n` ticks where the child
    /// completed (Success or Failure), the decorator returns Success.
    /// Running ticks do NOT count toward the limit ; the child may be
    /// mid-execution. Stage-0 stores `n` in the variant payload via a
    /// separate `Repeater(u32)` ; we use `Repeater(u32)` below.
    Repeater(u32),
    /// Run child until it returns Failure ; then the decorator returns
    /// Success. If child Succeeds the decorator stays Running.
    UntilFail,
}

/// Combination policy for a Parallel composite.
///
/// § Stage-0 covers the canonical three forms ; additional policies can
///   be appended without breaking ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParallelPolicy {
    /// Composite Succeeds iff ALL children Succeed.
    /// Composite Fails on first Failure.
    /// Otherwise Running.
    RequireAll,
    /// Composite Succeeds on first Success.
    /// Composite Fails iff ALL children Fail.
    /// Otherwise Running.
    RequireOne,
    /// Composite returns Success iff `>= threshold` children Succeed in
    /// the same tick. Failure if `>= (n - threshold + 1)` Fail.
    /// Otherwise Running.
    Tally { threshold: u32 },
}

/// A behavior-tree node.
///
/// § DESIGN
///   Stage-0 stores the tree as an explicit enum-shape rather than a
///   trait-object pool. This keeps tree-traversal allocation-free at
///   tick-time + makes the tree fully introspectable for audit dumps.
#[derive(Debug)]
pub enum BtNode {
    /// Sequence : children evaluated in order ; first Failure short-circuits.
    Sequence(Vec<BtNode>),
    /// Selector : children evaluated in order ; first Success short-circuits.
    Selector(Vec<BtNode>),
    /// Parallel : all children evaluated ; outcome combined per policy.
    Parallel(ParallelPolicy, Vec<BtNode>),
    /// Decorator : exactly one child, transformed via `DecoratorKind`.
    /// Stage-0 packs the decorator's transient state alongside the kind ;
    /// for `Repeater(n)`, see [`BehaviorTree::new`] for `state` init.
    Decorator(DecoratorKind, Box<BtNode>),
    /// Leaf : looked up via `LeafId` in the tree's `leaves` vector.
    Leaf(LeafId),
}

/// Identifier for a leaf in the tree's `leaves` vector. Issued at tree-
/// construction time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LeafId(pub u32);

/// Leaf-behavior trait. Implementors are the unit-of-work : "MoveTowardTarget",
/// "AttackEnemy", "Flee", etc.
///
/// § REQUIREMENTS
///   - `tick(&mut self, bb: &mut BlackBoard) -> BtStatus` : pure fn of
///     `(self, bb)` ; mutates the BlackBoard, returns the tick outcome.
///   - `name()` : audit-readable identifier.
pub trait BtLeaf: Send + Sync + 'static {
    /// Tick this leaf. Mutates BlackBoard ; returns Success/Failure/Running.
    fn tick(&mut self, bb: &mut BlackBoard) -> BtStatus;
    /// Audit-readable name for telemetry + debug.
    fn name(&self) -> &str;
}

/// Errors the BehaviorTree surfaces.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum BehaviorTreeError {
    /// Caller attempted to drive a Companion via BT.
    #[error("AIBEHAV0030 — BehaviorTree rejects Companion-archetype: {0}")]
    Companion(#[from] CompanionGuardError),

    /// Tree references a `LeafId` not in `leaves`.
    #[error("AIBEHAV0031 — leaf id {id} out of bounds (have {leaves} leaves)")]
    LeafIdOutOfBounds { id: u32, leaves: u32 },

    /// Decorator wraps an empty child slot (stage-0 disallows None children).
    #[error("AIBEHAV0032 — decorator child must be non-null")]
    EmptyDecoratorChild,

    /// Parallel-Tally threshold is 0 or > children-count.
    #[error("AIBEHAV0033 — Parallel-Tally threshold {threshold} invalid for {children} children")]
    InvalidTallyThreshold { threshold: u32, children: u32 },
}

impl BehaviorTreeError {
    /// Stable diagnostic code prefix.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Companion(_) => "AIBEHAV0030",
            Self::LeafIdOutOfBounds { .. } => "AIBEHAV0031",
            Self::EmptyDecoratorChild => "AIBEHAV0032",
            Self::InvalidTallyThreshold { .. } => "AIBEHAV0033",
        }
    }
}

/// A behavior tree — the executable form.
pub struct BehaviorTree {
    root: BtNode,
    leaves: Vec<Box<dyn BtLeaf>>,
    /// Decorator-Repeater state : key = "deterministic-path-string" ; value = remaining-runs.
    /// Stage-0 stores per-path state in the BlackBoard via the brain ;
    /// repeater bookkeeping kept here is local to the tick.
    repeater_remaining: Vec<u32>,
    /// Path-stack used to assign deterministic ids to repeater nodes
    /// during construction (so per-tick state is stable across runs).
    repeater_paths: Vec<u32>,
    tick_count: u64,
}

impl BehaviorTree {
    /// Construct a new BehaviorTree.
    ///
    /// § GUARD
    ///   Companion-archetype is rejected per PRIME_DIRECTIVE §3.
    pub fn new(
        kind: ActorKind,
        root: BtNode,
        leaves: Vec<Box<dyn BtLeaf>>,
    ) -> Result<Self, BehaviorTreeError> {
        assert_not_companion(kind)?;
        let leaves_count = leaves.len() as u32;
        let mut repeater_remaining = Vec::new();
        let mut repeater_paths = Vec::new();
        Self::validate_node(&root, leaves_count, &mut repeater_remaining, &mut repeater_paths)?;
        Ok(Self {
            root,
            leaves,
            repeater_remaining,
            repeater_paths,
            tick_count: 0,
        })
    }

    /// Recursive validation : every Leaf has a valid LeafId ; every
    /// Decorator(Repeater) registers a slot for state.
    fn validate_node(
        node: &BtNode,
        leaves_count: u32,
        repeater_remaining: &mut Vec<u32>,
        repeater_paths: &mut Vec<u32>,
    ) -> Result<(), BehaviorTreeError> {
        match node {
            BtNode::Leaf(LeafId(id)) => {
                if *id >= leaves_count {
                    return Err(BehaviorTreeError::LeafIdOutOfBounds {
                        id: *id,
                        leaves: leaves_count,
                    });
                }
                Ok(())
            }
            BtNode::Sequence(children) | BtNode::Selector(children) => {
                for child in children {
                    Self::validate_node(child, leaves_count, repeater_remaining, repeater_paths)?;
                }
                Ok(())
            }
            BtNode::Parallel(policy, children) => {
                if let ParallelPolicy::Tally { threshold } = policy {
                    let n = children.len() as u32;
                    if *threshold == 0 || *threshold > n {
                        return Err(BehaviorTreeError::InvalidTallyThreshold {
                            threshold: *threshold,
                            children: n,
                        });
                    }
                }
                for child in children {
                    Self::validate_node(child, leaves_count, repeater_remaining, repeater_paths)?;
                }
                Ok(())
            }
            BtNode::Decorator(kind, child) => {
                if let DecoratorKind::Repeater(n) = kind {
                    let idx = repeater_remaining.len() as u32;
                    repeater_paths.push(idx);
                    repeater_remaining.push(*n);
                }
                Self::validate_node(child, leaves_count, repeater_remaining, repeater_paths)
            }
        }
    }

    /// Tick the tree. Returns the root's status.
    pub fn tick(&mut self, bb: &mut BlackBoard) -> BtStatus {
        let mut repeater_idx_cursor = 0u32;
        let status = Self::tick_node(
            &self.root,
            &mut self.leaves,
            &mut self.repeater_remaining,
            &mut repeater_idx_cursor,
            bb,
        );
        self.tick_count = self.tick_count.saturating_add(1);
        status
    }

    /// Recursive tick implementation.
    fn tick_node(
        node: &BtNode,
        leaves: &mut [Box<dyn BtLeaf>],
        repeater_remaining: &mut [u32],
        repeater_idx_cursor: &mut u32,
        bb: &mut BlackBoard,
    ) -> BtStatus {
        match node {
            BtNode::Leaf(LeafId(id)) => {
                let leaf = &mut leaves[*id as usize];
                leaf.tick(bb)
            }
            BtNode::Sequence(children) => {
                // Children in order ; first Failure short-circuits ; first Running propagates.
                for child in children {
                    let s = Self::tick_node(child, leaves, repeater_remaining, repeater_idx_cursor, bb);
                    match s {
                        BtStatus::Failure => return BtStatus::Failure,
                        BtStatus::Running => return BtStatus::Running,
                        BtStatus::Success => continue,
                    }
                }
                BtStatus::Success
            }
            BtNode::Selector(children) => {
                // Children in order ; first Success short-circuits ; first Running propagates.
                for child in children {
                    let s = Self::tick_node(child, leaves, repeater_remaining, repeater_idx_cursor, bb);
                    match s {
                        BtStatus::Success => return BtStatus::Success,
                        BtStatus::Running => return BtStatus::Running,
                        BtStatus::Failure => continue,
                    }
                }
                BtStatus::Failure
            }
            BtNode::Parallel(policy, children) => {
                let mut succ = 0u32;
                let mut fail = 0u32;
                let mut running = 0u32;
                for child in children {
                    let s = Self::tick_node(child, leaves, repeater_remaining, repeater_idx_cursor, bb);
                    match s {
                        BtStatus::Success => succ += 1,
                        BtStatus::Failure => fail += 1,
                        BtStatus::Running => running += 1,
                    }
                }
                let n = children.len() as u32;
                match policy {
                    ParallelPolicy::RequireAll => {
                        if fail > 0 {
                            BtStatus::Failure
                        } else if succ == n {
                            BtStatus::Success
                        } else {
                            BtStatus::Running
                        }
                    }
                    ParallelPolicy::RequireOne => {
                        if succ > 0 {
                            BtStatus::Success
                        } else if fail == n {
                            BtStatus::Failure
                        } else {
                            BtStatus::Running
                        }
                    }
                    ParallelPolicy::Tally { threshold } => {
                        if succ >= *threshold {
                            BtStatus::Success
                        } else if fail >= n.saturating_sub(*threshold).saturating_add(1) {
                            BtStatus::Failure
                        } else if running > 0 {
                            BtStatus::Running
                        } else {
                            BtStatus::Failure
                        }
                    }
                }
            }
            BtNode::Decorator(kind, child) => {
                match kind {
                    DecoratorKind::Inverter => {
                        let s = Self::tick_node(child, leaves, repeater_remaining, repeater_idx_cursor, bb);
                        match s {
                            BtStatus::Success => BtStatus::Failure,
                            BtStatus::Failure => BtStatus::Success,
                            BtStatus::Running => BtStatus::Running,
                        }
                    }
                    DecoratorKind::AlwaysSuccess => {
                        let s = Self::tick_node(child, leaves, repeater_remaining, repeater_idx_cursor, bb);
                        match s {
                            BtStatus::Running => BtStatus::Running,
                            _ => BtStatus::Success,
                        }
                    }
                    DecoratorKind::AlwaysFailure => {
                        let s = Self::tick_node(child, leaves, repeater_remaining, repeater_idx_cursor, bb);
                        match s {
                            BtStatus::Running => BtStatus::Running,
                            _ => BtStatus::Failure,
                        }
                    }
                    DecoratorKind::Repeater(_) => {
                        // Each Repeater allocates one slot in repeater_remaining
                        // at validation time ; we walk the tree in the same
                        // order at tick-time so cursor positions match.
                        let idx = *repeater_idx_cursor as usize;
                        *repeater_idx_cursor += 1;
                        let s = Self::tick_node(child, leaves, repeater_remaining, repeater_idx_cursor, bb);
                        match s {
                            BtStatus::Running => BtStatus::Running,
                            BtStatus::Success | BtStatus::Failure => {
                                if repeater_remaining[idx] > 0 {
                                    repeater_remaining[idx] -= 1;
                                }
                                if repeater_remaining[idx] == 0 {
                                    BtStatus::Success
                                } else {
                                    BtStatus::Running
                                }
                            }
                        }
                    }
                    DecoratorKind::UntilFail => {
                        let s = Self::tick_node(child, leaves, repeater_remaining, repeater_idx_cursor, bb);
                        match s {
                            BtStatus::Failure => BtStatus::Success,
                            BtStatus::Success | BtStatus::Running => BtStatus::Running,
                        }
                    }
                }
            }
        }
    }

    /// Number of ticks since construction. Used by replay-tests.
    #[must_use]
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Number of leaves registered.
    #[must_use]
    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }

    /// Reset all repeater state — used by tests + scripted-event resets.
    pub fn reset_repeaters(&mut self) {
        // Re-validate the tree to refresh the repeater_remaining vector.
        let root_clone = std::mem::replace(&mut self.root, BtNode::Sequence(Vec::new()));
        self.repeater_remaining.clear();
        self.repeater_paths.clear();
        let _ = Self::validate_node(
            &root_clone,
            self.leaves.len() as u32,
            &mut self.repeater_remaining,
            &mut self.repeater_paths,
        );
        self.root = root_clone;
    }
}

impl fmt::Debug for BehaviorTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BehaviorTree")
            .field("root", &self.root)
            .field("leaf_count", &self.leaves.len())
            .field("tick_count", &self.tick_count)
            .field("repeater_count", &self.repeater_remaining.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial leaf that emits a fixed status.
    struct FixedLeaf {
        name: String,
        status: BtStatus,
    }
    impl BtLeaf for FixedLeaf {
        fn tick(&mut self, _bb: &mut BlackBoard) -> BtStatus {
            self.status
        }
        fn name(&self) -> &str {
            &self.name
        }
    }

    /// A leaf that increments a BB counter and returns Success.
    struct CounterLeaf {
        name: String,
        key: String,
    }
    impl BtLeaf for CounterLeaf {
        fn tick(&mut self, bb: &mut BlackBoard) -> BtStatus {
            let v = bb.get_int(&self.key).unwrap_or(0);
            bb.set_int(self.key.clone(), v + 1);
            BtStatus::Success
        }
        fn name(&self) -> &str {
            &self.name
        }
    }

    fn make_fixed(s: BtStatus, name: &str) -> Box<dyn BtLeaf> {
        Box::new(FixedLeaf {
            name: name.to_string(),
            status: s,
        })
    }

    #[test]
    fn bt_companion_rejected() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![make_fixed(BtStatus::Success, "x")];
        let err = BehaviorTree::new(
            ActorKind::Companion,
            BtNode::Leaf(LeafId(0)),
            leaves,
        )
        .unwrap_err();
        assert!(matches!(err, BehaviorTreeError::Companion(_)));
        assert_eq!(err.code(), "AIBEHAV0030");
    }

    #[test]
    fn bt_leaf_id_out_of_bounds() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![make_fixed(BtStatus::Success, "x")];
        let err = BehaviorTree::new(
            ActorKind::Npc,
            BtNode::Leaf(LeafId(99)),
            leaves,
        )
        .unwrap_err();
        assert!(matches!(err, BehaviorTreeError::LeafIdOutOfBounds { .. }));
        assert_eq!(err.code(), "AIBEHAV0031");
    }

    #[test]
    fn bt_single_leaf_success() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![make_fixed(BtStatus::Success, "ok")];
        let mut bt = BehaviorTree::new(ActorKind::Npc, BtNode::Leaf(LeafId(0)), leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Success);
    }

    #[test]
    fn bt_single_leaf_failure() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![make_fixed(BtStatus::Failure, "no")];
        let mut bt = BehaviorTree::new(ActorKind::Npc, BtNode::Leaf(LeafId(0)), leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Failure);
    }

    #[test]
    fn bt_sequence_all_success() {
        // Three success leaves ; sequence returns Success.
        let leaves: Vec<Box<dyn BtLeaf>> = vec![
            make_fixed(BtStatus::Success, "a"),
            make_fixed(BtStatus::Success, "b"),
            make_fixed(BtStatus::Success, "c"),
        ];
        let root = BtNode::Sequence(vec![
            BtNode::Leaf(LeafId(0)),
            BtNode::Leaf(LeafId(1)),
            BtNode::Leaf(LeafId(2)),
        ]);
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Success);
    }

    #[test]
    fn bt_sequence_first_failure_shortcircuits() {
        // a:Success, b:Failure, c:Success — sequence returns Failure ; c never ticks.
        let leaves: Vec<Box<dyn BtLeaf>> = vec![
            Box::new(CounterLeaf { name: "a".into(), key: "a-ticks".into() }),
            make_fixed(BtStatus::Failure, "b-fail"),
            Box::new(CounterLeaf { name: "c".into(), key: "c-ticks".into() }),
        ];
        let root = BtNode::Sequence(vec![
            BtNode::Leaf(LeafId(0)),
            BtNode::Leaf(LeafId(1)),
            BtNode::Leaf(LeafId(2)),
        ]);
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Failure);
        // a ticked once, c didn't tick.
        assert_eq!(bb.get_int("a-ticks").unwrap(), 1);
        assert!(bb.get_int("c-ticks").is_err()); // never set
    }

    #[test]
    fn bt_sequence_running_propagates() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![
            make_fixed(BtStatus::Success, "a"),
            make_fixed(BtStatus::Running, "b-run"),
            make_fixed(BtStatus::Success, "c"),
        ];
        let root = BtNode::Sequence(vec![
            BtNode::Leaf(LeafId(0)),
            BtNode::Leaf(LeafId(1)),
            BtNode::Leaf(LeafId(2)),
        ]);
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Running);
    }

    #[test]
    fn bt_selector_first_success_shortcircuits() {
        // a:Failure, b:Success, c:Success — selector returns Success ; c never ticks.
        let leaves: Vec<Box<dyn BtLeaf>> = vec![
            make_fixed(BtStatus::Failure, "a-fail"),
            Box::new(CounterLeaf { name: "b".into(), key: "b-ticks".into() }),
            Box::new(CounterLeaf { name: "c".into(), key: "c-ticks".into() }),
        ];
        let root = BtNode::Selector(vec![
            BtNode::Leaf(LeafId(0)),
            BtNode::Leaf(LeafId(1)),
            BtNode::Leaf(LeafId(2)),
        ]);
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Success);
        assert_eq!(bb.get_int("b-ticks").unwrap(), 1);
        assert!(bb.get_int("c-ticks").is_err());
    }

    #[test]
    fn bt_selector_all_failure() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![
            make_fixed(BtStatus::Failure, "a"),
            make_fixed(BtStatus::Failure, "b"),
        ];
        let root = BtNode::Selector(vec![BtNode::Leaf(LeafId(0)), BtNode::Leaf(LeafId(1))]);
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Failure);
    }

    #[test]
    fn bt_inverter_swaps_success_failure() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![make_fixed(BtStatus::Success, "x")];
        let root = BtNode::Decorator(DecoratorKind::Inverter, Box::new(BtNode::Leaf(LeafId(0))));
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Failure);
    }

    #[test]
    fn bt_inverter_running_pass_through() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![make_fixed(BtStatus::Running, "x")];
        let root = BtNode::Decorator(DecoratorKind::Inverter, Box::new(BtNode::Leaf(LeafId(0))));
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Running);
    }

    #[test]
    fn bt_always_success() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![make_fixed(BtStatus::Failure, "x")];
        let root = BtNode::Decorator(DecoratorKind::AlwaysSuccess, Box::new(BtNode::Leaf(LeafId(0))));
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Success);
    }

    #[test]
    fn bt_always_failure() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![make_fixed(BtStatus::Success, "x")];
        let root = BtNode::Decorator(DecoratorKind::AlwaysFailure, Box::new(BtNode::Leaf(LeafId(0))));
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Failure);
    }

    #[test]
    fn bt_repeater_n_runs_then_success() {
        // Repeater(3) wraps a counter ; after 3 ticks the decorator returns Success.
        let leaves: Vec<Box<dyn BtLeaf>> = vec![Box::new(CounterLeaf {
            name: "tick".into(),
            key: "n".into(),
        })];
        let root = BtNode::Decorator(
            DecoratorKind::Repeater(3),
            Box::new(BtNode::Leaf(LeafId(0))),
        );
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Running);
        assert_eq!(bt.tick(&mut bb), BtStatus::Running);
        assert_eq!(bt.tick(&mut bb), BtStatus::Success);
        // counter ran 3 times.
        assert_eq!(bb.get_int("n").unwrap(), 3);
    }

    #[test]
    fn bt_until_fail_repeats_on_success() {
        // First child Success ⇒ UntilFail returns Running ; switch to Failure ⇒ Success.
        struct ToggleLeaf {
            name: String,
            count: u32,
        }
        impl BtLeaf for ToggleLeaf {
            fn tick(&mut self, _bb: &mut BlackBoard) -> BtStatus {
                self.count += 1;
                if self.count <= 2 {
                    BtStatus::Success
                } else {
                    BtStatus::Failure
                }
            }
            fn name(&self) -> &str {
                &self.name
            }
        }
        let leaves: Vec<Box<dyn BtLeaf>> = vec![Box::new(ToggleLeaf {
            name: "toggle".into(),
            count: 0,
        })];
        let root = BtNode::Decorator(DecoratorKind::UntilFail, Box::new(BtNode::Leaf(LeafId(0))));
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Running);
        assert_eq!(bt.tick(&mut bb), BtStatus::Running);
        assert_eq!(bt.tick(&mut bb), BtStatus::Success);
    }

    #[test]
    fn bt_parallel_require_all_success() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![
            make_fixed(BtStatus::Success, "a"),
            make_fixed(BtStatus::Success, "b"),
        ];
        let root = BtNode::Parallel(
            ParallelPolicy::RequireAll,
            vec![BtNode::Leaf(LeafId(0)), BtNode::Leaf(LeafId(1))],
        );
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Success);
    }

    #[test]
    fn bt_parallel_require_all_one_failure_fails_composite() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![
            make_fixed(BtStatus::Success, "a"),
            make_fixed(BtStatus::Failure, "b"),
        ];
        let root = BtNode::Parallel(
            ParallelPolicy::RequireAll,
            vec![BtNode::Leaf(LeafId(0)), BtNode::Leaf(LeafId(1))],
        );
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Failure);
    }

    #[test]
    fn bt_parallel_require_one_first_success_wins() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![
            make_fixed(BtStatus::Failure, "a"),
            make_fixed(BtStatus::Success, "b"),
        ];
        let root = BtNode::Parallel(
            ParallelPolicy::RequireOne,
            vec![BtNode::Leaf(LeafId(0)), BtNode::Leaf(LeafId(1))],
        );
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Success);
    }

    #[test]
    fn bt_parallel_require_one_all_fail() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![
            make_fixed(BtStatus::Failure, "a"),
            make_fixed(BtStatus::Failure, "b"),
        ];
        let root = BtNode::Parallel(
            ParallelPolicy::RequireOne,
            vec![BtNode::Leaf(LeafId(0)), BtNode::Leaf(LeafId(1))],
        );
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Failure);
    }

    #[test]
    fn bt_parallel_tally_threshold() {
        // 3 children, threshold 2 → 2 successes ⇒ composite Success.
        let leaves: Vec<Box<dyn BtLeaf>> = vec![
            make_fixed(BtStatus::Success, "a"),
            make_fixed(BtStatus::Success, "b"),
            make_fixed(BtStatus::Failure, "c"),
        ];
        let root = BtNode::Parallel(
            ParallelPolicy::Tally { threshold: 2 },
            vec![
                BtNode::Leaf(LeafId(0)),
                BtNode::Leaf(LeafId(1)),
                BtNode::Leaf(LeafId(2)),
            ],
        );
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Success);
    }

    #[test]
    fn bt_parallel_tally_threshold_invalid() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![make_fixed(BtStatus::Success, "a")];
        let root = BtNode::Parallel(
            ParallelPolicy::Tally { threshold: 5 }, // > children count
            vec![BtNode::Leaf(LeafId(0))],
        );
        let err = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap_err();
        assert!(matches!(err, BehaviorTreeError::InvalidTallyThreshold { .. }));
        assert_eq!(err.code(), "AIBEHAV0033");
    }

    #[test]
    fn bt_parallel_tally_zero_threshold_invalid() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![make_fixed(BtStatus::Success, "a")];
        let root = BtNode::Parallel(
            ParallelPolicy::Tally { threshold: 0 },
            vec![BtNode::Leaf(LeafId(0))],
        );
        let err = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap_err();
        assert!(matches!(err, BehaviorTreeError::InvalidTallyThreshold { .. }));
    }

    #[test]
    fn bt_nested_sequence_in_selector() {
        // Selector{ Sequence{a-fail}, Sequence{b-success, c-success} } → Success
        let leaves: Vec<Box<dyn BtLeaf>> = vec![
            make_fixed(BtStatus::Failure, "a"),
            make_fixed(BtStatus::Success, "b"),
            make_fixed(BtStatus::Success, "c"),
        ];
        let root = BtNode::Selector(vec![
            BtNode::Sequence(vec![BtNode::Leaf(LeafId(0))]),
            BtNode::Sequence(vec![BtNode::Leaf(LeafId(1)), BtNode::Leaf(LeafId(2))]),
        ]);
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        assert_eq!(bt.tick(&mut bb), BtStatus::Success);
    }

    #[test]
    fn bt_tick_count_increments() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![make_fixed(BtStatus::Success, "x")];
        let mut bt = BehaviorTree::new(ActorKind::Npc, BtNode::Leaf(LeafId(0)), leaves).unwrap();
        let mut bb = BlackBoard::new();
        bt.tick(&mut bb);
        bt.tick(&mut bb);
        bt.tick(&mut bb);
        assert_eq!(bt.tick_count(), 3);
    }

    #[test]
    fn bt_leaf_count() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![
            make_fixed(BtStatus::Success, "a"),
            make_fixed(BtStatus::Success, "b"),
        ];
        let bt = BehaviorTree::new(
            ActorKind::Npc,
            BtNode::Sequence(vec![BtNode::Leaf(LeafId(0)), BtNode::Leaf(LeafId(1))]),
            leaves,
        )
        .unwrap();
        assert_eq!(bt.leaf_count(), 2);
    }

    #[test]
    fn bt_status_distinct() {
        assert_ne!(BtStatus::Success, BtStatus::Failure);
        assert_ne!(BtStatus::Success, BtStatus::Running);
        assert_ne!(BtStatus::Failure, BtStatus::Running);
    }

    #[test]
    fn bt_repeater_reset() {
        let leaves: Vec<Box<dyn BtLeaf>> = vec![Box::new(CounterLeaf {
            name: "tick".into(),
            key: "n".into(),
        })];
        let root = BtNode::Decorator(
            DecoratorKind::Repeater(2),
            Box::new(BtNode::Leaf(LeafId(0))),
        );
        let mut bt = BehaviorTree::new(ActorKind::Npc, root, leaves).unwrap();
        let mut bb = BlackBoard::new();
        bt.tick(&mut bb);
        bt.tick(&mut bb);
        assert_eq!(bt.tick(&mut bb), BtStatus::Success);
        // After Success, reset_repeaters should restore the n=2 budget.
        bt.reset_repeaters();
        assert_eq!(bt.tick(&mut bb), BtStatus::Running);
        assert_eq!(bt.tick(&mut bb), BtStatus::Success);
    }
}
