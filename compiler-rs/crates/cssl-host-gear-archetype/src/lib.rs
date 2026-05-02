//! # cssl-host-gear-archetype
//!
//! POD-2-B4 deliverable — modular gear composition for LoA-v13 per
//! `GDDs/GEAR_RARITY_SYSTEM.csl`.
//!
//! ## Composition
//!
//! `gear ≡ (base + N×prefix + N×suffix + glyph-slots) modular-composition`
//!
//! - **8 rarity tiers** (Q-06 Apocky-canonical 2026-05-01) :
//!   Common · Uncommon · Rare · Epic · Legendary · Mythic · Prismatic · Chaotic
//! - **13 gear slots** : Helm · Chest · Pants · Boots · Gloves · Belt · Cape ·
//!   MainHand · OffHand · RingA · RingB · Amulet · Trinket
//! - **24 prefixes** + **24 suffixes** (per GDD enumerations)
//! - **glyph-slots 0..6 per-rarity** (Common 0 · Chaotic 5..6)
//!
//! ## Determinism
//!
//! Stat-rolling uses a SplitMix64 `DetRng` keyed on `seed: u128`. Same seed +
//! same base + same rarity ⇒ bit-identical Gear (round-trip serde-stable).
//!
//! ## Upgrade paths (Q-06 8-tier)
//!
//! - `level_up(g, xp)` : XP → item-level
//! - `transmute(g, target_rarity, mat_cost)` : 5×N → 1×(N+1) tier-shift ;
//!   Legendary→Mythic FORBIDDEN · Mythic→Prismatic FORBIDDEN ·
//!   Prismatic→Chaotic FORBIDDEN (drop-only-or-bond ladder)
//! - `bond(g, player_id)` : Legendary+ binds-to-character (incl. Mythic+
//!   Prismatic + Chaotic per Q-06)
//! - `reroll_affix(g, target, seed)` : 1× per affix-slot
//!
//! ## Audit
//!
//! Every drop/transmute/bond/reroll emits an `AuditEvent` via the optional
//! `AuditSink`. `RecordingAuditSink` buffers events for tests ; host-side
//! integration wires `cssl-host-attestation`.

#![forbid(unsafe_code)]

pub mod affixes;
pub mod audit;
pub mod base;
pub mod drop_table;
pub mod gear;
pub mod glyph_slots;
pub mod rarity;
pub mod slots;
pub mod stat_rolling;
pub mod upgrade;

// Re-exports for convenient consumer-surface.
pub use affixes::{AffixDescriptor, AffixKind, Prefix, Suffix};
pub use audit::{AuditEvent, AuditSink, NoopAuditSink, RecordingAuditSink};
pub use base::{BaseItem, BaseMat, ItemClass};
pub use drop_table::{
    default_base_for_rarity, distribution_for_context, roll_drop, sample_rarity, Biome, DropContext,
};
pub use gear::Gear;
pub use glyph_slots::{
    glyph_slots_for_rarity, glyph_slots_lower_bound, roll_glyph_slots, GlyphFill, GlyphSlot,
};
pub use rarity::{rarity_drop_floor, Rarity};
pub use slots::{GearSlot, StatKind};
pub use stat_rolling::{
    clamp_to_class_max, prefix_for_descriptor, roll_affix, roll_gear, suffix_for_descriptor,
    tier_curve, DetRng, RolledAffix,
};
pub use upgrade::{bond, level_up, reroll_affix, transmute, RerollTarget, TransmuteResult};
