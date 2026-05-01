// § deconstruct : inverse-craft · returns frac × mats based-on (skill × tool-q)
// ══════════════════════════════════════════════════════════════════
//! Per `GDDs/CRAFT_DECONSTRUCT_ALCHEMY.csl § DECONSTRUCTION` :
//!
//! - tool-affinity :
//!     - Voidsmelter ↦ metals + gems     · base 0.45
//!     - Soulhammer  ↦ jewelry + woods   · base 0.50
//!     - Phaseloom   ↦ cloths + leathers · base 0.55
//! - recover-fn : `raw = base + 0.4×(skill/100) + 0.2×(tool_q/100)` ;
//!   clamp ∈ [0.30, 0.70]. ‼ NEVER 0% (no flat-zero return).
//! - 50% glyph-fragment-drop · item-consumed · tool -1 dura.

use crate::glyph::GlyphSlot;
use crate::material::Material;
use crate::recipe::{ItemClass, Tool};
use serde::{Deserialize, Serialize};

/// § DeconstructTool : alias-aware Tool subset (excludes AlchemyTable).
///
/// Per GDD only Voidsmelter/Soulhammer/Phaseloom may deconstruct ; AlchemyTable
/// is potion-domain. We expose a subset enum to forbid AlchemyTable at the
/// type-level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeconstructTool {
    Voidsmelter,
    Soulhammer,
    Phaseloom,
}

impl DeconstructTool {
    /// § from_tool : Tool → Option<DeconstructTool> (None for AlchemyTable).
    #[must_use]
    pub fn from_tool(t: Tool) -> Option<Self> {
        match t {
            Tool::Voidsmelter => Some(Self::Voidsmelter),
            Tool::Soulhammer => Some(Self::Soulhammer),
            Tool::Phaseloom => Some(Self::Phaseloom),
            Tool::AlchemyTable => None,
        }
    }

    /// § base_recovery : per-GDD base-rate per tool.
    #[must_use]
    pub fn base_recovery(self) -> f32 {
        match self {
            DeconstructTool::Voidsmelter => 0.45,
            DeconstructTool::Soulhammer => 0.50,
            DeconstructTool::Phaseloom => 0.55,
        }
    }
}

/// § CraftedItem : minimum lineage required to deconstruct.
///
/// Per GDD § FAILURE-MODES F-7 : missing CraftLineage ⇒ fallback to base-recovery.
/// We model `lineage` as the materials-used-list ; a missing or empty lineage
/// returns 0 mats but the function NEVER panics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CraftedItem {
    pub recipe_id: u32,
    pub class: ItemClass,
    pub tier: u8,
    /// § lineage : ordered (Material, count) pairs originally consumed in craft.
    pub lineage: Vec<(Material, u8)>,
    /// § filled glyph-slots, if any (deconstruct may recover shards).
    pub glyph_slots: Vec<GlyphSlot>,
}

/// § DeconstructResult : returned-mats + glyph-shard-drop signal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeconstructResult {
    /// § returned-materials (count rounded down by recover_frac).
    pub returned_mats: Vec<(Material, u8)>,
    /// § glyph-shard drop (true ⇒ caller awards 1 shard ; 3 shards = 1 glyph at workshop).
    pub glyph_shard_drop: bool,
    /// § realized-recovery-fraction (post-clamp ; ∈ [0.30, 0.70]).
    pub recovery_fraction: f32,
}

/// § deconstruct : inverse-craft per GDD § DECONSTRUCTION.
///
/// `tool_quality` ∈ ⟦0, 100⟧ (analogue to durability/quality of the deconstruct tool).
/// `glyph_roll` ∈ ⟦0.0, 1.0⟩ — caller-supplied roll for the 50% glyph-shard drop.
/// `skill` ∈ ⟦0, 100⟧.
///
/// Recovery formula : `raw = base + 0.4×(skill/100) + 0.2×(tool_q/100)`,
/// clamped to ⟦0.30, 0.70⟧. Per-material count returned = floor(orig × frac),
/// minimum 1 if orig ≥ 1 (anti-frustration guard ; matches GDD "NEVER 0%").
#[must_use]
pub fn deconstruct(
    item: &CraftedItem,
    tool: DeconstructTool,
    skill: u8,
    tool_quality: u8,
    glyph_roll: f32,
) -> DeconstructResult {
    let base = tool.base_recovery();
    let s = f32::from(skill.min(100)) / 100.0;
    let q = f32::from(tool_quality.min(100)) / 100.0;

    let raw = base + 0.4 * s + 0.2 * q;
    let frac = raw.clamp(0.30, 0.70);

    // § Per-material : floor(count × frac) ; bump to 1 if original was ≥ 1
    // (GDD anti-frustration : NEVER 0%).
    let returned_mats: Vec<(Material, u8)> = item
        .lineage
        .iter()
        .filter_map(|&(mat, count)| {
            if count == 0 {
                return None;
            }
            let scaled = (f32::from(count) * frac).floor() as i32;
            let final_count = scaled.max(1) as u8;
            Some((mat, final_count))
        })
        .collect();

    // § glyph-shard-drop : 50% per GDD (only if any slots filled).
    let any_filled = item.glyph_slots.iter().any(GlyphSlot::is_filled);
    let glyph_shard_drop = any_filled && glyph_roll < 0.50;

    DeconstructResult {
        returned_mats,
        glyph_shard_drop,
        recovery_fraction: frac,
    }
}
