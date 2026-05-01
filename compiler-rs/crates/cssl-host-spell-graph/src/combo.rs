// § combo.rs — multi-element combo-system per GDD § COMBO-SYSTEM
// ════════════════════════════════════════════════════════════════════
// 5 default combos :
//   Fire  + Air-Soaked    ⇒ super-evaporate     (×2.0   damage)
//   Frost + Air-Soaked    ⇒ flash-freeze        (×1.5   freeze-duration)
//   Shock + Air-Soaked    ⇒ chain-amplify       (chain-to-5 ¬ 2)
//   Holy  + Void-Curse    ⇒ purge-detonate      (curse-stacks→damage)
//   Earth + Frost-Freeze  ⇒ shatter             (×3.0   damage)
// § I> combos audit-tagged : Audit<"spell-combo", elements>
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

use crate::element::Element;

/// A multi-element combo : two elements that, when both present on the same
/// target-cell, multiply the cast-magnitude by `bonus_multiplier`.
///
/// Storage uses unordered-pair semantics : `(a, b)` matches `(b, a)`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Combo {
    pub elements_required: [Element; 2],
    pub bonus_multiplier: f32,
    pub name: &'static str,
}

impl Combo {
    /// True iff this combo's element-set matches `(a, b)` (order-insensitive).
    #[must_use]
    pub fn matches(&self, a: Element, b: Element) -> bool {
        let ours = sorted(self.elements_required[0], self.elements_required[1]);
        let theirs = sorted(a, b);
        ours == theirs
    }
}

fn sorted(a: Element, b: Element) -> (Element, Element) {
    if a.index() <= b.index() { (a, b) } else { (b, a) }
}

/// All 5 default combos per GDD § COMBO-SYSTEM.
#[must_use]
pub fn default_combos() -> [Combo; 5] {
    [
        Combo {
            elements_required: [Element::Fire, Element::Air],
            bonus_multiplier: 2.0,
            name: "super-evaporate",
        },
        Combo {
            elements_required: [Element::Frost, Element::Air],
            bonus_multiplier: 1.5,
            name: "flash-freeze",
        },
        Combo {
            elements_required: [Element::Shock, Element::Air],
            bonus_multiplier: 1.0, // chain-count ↑ ; raw-damage neutral
            name: "chain-amplify",
        },
        Combo {
            elements_required: [Element::Holy, Element::Void],
            bonus_multiplier: 1.75,
            name: "purge-detonate",
        },
        Combo {
            elements_required: [Element::Earth, Element::Frost],
            bonus_multiplier: 3.0,
            name: "shatter",
        },
    ]
}

/// Search the default combo-list for a matching pair. Returns the first match.
#[must_use]
pub fn find_combo(a: Element, b: Element) -> Option<Combo> {
    default_combos().iter().copied().find(|c| c.matches(a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shatter_combo_matches_either_order() {
        let c = find_combo(Element::Earth, Element::Frost);
        assert!(c.is_some());
        let c2 = find_combo(Element::Frost, Element::Earth);
        assert_eq!(c, c2);
    }

    #[test]
    fn no_match_for_nonexistent_pair() {
        // Fire + Holy is not a default combo
        assert!(find_combo(Element::Fire, Element::Holy).is_none());
    }
}
