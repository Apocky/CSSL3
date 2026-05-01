// § alchemy : potion-brew (additive) + transmutation (multiplicative)
// ══════════════════════════════════════════════════════════════════
//! Per `GDDs/CRAFT_DECONSTRUCT_ALCHEMY.csl § ALCHEMY` :
//! - POTION-BREW : Vial + 1..3 reagents (catalyst optional). skill < required ⇒
//!   50% bottle-explodes (lose-mats · audit-emit).
//! - TRANSMUTATION : 3-of-tier-N + catalyst → 1-of-tier-(N+1) probabilistic.
//!   base = 0.20 + 0.005×skill ; clamp ≤ 0.95 (per GDD).
//!   Catalysts : Voidessence 1.5× · Soulflux 2.0× · Catalystgem 1.3×.
//!   Stack {Voidessence + Soulflux} = 3.0× (special-case).
//!   skill-gate : transmute-skill ≥ 20×N ; on-fail ⇒ 1-of-tier-N preserved.
//!   forbidden : ¬ transmute consumables ↔ equipment (cross-category-block).

use crate::material::{material_category, Material, MaterialCategory};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Potion {
    pub recipe_id: u32,
    pub brewed_effect: String,
    pub magnitude: i32,
    pub duration_ticks: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrewErr {
    EmptyReagents,
    TooManyReagents,
    BottleExploded,
    SkillTooLow,
    CrossCategoryBlock,
}

/// § brew : potion-brew evaluator.
///
/// `explode_roll` ∈ ⟦0, 1⟩ ; if `skill < required`, roll < 0.50 ⇒ BottleExploded
/// (per GDD § POTION-BREW § failure : 50% explosion rate).
pub fn brew(
    reagents: &[(Material, u8)],
    catalysts: &[(Material, u8)],
    skill: u8,
    recipe_id: u32,
    required_skill: u8,
    explode_roll: f32,
) -> Result<Potion, BrewErr> {
    if reagents.is_empty() {
        return Err(BrewErr::EmptyReagents);
    }
    if reagents.len() > 3 {
        return Err(BrewErr::TooManyReagents);
    }

    // § Cross-category sanity-check : pure-metal reagent-list = equipment-domain.
    let all_metal = reagents
        .iter()
        .chain(catalysts.iter())
        .all(|(m, _)| material_category(*m) == MaterialCategory::Metal);
    if all_metal {
        return Err(BrewErr::CrossCategoryBlock);
    }

    if skill < required_skill {
        if explode_roll < 0.50 {
            return Err(BrewErr::BottleExploded);
        }
        return Err(BrewErr::SkillTooLow);
    }

    // § Magnitude : base 25 · +5 per reagent · +10 per catalyst.
    let magnitude = 25_i32 + 5 * reagents.len() as i32 + 10 * catalysts.len() as i32;

    // § Duration : 200-tick base (10s @ 20Hz), -40 ticks per skill-tier (skill/20). Floor 40.
    let skill_tier = u32::from(skill / 20);
    let duration_ticks = 200_u32.saturating_sub(skill_tier.saturating_mul(40)).max(40);

    let brewed_effect = match material_category(reagents[0].0) {
        MaterialCategory::Catalyst => "Catalyst-Brew",
        MaterialCategory::Gem => "Crystal-Tonic",
        MaterialCategory::Wood => "Herbal-Draught",
        MaterialCategory::Cloth => "Linen-Vial-Mix",
        MaterialCategory::Leather => "Hide-Trim-Elixir",
        MaterialCategory::Metal => "Mineral-Infusion",
    }
    .to_string();

    Ok(Potion { recipe_id, brewed_effect, magnitude, duration_ticks })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransmuteResult {
    pub output_count: u8,
    pub preserved_input_count: u8,
    pub success_prob: f32,
    pub success: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransmuteErr {
    SkillBelowGate,
    InsufficientInputs,
    AtTierCap,
    CrossCategoryBlock,
}

/// § transmute_tier : 3-of-tier-N + catalyst → 1-of-tier-(N+1) probabilistic.
///
/// `cross_category` : caller flags consumable↔equipment domain mix (forbidden).
pub fn transmute_tier(
    input_tier: u8,
    count: u8,
    catalyst_mult: f32,
    skill: u8,
    success_roll: f32,
    cross_category: bool,
) -> Result<TransmuteResult, TransmuteErr> {
    if cross_category {
        return Err(TransmuteErr::CrossCategoryBlock);
    }
    if input_tier == 0 || input_tier >= 6 {
        return Err(TransmuteErr::AtTierCap);
    }
    if count < 3 {
        return Err(TransmuteErr::InsufficientInputs);
    }
    if skill < input_tier.saturating_mul(20) {
        return Err(TransmuteErr::SkillBelowGate);
    }

    let base = 0.20_f32 + 0.005_f32 * f32::from(skill.min(100));
    let success_prob = (base * catalyst_mult.max(1.0)).clamp(0.0, 0.95);
    let success = success_roll < success_prob;
    let (output_count, preserved_input_count) = if success { (1, 0) } else { (0, 1) };

    Ok(TransmuteResult { output_count, preserved_input_count, success_prob, success })
}

/// § catalyst_multiplier : Voidessence 1.5× · Soulflux 2.0× · Catalystgem 1.3× ;
/// stack-{Voidessence + Soulflux} = 3.0× (per GDD § TRANSMUTATION).
#[must_use]
pub fn catalyst_multiplier(catalysts: &[Material]) -> f32 {
    let has_void = catalysts.contains(&Material::Voidessence);
    let has_soul = catalysts.contains(&Material::Soulflux);
    let has_cat = catalysts.contains(&Material::Catalystgem);

    if has_void && has_soul {
        return 3.0;
    }
    let mut mult = 1.0_f32;
    if has_void { mult *= 1.5; }
    if has_soul { mult *= 2.0; }
    if has_cat { mult *= 1.3; }
    mult
}
