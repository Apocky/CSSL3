// § T11-W5c-FAB-PROCGEN : crate root
// ══════════════════════════════════════════════════════════════════
//! # cssl-host-fab-procgen
//!
//! § Factor-Augmented Blueprint procedural generation.
//!
//! Higher-level grammar layered atop [`cssl_host_procgen_rooms`] : combine
//! multiple base [`cssl_host_procgen_rooms::RoomKind`] recipes into a
//! compound [`Blueprint`] with style-rule constraints (symmetry, density
//! falloff, doorway alignment, max-parts).
//!
//! ## Pipeline
//! 1. Author a [`Blueprint`] (manually or via [`library`]) — parts +
//!    connections + seed.
//! 2. Configure [`StyleRules`] (or default).
//! 3. [`Composer::compose_to_recipes`] expands every part into its base
//!    `RoomRecipe` with proper seed-offset.
//! 4. [`Composer::compose_to_world_tiles`] flattens recipes to a single
//!    16-byte-stride [`WorldTile`] list with positional offsets and
//!    per-part rotation applied.
//!
//! ## Determinism
//! Identical `(Blueprint.seed, parts, connections)` produce a bit-identical
//! `Vec<RoomRecipe>` and `Vec<WorldTile>`.
//!
//! ## Forbidden
//! - `unsafe` — `#![forbid(unsafe_code)]`
//! - panics — composition errors are surfaced via [`ComposeErr`].

#![forbid(unsafe_code)]
// § Pedantic/nursery noise that's purely cosmetic in a scaffold-tier crate ;
// matches the parent rooms crate's allow-list.
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::similar_names)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::single_char_lifetime_names)]
#![allow(clippy::min_ident_chars)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::float_cmp)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::arithmetic_side_effects)]
#![allow(clippy::float_arithmetic)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::missing_errors_doc)]

pub mod blueprint;
pub mod composer;
pub mod library;
pub mod style_rules;

pub use blueprint::{Blueprint, BlueprintConnection, BlueprintErr, BlueprintPart};
pub use composer::{ComposeErr, Composer, WorldTile};
pub use library::{cathedral_blueprint, color_pavilion_blueprint, maze_dungeon_blueprint};
pub use style_rules::{DoorwayAlignmentRule, StyleRules, StyleViolation, SymmetryRule};
