// § conditions.rs — BT predicate-only nodes ; ¬ side-effect
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § BEHAVIOR-TREE-NODES § CONDITIONS (≥9 canonical)
// § I> evaluated against NpcWorldRef ; pure-read ; no audit-emit
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Predicate-condition variants for BT Condition nodes.
///
/// § I> ≥ 9 per GDD : InZone · ContainsRes · IsHostile · Idle · LowHP ·
///   LowMana · NearbyAlly · TimeIsX · DialogueOpen
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConditionKind {
    /// NPC currently inside zone-id `z`.
    InZone(u32),
    /// NPC inventory contains `n` of resource-kind `k`.
    ContainsRes { kind: u32, count: u32 },
    /// Sensed-target with handle `t` is hostile.
    IsHostile(u64),
    /// NPC's current BT-cursor is at the Idle leaf.
    Idle,
    /// HP ratio below 30%.
    LowHP,
    /// Mana ratio below 30%.
    LowMana,
    /// At least one allied NPC sensed within zone.
    NearbyAlly,
    /// Current game-hour-block matches `block` (0..24 hour-bucket).
    TimeIsX { block: u8 },
    /// A dialogue scene is currently open with player.
    DialogueOpen,
}

impl ConditionKind {
    /// Stable string-id for audit-attribs + serde-debug.
    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            ConditionKind::InZone(_) => "InZone",
            ConditionKind::ContainsRes { .. } => "ContainsRes",
            ConditionKind::IsHostile(_) => "IsHostile",
            ConditionKind::Idle => "Idle",
            ConditionKind::LowHP => "LowHP",
            ConditionKind::LowMana => "LowMana",
            ConditionKind::NearbyAlly => "NearbyAlly",
            ConditionKind::TimeIsX { .. } => "TimeIsX",
            ConditionKind::DialogueOpen => "DialogueOpen",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_nine_tags_distinct() {
        let kinds = [
            ConditionKind::InZone(1),
            ConditionKind::ContainsRes { kind: 0, count: 1 },
            ConditionKind::IsHostile(0),
            ConditionKind::Idle,
            ConditionKind::LowHP,
            ConditionKind::LowMana,
            ConditionKind::NearbyAlly,
            ConditionKind::TimeIsX { block: 0 },
            ConditionKind::DialogueOpen,
        ];
        let tags: Vec<_> = kinds.iter().map(ConditionKind::tag).collect();
        // ≥ 9 distinct
        let mut sorted = tags.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 9);
    }

    #[test]
    fn condition_serde_roundtrip() {
        let c = ConditionKind::ContainsRes { kind: 7, count: 3 };
        let j = serde_json::to_string(&c).expect("ser");
        let back: ConditionKind = serde_json::from_str(&j).expect("de");
        assert_eq!(c, back);
    }
}
