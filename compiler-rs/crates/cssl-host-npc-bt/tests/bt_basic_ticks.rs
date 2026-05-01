// § tests/bt_basic_ticks.rs — BT composite-shape ticks
// ════════════════════════════════════════════════════════════════════
// § I> 4 tests covering Selector, Sequence, Action+audit, Decorator coverage
// ════════════════════════════════════════════════════════════════════

use cssl_host_npc_bt::actions::ActionKind;
use cssl_host_npc_bt::audit::{NoopAuditSink, RecordingAuditSink, kind};
use cssl_host_npc_bt::bt::{BtNode, BtStatus, NpcWorldRef, tick};
use cssl_host_npc_bt::conditions::ConditionKind;
use cssl_host_npc_bt::decorators::DecoratorKind;

struct W {
    hp: f32,
}

impl NpcWorldRef for W {
    fn current_zone(&self) -> u32 {
        1
    }
    fn hp_ratio(&self) -> f32 {
        self.hp
    }
    fn mana_ratio(&self) -> f32 {
        0.5
    }
    fn nearby_ally(&self) -> bool {
        true
    }
    fn target_is_hostile(&self, _t: u64) -> bool {
        true
    }
    fn game_hour_block(&self) -> u8 {
        12
    }
    fn dialogue_open(&self) -> bool {
        false
    }
    fn resource_count(&self, _k: u32) -> u32 {
        5
    }
    fn is_idle(&self) -> bool {
        false
    }
}

#[test]
fn selector_picks_first_success() {
    let n = BtNode::Selector(vec![
        BtNode::Condition(ConditionKind::Idle),    // false → fail
        BtNode::Condition(ConditionKind::LowHP),   // hp=0.9 → fail
        BtNode::Condition(ConditionKind::NearbyAlly), // true
    ]);
    let w = W { hp: 0.9 };
    assert_eq!(tick(&n, &w, &NoopAuditSink), BtStatus::Success);
}

#[test]
fn sequence_aborts_on_failure() {
    let n = BtNode::Sequence(vec![
        BtNode::Condition(ConditionKind::NearbyAlly), // true
        BtNode::Condition(ConditionKind::LowHP),      // hp=0.9 → fail
        BtNode::Action(ActionKind::Rest),             // never reached
    ]);
    let w = W { hp: 0.9 };
    assert_eq!(tick(&n, &w, &NoopAuditSink), BtStatus::Failure);
}

#[test]
fn action_emits_audit_event() {
    let rec = RecordingAuditSink::new();
    let n = BtNode::Action(ActionKind::Eat);
    let w = W { hp: 0.9 };
    let res = tick(&n, &w, &rec);
    assert_eq!(res, BtStatus::Success);
    assert!(rec.contains_kind(kind::BT_TICK));
    assert_eq!(rec.count_kind(kind::BT_TICK), 1);
}

#[test]
fn tree_covers_canonical_27_kinds() {
    // Build a tree containing every canonical leaf-tag : 9 conditions + 14 actions +
    // 4 decorators + 2 composites (Selector, Sequence) = 29 distinct tag-strings.
    let conds: Vec<BtNode> = vec![
        BtNode::Condition(ConditionKind::InZone(0)),
        BtNode::Condition(ConditionKind::ContainsRes { kind: 0, count: 0 }),
        BtNode::Condition(ConditionKind::IsHostile(0)),
        BtNode::Condition(ConditionKind::Idle),
        BtNode::Condition(ConditionKind::LowHP),
        BtNode::Condition(ConditionKind::LowMana),
        BtNode::Condition(ConditionKind::NearbyAlly),
        BtNode::Condition(ConditionKind::TimeIsX { block: 0 }),
        BtNode::Condition(ConditionKind::DialogueOpen),
    ];
    let actions: Vec<BtNode> = vec![
        BtNode::Action(ActionKind::MoveTo([0.0; 3])),
        BtNode::Action(ActionKind::Attack(0)),
        BtNode::Action(ActionKind::Defend),
        BtNode::Action(ActionKind::Talk(0)),
        BtNode::Action(ActionKind::Trade {
            target: 0,
            offer: 0,
        }),
        BtNode::Action(ActionKind::Craft(0)),
        BtNode::Action(ActionKind::Rest),
        BtNode::Action(ActionKind::Patrol(0)),
        BtNode::Action(ActionKind::Flee(0)),
        BtNode::Action(ActionKind::Cast {
            spell: 0,
            target: 0,
        }),
        BtNode::Action(ActionKind::Pray),
        BtNode::Action(ActionKind::Buy {
            item: 0,
            max_price: 0,
        }),
        BtNode::Action(ActionKind::Sell {
            item: 0,
            min_price: 0,
        }),
        BtNode::Action(ActionKind::Eat),
    ];
    let decorators: Vec<BtNode> = vec![
        BtNode::Decorator(
            DecoratorKind::Repeat { n: 1 },
            Box::new(BtNode::Action(ActionKind::Eat)),
        ),
        BtNode::Decorator(
            DecoratorKind::Invert,
            Box::new(BtNode::Condition(ConditionKind::Idle)),
        ),
        BtNode::Decorator(
            DecoratorKind::Limiter { max_per_tick: 1 },
            Box::new(BtNode::Action(ActionKind::Eat)),
        ),
        BtNode::Decorator(
            DecoratorKind::Cooldown { cooldown_ms: 1 },
            Box::new(BtNode::Action(ActionKind::Eat)),
        ),
    ];

    let tree = BtNode::Selector(vec![
        BtNode::Sequence(conds),
        BtNode::Sequence(actions),
        BtNode::Sequence(decorators),
    ]);

    let mut tags = Vec::new();
    tree.collect_tags(&mut tags);
    tags.sort_unstable();
    tags.dedup();
    // Selector + Sequence + 9 conds + 14 actions + 4 decorators = 29 distinct
    assert!(tags.len() >= 27, "expected ≥27 canonical tags, got {}", tags.len());
}
