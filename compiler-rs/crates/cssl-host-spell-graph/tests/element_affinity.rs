// § integration : element-affinity matrix + primary-pair-counters
// ════════════════════════════════════════════════════════════════════
// Per GDD § ELEMENTAL-AFFINITIES § AFFINITY-MATRIX + § PRIMARY-PAIR-COUNTERS.

use cssl_host_spell_graph::{
    affinity_table, primary_pair_counters,
    Element,
};

#[test]
fn matrix_is_8x8_with_unit_diagonal() {
    let t = affinity_table();
    assert_eq!(t.len(), 8);
    for row in &t {
        assert_eq!(row.len(), 8);
    }
    // Diagonal (self vs self) defaults to 1.0 (no override).
    for e in Element::ALL {
        let i = e.index();
        assert!((t[i][i] - 1.0).abs() < 1e-3);
    }
}

#[test]
fn fire_frost_mutual_cancel() {
    let t = affinity_table();
    assert!((t[Element::Fire.index()][Element::Frost.index()] - 0.0).abs() < 1e-3);
    assert!((t[Element::Frost.index()][Element::Fire.index()] - 0.0).abs() < 1e-3);
}

#[test]
fn shock_earth_mutual_resist() {
    let t = affinity_table();
    assert!((t[Element::Shock.index()][Element::Earth.index()] - 0.5).abs() < 1e-3);
    assert!((t[Element::Earth.index()][Element::Shock.index()] - 0.5).abs() < 1e-3);
}

#[test]
fn primary_pair_counters_match_gdd() {
    let pairs = primary_pair_counters();
    assert_eq!(pairs.len(), 4);
    assert!(pairs.contains(&(Element::Fire,  Element::Frost)));
    assert!(pairs.contains(&(Element::Shock, Element::Earth)));
    assert!(pairs.contains(&(Element::Air,   Element::Phase)));
    assert!(pairs.contains(&(Element::Holy,  Element::Void)));
}

#[test]
fn air_phase_pierce_asymmetric() {
    let t = affinity_table();
    // Per GDD : Air → Phase = 0.50 ; Phase → Air = 1.50 (Phase pierces).
    assert!((t[Element::Air.index()][Element::Phase.index()] - 0.5).abs() < 1e-3);
    assert!((t[Element::Phase.index()][Element::Air.index()] - 1.5).abs() < 1e-3);
}

#[test]
fn holy_void_mutual_counter_at_15() {
    let t = affinity_table();
    assert!((t[Element::Holy.index()][Element::Void.index()] - 1.5).abs() < 1e-3);
    assert!((t[Element::Void.index()][Element::Holy.index()] - 1.5).abs() < 1e-3);
}
