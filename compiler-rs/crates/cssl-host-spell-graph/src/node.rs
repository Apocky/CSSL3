// § node.rs — 5 node-type taxonomy per GDD § SPELL-GRAPH-COMPOSITION
// ════════════════════════════════════════════════════════════════════
// § I> Source     : 1-of-8-elements (Element)
// § I> Modifier   : amplify · slow · pierce · multi-target · DOT
// § I> Shape      : ray · sphere · cone · self-aura · ground-AOE · projectile · seeking
// § I> Trigger    : on-cast · on-impact · on-condition (low-HP · status-active · enemy-class)
// § I> Conduit    : staff · focus · runebook · voicebound · gestural
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

use crate::element::Element;

/// Modifier-node kinds — multiplicative or additive on Source/Shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ModifierKind {
    Amplify,
    Slow,
    Pierce,
    MultiTarget,
    Dot,
}

/// Spell-shape geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ShapeKind {
    Ray,
    Sphere,
    Cone,
    SelfAura,
    GroundAoe,
    Projectile,
    Seeking,
}

/// Trigger-node kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TriggerKind {
    OnCast,
    OnImpact,
    OnConditionLowHp,
    OnConditionStatusActive,
    OnConditionEnemyClass,
}

/// Conduit-node kinds (input modality / required-item).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ConduitKind {
    Staff,
    Focus,
    Runebook,
    Voicebound,
    Gestural,
}

/// One node in a spell-graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SpellNode {
    Source(Element),
    Modifier(ModifierKind),
    Shape(ShapeKind),
    Trigger(TriggerKind),
    Conduit(ConduitKind),
}

/// Node-kind discriminant — useful for grimoire `known_nodes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NodeKind {
    Source,
    Modifier,
    Shape,
    Trigger,
    Conduit,
}

impl SpellNode {
    /// Discriminant of this node.
    #[must_use]
    pub fn kind(&self) -> NodeKind {
        match self {
            Self::Source(_)   => NodeKind::Source,
            Self::Modifier(_) => NodeKind::Modifier,
            Self::Shape(_)    => NodeKind::Shape,
            Self::Trigger(_)  => NodeKind::Trigger,
            Self::Conduit(_)  => NodeKind::Conduit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_classifies_each_variant() {
        assert_eq!(SpellNode::Source(Element::Fire).kind(), NodeKind::Source);
        assert_eq!(SpellNode::Modifier(ModifierKind::Amplify).kind(), NodeKind::Modifier);
        assert_eq!(SpellNode::Shape(ShapeKind::Ray).kind(), NodeKind::Shape);
        assert_eq!(SpellNode::Trigger(TriggerKind::OnCast).kind(), NodeKind::Trigger);
        assert_eq!(SpellNode::Conduit(ConduitKind::Gestural).kind(), NodeKind::Conduit);
    }
}
