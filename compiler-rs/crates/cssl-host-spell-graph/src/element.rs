// § element.rs — 8-element roster + affinity-matrix + primary-pair-counters
// ════════════════════════════════════════════════════════════════════
// § I> per GDDs/MAGIC_SYSTEM.csl § ELEMENTAL-AFFINITIES «8»
// § I> roster : Fire Frost Shock Earth Air Holy Void Phase
// § I> matrix : 8×8 attacker-vs-defender ; 1.00 default · 1.50 strong · 0.50 resist · 0.00 cancel
// § I> primary-pair-counters : Fire⊕Frost · Shock⊕Earth · Air⊕Phase · Holy⊕Void
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// 8 elemental affinities. Roster + ordering MUST match GDD § ROSTER.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Element {
    Fire,
    Frost,
    Shock,
    Earth,
    Air,
    Holy,
    Void,
    Phase,
}

impl Element {
    /// All elements in canonical order (matrix-index = enum-discriminant).
    pub const ALL: [Self; 8] = [
        Self::Fire, Self::Frost, Self::Shock, Self::Earth,
        Self::Air,  Self::Holy,  Self::Void,  Self::Phase,
    ];

    /// Stable matrix-index `[0..=7]` for affinity-table lookup.
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Fire  => 0,
            Self::Frost => 1,
            Self::Shock => 2,
            Self::Earth => 3,
            Self::Air   => 4,
            Self::Holy  => 5,
            Self::Void  => 6,
            Self::Phase => 7,
        }
    }
}

/// Affinity-multiplier between attacker → defender pair.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ElementAffinity(pub f32);

/// Build the canonical 8×8 affinity matrix per GDD § AFFINITY-MATRIX.
///
/// `table[attacker.index()][defender.index()]` = multiplier.
///
/// Defaults to 1.00 ; per-cell overrides per GDD :
/// - Fire ↔ Frost   = 0.00 (mutual-cancel)
/// - Shock ↔ Earth  = 0.50 (mutual-resist)
/// - Air → Phase    = 0.50 (Air-resists-Phase  ; spec wording : "Phase-pierces-Air")
/// - Phase → Air    = 1.50 (Phase-pierces-Air)
/// - Holy ↔ Void    = 1.50 (mutual-counter)
#[must_use]
pub fn affinity_table() -> [[f32; 8]; 8] {
    let mut t = [[1.0_f32; 8]; 8];

    // Fire ↔ Frost : mutual-cancel
    t[Element::Fire.index()][Element::Frost.index()] = 0.0;
    t[Element::Frost.index()][Element::Fire.index()] = 0.0;

    // Shock ↔ Earth : mutual-resist
    t[Element::Shock.index()][Element::Earth.index()] = 0.5;
    t[Element::Earth.index()][Element::Shock.index()] = 0.5;

    // Air ↔ Phase : Phase-pierces-Air
    t[Element::Air.index()][Element::Phase.index()]   = 0.5;
    t[Element::Phase.index()][Element::Air.index()]   = 1.5;

    // Holy ↔ Void : mutual-counter (both 1.5)
    t[Element::Holy.index()][Element::Void.index()]   = 1.5;
    t[Element::Void.index()][Element::Holy.index()]   = 1.5;

    t
}

/// Primary-pair-counter list per GDD § PRIMARY-PAIR-COUNTERS «load-bearing».
///
/// Returned as ordered pairs `(a, b)` where `a.index() < b.index()` so callers
/// can compare set-theoretically without ordering ambiguity.
#[must_use]
pub fn primary_pair_counters() -> [(Element, Element); 4] {
    [
        (Element::Fire,  Element::Frost),
        (Element::Shock, Element::Earth),
        (Element::Air,   Element::Phase),
        (Element::Holy,  Element::Void),
    ]
}

/// Lookup multiplier for an `attacker → defender` matchup.
#[must_use]
pub fn affinity_of(attacker: Element, defender: Element) -> f32 {
    affinity_table()[attacker.index()][defender.index()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_elements_have_unique_indices() {
        let mut seen = [false; 8];
        for e in Element::ALL {
            seen[e.index()] = true;
        }
        assert!(seen.iter().all(|s| *s));
    }

    #[test]
    fn default_pair_is_neutral() {
        assert_eq!(affinity_of(Element::Fire, Element::Holy), 1.0);
    }
}
