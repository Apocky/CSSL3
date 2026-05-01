// § T11-W7-RD-B5 : cssl-host-spell-graph — root module
// ════════════════════════════════════════════════════════════════════
// § I> spell-graph composition kernel : 8-element + 5-node-types + mana-economy
// § I> ⟨GDDs/MAGIC_SYSTEM.csl⟩ POD-2-B5 — load-bearing on cssl-substrate-omega-field
// § I> exports : element ⊔ node ⊔ graph ⊔ cost ⊔ mana ⊔ cast ⊔ status_map ⊔ combo ⊔ grimoire
// § I> determinism : BTreeMap/BTreeSet iteration ; no HashMap in serialized state
// § I> safety : forbid(unsafe_code) · no panics in lib · all-failures via Result
// § I> consent : Σ-mask placeholder hooks ¬ overridable ; per PRIME_DIRECTIVE
// ════════════════════════════════════════════════════════════════════

#![forbid(unsafe_code)]
#![doc = include_str!("../Cargo.toml")]

pub mod element;
pub mod node;
pub mod graph;
pub mod cost;
pub mod mana;
pub mod cast;
pub mod status_map;
pub mod combo;
pub mod grimoire;

pub use element::{Element, ElementAffinity, affinity_table, primary_pair_counters};
pub use node::{
    SpellNode, NodeKind, ModifierKind, ShapeKind, TriggerKind, ConduitKind,
};
pub use graph::{SpellGraph, GraphErr, NodeIdx};
pub use cost::mana_cost;
pub use mana::ManaPool;
pub use cast::{cast, CastResult, EventCell, Target};
pub use status_map::{StatusEffect, element_to_status};
pub use combo::{Combo, default_combos, find_combo};
pub use grimoire::{Grimoire, GrimoireErr};

/// Crate-level metadata banner ← attests § PRIME-DIRECTIVE structurally.
///
/// § I> consent=OS · violation=bug · no-override-exists
/// § I> reads-only spell-graph + target ; writes-only substrate-event-cell ; no surveillance
pub const PRIME_DIRECTIVE_BANNER: &str =
    "consent=OS • violation=bug • no-override-exists";

/// Crate version (matches Cargo.toml) — surfaced for serialized grimoire headers.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// GDD-source-of-truth pointer — for traceability.
pub const GDD_REF: &str = "GDDs/MAGIC_SYSTEM.csl";

#[cfg(test)]
mod root_tests {
    use super::*;

    #[test]
    fn prime_directive_banner_nonempty() {
        assert!(!PRIME_DIRECTIVE_BANNER.is_empty());
        assert!(PRIME_DIRECTIVE_BANNER.contains("consent=OS"));
    }

    #[test]
    fn version_matches_cargo() {
        assert!(!VERSION.is_empty());
        assert!(VERSION.contains('.'));
    }

    #[test]
    fn gdd_ref_points_to_magic_system() {
        assert_eq!(GDD_REF, "GDDs/MAGIC_SYSTEM.csl");
    }
}
