// § actions.rs — BT side-effect-emitting nodes ; cap-checked
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § BEHAVIOR-TREE-NODES § ACTIONS (≥14 canonical)
// § I> emit one audit-event per action-tick ; cap-bleed → SIG-cap-bleed
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Side-effect-emitting BT actions.
///
/// § I> ≥ 14 per GDD : MoveTo · Attack · Defend · Talk · Trade · Craft ·
///   Rest · Patrol · Flee · Cast · Pray · Buy · Sell · Eat
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ActionKind {
    /// Move toward world-position [x, y, z]. cap=NPC_CAP_MOVE.
    MoveTo([f32; 3]),
    /// Attack target-handle. cap=NPC_CAP_COMBAT.
    Attack(u64),
    /// Block-stance. cap=NPC_CAP_COMBAT.
    Defend,
    /// Open dialogue scene with target. GM-handoff for prose.
    Talk(u64),
    /// Trade with target ; offered-item. cap=NPC_CAP_TRADE.
    Trade { target: u64, offer: u32 },
    /// Craft recipe-id `r`. cap=NPC_CAP_CRAFT.
    Craft(u32),
    /// Rest — regenerate HP/Mana.
    Rest,
    /// Patrol along route-id `r` ; waypoint loop.
    Patrol(u32),
    /// Flee from threat-handle ; speed-burst opposite-direction.
    Flee(u64),
    /// Cast spell-id `s` at target-handle. magic-system tick-in.
    Cast { spell: u32, target: u64 },
    /// Pray — flavor + mood-mod.
    Pray,
    /// Buy item from market. cap=NPC_CAP_TRADE.
    Buy { item: u32, max_price: u32 },
    /// Sell item to market. cap=NPC_CAP_TRADE.
    Sell { item: u32, min_price: u32 },
    /// Eat — hunger-decrement.
    Eat,
}

impl ActionKind {
    /// Stable tag-string for audit-attribs.
    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            ActionKind::MoveTo(_) => "MoveTo",
            ActionKind::Attack(_) => "Attack",
            ActionKind::Defend => "Defend",
            ActionKind::Talk(_) => "Talk",
            ActionKind::Trade { .. } => "Trade",
            ActionKind::Craft(_) => "Craft",
            ActionKind::Rest => "Rest",
            ActionKind::Patrol(_) => "Patrol",
            ActionKind::Flee(_) => "Flee",
            ActionKind::Cast { .. } => "Cast",
            ActionKind::Pray => "Pray",
            ActionKind::Buy { .. } => "Buy",
            ActionKind::Sell { .. } => "Sell",
            ActionKind::Eat => "Eat",
        }
    }

    /// Cap-name required to perform this action ; `"NONE"` for cap-free actions.
    #[must_use]
    pub fn cap_required(&self) -> &'static str {
        match self {
            ActionKind::MoveTo(_) | ActionKind::Patrol(_) | ActionKind::Flee(_) => "NPC_CAP_MOVE",
            ActionKind::Attack(_) | ActionKind::Defend | ActionKind::Cast { .. } => {
                "NPC_CAP_COMBAT"
            }
            ActionKind::Trade { .. } | ActionKind::Buy { .. } | ActionKind::Sell { .. } => {
                "NPC_CAP_TRADE"
            }
            ActionKind::Craft(_) => "NPC_CAP_CRAFT",
            ActionKind::Talk(_) | ActionKind::Rest | ActionKind::Pray | ActionKind::Eat => "NONE",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fourteen_distinct_tags() {
        let actions = [
            ActionKind::MoveTo([0.0; 3]),
            ActionKind::Attack(0),
            ActionKind::Defend,
            ActionKind::Talk(0),
            ActionKind::Trade {
                target: 0,
                offer: 0,
            },
            ActionKind::Craft(0),
            ActionKind::Rest,
            ActionKind::Patrol(0),
            ActionKind::Flee(0),
            ActionKind::Cast {
                spell: 0,
                target: 0,
            },
            ActionKind::Pray,
            ActionKind::Buy {
                item: 0,
                max_price: 0,
            },
            ActionKind::Sell {
                item: 0,
                min_price: 0,
            },
            ActionKind::Eat,
        ];
        let mut tags: Vec<_> = actions.iter().map(ActionKind::tag).collect();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), 14);
    }

    #[test]
    fn cap_mapping_correct() {
        assert_eq!(ActionKind::MoveTo([0.0; 3]).cap_required(), "NPC_CAP_MOVE");
        assert_eq!(ActionKind::Attack(0).cap_required(), "NPC_CAP_COMBAT");
        assert_eq!(
            ActionKind::Trade {
                target: 0,
                offer: 0
            }
            .cap_required(),
            "NPC_CAP_TRADE"
        );
        assert_eq!(ActionKind::Craft(0).cap_required(), "NPC_CAP_CRAFT");
        assert_eq!(ActionKind::Eat.cap_required(), "NONE");
    }
}
