// § recipe : RecipeNode + ItemClass + Tool
// ══════════════════════════════════════════════════════════════════
//! Single-recipe data-model. Recipes form a DAG via [`crate::recipe_graph`].
//!
//! Per `GDDs/CRAFT_DECONSTRUCT_ALCHEMY.csl § RECIPE-GRAPH` :
//! - Each recipe has a tier-output, an item-class, an ingredient list (with counts),
//!   an optional Tool affordance, and a skill-min gate.
//! - Tier-N recipes only reference tier-(<N) outputs (DAG-by-construction).
//! - skill_min gates per the GDD's "⟦skill≥X⟧" annotations.

use crate::material::Material;
use serde::{Deserialize, Serialize};

/// § ItemClass : output category for crafted items (per GDD § ITEM CLASSES).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ItemClass {
    Weapon,
    Armor,
    Jewelry,
    Consumable,
    Trinket,
}

/// § Tool : workshop-tool affordance required to evaluate a recipe.
///
/// Per GDD § DECONSTRUCTION + § ALCHEMY :
/// - Voidsmelter → metals + gems (deconstruct base 0.45)
/// - Soulhammer → jewelry + woods (deconstruct base 0.50)
/// - Phaseloom → cloths + leathers (deconstruct base 0.55)
/// - AlchemyTable → potion-brew + transmute
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Tool {
    Voidsmelter,
    Soulhammer,
    Phaseloom,
    AlchemyTable,
}

/// § RecipeNode : single craft-graph vertex.
///
/// - `id` — unique u32 identifier (R01..R40 in GDD ; encoded as 1..40).
/// - `output_class` — what kind of item this produces.
/// - `output_tier` — tier ∈ ⟦1, 6⟧ of resulting item.
/// - `ingredients` — material + count list. Ingredients are required for evaluation.
/// - `tool` — optional Tool affordance ; some recipes (potions) require AlchemyTable.
/// - `skill_min` — minimum craft-skill ∈ ⟦0, 100⟧ ; below this the recipe is locked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeNode {
    pub id: u32,
    pub output_class: ItemClass,
    pub output_tier: u8,
    pub ingredients: Vec<(Material, u8)>,
    pub tool: Option<Tool>,
    pub skill_min: u8,
}

impl RecipeNode {
    /// § builder-helper for terse recipe-table construction.
    #[must_use]
    pub fn new(
        id: u32,
        output_class: ItemClass,
        output_tier: u8,
        ingredients: Vec<(Material, u8)>,
        tool: Option<Tool>,
        skill_min: u8,
    ) -> Self {
        Self {
            id,
            output_class,
            output_tier,
            ingredients,
            tool,
            skill_min,
        }
    }

    /// § available : skill-gate predicate ; true iff caller-skill ≥ recipe's skill_min.
    #[must_use]
    pub fn available_for(&self, skill: u8) -> bool {
        skill >= self.skill_min
    }
}
