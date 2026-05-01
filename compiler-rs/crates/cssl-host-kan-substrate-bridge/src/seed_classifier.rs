//! Seed-cell emission trait abstraction + stage-0 keyword-table + stage-1 KAN-stub impls.
//!
//! § T11-W6-KAN-BRIDGE — module 3/4
//!
//! § ROLE
//!   LoA's spontaneous-condensation pipeline maps a typed intent
//!   (`IntentClass.kind` + `args`) into a list of seed-cells stamped
//!   into the ω-field substrate. Each cell carries a kind-tag, a 3D
//!   coordinate (u8 grid), a charge, and a color hint. Stage-0 today
//!   does this via a fixed kind→template table ; stage-1+ replaces it
//!   with a KAN sampler that conditions on the live ω-field. The trait
//!   below pins the `intent_to_seed_cells` contract.

use serde::{Deserialize, Serialize};

/// Trait for any seed-cell emission backend.
///
/// Object-safe : registry stores `Box<dyn SeedCellClassifier>`.
pub trait SeedCellClassifier: Send + Sync {
    /// Stable identifier (e.g. `"stage0-keyword-seed"`).
    fn name(&self) -> &str;

    /// Convert an intent (kind + flat args) into seed-cells.
    /// Implementations MUST return an empty `Vec` for unknown kinds
    /// rather than panic.
    fn intent_to_seed_cells(
        &self,
        intent_kind: &str,
        intent_args: &[(String, String)],
    ) -> Vec<SeedCell>;
}

/// One seed-cell to be stamped into the substrate.
///
/// All spatial fields are u8-bounded ; charge is f32 in -1.0..=1.0
/// (clamped on construction) ; color_hint is a free 0..=255 byte the
/// renderer interprets per its palette.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedCell {
    pub kind: u8,
    pub x: u8,
    pub y: u8,
    pub z: u8,
    pub charge: f32,
    pub color_hint: u8,
}

impl SeedCell {
    /// Construct with charge clamped to -1.0..=1.0.
    #[must_use]
    pub fn new(kind: u8, x: u8, y: u8, z: u8, charge: f32, color_hint: u8) -> Self {
        Self {
            kind,
            x,
            y,
            z,
            charge: charge.clamp(-1.0, 1.0),
            color_hint,
        }
    }
}

/// Stage-0 keyword-driven seed classifier.
///
/// `kind_table` is `(intent_kind, cell_kind, charge, color_hint)`.
/// Walks the table in order ; emits one cell at a fixed test-coordinate
/// per first-matching row.
pub struct Stage0KeywordSeedClassifier {
    pub kind_table: Vec<(String, u8, f32, u8)>,
}

impl Stage0KeywordSeedClassifier {
    /// Construct from an explicit table.
    #[must_use]
    pub fn new(kind_table: Vec<(String, u8, f32, u8)>) -> Self {
        Self { kind_table }
    }

    /// Construct with default mappings for the four canonical intents.
    #[must_use]
    pub fn default_table() -> Self {
        Self::new(vec![
            (String::from("move"), 1_u8, 0.5_f32, 10_u8),
            (String::from("talk"), 2_u8, 0.3_f32, 20_u8),
            (String::from("examine"), 3_u8, 0.4_f32, 30_u8),
            (String::from("cocreate"), 4_u8, 0.8_f32, 40_u8),
        ])
    }

    /// Pick a deterministic test-coordinate from the args : looks for an
    /// `x` / `y` / `z` arg, falls back to (8, 8, 8) center-cell.
    fn coord_from_args(args: &[(String, String)]) -> (u8, u8, u8) {
        let pick = |k: &str| -> Option<u8> {
            args.iter()
                .find(|(name, _)| name == k)
                .and_then(|(_, v)| v.parse::<u8>().ok())
        };
        (
            pick("x").unwrap_or(8),
            pick("y").unwrap_or(8),
            pick("z").unwrap_or(8),
        )
    }
}

impl SeedCellClassifier for Stage0KeywordSeedClassifier {
    fn name(&self) -> &str {
        "stage0-keyword-seed"
    }

    fn intent_to_seed_cells(
        &self,
        intent_kind: &str,
        intent_args: &[(String, String)],
    ) -> Vec<SeedCell> {
        for (k, cell_kind, charge, color_hint) in &self.kind_table {
            if k == intent_kind {
                let (x, y, z) = Self::coord_from_args(intent_args);
                return vec![SeedCell::new(*cell_kind, x, y, z, *charge, *color_hint)];
            }
        }
        Vec::new()
    }
}

/// Stage-1 KAN-stub seed classifier.
///
/// When `kan_handle.is_some()`, returns a canned 2-cell mocked
/// emission ; when `None`, delegates to `fallback`.
pub struct Stage1KanStubSeedClassifier {
    pub fallback: Box<dyn SeedCellClassifier>,
    pub kan_handle: Option<String>,
}

impl Stage1KanStubSeedClassifier {
    /// Construct with fallback + no KAN handle.
    #[must_use]
    pub fn new(fallback: Box<dyn SeedCellClassifier>) -> Self {
        Self {
            fallback,
            kan_handle: None,
        }
    }

    /// Construct with fallback + mock KAN handle.
    #[must_use]
    pub fn with_handle(fallback: Box<dyn SeedCellClassifier>, handle: String) -> Self {
        Self {
            fallback,
            kan_handle: Some(handle),
        }
    }
}

impl SeedCellClassifier for Stage1KanStubSeedClassifier {
    fn name(&self) -> &str {
        "stage1-kan-stub-seed"
    }

    fn intent_to_seed_cells(
        &self,
        intent_kind: &str,
        intent_args: &[(String, String)],
    ) -> Vec<SeedCell> {
        if self.kan_handle.is_some() {
            // Mocked KAN emission : a deterministic 2-cell pair so callers
            // can validate cardinality + bounds without invoking real KAN.
            let _ = intent_kind;
            let _ = intent_args;
            vec![
                SeedCell::new(99, 4, 4, 4, 0.7, 50),
                SeedCell::new(99, 12, 12, 12, -0.7, 60),
            ]
        } else {
            self.fallback.intent_to_seed_cells(intent_kind, intent_args)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage0_known_intent_yields_cells() {
        let c = Stage0KeywordSeedClassifier::default_table();
        let cells = c.intent_to_seed_cells("examine", &[]);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].kind, 3);
        assert!((cells[0].charge - 0.4).abs() < 1e-6);
    }

    #[test]
    fn stage0_unknown_yields_empty() {
        let c = Stage0KeywordSeedClassifier::default_table();
        let cells = c.intent_to_seed_cells("unknown", &[]);
        assert!(cells.is_empty());
    }

    #[test]
    fn stage1_no_kan_falls_through() {
        let stage0 = Box::new(Stage0KeywordSeedClassifier::default_table());
        let s1 = Stage1KanStubSeedClassifier::new(stage0);
        let cells = s1.intent_to_seed_cells("move", &[]);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].kind, 1);
    }

    #[test]
    fn stage1_with_kan_mocked() {
        let stage0 = Box::new(Stage0KeywordSeedClassifier::default_table());
        let s1 = Stage1KanStubSeedClassifier::with_handle(stage0, String::from("kan-mock"));
        let cells = s1.intent_to_seed_cells("anything", &[]);
        assert_eq!(cells.len(), 2);
        assert!(cells.iter().all(|c| c.kind == 99));
    }

    #[test]
    fn seed_cell_bounded() {
        // Charge should clamp.
        let cell = SeedCell::new(0, 0, 0, 0, 5.0, 0);
        assert_eq!(cell.charge, 1.0);
        let cell = SeedCell::new(0, 0, 0, 0, -5.0, 0);
        assert_eq!(cell.charge, -1.0);
        // Coordinates are u8-typed so out-of-range is unrepresentable.
    }

    #[test]
    fn trait_object_safe() {
        let _v: Vec<Box<dyn SeedCellClassifier>> = vec![
            Box::new(Stage0KeywordSeedClassifier::default_table()),
            Box::new(Stage1KanStubSeedClassifier::new(Box::new(
                Stage0KeywordSeedClassifier::default_table(),
            ))),
        ];
        assert_eq!(_v.len(), 2);
    }
}
