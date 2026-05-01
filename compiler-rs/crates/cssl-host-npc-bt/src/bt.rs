// § bt.rs — L1 layer ; behavior-tree DFS-left-right tick
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § ARCHITECTURE-LAYERS L1 + § BEHAVIOR-TREE-NODES
// § I> 27-node canonical-set : 9 conditions + 14 actions + 4 decorators
// § I>                       + 5 composites (Selector, Sequence, Action,
// § I>                                       Condition, Decorator wrapper)
// § I> tick → BtStatus { Success, Failure, Running }
// § I> determinism : pure-fn over NpcWorldRef ; no panics
// ════════════════════════════════════════════════════════════════════

use crate::actions::ActionKind;
use crate::audit::{AuditEvent, AuditSink, kind};
use crate::conditions::ConditionKind;
use crate::decorators::DecoratorKind;
use serde::{Deserialize, Serialize};

/// Result of ticking a BT node ; standard 3-state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BtStatus {
    /// Sub-tree completed successfully.
    Success,
    /// Sub-tree failed.
    Failure,
    /// Sub-tree still in progress (multi-tick).
    Running,
}

/// One node in a behavior-tree.
///
/// § I> 5 composite-shapes wrap the 27 leaf/decorator-kinds :
///   Selector (OR) · Sequence (AND) · Action (effect) · Condition (predicate) ·
///   Decorator (wrap-child).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BtNode {
    /// Selector — try children left→right ; first Success wins. Failure if all fail.
    Selector(Vec<BtNode>),
    /// Sequence — run children left→right ; abort on Failure ; Success if all succeed.
    Sequence(Vec<BtNode>),
    /// Leaf action — emit side-effect.
    Action(ActionKind),
    /// Leaf predicate — pure-read.
    Condition(ConditionKind),
    /// Wrap one child ; transform its BtStatus per `DecoratorKind`.
    Decorator(DecoratorKind, Box<BtNode>),
}

impl BtNode {
    /// Recursive node-count (for spec-coverage tests).
    #[must_use]
    pub fn count(&self) -> usize {
        match self {
            BtNode::Selector(c) | BtNode::Sequence(c) => {
                1 + c.iter().map(BtNode::count).sum::<usize>()
            }
            BtNode::Action(_) | BtNode::Condition(_) => 1,
            BtNode::Decorator(_, child) => 1 + child.count(),
        }
    }

    /// Walk the tree depth-first, collecting unique tag-strings of every leaf
    /// + decorator. Used for spec-coverage tests (≥27 canonical kinds).
    pub fn collect_tags(&self, out: &mut Vec<&'static str>) {
        match self {
            BtNode::Selector(c) | BtNode::Sequence(c) => {
                out.push(if matches!(self, BtNode::Selector(_)) {
                    "Selector"
                } else {
                    "Sequence"
                });
                for ch in c {
                    ch.collect_tags(out);
                }
            }
            BtNode::Action(a) => out.push(a.tag()),
            BtNode::Condition(c) => out.push(c.tag()),
            BtNode::Decorator(d, child) => {
                out.push(d.tag());
                child.collect_tags(out);
            }
        }
    }
}

/// Read-only world-view for BT evaluation.
///
/// § I> Trait so the host can supply its real-world without a circular dep.
/// § I> All getters are pure-read ; impls **must not** read Sensitive<*> data.
pub trait NpcWorldRef {
    /// Current zone-id of the NPC.
    fn current_zone(&self) -> u32;
    /// HP ratio ∈ [0, 1].
    fn hp_ratio(&self) -> f32;
    /// Mana ratio ∈ [0, 1].
    fn mana_ratio(&self) -> f32;
    /// True iff at least one allied NPC sensed.
    fn nearby_ally(&self) -> bool;
    /// True iff target-handle is sensed AND classified hostile.
    fn target_is_hostile(&self, target: u64) -> bool;
    /// Current game-hour-block (0..24).
    fn game_hour_block(&self) -> u8;
    /// True iff a dialogue is open with the player.
    fn dialogue_open(&self) -> bool;
    /// Inventory count of resource-kind `k`.
    fn resource_count(&self, kind: u32) -> u32;
    /// True iff BT-cursor is currently parked at the Idle leaf.
    fn is_idle(&self) -> bool;
}

/// Tick a BT node against the world ; return resulting BtStatus.
///
/// § I> sink receives `npc.bt_tick` for every Action ; cap-bleed → SIG-cap-bleed
/// § I> NEVER panics ; pure-fn except for `sink.emit`.
pub fn tick<W: NpcWorldRef>(node: &BtNode, world: &W, sink: &dyn AuditSink) -> BtStatus {
    match node {
        BtNode::Selector(children) => {
            for ch in children {
                match tick(ch, world, sink) {
                    BtStatus::Success => return BtStatus::Success,
                    BtStatus::Running => return BtStatus::Running,
                    BtStatus::Failure => continue,
                }
            }
            BtStatus::Failure
        }
        BtNode::Sequence(children) => {
            for ch in children {
                match tick(ch, world, sink) {
                    BtStatus::Failure => return BtStatus::Failure,
                    BtStatus::Running => return BtStatus::Running,
                    BtStatus::Success => continue,
                }
            }
            BtStatus::Success
        }
        BtNode::Action(a) => {
            sink.emit(
                AuditEvent::bare(kind::BT_TICK)
                    .with("action", a.tag())
                    .with("cap", a.cap_required()),
            );
            BtStatus::Success
        }
        BtNode::Condition(c) => {
            let pass = match c {
                ConditionKind::InZone(z) => world.current_zone() == *z,
                ConditionKind::ContainsRes { kind: k, count } => world.resource_count(*k) >= *count,
                ConditionKind::IsHostile(t) => world.target_is_hostile(*t),
                ConditionKind::Idle => world.is_idle(),
                ConditionKind::LowHP => world.hp_ratio() < 0.30,
                ConditionKind::LowMana => world.mana_ratio() < 0.30,
                ConditionKind::NearbyAlly => world.nearby_ally(),
                ConditionKind::TimeIsX { block } => world.game_hour_block() == *block,
                ConditionKind::DialogueOpen => world.dialogue_open(),
            };
            if pass {
                BtStatus::Success
            } else {
                BtStatus::Failure
            }
        }
        BtNode::Decorator(d, child) => match d {
            DecoratorKind::Invert => match tick(child, world, sink) {
                BtStatus::Success => BtStatus::Failure,
                BtStatus::Failure => BtStatus::Success,
                BtStatus::Running => BtStatus::Running,
            },
            DecoratorKind::Repeat { n } => {
                let mut last = BtStatus::Success;
                for _ in 0..*n {
                    last = tick(child, world, sink);
                    if matches!(last, BtStatus::Failure | BtStatus::Running) {
                        return last;
                    }
                }
                last
            }
            DecoratorKind::Limiter { max_per_tick } => {
                if *max_per_tick == 0 {
                    BtStatus::Failure
                } else {
                    tick(child, world, sink)
                }
            }
            DecoratorKind::Cooldown { cooldown_ms: _ } => {
                // Cooldown is host-stateful ; default-impl evaluates child.
                // Real cooldown-state lives in host's per-NPC scratchpad.
                tick(child, world, sink)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::NoopAuditSink;

    struct StubWorld;
    impl NpcWorldRef for StubWorld {
        fn current_zone(&self) -> u32 {
            1
        }
        fn hp_ratio(&self) -> f32 {
            0.5
        }
        fn mana_ratio(&self) -> f32 {
            0.5
        }
        fn nearby_ally(&self) -> bool {
            false
        }
        fn target_is_hostile(&self, _t: u64) -> bool {
            false
        }
        fn game_hour_block(&self) -> u8 {
            12
        }
        fn dialogue_open(&self) -> bool {
            false
        }
        fn resource_count(&self, _k: u32) -> u32 {
            0
        }
        fn is_idle(&self) -> bool {
            true
        }
    }

    #[test]
    fn idle_condition_succeeds() {
        let n = BtNode::Condition(ConditionKind::Idle);
        assert_eq!(tick(&n, &StubWorld, &NoopAuditSink), BtStatus::Success);
    }

    #[test]
    fn invert_flips_status() {
        let n = BtNode::Decorator(
            DecoratorKind::Invert,
            Box::new(BtNode::Condition(ConditionKind::Idle)),
        );
        assert_eq!(tick(&n, &StubWorld, &NoopAuditSink), BtStatus::Failure);
    }
}
