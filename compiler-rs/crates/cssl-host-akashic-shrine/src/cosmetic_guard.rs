// § cosmetic_guard : structural enforcement of cosmetic-only-axiom.
// § Construction-time check rejects any field-set that exhibits gameplay-effect.

use core::fmt;

/// § CosmeticOnlyError : a proposed shrine-config violated the
/// cosmetic-channel-only-axiom. Variants enumerate the prohibited classes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CosmeticOnlyError {
    /// Stat-mod field detected (e.g. +damage, +regen).
    StatMod(&'static str),
    /// Resource yield (currency, XP, items).
    ResourceYield(&'static str),
    /// Per-fight buff/debuff/aura affecting gameplay.
    CombatAura(&'static str),
    /// Drop-rate or RNG-skew modifier.
    RngBias(&'static str),
    /// Unknown gameplay-effect tag flagged by audit.
    Unknown(&'static str),
}

impl fmt::Display for CosmeticOnlyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StatMod(t)       => write!(f, "cosmetic-only violation: stat_mod[{t}]"),
            Self::ResourceYield(t) => write!(f, "cosmetic-only violation: resource_yield[{t}]"),
            Self::CombatAura(t)    => write!(f, "cosmetic-only violation: combat_aura[{t}]"),
            Self::RngBias(t)       => write!(f, "cosmetic-only violation: rng_bias[{t}]"),
            Self::Unknown(t)       => write!(f, "cosmetic-only violation: unknown[{t}]"),
        }
    }
}

impl std::error::Error for CosmeticOnlyError {}

/// § Audit a proposed effect-tag list. Any tag that names a gameplay-effect
/// class is rejected. Caller passes a slice of `&'static str` tags (¬ free-form).
///
/// Allowed cosmetic prefixes : `visual.` `audio.` `particle.` `glyph.` `palette.`
/// All other prefixes → reject.
pub fn assert_cosmetic_only(effect_tags: &[&'static str]) -> Result<(), CosmeticOnlyError> {
    for tag in effect_tags {
        if tag.starts_with("stat.") || tag.starts_with("dmg.") || tag.starts_with("hp.") || tag.starts_with("regen.") {
            return Err(CosmeticOnlyError::StatMod(tag));
        }
        if tag.starts_with("yield.") || tag.starts_with("currency.") || tag.starts_with("xp.") || tag.starts_with("loot.") {
            return Err(CosmeticOnlyError::ResourceYield(tag));
        }
        if tag.starts_with("aura.") || tag.starts_with("buff.") || tag.starts_with("debuff.") || tag.starts_with("combat.") {
            return Err(CosmeticOnlyError::CombatAura(tag));
        }
        if tag.starts_with("rng.") || tag.starts_with("droprate.") || tag.starts_with("luck.") {
            return Err(CosmeticOnlyError::RngBias(tag));
        }
        let allowed = tag.starts_with("visual.")
            || tag.starts_with("audio.")
            || tag.starts_with("particle.")
            || tag.starts_with("glyph.")
            || tag.starts_with("palette.");
        if !allowed {
            return Err(CosmeticOnlyError::Unknown(tag));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_pure_cosmetic_tags() {
        assert!(assert_cosmetic_only(&["visual.glow", "audio.whisper", "particle.mist", "glyph.flame", "palette.gold"]).is_ok());
    }

    #[test]
    fn rejects_stat_mod() {
        let err = assert_cosmetic_only(&["stat.damage"]).unwrap_err();
        assert!(matches!(err, CosmeticOnlyError::StatMod(_)));
    }

    #[test]
    fn rejects_resource_yield() {
        assert!(matches!(
            assert_cosmetic_only(&["yield.gold"]).unwrap_err(),
            CosmeticOnlyError::ResourceYield(_)
        ));
    }

    #[test]
    fn rejects_combat_aura() {
        assert!(matches!(
            assert_cosmetic_only(&["aura.regen"]).unwrap_err(),
            CosmeticOnlyError::CombatAura(_)
        ));
        assert!(matches!(
            assert_cosmetic_only(&["buff.attack"]).unwrap_err(),
            CosmeticOnlyError::CombatAura(_)
        ));
    }

    #[test]
    fn rejects_rng_bias() {
        assert!(matches!(
            assert_cosmetic_only(&["luck.crit"]).unwrap_err(),
            CosmeticOnlyError::RngBias(_)
        ));
    }

    #[test]
    fn rejects_unknown_namespace() {
        assert!(matches!(
            assert_cosmetic_only(&["wallhack.true"]).unwrap_err(),
            CosmeticOnlyError::Unknown(_)
        ));
    }

    #[test]
    fn empty_tag_list_ok() {
        assert!(assert_cosmetic_only(&[]).is_ok());
    }
}
