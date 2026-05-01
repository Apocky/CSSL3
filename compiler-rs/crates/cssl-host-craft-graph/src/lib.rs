// § T11-W7-RD-B3 cssl-host-craft-graph : crate root
// ══════════════════════════════════════════════════════════════════
//! # cssl-host-craft-graph
//!
//! § Recipe-graph DAG · material-pool · skill-scaling · alchemy · transmutation.
//!
//! Per `GDDs/CRAFT_DECONSTRUCT_ALCHEMY.csl` :
//! - 33 base materials across 6 categories (METALS · WOODS · CLOTHS · LEATHERS · GEMS · CATALYSTS).
//! - 40 recipe-nodes forming an acyclic graph (each tier-N references only earlier-tier outputs).
//! - 6 quality-tiers (Common → Legendary) with ≤ 1.50× stat-cap (anti-power-creep).
//! - Skill-scaling = quality-tier-shift, NOT raw-stat-multiplier.
//! - Deconstruct returns 30%..70% materials (clamped, NEVER 0).
//! - Alchemy potion-brew (additive) + transmutation (multiplicative, lead-to-gold).
//! - Every craft / deconstruct / transmute emits to an `AuditSink` (anti-cheat anchor).
//!
//! ## Determinism
//! All probabilistic operations accept an explicit RNG-state input ; same seed →
//! bit-identical result. Round-trip `serde_json` equality holds for every public
//! data-type.
//!
//! ## Forbidden
//! - `unsafe` — `#![forbid(unsafe_code)]`
//! - external substrate-deps — this crate is leaf-level for craft logic.

#![forbid(unsafe_code)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::similar_names)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::min_ident_chars)]
#![allow(clippy::float_cmp)]
#![allow(clippy::missing_docs_in_private_items)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::arithmetic_side_effects)]
#![allow(clippy::float_arithmetic)]
#![allow(clippy::module_name_repetitions)]

pub mod alchemy;
pub mod audit;
pub mod deconstruct;
pub mod glyph;
pub mod material;
pub mod quality_tier;
pub mod recipe;
pub mod recipe_graph;
pub mod skill;

pub use alchemy::{brew, transmute_tier, BrewErr, Potion, TransmuteResult};
pub use audit::{AuditEvent, AuditSink, NoopAuditSink, RecordingAuditSink};
pub use deconstruct::{deconstruct, CraftedItem, DeconstructResult, DeconstructTool};
pub use glyph::{glyph_slots_for_rarity, AffixDescriptor, GlyphInstance, GlyphSlot};
pub use material::{material_category, material_tier, Material, MaterialCategory};
pub use quality_tier::{quality_tier_for_skill, QualityTier};
pub use recipe::{ItemClass, RecipeNode, Tool};
pub use recipe_graph::{default_recipe_graph, RecipeGraph};
pub use skill::{apply_xp, quality_tier_shift_prob, CraftSkill};
