// § material : 33 base materials · 6 categories · per-material tier-lookup
// ══════════════════════════════════════════════════════════════════
//! Per `GDDs/CRAFT_DECONSTRUCT_ALCHEMY.csl § MATERIAL-POOL` :
//! METALS(6) · WOODS(5) · CLOTHS(5) · LEATHERS(4) · GEMS(7) · CATALYSTS(6) = 33 ≥ 32.
//! Tiers ∈ ⟦1, 6⟧.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MaterialCategory {
    Metal, Wood, Cloth, Leather, Gem, Catalyst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Material {
    // METALS (6)
    Iron, Silver, Mithril, Adamant, Voidsteel, Soulalloy,
    // WOODS (5)
    Oak, Yew, Ironwood, Soulwood, Ghostwood,
    // CLOTHS (5)
    Linen, Silk, Spidersilk, Phaseweave, Voidweave,
    // LEATHERS (4)
    Hide, Drakeskin, Wyvernhide, Voidleather,
    // GEMS (7)
    Quartz, Sapphire, Ruby, Emerald, Voidcrystal, Soulgem, Catalystgem,
    // CATALYSTS (6)
    Saltpeter, Alkahest, Mithrilshade, Voidessence, Soulflux, Phaseether,
}

/// § material_tier : MaterialTier ∈ ⟦1, 6⟧ per GDD § MATERIAL-POOL.
#[must_use]
#[allow(clippy::match_same_arms)]
pub fn material_tier(mat: Material) -> u8 {
    use Material::*;
    match mat {
        Iron => 1, Silver => 2, Mithril => 3, Adamant => 4, Voidsteel => 5, Soulalloy => 6,
        Oak => 1, Yew => 2, Ironwood => 3, Soulwood => 4, Ghostwood => 5,
        Linen => 1, Silk => 2, Spidersilk => 3, Phaseweave => 4, Voidweave => 5,
        Hide => 1, Drakeskin => 3, Wyvernhide => 4, Voidleather => 5,
        Quartz => 1, Sapphire => 2, Ruby => 2, Emerald => 3,
        Voidcrystal => 4, Soulgem => 5, Catalystgem => 6,
        Saltpeter => 1, Alkahest => 2, Mithrilshade => 3,
        Voidessence => 4, Soulflux => 5, Phaseether => 6,
    }
}

/// § material_category : Material → MaterialCategory.
#[must_use]
pub fn material_category(mat: Material) -> MaterialCategory {
    use Material::*;
    use MaterialCategory as C;
    match mat {
        Iron | Silver | Mithril | Adamant | Voidsteel | Soulalloy => C::Metal,
        Oak | Yew | Ironwood | Soulwood | Ghostwood => C::Wood,
        Linen | Silk | Spidersilk | Phaseweave | Voidweave => C::Cloth,
        Hide | Drakeskin | Wyvernhide | Voidleather => C::Leather,
        Quartz | Sapphire | Ruby | Emerald | Voidcrystal | Soulgem | Catalystgem => C::Gem,
        Saltpeter | Alkahest | Mithrilshade | Voidessence | Soulflux | Phaseether => C::Catalyst,
    }
}

/// § all_materials : enumerate the 33-material pool (test-helper).
#[must_use]
pub fn all_materials() -> Vec<Material> {
    use Material::*;
    vec![
        Iron, Silver, Mithril, Adamant, Voidsteel, Soulalloy,
        Oak, Yew, Ironwood, Soulwood, Ghostwood,
        Linen, Silk, Spidersilk, Phaseweave, Voidweave,
        Hide, Drakeskin, Wyvernhide, Voidleather,
        Quartz, Sapphire, Ruby, Emerald, Voidcrystal, Soulgem, Catalystgem,
        Saltpeter, Alkahest, Mithrilshade, Voidessence, Soulflux, Phaseether,
    ]
}
