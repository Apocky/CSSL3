// § integration : 5 default combos + multi-element bonus
// ════════════════════════════════════════════════════════════════════
// Per GDD § COMBO-SYSTEM.

use cssl_host_spell_graph::{default_combos, find_combo, Element};

#[test]
fn five_default_combos_present() {
    let combos = default_combos();
    assert_eq!(combos.len(), 5);
}

#[test]
fn super_evaporate_doubles_damage() {
    let c = find_combo(Element::Fire, Element::Air).expect("super-evaporate");
    assert!((c.bonus_multiplier - 2.0).abs() < 1e-3);
    assert_eq!(c.name, "super-evaporate");
}

#[test]
fn shatter_triples_damage() {
    let c = find_combo(Element::Earth, Element::Frost).expect("shatter");
    assert!((c.bonus_multiplier - 3.0).abs() < 1e-3);
    assert_eq!(c.name, "shatter");
}

#[test]
fn purge_detonate_holy_void_combo() {
    let c = find_combo(Element::Holy, Element::Void).expect("purge-detonate");
    assert_eq!(c.name, "purge-detonate");
    // commutative search
    let c2 = find_combo(Element::Void, Element::Holy).expect("purge-detonate-rev");
    assert_eq!(c, c2);
}

#[test]
fn non_combo_pair_returns_none() {
    assert!(find_combo(Element::Fire, Element::Earth).is_none());
}

#[test]
fn flash_freeze_chain_amplify_present() {
    assert!(find_combo(Element::Frost, Element::Air).is_some());
    assert!(find_combo(Element::Shock, Element::Air).is_some());
}
