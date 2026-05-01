// § cast.rs — cast-mechanics : produce substrate-event-cell + status-effect
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § CAST-MECHANICS § CAST-RESULT :
//   substrate-event-cell stamped into-ω-field
//   Φ-handle ≡ spell-tag (Φ-pattern-pool semantic-id)
//   Σ-mask    ≡ caster-Sovereign-handle ⊕ consent-bits-from-target
//   cssl-host-causal-seed records cast as story-DAG-node (out-of-band ; not here)
// § I> mana-underflow ⇒ cancel-cast (CastResult.success = false ; no cell emitted)
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

use crate::cost::mana_cost;
use crate::graph::SpellGraph;
use crate::mana::ManaPool;
use crate::node::SpellNode;
use crate::status_map::{element_to_status, StatusEffect};
use crate::element::Element;

/// Target descriptor — abstract enough not to couple to a specific game-state.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Caster-asserted consent-bits gleaned from target Σ-mask. `false` skips
    /// the effect-application but still consumes mana per GDD § F-5 :
    /// "target-cell-Σ-revoked ⇒ cast-completes ⊕ effect-skipped + Audit".
    pub consent_present: bool,
    /// Intelligence stat for capacity calculation.
    pub caster_intelligence: u8,
}

/// Substrate-event-cell stamped into ω-field upon successful cast.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EventCell {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub element: Element,
    pub magnitude: f32,
}

/// Cast-result : success-flag + optional substrate-cell + optional status-effect.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CastResult {
    pub success: bool,
    pub substrate_event_cell: Option<EventCell>,
    pub status_effect_applied: Option<StatusEffect>,
}

impl CastResult {
    pub const FAILED: Self = Self {
        success: false,
        substrate_event_cell: None,
        status_effect_applied: None,
    };
}

/// Find the (single) Source element in the graph, or `None`.
fn primary_element(spell: &SpellGraph) -> Option<Element> {
    for n in &spell.nodes {
        if let SpellNode::Source(e) = n {
            return Some(*e);
        }
    }
    None
}

/// Per-modifier magnitude bonus (mirrors cost::modifier_mult shape but applied
/// to magnitude rather than cost).
fn magnitude_multiplier(spell: &SpellGraph) -> f32 {
    use crate::cost::modifier_mult;
    let mut m = 1.0_f32;
    for n in &spell.nodes {
        if let SpellNode::Modifier(k) = n {
            m *= modifier_mult(*k);
        }
    }
    m
}

/// Cast a spell against a target, debiting the mana-pool.
///
/// Returns `CastResult::FAILED` if :
/// - graph fails validation
/// - mana-pool cannot afford the cost
///
/// On success, emits an `EventCell` stamped at target coordinates with the
/// primary-Source element + a magnitude derived from base-1.0 × Modifier-product.
/// If `target.consent_present == false`, the cell is still emitted but the
/// status-effect is suppressed per GDD § F-5.
pub fn cast(spell: &SpellGraph, target: Target, mana: &mut ManaPool) -> CastResult {
    if spell.validate().is_err() {
        return CastResult::FAILED;
    }
    let cost = mana_cost(spell, target.caster_intelligence);
    if !mana.try_consume(cost) {
        return CastResult::FAILED;
    }
    let Some(elem) = primary_element(spell) else {
        return CastResult::FAILED;
    };
    let magnitude = magnitude_multiplier(spell);
    let cell = EventCell {
        x: target.x,
        y: target.y,
        z: target.z,
        element: elem,
        magnitude,
    };
    let status = if target.consent_present {
        Some(element_to_status(elem))
    } else {
        None
    };
    CastResult {
        success: true,
        substrate_event_cell: Some(cell),
        status_effect_applied: status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{ShapeKind, TriggerKind};

    fn minimal_fire_ray() -> SpellGraph {
        let mut g = SpellGraph::new();
        let s = g.add_node(SpellNode::Source(Element::Fire));
        let sh = g.add_node(SpellNode::Shape(ShapeKind::Ray));
        let tr = g.add_node(SpellNode::Trigger(TriggerKind::OnCast));
        g.add_edge(s, sh);
        g.add_edge(sh, tr);
        g
    }

    #[test]
    fn cast_with_consent_applies_status() {
        let g = minimal_fire_ray();
        let mut mana = ManaPool::new(100.0);
        let target = Target { x: 1, y: 2, z: 3, consent_present: true, caster_intelligence: 5 };
        let r = cast(&g, target, &mut mana);
        assert!(r.success);
        assert_eq!(r.status_effect_applied, Some(StatusEffect::Burn));
    }
}
