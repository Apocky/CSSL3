// § recipe_graph : DAG container + 40-recipe default table
// ══════════════════════════════════════════════════════════════════
//! Recipe-graph. `BTreeMap<u32, RecipeNode>` for deterministic iteration.
//!
//! Per GDD § DAG-PROPERTY : recipes reference Materials only ; bipartite
//! recipe→material graph is acyclic by construction. `is_acyclic` reduces
//! to a structural well-formedness check.
//!
//! 40 default recipes seeded by [`default_recipe_graph`] (R01..R40).

use crate::material::Material;
use crate::recipe::{ItemClass, RecipeNode, Tool};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// § RecipeGraph : ordered map of recipes by id (BTreeMap = deterministic serde).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RecipeGraph {
    pub nodes: BTreeMap<u32, RecipeNode>,
}

impl RecipeGraph {
    #[must_use]
    pub fn new() -> Self {
        Self { nodes: BTreeMap::new() }
    }
    pub fn insert(&mut self, node: RecipeNode) {
        self.nodes.insert(node.id, node);
    }
    #[must_use]
    pub fn get(&self, id: u32) -> Option<&RecipeNode> { self.nodes.get(&id) }
    #[must_use]
    pub fn len(&self) -> usize { self.nodes.len() }
    #[must_use]
    pub fn is_empty(&self) -> bool { self.nodes.is_empty() }

    /// § is_acyclic : structural DAG check.
    /// Per GDD § DAG-PROPERTY (acyclic-by-construction). Verifies :
    /// - ≥ 1 ingredient per recipe · count ≥ 1 · output_tier ∈ ⟦1,6⟧ · skill_min ≤ 100.
    #[must_use]
    pub fn is_acyclic(&self) -> bool {
        for n in self.nodes.values() {
            if n.ingredients.is_empty() { return false; }
            if !(1..=6).contains(&n.output_tier) { return false; }
            if n.skill_min > 100 { return false; }
            for &(_m, count) in &n.ingredients {
                if count == 0 { return false; }
            }
        }
        true
    }

    /// § available_for_skill : recipes the caller can attempt at the given skill-level.
    #[must_use]
    pub fn available_for_skill(&self, skill: u8) -> Vec<&RecipeNode> {
        self.nodes.values().filter(|n| n.available_for(skill)).collect()
    }

    /// § by_class : filter by item-class.
    #[must_use]
    pub fn by_class(&self, class: ItemClass) -> Vec<&RecipeNode> {
        self.nodes.values().filter(|n| n.output_class == class).collect()
    }
}

// ══════════════════════════════════════════════════════════════════
// § default_recipe_graph : 40-recipe seed-table per GDD § RECIPE-GRAPH
// ══════════════════════════════════════════════════════════════════

// § Compact entry tuple : (id, class, output_tier, tool, skill_min, &[(mat, count)]).
type Entry = (u32, ItemClass, u8, Tool, u8, &'static [(Material, u8)]);

/// § default_recipe_graph : 40-recipe seed (R01..R40 per GDD).
///
/// Distribution :
/// - R01..R04 BASES (4 ; one per class)
/// - R05..R14 WEAPONS (10 : sword-T1..T5, bow-T1, bow-T3, staff-T2, staff-T4, voidblade-T6)
/// - R15..R24 ARMORS  (10 : plate-T1..T5, robe-T1, robe-T3, robe-T4, cloak-T2, voidshroud-T6)
/// - R25..R29 JEWELRY (5  : ring-T1, ring-T3, amulet-T4, soulring-T5, voidcrown-T6)
/// - R30..R40 CONSUMABLES (11 alchemy potions)
#[must_use]
pub fn default_recipe_graph() -> RecipeGraph {
    use ItemClass::*;
    use Material::*;
    use Tool::{AlchemyTable as AT, Phaseloom as PL, Soulhammer as SH, Voidsmelter as VS};

    let entries: &[Entry] = &[
        // ── BASES (4) ──
        (1, Weapon,     1, SH, 0,   &[(Iron, 2), (Oak, 1)]),
        (2, Armor,      1, SH, 0,   &[(Iron, 3), (Hide, 2)]),
        (3, Jewelry,    1, SH, 0,   &[(Silver, 1), (Quartz, 1)]),
        (4, Consumable, 1, AT, 0,   &[(Linen, 1), (Saltpeter, 1)]),
        // ── WEAPONS (10) ──
        (5, Weapon, 1, VS, 0,   &[(Iron, 1)]),
        (6, Weapon, 2, VS, 20,  &[(Silver, 2), (Yew, 1)]),
        (7, Weapon, 3, VS, 40,  &[(Mithril, 2), (Ironwood, 1)]),
        (8, Weapon, 4, VS, 60,  &[(Adamant, 2), (Soulwood, 1)]),
        (9, Weapon, 5, VS, 80,  &[(Voidsteel, 2), (Ghostwood, 1)]),
        (10, Weapon, 1, SH, 0,  &[(Oak, 1), (Spidersilk, 1)]),
        (11, Weapon, 3, SH, 40, &[(Ironwood, 2), (Spidersilk, 2)]),
        (12, Weapon, 2, SH, 20, &[(Yew, 2), (Quartz, 1)]),
        (13, Weapon, 4, SH, 60, &[(Soulwood, 2), (Voidcrystal, 1)]),
        (14, Weapon, 6, VS, 100,&[(Soulalloy, 2), (Catalystgem, 1), (Phaseether, 1)]),
        // ── ARMORS (10) ──
        (15, Armor, 1, SH, 0,   &[(Iron, 2)]),
        (16, Armor, 2, SH, 20,  &[(Silver, 2), (Hide, 2)]),
        (17, Armor, 3, SH, 40,  &[(Mithril, 3), (Drakeskin, 1)]),
        (18, Armor, 4, SH, 60,  &[(Adamant, 3), (Wyvernhide, 1)]),
        (19, Armor, 5, SH, 80,  &[(Voidsteel, 3), (Voidleather, 1)]),
        (20, Armor, 1, PL, 0,   &[(Linen, 3)]),
        (21, Armor, 3, PL, 40,  &[(Spidersilk, 3), (Emerald, 1)]),
        (22, Armor, 4, PL, 60,  &[(Phaseweave, 3), (Voidcrystal, 1)]),
        (23, Armor, 2, PL, 20,  &[(Hide, 3), (Silk, 2)]),
        (24, Armor, 6, PL, 100, &[(Voidweave, 2), (Soulgem, 1), (Phaseether, 1)]),
        // ── JEWELRY (5) ──
        (25, Jewelry, 1, SH, 0,   &[(Silver, 1), (Quartz, 1)]),
        (26, Jewelry, 3, SH, 40,  &[(Mithril, 1), (Sapphire, 1)]),
        (27, Jewelry, 4, SH, 60,  &[(Adamant, 1), (Voidcrystal, 1)]),
        (28, Jewelry, 5, SH, 80,  &[(Soulalloy, 1), (Soulgem, 1)]),
        (29, Jewelry, 6, SH, 100, &[(Soulalloy, 2), (Catalystgem, 2)]),
        // ── CONSUMABLES (11 ; alchemy potions R30..R40) ──
        (30, Consumable, 1, AT, 0,   &[(Saltpeter, 1), (Quartz, 1)]),
        (31, Consumable, 2, AT, 20,  &[(Alkahest, 1), (Emerald, 1)]),
        (32, Consumable, 1, AT, 0,   &[(Saltpeter, 1), (Sapphire, 1)]),
        (33, Consumable, 2, AT, 0,   &[(Alkahest, 1), (Yew, 1)]),
        (34, Consumable, 3, AT, 40,  &[(Mithrilshade, 1), (Drakeskin, 1)]),
        (35, Consumable, 3, AT, 40,  &[(Voidessence, 1), (Ruby, 1)]),
        (36, Consumable, 2, AT, 20,  &[(Alkahest, 1), (Emerald, 1)]),
        (37, Consumable, 4, AT, 60,  &[(Soulflux, 1), (Soulgem, 1)]),
        (38, Consumable, 5, AT, 80,  &[(Phaseether, 1), (Voidcrystal, 1)]),
        (39, Consumable, 5, AT, 80,  &[(Soulflux, 1), (Soulgem, 1), (Voidessence, 1)]),
        (40, Consumable, 6, AT, 100, &[(Catalystgem, 1), (Phaseether, 1), (Soulflux, 1), (Voidessence, 1)]),
    ];

    let mut g = RecipeGraph::new();
    for &(id, class, tier, tool, skill, mats) in entries {
        g.insert(RecipeNode::new(id, class, tier, mats.to_vec(), Some(tool), skill));
    }
    g
}
