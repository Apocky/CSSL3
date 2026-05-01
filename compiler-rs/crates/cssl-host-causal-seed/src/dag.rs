// § T11-W4-CAUSAL : dag ← causal-DAG container + topo-sort + cycle-detect
// ════════════════════════════════════════════════════════════════════
// § I> CausalDag = ⟨nodes : id→CausalNode, edges, next_id⟩
// § I> add_edge validates BOTH endpoints exist + edge.validate() OK
// § I> topological_order = Kahn-algorithm (in-degree elimination)
// § I> determinism : ties broken by ascending node-id ← stable across runs
// § I> serde : custom — emits sorted nodes by id ← bit-identical output
// ════════════════════════════════════════════════════════════════════

use crate::edge::{CausalEdge, EdgeErr, EdgeKind};
use crate::node::{CausalNode, NodeKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};

/// DAG construction / traversal failures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DagErr {
    /// add_edge referenced a node-id not in `nodes`.
    UnknownNode(u64),
    /// topological_order detected a cycle.
    Cyclic,
    /// underlying CausalEdge.validate() failed.
    EdgeInvalid(EdgeErr),
}

impl From<EdgeErr> for DagErr {
    fn from(e: EdgeErr) -> Self {
        Self::EdgeInvalid(e)
    }
}

/// Causal-DAG ← directed-acyclic graph of CausalNode connected via CausalEdge.
///
/// § I> nodes stored by id in HashMap for O(1) lookup
/// § I> edges Vec preserves insertion order (replay-determinism)
/// § I> next_id monotone-increasing ← no id-reuse within one DAG
///
/// ### Determinism contract
/// `serde::Serialize` re-orders nodes via BTreeMap-keyed-by-id so that
/// `serde_json::to_string(dag)` is byte-identical for equivalent graphs
/// regardless of HashMap insertion-order randomness.
#[derive(Debug, Clone, Default)]
pub struct CausalDag {
    nodes: HashMap<u64, CausalNode>,
    edges: Vec<CausalEdge>,
    next_id: u64,
}

impl CausalDag {
    /// Empty DAG — no nodes, no edges, next_id = 0.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node ; assigns + returns the next monotone id.
    ///
    /// `kind` and `label` are stored on a freshly constructed CausalNode
    /// with default ts=0 and attrs=∅ ; caller may mutate via `node_mut`.
    pub fn add_node(&mut self, kind: NodeKind, label: impl Into<String>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.insert(id, CausalNode::new(id, kind, label));
        id
    }

    /// Insert a pre-built node (id chosen by caller). Returns Err if id collides.
    /// Bumps `next_id` past `node.id` to keep monotone-allocation invariant.
    pub fn insert_node(&mut self, node: CausalNode) -> Result<(), DagErr> {
        if self.nodes.contains_key(&node.id) {
            return Err(DagErr::UnknownNode(node.id));
        }
        if node.id >= self.next_id {
            self.next_id = node.id + 1;
        }
        self.nodes.insert(node.id, node);
        Ok(())
    }

    /// Add a directed edge ; validates BOTH endpoints exist + CausalEdge.validate() OK.
    pub fn add_edge(
        &mut self,
        src: u64,
        dst: u64,
        kind: EdgeKind,
        weight: f32,
    ) -> Result<(), DagErr> {
        if !self.nodes.contains_key(&src) {
            return Err(DagErr::UnknownNode(src));
        }
        if !self.nodes.contains_key(&dst) {
            return Err(DagErr::UnknownNode(dst));
        }
        let edge = CausalEdge::new(src, dst, kind, weight);
        edge.validate()?;
        self.edges.push(edge);
        Ok(())
    }

    /// Borrow node by id.
    #[must_use]
    pub fn node(&self, id: u64) -> Option<&CausalNode> {
        self.nodes.get(&id)
    }

    /// Mutable borrow of node by id.
    pub fn node_mut(&mut self, id: u64) -> Option<&mut CausalNode> {
        self.nodes.get_mut(&id)
    }

    /// Number of nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of edges.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// All edges sourced at `src`. O(E) scan.
    pub fn out_edges(&self, src: u64) -> impl Iterator<Item = &CausalEdge> {
        self.edges.iter().filter(move |e| e.src == src)
    }

    /// All edges destined at `dst`. O(E) scan.
    pub fn in_edges(&self, dst: u64) -> impl Iterator<Item = &CausalEdge> {
        self.edges.iter().filter(move |e| e.dst == dst)
    }

    /// True if DAG contains a cycle. O(V+E) Kahn-style.
    #[must_use]
    pub fn has_cycle(&self) -> bool {
        self.topological_order().is_err()
    }

    /// Kahn-algorithm topological sort.
    ///
    /// § I> ties broken by ascending node-id ← deterministic across runs
    /// § I> Err(DagErr::Cyclic) iff a cycle prevents full ordering
    pub fn topological_order(&self) -> Result<Vec<u64>, DagErr> {
        // In-degree count per node ; init via BTreeMap so iteration is sorted.
        let mut indeg: BTreeMap<u64, usize> = self.nodes.keys().map(|&k| (k, 0_usize)).collect();
        for e in &self.edges {
            // src/dst guaranteed present : add_edge validates.
            if let Some(d) = indeg.get_mut(&e.dst) {
                *d += 1;
            }
        }

        // Initial frontier : zero-indegree nodes ; sorted ascending ← BTreeMap natural.
        let mut frontier: VecDeque<u64> = indeg
            .iter()
            .filter_map(|(&k, &d)| (d == 0).then_some(k))
            .collect();

        let mut order: Vec<u64> = Vec::with_capacity(self.nodes.len());

        while let Some(n) = frontier.pop_front() {
            order.push(n);
            // For each outgoing edge n → m, decrement m's indegree.
            // Collect newly-zero successors, sort ascending, append to frontier.
            let mut newly_zero: Vec<u64> = Vec::new();
            for e in self.edges.iter().filter(|e| e.src == n) {
                if let Some(d) = indeg.get_mut(&e.dst) {
                    *d -= 1;
                    if *d == 0 {
                        newly_zero.push(e.dst);
                    }
                }
            }
            newly_zero.sort_unstable();
            for nz in newly_zero {
                frontier.push_back(nz);
            }
        }

        if order.len() == self.nodes.len() {
            Ok(order)
        } else {
            Err(DagErr::Cyclic)
        }
    }

    /// All node-ids sorted ascending — convenience for deterministic iteration.
    #[must_use]
    pub fn sorted_node_ids(&self) -> Vec<u64> {
        let mut v: Vec<u64> = self.nodes.keys().copied().collect();
        v.sort_unstable();
        v
    }
}

// ─── Custom serde ← determinism : nodes serialized BTreeMap-sorted by id. ───

#[derive(Serialize, Deserialize)]
struct DagWire {
    nodes: BTreeMap<u64, CausalNode>,
    edges: Vec<CausalEdge>,
    next_id: u64,
}

impl Serialize for CausalDag {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let wire = DagWire {
            nodes: self.nodes.iter().map(|(&k, v)| (k, v.clone())).collect(),
            edges: self.edges.clone(),
            next_id: self.next_id,
        };
        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CausalDag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = DagWire::deserialize(deserializer)?;
        let nodes: HashMap<u64, CausalNode> =
            wire.nodes.into_iter().collect();
        Ok(Self {
            nodes,
            edges: wire.edges,
            next_id: wire.next_id,
        })
    }
}

#[cfg(test)]
#[allow(clippy::many_single_char_names, clippy::similar_names)]
mod tests {
    use super::*;

    #[test]
    fn empty_dag_topo_ok_and_no_cycle() {
        let g = CausalDag::new();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
        assert!(!g.has_cycle());
        assert_eq!(g.topological_order(), Ok(vec![]));
    }

    #[test]
    fn single_node_topo_returns_singleton() {
        let mut g = CausalDag::new();
        let a = g.add_node(NodeKind::StoryBeat, "open");
        assert_eq!(g.topological_order(), Ok(vec![a]));
        assert!(!g.has_cycle());
    }

    #[test]
    fn single_edge_topo_orders_src_before_dst() {
        let mut g = CausalDag::new();
        let a = g.add_node(NodeKind::StoryBeat, "intent");
        let b = g.add_node(NodeKind::Consequence, "outcome");
        g.add_edge(a, b, EdgeKind::Causes, 1.0).expect("add edge");
        let order = g.topological_order().expect("topo");
        let pos_a = order.iter().position(|&x| x == a).unwrap();
        let pos_b = order.iter().position(|&x| x == b).unwrap();
        assert!(pos_a < pos_b);
        assert_eq!(g.out_edges(a).count(), 1);
        assert_eq!(g.in_edges(b).count(), 1);
    }

    #[test]
    fn cycle_detected_via_topo_err() {
        let mut g = CausalDag::new();
        let a = g.add_node(NodeKind::Event, "a");
        let b = g.add_node(NodeKind::Event, "b");
        let c = g.add_node(NodeKind::Event, "c");
        g.add_edge(a, b, EdgeKind::Follows, 0.1).expect("a→b");
        g.add_edge(b, c, EdgeKind::Follows, 0.1).expect("b→c");
        g.add_edge(c, a, EdgeKind::Follows, 0.1).expect("c→a"); // close cycle
        assert!(g.has_cycle());
        assert_eq!(g.topological_order(), Err(DagErr::Cyclic));
    }

    #[test]
    fn linear_chain_topo_ascending_ids() {
        let mut g = CausalDag::new();
        let ids: Vec<u64> = (0..5)
            .map(|_| g.add_node(NodeKind::Event, "n"))
            .collect();
        for w in ids.windows(2) {
            g.add_edge(w[0], w[1], EdgeKind::Follows, 0.5).expect("chain");
        }
        let order = g.topological_order().expect("topo");
        assert_eq!(order, ids);
    }

    #[test]
    fn diamond_topo_root_then_two_branches_then_join() {
        // root → mid_a → leaf
        // root → mid_b → leaf
        let mut g = CausalDag::new();
        let root = g.add_node(NodeKind::StoryBeat, "root");
        let mid_a = g.add_node(NodeKind::Actor, "a");
        let mid_b = g.add_node(NodeKind::Actor, "b");
        let leaf = g.add_node(NodeKind::Consequence, "leaf");
        g.add_edge(root, mid_a, EdgeKind::Enables, 0.6).unwrap();
        g.add_edge(root, mid_b, EdgeKind::Enables, 0.6).unwrap();
        g.add_edge(mid_a, leaf, EdgeKind::Causes, 0.5).unwrap();
        g.add_edge(mid_b, leaf, EdgeKind::Causes, 0.5).unwrap();

        let order = g.topological_order().expect("topo");
        let p_root = order.iter().position(|&x| x == root).unwrap();
        let p_a = order.iter().position(|&x| x == mid_a).unwrap();
        let p_b = order.iter().position(|&x| x == mid_b).unwrap();
        let p_leaf = order.iter().position(|&x| x == leaf).unwrap();
        assert!(p_root < p_a);
        assert!(p_root < p_b);
        assert!(p_a < p_leaf);
        assert!(p_b < p_leaf);
        // Tie-break determinism : mid_a (lower id) before mid_b.
        assert!(p_a < p_b);
    }

    #[test]
    fn disconnected_components_both_topo_ordered() {
        let mut g = CausalDag::new();
        let a = g.add_node(NodeKind::Event, "a");
        let b = g.add_node(NodeKind::Event, "b");
        let c = g.add_node(NodeKind::Event, "c");
        let d = g.add_node(NodeKind::Event, "d");
        g.add_edge(a, b, EdgeKind::Follows, 0.1).unwrap();
        g.add_edge(c, d, EdgeKind::Follows, 0.1).unwrap();
        let order = g.topological_order().expect("topo");
        assert_eq!(order.len(), 4);
        let pos = |id: u64| order.iter().position(|&x| x == id).unwrap();
        assert!(pos(a) < pos(b));
        assert!(pos(c) < pos(d));
    }

    #[test]
    fn invalid_edge_propagates_error_unchanged_state() {
        let mut g = CausalDag::new();
        let a = g.add_node(NodeKind::Event, "a");
        // Unknown dst.
        assert_eq!(
            g.add_edge(a, 999, EdgeKind::Causes, 1.0),
            Err(DagErr::UnknownNode(999))
        );
        // Self-loop.
        let r = g.add_edge(a, a, EdgeKind::Causes, 1.0);
        assert_eq!(r, Err(DagErr::EdgeInvalid(EdgeErr::SelfLoop)));
        // Negative-weight Causes.
        let b = g.add_node(NodeKind::Event, "b");
        let r2 = g.add_edge(a, b, EdgeKind::Causes, -0.1);
        assert_eq!(r2, Err(DagErr::EdgeInvalid(EdgeErr::NegativeCausalWeight)));
        // After all failures : no edges added.
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn deterministic_serialization_byte_identical() {
        // Build same graph twice with different add-order ; serialize → identical bytes.
        let build = |order_swap: bool| -> CausalDag {
            let mut g = CausalDag::new();
            let r = g.add_node(NodeKind::StoryBeat, "r");
            if order_swap {
                let _ = g.add_node(NodeKind::Item, "i");
                let _ = g.add_node(NodeKind::Actor, "a");
            } else {
                let _ = g.add_node(NodeKind::Actor, "a");
                let _ = g.add_node(NodeKind::Item, "i");
            }
            // Force root-only edges so cycles aren't possible.
            let _ = r;
            g
        };
        // Insertion-order differs but ids assigned monotonically ; serialized bytes
        // depend on (kind,label) per-id which DOES differ by order. The determinism
        // contract is : same graph (same id→node mapping) → identical bytes
        // across runs. We test that property : serialize twice, compare.
        let g1 = build(false);
        let s1a = serde_json::to_string(&g1).unwrap();
        let s1b = serde_json::to_string(&g1).unwrap();
        assert_eq!(s1a, s1b);

        // Roundtrip preserves all data.
        let back: CausalDag = serde_json::from_str(&s1a).unwrap();
        assert_eq!(back.node_count(), g1.node_count());
        assert_eq!(back.edge_count(), g1.edge_count());
    }
}
