// § cost.rs — mana-cost formula per GDD § COST-FORMULA
// ════════════════════════════════════════════════════════════════════
// § I> final_cost ≡ Σ(Source-cost × Modifier-mult)
//                  + Σ(Shape-cost) + Σ(Trigger-cost)
//                  ⊓ capacity   (clamp ≤ current-mana)
// § I> capacity = base 100 + Intelligence × 5
// § I> underflow ⇒ caller cancels-cast + audit-row + UI-warn
// ════════════════════════════════════════════════════════════════════

use crate::graph::SpellGraph;
use crate::node::{ConduitKind, ModifierKind, ShapeKind, SpellNode, TriggerKind};

/// Base capacity (GDD § CAPACITY).
pub const BASE_CAPACITY: f32 = 100.0;
/// Per-Intelligence-stat-point bonus (GDD § CAPACITY).
pub const INTELLIGENCE_SCALE: f32 = 5.0;

/// Source per-element base cost.
pub const SOURCE_COST: f32 = 8.0;

/// Shape costs (GDD : higher-area-effect = higher cost).
#[must_use]
pub fn shape_cost(s: ShapeKind) -> f32 {
    match s {
        ShapeKind::Ray        => 4.0,
        ShapeKind::Sphere     => 8.0,
        ShapeKind::Cone       => 6.0,
        ShapeKind::SelfAura   => 5.0,
        ShapeKind::GroundAoe  => 10.0,
        ShapeKind::Projectile => 3.0,
        ShapeKind::Seeking    => 7.0,
    }
}

/// Trigger costs (on-cast cheap ; conditional adds tax).
#[must_use]
pub fn trigger_cost(t: TriggerKind) -> f32 {
    match t {
        TriggerKind::OnCast                  => 1.0,
        TriggerKind::OnImpact                => 2.0,
        TriggerKind::OnConditionLowHp        => 4.0,
        TriggerKind::OnConditionStatusActive => 4.0,
        TriggerKind::OnConditionEnemyClass   => 4.0,
    }
}

/// Modifier multiplicative effect on Source-cost.
#[must_use]
pub fn modifier_mult(m: ModifierKind) -> f32 {
    match m {
        ModifierKind::Amplify     => 1.5,
        ModifierKind::Slow        => 1.1,
        ModifierKind::Pierce      => 1.2,
        ModifierKind::MultiTarget => 1.4,
        ModifierKind::Dot         => 1.3,
    }
}

/// Conduit additive bonus (Gestural-default ⇒ 0 ; physical-conduits-cheaper).
#[must_use]
pub fn conduit_bonus(c: ConduitKind) -> f32 {
    match c {
        ConduitKind::Staff      => -1.0,
        ConduitKind::Focus      => -0.5,
        ConduitKind::Runebook   => 0.0,
        ConduitKind::Voicebound => 1.0,
        ConduitKind::Gestural   => 0.0,
    }
}

/// Compute capacity for a given Intelligence stat.
#[must_use]
pub fn capacity_for(intelligence: u8) -> f32 {
    BASE_CAPACITY + f32::from(intelligence) * INTELLIGENCE_SCALE
}

/// Total mana-cost of a spell. Clamped to capacity.
///
/// Formula : `Σ(Source-cost × Π(Modifier-mult)) + Σ(Shape-cost) + Σ(Trigger-cost) + Σ(Conduit-bonus)`,
/// clamped to `[0, capacity]`. Modifier-mults compose multiplicatively so a
/// 3-modifier stack of (Amplify, Pierce, Dot) produces 1.5*1.2*1.3 = 2.34×.
#[must_use]
pub fn mana_cost(spell: &SpellGraph, intelligence: u8) -> f32 {
    let cap = capacity_for(intelligence);

    let mut source_total = 0.0_f32;
    let mut shape_total = 0.0_f32;
    let mut trigger_total = 0.0_f32;
    let mut conduit_total = 0.0_f32;
    let mut mod_mult = 1.0_f32;

    for n in &spell.nodes {
        match n {
            SpellNode::Source(_)    => source_total  += SOURCE_COST,
            SpellNode::Shape(s)     => shape_total   += shape_cost(*s),
            SpellNode::Trigger(t)   => trigger_total += trigger_cost(*t),
            SpellNode::Modifier(m)  => mod_mult      *= modifier_mult(*m),
            SpellNode::Conduit(c)   => conduit_total += conduit_bonus(*c),
        }
    }

    let raw = source_total
        .mul_add(mod_mult, shape_total + trigger_total + conduit_total);
    raw.clamp(0.0, cap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::Element;
    use crate::node::SpellNode;

    #[test]
    fn capacity_grows_with_intelligence() {
        assert!((capacity_for(0) - 100.0).abs() < 1e-3);
        assert!((capacity_for(10) - 150.0).abs() < 1e-3);
    }

    #[test]
    fn empty_spell_zero_cost() {
        let g = SpellGraph::new();
        assert!((mana_cost(&g, 0) - 0.0).abs() < 1e-3);
    }

    #[test]
    fn single_source_costs_source_constant() {
        let mut g = SpellGraph::new();
        g.add_node(SpellNode::Source(Element::Fire));
        assert!((mana_cost(&g, 0) - SOURCE_COST).abs() < 1e-3);
    }
}
