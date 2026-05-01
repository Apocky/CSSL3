// § grimoire.rs — 6-slot equipped-spells + known-nodes set
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § GRIMOIRE-MODEL :
//   spell-slots  : 6 active-equipped (limit ⇒ ¬ bloat ; tactical-pick)
//   nodes-collected : known-set ; drop-from-bosses · alchemy · NPC-vendor
//   spells-saved : Vec<SpellGraph> (player-authored · auditable)
//   share-spell-template : DEFERRED (POD-4 multiplayer territory)
// ════════════════════════════════════════════════════════════════════

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::graph::SpellGraph;
use crate::node::NodeKind;

/// Number of equipped-spell slots (GDD load-bearing constant).
pub const SPELL_SLOTS: usize = 6;

/// Grimoire errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrimoireErr {
    /// `equip(slot, _)` with `slot >= SPELL_SLOTS`.
    SlotOutOfRange { slot: u8, max: u8 },
    /// Equipped-spell failed `SpellGraph::validate()`.
    InvalidSpell,
}

/// Per-character grimoire : 6 equipped spells + collected node-kinds.
///
/// `spells_saved` (player-authored named library) is intentionally omitted
/// from this MVP scope ; equipped slots ARE the saved-spell list for now.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Grimoire {
    pub equipped_spells: [Option<SpellGraph>; SPELL_SLOTS],
    pub known_nodes: BTreeSet<NodeKind>,
}

impl Grimoire {
    /// New empty grimoire.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Equip a spell into a slot. Validates the spell-graph first.
    pub fn equip(&mut self, slot: u8, spell: SpellGraph) -> Result<(), GrimoireErr> {
        if usize::from(slot) >= SPELL_SLOTS {
            return Err(GrimoireErr::SlotOutOfRange {
                slot,
                max: SPELL_SLOTS as u8 - 1,
            });
        }
        spell.validate().map_err(|_| GrimoireErr::InvalidSpell)?;
        self.equipped_spells[usize::from(slot)] = Some(spell);
        Ok(())
    }

    /// Clear a slot.
    pub fn unequip(&mut self, slot: u8) -> Result<(), GrimoireErr> {
        if usize::from(slot) >= SPELL_SLOTS {
            return Err(GrimoireErr::SlotOutOfRange {
                slot,
                max: SPELL_SLOTS as u8 - 1,
            });
        }
        self.equipped_spells[usize::from(slot)] = None;
        Ok(())
    }

    /// Add a node-kind to the collected-set.
    pub fn know_node(&mut self, kind: NodeKind) {
        self.known_nodes.insert(kind);
    }

    /// Count of currently-equipped spells.
    #[must_use]
    pub fn equipped_count(&self) -> usize {
        self.equipped_spells.iter().filter(|s| s.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::Element;
    use crate::node::{ShapeKind, SpellNode, TriggerKind};

    fn minimal_spell() -> SpellGraph {
        let mut g = SpellGraph::new();
        let s = g.add_node(SpellNode::Source(Element::Fire));
        let sh = g.add_node(SpellNode::Shape(ShapeKind::Ray));
        let tr = g.add_node(SpellNode::Trigger(TriggerKind::OnCast));
        g.add_edge(s, sh);
        g.add_edge(sh, tr);
        g
    }

    #[test]
    fn slot_out_of_range_rejected() {
        let mut g = Grimoire::new();
        let r = g.equip(99, minimal_spell());
        assert!(matches!(r, Err(GrimoireErr::SlotOutOfRange { .. })));
    }

    #[test]
    fn equip_round_trip() {
        let mut g = Grimoire::new();
        assert!(g.equip(0, minimal_spell()).is_ok());
        assert_eq!(g.equipped_count(), 1);
        assert!(g.unequip(0).is_ok());
        assert_eq!(g.equipped_count(), 0);
    }

    #[test]
    fn invalid_spell_rejected() {
        let mut g = Grimoire::new();
        let bad = SpellGraph::new(); // no Source
        assert!(matches!(g.equip(0, bad), Err(GrimoireErr::InvalidSpell)));
    }
}
