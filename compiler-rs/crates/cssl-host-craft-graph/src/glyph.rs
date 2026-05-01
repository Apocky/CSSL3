// § glyph : item-customization slot layer
// ══════════════════════════════════════════════════════════════════
//! Per `GDDs/CRAFT_DECONSTRUCT_ALCHEMY.csl § GLYPH-SLOTS` :
//! - capacity 0..3 per item · per-quality-tier
//! - glyph = consumable-fragment · inserted-once · grants-affix
//! - sources : deconstruct 50%-drop · transmute-fail 30%-drop · workshop-discovery
//! - sample affixes : A-fire +10% · A-frost +10% · A-life +20HP · A-haste +5% · etc.
//! - 3 shards = 1 glyph at workshop

use crate::quality_tier::QualityTier;
use serde::{Deserialize, Serialize};

/// § AffixDescriptor : human-readable + machine-discriminable affix data.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AffixDescriptor {
    /// § stable-key (e.g. "A-fire", "A-frost") — used by audit/lineage trace.
    pub key: String,
    /// § magnitude (basis-points or absolute-int per affix-kind).
    pub magnitude: i32,
}

impl AffixDescriptor {
    #[must_use]
    pub fn new(key: impl Into<String>, magnitude: i32) -> Self {
        Self {
            key: key.into(),
            magnitude,
        }
    }
}

/// § GlyphInstance : a slotted glyph (filled affordance).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlyphInstance {
    pub shard_id: u32,
    pub affix: AffixDescriptor,
}

/// § GlyphSlot : either empty or filled.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GlyphSlot {
    pub filled: Option<GlyphInstance>,
}

impl GlyphSlot {
    #[must_use]
    pub fn empty() -> Self {
        Self { filled: None }
    }

    #[must_use]
    pub fn with(instance: GlyphInstance) -> Self {
        Self {
            filled: Some(instance),
        }
    }

    /// § fill : insert a glyph ; returns previous instance if slot was filled.
    pub fn fill(&mut self, instance: GlyphInstance) -> Option<GlyphInstance> {
        self.filled.replace(instance)
    }

    #[must_use]
    pub fn is_filled(&self) -> bool {
        self.filled.is_some()
    }
}

/// § glyph_slots_for_rarity : capacity per quality-tier per GDD § QUALITY-TIERS.
///
/// Returns u8 ∈ ⟦0, 3⟧. Common=0, Fine=1, Superior=2, Master=3, Heroic=3, Legendary=3.
#[must_use]
pub fn glyph_slots_for_rarity(tier: QualityTier) -> u8 {
    match tier {
        QualityTier::Common => 0,
        QualityTier::Fine => 1,
        QualityTier::Superior => 2,
        QualityTier::Master | QualityTier::Heroic | QualityTier::Legendary => 3,
    }
}
