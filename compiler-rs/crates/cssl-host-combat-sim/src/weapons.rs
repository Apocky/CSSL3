// § weapons.rs — 8 weapon-archetypes per GDD § WEAPON-ARCHETYPES
// ════════════════════════════════════════════════════════════════════
// § I> table FROZEN ; tuning-numbers may shift @ POD-2 balance-pass
// § I> stats_for(arch) is pure-fn ; const-friendly
// § I> range-class ∈ {Close, Mid, Long, Ranged}
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// 8 weapon-archetypes ; matches GDD table exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WeaponArchetype {
    Sword,
    Axe,
    Spear,
    Dagger,
    Bow,
    Staff,
    Hammer,
    ShieldFist,
}

/// Range classification per GDD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RangeClass {
    Close,
    Mid,
    Long,
    Ranged,
}

/// Special-move identifier ; one per archetype per GDD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpecialMoveId {
    Riposte,
    ArmorBreak,
    ThrustHold,
    Backstab,
    ChargedShot,
    SpellChannel,
    GroundSlam,
    ShieldBash,
}

/// Per-archetype stat block ; FFI-friendly Copy struct.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeaponStats {
    /// Reach in metres (1.4m sword … 30m bow).
    pub reach: f32,
    /// Base damage units (pre-affinity).
    pub base_damage: f32,
    /// Stamina cost per swing.
    pub stamina_cost: f32,
    /// Animation speed multiplier (1.0 = baseline ; 1.5 = dagger fast).
    pub animation_speed: f32,
    /// Range classification.
    pub range_class: RangeClass,
    /// Special-move identifier.
    pub special_move_id: SpecialMoveId,
}

/// Pure-fn stat-table lookup. Numbers FROZEN per GDD § WEAPON-ARCHETYPES.
#[must_use]
pub const fn stats_for(arch: WeaponArchetype) -> WeaponStats {
    match arch {
        WeaponArchetype::Sword => WeaponStats {
            reach: 1.4,
            base_damage: 30.0,
            stamina_cost: 18.0,
            animation_speed: 1.0,
            range_class: RangeClass::Mid,
            special_move_id: SpecialMoveId::Riposte,
        },
        WeaponArchetype::Axe => WeaponStats {
            reach: 1.2,
            base_damage: 42.0,
            stamina_cost: 30.0,
            animation_speed: 0.7,
            range_class: RangeClass::Mid,
            special_move_id: SpecialMoveId::ArmorBreak,
        },
        WeaponArchetype::Spear => WeaponStats {
            reach: 2.4,
            base_damage: 28.0,
            stamina_cost: 20.0,
            animation_speed: 0.9,
            range_class: RangeClass::Long,
            special_move_id: SpecialMoveId::ThrustHold,
        },
        WeaponArchetype::Dagger => WeaponStats {
            reach: 0.6,
            base_damage: 14.0,
            stamina_cost: 10.0,
            animation_speed: 1.5,
            range_class: RangeClass::Close,
            special_move_id: SpecialMoveId::Backstab,
        },
        WeaponArchetype::Bow => WeaponStats {
            reach: 30.0,
            base_damage: 35.0,
            stamina_cost: 22.0,
            animation_speed: 0.8,
            range_class: RangeClass::Ranged,
            special_move_id: SpecialMoveId::ChargedShot,
        },
        WeaponArchetype::Staff => WeaponStats {
            reach: 1.6,
            base_damage: 12.0,
            stamina_cost: 15.0,
            animation_speed: 1.0,
            range_class: RangeClass::Mid,
            special_move_id: SpecialMoveId::SpellChannel,
        },
        WeaponArchetype::Hammer => WeaponStats {
            reach: 1.5,
            base_damage: 55.0,
            stamina_cost: 38.0,
            animation_speed: 0.55,
            range_class: RangeClass::Mid,
            special_move_id: SpecialMoveId::GroundSlam,
        },
        WeaponArchetype::ShieldFist => WeaponStats {
            reach: 0.8,
            base_damage: 10.0,
            stamina_cost: 8.0,
            animation_speed: 1.2,
            range_class: RangeClass::Close,
            special_move_id: SpecialMoveId::ShieldBash,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dagger_fastest_animation_speed() {
        assert!(
            stats_for(WeaponArchetype::Dagger).animation_speed
                > stats_for(WeaponArchetype::Hammer).animation_speed
        );
    }

    #[test]
    fn bow_is_ranged() {
        assert_eq!(stats_for(WeaponArchetype::Bow).range_class, RangeClass::Ranged);
    }

    #[test]
    fn hammer_highest_base_damage() {
        let h = stats_for(WeaponArchetype::Hammer).base_damage;
        for arch in [
            WeaponArchetype::Sword,
            WeaponArchetype::Axe,
            WeaponArchetype::Spear,
            WeaponArchetype::Dagger,
            WeaponArchetype::Bow,
            WeaponArchetype::Staff,
            WeaponArchetype::ShieldFist,
        ] {
            assert!(h >= stats_for(arch).base_damage);
        }
    }
}
