// § integration : cast → substrate-event-cell + status-effect
// ════════════════════════════════════════════════════════════════════
// Per GDD § CAST-MECHANICS § CAST-RESULT + § F-5 (Σ-revoked → effect-skipped).

use cssl_host_spell_graph::{
    cast, Element, ManaPool, ShapeKind, SpellGraph, SpellNode, StatusEffect,
    Target, TriggerKind,
};

fn frost_ray() -> SpellGraph {
    let mut g = SpellGraph::new();
    let s = g.add_node(SpellNode::Source(Element::Frost));
    let sh = g.add_node(SpellNode::Shape(ShapeKind::Ray));
    let tr = g.add_node(SpellNode::Trigger(TriggerKind::OnCast));
    g.add_edge(s, sh);
    g.add_edge(sh, tr);
    g
}

#[test]
fn cast_emits_event_cell_at_target_with_correct_element() {
    let g = frost_ray();
    let mut mana = ManaPool::new(200.0);
    let target = Target { x: 7, y: -3, z: 11, consent_present: true, caster_intelligence: 10 };
    let r = cast(&g, target, &mut mana);
    assert!(r.success);
    let cell = r.substrate_event_cell.expect("cell emitted");
    assert_eq!(cell.x, 7);
    assert_eq!(cell.y, -3);
    assert_eq!(cell.z, 11);
    assert_eq!(cell.element, Element::Frost);
    assert_eq!(r.status_effect_applied, Some(StatusEffect::Freeze));
}

#[test]
fn underflow_cancels_cast_no_event_cell() {
    let g = frost_ray();
    let mut mana = ManaPool::new(1.0); // way under cost
    let target = Target { x: 0, y: 0, z: 0, consent_present: true, caster_intelligence: 0 };
    let r = cast(&g, target, &mut mana);
    assert!(!r.success);
    assert!(r.substrate_event_cell.is_none());
    assert!(r.status_effect_applied.is_none());
}

#[test]
fn revoked_consent_skips_status_but_emits_cell() {
    // Per GDD § F-5 : cast-completes ⊕ effect-skipped + Audit
    let g = frost_ray();
    let mut mana = ManaPool::new(200.0);
    let target = Target { x: 0, y: 0, z: 0, consent_present: false, caster_intelligence: 10 };
    let r = cast(&g, target, &mut mana);
    assert!(r.success);
    assert!(r.substrate_event_cell.is_some());
    assert!(r.status_effect_applied.is_none(), "Σ-revoked → status suppressed");
}

#[test]
fn cast_determinism_same_input_same_output() {
    let g = frost_ray();
    let target = Target { x: 1, y: 2, z: 3, consent_present: true, caster_intelligence: 5 };
    let mut mana_a = ManaPool::new(200.0);
    let mut mana_b = ManaPool::new(200.0);
    let r_a = cast(&g, target, &mut mana_a);
    let r_b = cast(&g, target, &mut mana_b);
    assert_eq!(r_a, r_b);
    assert!((mana_a.current - mana_b.current).abs() < 1e-3);
}
