// § tests : recipe-graph DAG-property + 40-recipe seed
// ══════════════════════════════════════════════════════════════════
use cssl_host_craft_graph::{default_recipe_graph, ItemClass};

#[test]
fn t_default_graph_has_40_nodes() {
    let g = default_recipe_graph();
    assert_eq!(g.len(), 40, "GDD specifies 40 recipe-nodes");
}

#[test]
fn t_default_graph_is_acyclic() {
    let g = default_recipe_graph();
    assert!(g.is_acyclic(), "Default recipe graph must be DAG by construction");
}

#[test]
fn t_class_distribution_matches_gdd() {
    // GDD : 4 BASES + 10 WEAPONS + 10 ARMORS + 5 JEWELRY + 11 CONSUMABLES = 40
    // BASES split into class : 1 weapon-base + 1 armor-base + 1 jewelry-base + 1 consumable-base
    // ⇒ Weapon: 1 + 10 = 11 ; Armor: 1 + 10 = 11 ; Jewelry: 1 + 5 = 6 ; Consumable: 1 + 11 = 12
    let g = default_recipe_graph();
    assert_eq!(g.by_class(ItemClass::Weapon).len(), 11);
    assert_eq!(g.by_class(ItemClass::Armor).len(), 11);
    assert_eq!(g.by_class(ItemClass::Jewelry).len(), 6);
    assert_eq!(g.by_class(ItemClass::Consumable).len(), 12);
}
