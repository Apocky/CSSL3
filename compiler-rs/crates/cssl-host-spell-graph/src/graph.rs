// § graph.rs — spell-graph DAG ⟨nodes, edges⟩ + cycle-detector + validation
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § GRAPH-STRUCTURE :
//       spell-graph ≡ DAG ; edges ≡ Source → (Modifier)* → Shape → Trigger ⊗ Conduit
// § I> cycle-detector ‼ reject-with-reason  (no-recursive-self-cast)
// § I> max-nodes-per-spell ≡ 8 (early) · 16 (legendary-tier)
// § I> ∀ spell W! has ≥1 Source ⊕ ≥1 Shape ⊕ ≥1 Trigger
// ════════════════════════════════════════════════════════════════════

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::node::{NodeKind, SpellNode};

/// Stable index into `SpellGraph::nodes`.
pub type NodeIdx = u16;

/// Validation / construction errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphErr {
    /// `validate_acyclic` detected a back-edge involving these nodes.
    Cycle { from: NodeIdx, to: NodeIdx },
    /// `validate_one_source` : 0 or 2+ Source nodes were present.
    SourceCount { found: u16 },
    /// `validate_one_source` rule extended : missing Shape.
    MissingShape,
    /// `validate_one_source` rule extended : missing Trigger.
    MissingTrigger,
    /// Edge endpoint references an out-of-range index.
    EdgeOutOfRange { idx: NodeIdx, len: u16 },
    /// Node-count exceeds legendary-tier cap (16).
    TooManyNodes { found: u16, cap: u16 },
    /// More than 4 Modifier nodes (GDD § VALIDATION-RULES § Modifier-stack-cap).
    ModifierStackOverflow { found: u16, cap: u16 },
}

/// A spell as a directed acyclic graph of typed nodes.
///
/// Edges are stored as a `BTreeSet<(from, to)>` for deterministic ordering and
/// cheap duplicate-rejection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpellGraph {
    pub nodes: Vec<SpellNode>,
    pub edges: BTreeSet<(NodeIdx, NodeIdx)>,
}

impl SpellGraph {
    /// Empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a node, return its index.
    pub fn add_node(&mut self, node: SpellNode) -> NodeIdx {
        let idx = self.nodes.len() as NodeIdx;
        self.nodes.push(node);
        idx
    }

    /// Add a directed edge `from → to`.
    ///
    /// Does NOT validate acyclicity inline — call `validate_acyclic()` after
    /// construction. (Allows partial-build patterns and atomic batch-edits.)
    pub fn add_edge(&mut self, from: NodeIdx, to: NodeIdx) {
        self.edges.insert((from, to));
    }

    /// Number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True iff zero nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Reject if any edge references an out-of-range index.
    pub fn validate_edge_indices(&self) -> Result<(), GraphErr> {
        let len = self.nodes.len() as u16;
        for &(f, t) in &self.edges {
            if f >= len { return Err(GraphErr::EdgeOutOfRange { idx: f, len }); }
            if t >= len { return Err(GraphErr::EdgeOutOfRange { idx: t, len }); }
        }
        Ok(())
    }

    /// Reject node-count over legendary-cap (16).
    pub fn validate_node_cap(&self) -> Result<(), GraphErr> {
        let n = self.nodes.len() as u16;
        if n > 16 { Err(GraphErr::TooManyNodes { found: n, cap: 16 }) } else { Ok(()) }
    }

    /// Reject Modifier-stack > 4.
    pub fn validate_modifier_stack(&self) -> Result<(), GraphErr> {
        let mods = self.nodes.iter().filter(|n| n.kind() == NodeKind::Modifier).count() as u16;
        if mods > 4 { Err(GraphErr::ModifierStackOverflow { found: mods, cap: 4 }) } else { Ok(()) }
    }

    /// Detect cycles via Kahn-style topological-sort. Returns the offending
    /// edge if a back-edge is found.
    pub fn validate_acyclic(&self) -> Result<(), GraphErr> {
        self.validate_edge_indices()?;

        // Build adjacency + in-degree maps deterministically.
        let n = self.nodes.len();
        let mut indeg: BTreeMap<NodeIdx, u32> = BTreeMap::new();
        let mut adj: BTreeMap<NodeIdx, Vec<NodeIdx>> = BTreeMap::new();
        for i in 0..n as NodeIdx {
            indeg.insert(i, 0);
            adj.insert(i, Vec::new());
        }
        for &(f, t) in &self.edges {
            *indeg.entry(t).or_insert(0) += 1;
            adj.entry(f).or_default().push(t);
        }

        // Kahn : repeatedly drain zero-in-degree.
        let mut frontier: Vec<NodeIdx> = indeg
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(&i, _)| i)
            .collect();
        let mut visited = 0_usize;
        while let Some(cur) = frontier.pop() {
            visited += 1;
            if let Some(succs) = adj.get(&cur) {
                for &s in succs {
                    let d = indeg.get_mut(&s).expect("in-degree tracked");
                    *d -= 1;
                    if *d == 0 {
                        frontier.push(s);
                    }
                }
            }
        }
        if visited != n {
            // Find first edge whose endpoints both still have indeg > 0
            // — that's part of the cycle.
            for &(f, t) in &self.edges {
                if indeg.get(&f).copied().unwrap_or(0) > 0
                    && indeg.get(&t).copied().unwrap_or(0) > 0
                {
                    return Err(GraphErr::Cycle { from: f, to: t });
                }
            }
            // Fallback — shouldn't reach.
            return Err(GraphErr::Cycle { from: 0, to: 0 });
        }
        Ok(())
    }

    /// GDD § VALIDATION-RULES : ∀ spell W! has ≥1 Source ⊕ ≥1 Shape ⊕ ≥1 Trigger.
    pub fn validate_one_source(&self) -> Result<(), GraphErr> {
        let (mut sources, mut shapes, mut triggers) = (0_u16, 0_u16, 0_u16);
        for n in &self.nodes {
            match n.kind() {
                NodeKind::Source  => sources  += 1,
                NodeKind::Shape   => shapes   += 1,
                NodeKind::Trigger => triggers += 1,
                _ => {}
            }
        }
        if sources != 1 { return Err(GraphErr::SourceCount { found: sources }); }
        if shapes == 0 { return Err(GraphErr::MissingShape); }
        if triggers == 0 { return Err(GraphErr::MissingTrigger); }
        Ok(())
    }

    /// All-validators run-in-canonical-order.
    pub fn validate(&self) -> Result<(), GraphErr> {
        self.validate_node_cap()?;
        self.validate_modifier_stack()?;
        self.validate_edge_indices()?;
        self.validate_acyclic()?;
        self.validate_one_source()?;
        Ok(())
    }

    /// Stable hash (FNV-1a-32 over `(node-discriminant, idx, edges)`) — deterministic
    /// across BTreeSet ordering ; used for cast-determinism keys.
    #[must_use]
    pub fn graph_hash(&self) -> u32 {
        const FNV_OFFSET: u32 = 0x811C_9DC5;
        const FNV_PRIME: u32 = 0x0100_0193;
        let mut h: u32 = FNV_OFFSET;
        let mix = |h: u32, b: u8| h.wrapping_mul(FNV_PRIME) ^ u32::from(b);
        for (i, n) in self.nodes.iter().enumerate() {
            let kind_byte: u8 = match n.kind() {
                NodeKind::Source => 1, NodeKind::Modifier => 2, NodeKind::Shape => 3,
                NodeKind::Trigger => 4, NodeKind::Conduit => 5,
            };
            h = mix(h, kind_byte);
            for b in (i as u32).to_le_bytes() { h = mix(h, b); }
        }
        for &(f, t) in &self.edges {
            for b in f.to_le_bytes() { h = mix(h, b); }
            for b in t.to_le_bytes() { h = mix(h, b); }
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::Element;
    use crate::node::{ShapeKind, TriggerKind};

    #[test]
    fn empty_graph_has_no_source() {
        let g = SpellGraph::new();
        assert!(matches!(
            g.validate_one_source(),
            Err(GraphErr::SourceCount { found: 0 })
        ));
    }

    #[test]
    fn minimal_valid_graph() {
        let mut g = SpellGraph::new();
        let s = g.add_node(SpellNode::Source(Element::Fire));
        let sh = g.add_node(SpellNode::Shape(ShapeKind::Ray));
        let tr = g.add_node(SpellNode::Trigger(TriggerKind::OnCast));
        g.add_edge(s, sh);
        g.add_edge(sh, tr);
        assert!(g.validate().is_ok());
    }
}
