//! § cssl-host-integration-tests — POD-4-D1 e2e cross-wiring crate.
//! ════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Hosts integration-tests that exercise the loop :
//!     combat-tick → loot-drop → craft → gear-equip → spell-cast
//!   plus run-lifecycle (roguelike-run advance-phase) and NPC-BT-tick.
//!   No source-crate is modified ; type-coercions live in [`fixtures`].
//!
//! § FIXTURES SURFACE
//!   - [`fixtures::make_combat_session`] · seeded combat-tick + roll-fixture
//!   - [`fixtures::make_player_inventory`] · 13-slot inventory + bond-state
//!   - [`fixtures::make_recipe_book`]      · default-recipe-graph + skill
//!   - [`fixtures::make_grimoire`]         · 6-slot grimoire + mana-pool
//!   - [`fixtures::material_to_basemat`]   · craft Material → gear BaseMat coercion
//!   - [`fixtures::run_full_loop`]         · drives combat → loot → craft → equip → cast
//!
//! § DETERMINISM
//!   All fixtures accept an explicit u128/u64 seed ; same-seed → bit-equal
//!   results across runs. BTreeMap-shaped serde keeps JSON diffs stable.
//!
//! § DISCIPLINE
//!   #![forbid(unsafe_code)] · no .unwrap() outside #[cfg(test)] · no panics
//!   in fixture-fns (Result-bearing where fallible).
//!
//! § PRIME-DIRECTIVE
//!   Pure-test crate : no surveillance, no network, no biometric. Mirrors
//!   PoD-2 sibling-crate banners structurally.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]

pub mod fixtures;

pub use fixtures::{
    cast_minimal_fire_ray, craft_a_t1_weapon, deconstruct_a_crafted_item, equip_into_slot,
    item_class_coerce, make_combat_session, make_dm, make_gm, make_grimoire,
    make_player_inventory, make_recipe_book, material_to_basemat, minimal_fire_ray,
    run_full_loop, tiny_bt, CombatSession, FullLoopOutcome, PlayerInventory, RecipeBook,
    StubNpcWorld,
};

/// Crate-level PRIME-DIRECTIVE attestation banner.
pub const PRIME_DIRECTIVE_BANNER: &str =
    "consent=OS • integration-tests=pure • no-surveillance • no-network";

/// Crate version (matches Cargo.toml) — surfaced for replay-manifest headers.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod root_tests {
    use super::*;

    #[test]
    fn banner_nonempty_and_mentions_consent() {
        assert!(!PRIME_DIRECTIVE_BANNER.is_empty());
        assert!(PRIME_DIRECTIVE_BANNER.contains("consent=OS"));
    }

    #[test]
    fn version_present() {
        assert!(!VERSION.is_empty());
        assert!(VERSION.contains('.'));
    }
}
