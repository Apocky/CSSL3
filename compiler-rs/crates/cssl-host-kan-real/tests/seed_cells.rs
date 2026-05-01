//! § Integration tests : RealSeedCellKanClassifier end-to-end.
//!
//! 5 tests per task spec.

#![allow(clippy::manual_range_contains)]

use cssl_host_kan_real::{RealSeedCellKanClassifier, MAX_SEED_CELLS};
use cssl_host_kan_substrate_bridge::seed_classifier::{
    SeedCellClassifier, Stage0KeywordSeedClassifier,
};

fn fresh_fb() -> Box<dyn SeedCellClassifier> {
    Box::new(Stage0KeywordSeedClassifier::default_table())
}

#[test]
fn emits_at_least_one_cell_for_known_intent() {
    let c = RealSeedCellKanClassifier::with_baked_seed(5, fresh_fb());
    let cells = c.intent_to_seed_cells("examine", &[]);
    assert!(!cells.is_empty());
}

#[test]
fn emits_no_more_than_max_cells() {
    let c = RealSeedCellKanClassifier::with_baked_seed(5, fresh_fb());
    let cells = c.intent_to_seed_cells("cocreate", &[]);
    assert!(cells.len() <= MAX_SEED_CELLS);
}

#[test]
fn cells_have_finite_charge_and_bounded_coords() {
    let c = RealSeedCellKanClassifier::with_baked_seed(5, fresh_fb());
    let cells = c.intent_to_seed_cells("move", &[]);
    for cell in &cells {
        assert!(cell.charge.is_finite());
        assert!(cell.charge >= -1.0 && cell.charge <= 1.0);
        // Decoder limits coords to 0..=15.
        assert!(cell.x <= 15);
        assert!(cell.y <= 15);
        assert!(cell.z <= 15);
    }
}

#[test]
fn determinism_same_seed_same_cells() {
    let c1 = RealSeedCellKanClassifier::with_baked_seed(7, fresh_fb());
    let c2 = RealSeedCellKanClassifier::with_baked_seed(7, fresh_fb());
    let a = c1.intent_to_seed_cells("examine", &[]);
    let b = c2.intent_to_seed_cells("examine", &[]);
    assert_eq!(a, b);
}

#[test]
fn pure_fallback_uses_stage0_table() {
    let c = RealSeedCellKanClassifier::pure_fallback(fresh_fb());
    let cells = c.intent_to_seed_cells("talk", &[]);
    // Stage-0 has "talk" → kind=2 + 1-cell.
    assert_eq!(cells.len(), 1);
    assert_eq!(cells[0].kind, 2);
}
