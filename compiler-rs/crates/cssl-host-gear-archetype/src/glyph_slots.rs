//! § GlyphSlots — per-rarity glyph-socket count per GDD § GLYPH-SLOTS.
//!
//! slot-count rolled-on-drop :
//!   Common    : 0
//!   Uncommon  : 0..1
//!   Rare      : 1
//!   Epic      : 1..2
//!   Legendary : 2..3
//!   Mythic    : 3
//!
//! `glyph_slots_for_rarity(r)` returns the deterministic slot-count given the
//! rarity-tier. Variable-band rarities (Uncommon · Epic · Legendary) resolve
//! to the upper-bound here ; lower-bound paths roll via `roll_glyph_slots(seed,
//! r)` which uses the seed's parity to break ties (deterministic + replayable).

// Per-rarity match-arms with identical bodies are intentional (GDD § GLYPH-SLOTS
// slot-count table) — preserved for tier-by-tier readability.
#![allow(clippy::match_same_arms)]

use serde::{Deserialize, Serialize};

use crate::rarity::Rarity;

// ───────────────────────────────────────────────────────────────────────
// § GlyphFill
// ───────────────────────────────────────────────────────────────────────

/// Filled-slot payload : a glyph-affix-id (closed-set future-extended).
/// Stub-typed as `u32` for now ; downstream `cssl-host-glyph-pool` (post-MVP)
/// supplies the resolution table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GlyphFill {
    /// Glyph-id ordinal. Resolved against a host-side pool ; opaque here.
    pub glyph_id: u32,
}

// ───────────────────────────────────────────────────────────────────────
// § GlyphSlot
// ───────────────────────────────────────────────────────────────────────

/// One glyph-socket. `None` = empty + insertable ; `Some` = filled.
/// Pre-bond glyphs are un-socket-able ; post-bond glyphs immutable (Legendary+).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlyphSlot {
    /// Optional fill. Empty by default on roll.
    pub filled: Option<GlyphFill>,
}

impl GlyphSlot {
    /// Construct an empty slot.
    #[must_use]
    pub const fn empty() -> Self {
        Self { filled: None }
    }

    /// Insert a glyph. Returns `Err` if already filled (caller-discipline).
    pub fn insert(&mut self, glyph: GlyphFill) -> Result<(), &'static str> {
        if self.filled.is_some() {
            return Err("glyph-slot already filled");
        }
        self.filled = Some(glyph);
        Ok(())
    }

    /// Pop a glyph (only if pre-bond — caller enforces). Returns the popped fill.
    pub fn pop(&mut self) -> Option<GlyphFill> {
        self.filled.take()
    }

    /// True iff slot has a glyph.
    #[must_use]
    pub const fn is_filled(&self) -> bool {
        self.filled.is_some()
    }
}

impl Default for GlyphSlot {
    fn default() -> Self {
        Self::empty()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § glyph_slots_for_rarity  — deterministic upper-bound resolver
// ───────────────────────────────────────────────────────────────────────

/// Per-rarity glyph-slot count. Variable bands (Uncommon · Epic · Legendary)
/// return the upper-bound ; use `roll_glyph_slots(seed, r)` for seeded-roll.
#[must_use]
pub const fn glyph_slots_for_rarity(r: Rarity) -> u8 {
    match r {
        Rarity::Common => 0,
        Rarity::Uncommon => 1,   // 0..1 upper-bound
        Rarity::Rare => 1,
        Rarity::Epic => 2,       // 1..2 upper-bound
        Rarity::Legendary => 3,  // 2..3 upper-bound
        Rarity::Mythic => 3,
    }
}

/// Per-rarity glyph-slot lower-bound. Pair with `glyph_slots_for_rarity` for the
/// closed-range `[lower, upper]` inclusive.
#[must_use]
pub const fn glyph_slots_lower_bound(r: Rarity) -> u8 {
    match r {
        Rarity::Common => 0,
        Rarity::Uncommon => 0,
        Rarity::Rare => 1,
        Rarity::Epic => 1,
        Rarity::Legendary => 2,
        Rarity::Mythic => 3,
    }
}

/// Seeded glyph-slot count. Returns lower..=upper inclusive based on seed-parity-bits.
/// Deterministic : same seed × rarity → same count. Replay-bit-equal.
#[must_use]
pub fn roll_glyph_slots(seed: u128, r: Rarity) -> u8 {
    let lo = glyph_slots_lower_bound(r);
    let hi = glyph_slots_for_rarity(r);
    if hi == lo {
        return hi;
    }
    // Use the high-32-bits XOR low-32-bits of seed to derive a stable u8.
    let mixed = ((seed >> 64) ^ (seed & 0xFFFF_FFFF_FFFF_FFFF)) as u64;
    let span = (hi - lo + 1) as u64;
    let r = (mixed % span) as u8;
    lo + r
}
