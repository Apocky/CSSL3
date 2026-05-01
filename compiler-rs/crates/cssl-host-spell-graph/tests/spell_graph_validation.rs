// § integration : spell-graph validation rules
// ════════════════════════════════════════════════════════════════════
// Per GDD § GRAPH-STRUCTURE + § VALIDATION-RULES.

use cssl_host_spell_graph::{
    Element, GraphErr, ModifierKind, ShapeKind, SpellGraph, SpellNode, TriggerKind,
};

fn minimal_valid() -> SpellGraph {
    let mut g = SpellGraph::new();
    let s = g.add_node(SpellNode::Source(Element::Fire));
    let sh = g.add_node(SpellNode::Shape(ShapeKind::Ray));
    let tr = g.add_node(SpellNode::Trigger(TriggerKind::OnCast));
    g.add_edge(s, sh);
    g.add_edge(sh, tr);
    g
}

#[test]
fn minimal_graph_is_acyclic_and_valid() {
    let g = minimal_valid();
    assert!(g.validate_acyclic().is_ok());
    assert!(g.validate_one_source().is_ok());
    assert!(g.validate().is_ok());
}

#[test]
fn cycle_is_rejected() {
    let mut g = minimal_valid();
    // Introduce back-edge Trigger → Source : forms cycle.
    g.add_edge(2, 0);
    let r = g.validate_acyclic();
    assert!(matches!(r, Err(GraphErr::Cycle { .. })));
}

#[test]
fn two_sources_rejected() {
    let mut g = minimal_valid();
    g.add_node(SpellNode::Source(Element::Frost)); // 2nd source
    let r = g.validate_one_source();
    assert!(matches!(r, Err(GraphErr::SourceCount { found: 2 })));
}

#[test]
fn no_source_rejected() {
    let mut g = SpellGraph::new();
    g.add_node(SpellNode::Shape(ShapeKind::Ray));
    g.add_node(SpellNode::Trigger(TriggerKind::OnCast));
    let r = g.validate_one_source();
    assert!(matches!(r, Err(GraphErr::SourceCount { found: 0 })));
}

#[test]
fn modifier_stack_overflow_rejected() {
    let mut g = minimal_valid();
    for _ in 0..5 {
        g.add_node(SpellNode::Modifier(ModifierKind::Amplify));
    }
    let r = g.validate_modifier_stack();
    assert!(matches!(r, Err(GraphErr::ModifierStackOverflow { .. })));
}
