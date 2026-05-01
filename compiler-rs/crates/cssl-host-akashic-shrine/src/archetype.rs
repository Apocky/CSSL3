// § archetype : 8 shrine archetypes (≥ 7 required).
// § cosmetic-only · zero gameplay-effect on enum variants.

use serde::{Deserialize, Serialize};

/// § ShrineArchetype enum — purely visual classification of a shrine's
/// physical form. NO gameplay-effect MUST attach to the discriminant.
/// Custom is intentional — user-authored shape (still cosmetic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ShrineArchetype {
    Pillar,
    Altar,
    Reliquary,
    Obelisk,
    Mandala,
    Tree,
    Brazier,
    Custom,
}

impl ShrineArchetype {
    /// All canonical archetypes in declaration-order. Stable.
    pub const ALL: [ShrineArchetype; 8] = [
        ShrineArchetype::Pillar,
        ShrineArchetype::Altar,
        ShrineArchetype::Reliquary,
        ShrineArchetype::Obelisk,
        ShrineArchetype::Mandala,
        ShrineArchetype::Tree,
        ShrineArchetype::Brazier,
        ShrineArchetype::Custom,
    ];

    /// Stable string-tag for serde / log / audit. ¬ user-facing-name.
    pub fn tag(self) -> &'static str {
        match self {
            ShrineArchetype::Pillar    => "pillar",
            ShrineArchetype::Altar     => "altar",
            ShrineArchetype::Reliquary => "reliquary",
            ShrineArchetype::Obelisk   => "obelisk",
            ShrineArchetype::Mandala   => "mandala",
            ShrineArchetype::Tree      => "tree",
            ShrineArchetype::Brazier   => "brazier",
            ShrineArchetype::Custom    => "custom",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_archetypes_distinct() {
        let mut seen = std::collections::BTreeSet::new();
        for a in ShrineArchetype::ALL {
            assert!(seen.insert(a.tag()), "duplicate tag {}", a.tag());
        }
        assert_eq!(seen.len(), 8);
    }

    #[test]
    fn at_least_seven_archetypes() {
        assert!(ShrineArchetype::ALL.len() >= 7, "spec requires ≥ 7 archetypes");
    }

    #[test]
    fn construct_each_variant() {
        let _ = ShrineArchetype::Pillar;
        let _ = ShrineArchetype::Altar;
        let _ = ShrineArchetype::Reliquary;
        let _ = ShrineArchetype::Obelisk;
        let _ = ShrineArchetype::Mandala;
        let _ = ShrineArchetype::Tree;
        let _ = ShrineArchetype::Brazier;
        let _ = ShrineArchetype::Custom;
    }

    #[test]
    fn tag_round_trip() {
        for a in ShrineArchetype::ALL {
            let json = serde_json::to_string(&a).unwrap();
            let back: ShrineArchetype = serde_json::from_str(&json).unwrap();
            assert_eq!(a, back);
        }
    }
}
