// § T11-W4-CAUSAL : node ← causal-DAG vertex
// ════════════════════════════════════════════════════════════════════
// § I> CausalNode = ⟨id, kind, label, ts_micros_intent, attrs⟩
// § I> NodeKind ∈ ⟪StoryBeat, WorldState, Actor, Item, Place, Event, Consequence⟫
// § I> attrs sorted on insert + comparison ← determinism-of-serialized-form
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Variant kinds a `CausalNode` can take in the causal-DAG.
///
/// § I> StoryBeat = author-intent (root-cause-edge-source)
/// § I> WorldState = scalar/vector field-snapshot
/// § I> Actor = agent-with-volition · Item = bearer-of-properties
/// § I> Place = spatial-locus · Event = time-instant occurrence
/// § I> Consequence = downstream-effect-leaf
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    StoryBeat,
    WorldState,
    Actor,
    Item,
    Place,
    Event,
    Consequence,
}

/// Causal-DAG node — vertex in story-as-physics graph.
///
/// `attrs` is `Vec<(String,String)>` not `HashMap` ← preserves caller-provided
/// insertion order AND allows duplicate-key shadowing if a caller models that.
/// Sorting policy : caller invokes `with_attr` in deterministic order ;
/// the node never re-orders attrs internally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CausalNode {
    pub id: u64,
    pub kind: NodeKind,
    pub label: String,
    pub ts_micros_intent: u64,
    pub attrs: Vec<(String, String)>,
}

impl CausalNode {
    /// Construct a node with given id+kind+label ; ts=0 ; attrs=∅.
    #[must_use]
    pub fn new(id: u64, kind: NodeKind, label: impl Into<String>) -> Self {
        Self {
            id,
            kind,
            label: label.into(),
            ts_micros_intent: 0,
            attrs: Vec::new(),
        }
    }

    /// Builder : append `(k, v)` to `attrs` ; consumes self for chain-style.
    #[must_use]
    pub fn with_attr(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.attrs.push((k.into(), v.into()));
        self
    }

    /// Builder : set intent-timestamp ; consumes self for chain-style.
    #[must_use]
    pub fn with_ts(mut self, ts_micros: u64) -> Self {
        self.ts_micros_intent = ts_micros;
        self
    }

    /// Lookup first matching attribute by key ; `None` if absent.
    ///
    /// O(n) — attrs is Vec ; n typically ≤ 16 for story-beat metadata.
    #[must_use]
    pub fn attr(&self, k: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find_map(|(kk, vv)| (kk == k).then_some(vv.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_construction_zero_state() {
        let n = CausalNode::new(7, NodeKind::StoryBeat, "open-door");
        assert_eq!(n.id, 7);
        assert_eq!(n.kind, NodeKind::StoryBeat);
        assert_eq!(n.label, "open-door");
        assert_eq!(n.ts_micros_intent, 0);
        assert!(n.attrs.is_empty());
    }

    #[test]
    fn with_attr_chains_and_appends() {
        let n = CausalNode::new(1, NodeKind::Actor, "alice")
            .with_attr("hp", "100")
            .with_attr("loc", "vault");
        assert_eq!(n.attrs.len(), 2);
        assert_eq!(n.attrs[0], ("hp".to_string(), "100".to_string()));
        assert_eq!(n.attrs[1], ("loc".to_string(), "vault".to_string()));
    }

    #[test]
    fn attr_get_returns_first_match_or_none() {
        let n = CausalNode::new(2, NodeKind::Item, "key")
            .with_attr("color", "brass")
            .with_attr("weight_g", "42");
        assert_eq!(n.attr("color"), Some("brass"));
        assert_eq!(n.attr("weight_g"), Some("42"));
        assert_eq!(n.attr("missing"), None);
    }

    #[test]
    fn roundtrip_serde_json_preserves_state() {
        let n = CausalNode::new(99, NodeKind::Consequence, "door-creaks")
            .with_ts(1_234_567)
            .with_attr("volume_db", "62");
        let json = serde_json::to_string(&n).expect("serialize");
        let back: CausalNode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(n, back);
    }

    #[test]
    fn all_node_kinds_distinct() {
        let kinds = [
            NodeKind::StoryBeat,
            NodeKind::WorldState,
            NodeKind::Actor,
            NodeKind::Item,
            NodeKind::Place,
            NodeKind::Event,
            NodeKind::Consequence,
        ];
        // Cross-pair distinctness — every (i,j ; i!=j) must differ.
        for i in 0..kinds.len() {
            for j in 0..kinds.len() {
                if i == j {
                    assert_eq!(kinds[i], kinds[j]);
                } else {
                    assert_ne!(kinds[i], kinds[j]);
                }
            }
        }
        // Sanity : exactly 7 distinct variants.
        assert_eq!(kinds.len(), 7);
    }
}
