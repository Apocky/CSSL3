// § status_map.rs — element → status-effect map per GDD § ELEMENT-TO-STATUS-MAP
// ════════════════════════════════════════════════════════════════════
// Fire    → Burn      (DOT 3s · stack-to-3)
// Frost   → Freeze    (immobile 1.5s · brittle-bonus)
// Shock   → Stun      (action-cancel 0.75s · chain-to-2-targets)
// Earth   → Petrify   (immobile 2s · armor-up · damage-down)
// Air     → Soaked    (move-buff · electric-amplify)
// Holy    → Mark      (next-hit +50% · purge-buffs)
// Void    → Curse     (heal-reduced · drain-to-caster)
// Phase   → Phased    (intangible 0.5s · ¬ stagger ⊕ ¬ block)
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

use crate::element::Element;

/// Status-effects produced by elemental impact. Variant order MUST match the
/// GDD element-to-status table exactly so Apocky-canonical balance-tests
/// remain reproducible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StatusEffect {
    Burn,
    Freeze,
    Stun,
    Petrify,
    Soaked,
    Mark,
    Curse,
    Phased,
}

/// Map element → its canonical status-effect per GDD.
#[must_use]
pub fn element_to_status(e: Element) -> StatusEffect {
    match e {
        Element::Fire  => StatusEffect::Burn,
        Element::Frost => StatusEffect::Freeze,
        Element::Shock => StatusEffect::Stun,
        Element::Earth => StatusEffect::Petrify,
        Element::Air   => StatusEffect::Soaked,
        Element::Holy  => StatusEffect::Mark,
        Element::Void  => StatusEffect::Curse,
        Element::Phase => StatusEffect::Phased,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_burns() {
        assert_eq!(element_to_status(Element::Fire), StatusEffect::Burn);
    }

    #[test]
    fn all_eight_elements_map_distinctly() {
        let mut seen = std::collections::BTreeSet::new();
        for e in Element::ALL {
            seen.insert(element_to_status(e));
        }
        assert_eq!(seen.len(), 8);
    }
}
