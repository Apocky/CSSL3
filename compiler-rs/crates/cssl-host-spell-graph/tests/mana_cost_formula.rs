// § integration : mana-cost formula + capacity scaling
// ════════════════════════════════════════════════════════════════════
// Per GDD § COST-FORMULA + § CAPACITY.

use cssl_host_spell_graph::{
    mana_cost, Element, ModifierKind, ShapeKind, SpellGraph, SpellNode, TriggerKind,
};

#[test]
fn capacity_clamps_outsized_costs() {
    // Stack so many sources we'd exceed any reasonable capacity.
    let mut g = SpellGraph::new();
    for _ in 0..50 {
        g.add_node(SpellNode::Source(Element::Fire));
    }
    // Capacity at INT=0 is 100 ; cost SHOULD clamp.
    let cost = mana_cost(&g, 0);
    assert!(cost <= 100.0 + 1e-3);
}

#[test]
fn modifier_multiplies_source_cost() {
    let mut bare = SpellGraph::new();
    bare.add_node(SpellNode::Source(Element::Fire));
    let bare_cost = mana_cost(&bare, 99);

    let mut amplified = SpellGraph::new();
    amplified.add_node(SpellNode::Source(Element::Fire));
    amplified.add_node(SpellNode::Modifier(ModifierKind::Amplify));
    let amp_cost = mana_cost(&amplified, 99);

    // Amplify multiplier = 1.5 ; cost should be exactly 1.5× higher.
    assert!((amp_cost - bare_cost * 1.5).abs() < 1e-3);
}

#[test]
fn shape_and_trigger_costs_additive() {
    let mut g = SpellGraph::new();
    g.add_node(SpellNode::Source(Element::Fire));
    g.add_node(SpellNode::Shape(ShapeKind::GroundAoe)); // 10
    g.add_node(SpellNode::Trigger(TriggerKind::OnConditionLowHp)); // 4
    let cost = mana_cost(&g, 99);
    // 8 (source) + 10 (ground-AOE shape) + 4 (low-HP trigger) = 22
    assert!((cost - 22.0).abs() < 1e-3);
}

#[test]
fn empty_graph_costs_zero() {
    let g = SpellGraph::new();
    assert!(mana_cost(&g, 0).abs() < 1e-3);
}
