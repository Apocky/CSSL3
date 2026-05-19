#![forbid(unsafe_code)]
#![doc = "cssl-hgraph — typed hypergraph IR.\n\n\
Spec: `specs/Upgrade/impl/IMPL_02_FOUNDATION.csl` § cssl-hgraph. \
Nodes are typed values ; ports are typed edge-attachment-points ; hyperedges are \
n-ary typed relations between ports. Every graph has a canonical `Cid` derived \
from a deterministic encoding of (sorted nodes, sorted edges)."]

use cssl_cas::{cid_of_bytes, CanonicalEncode, Cid};
use smallvec::SmallVec;
use thiserror::Error;

/// Newtype for type-Cids ; distinguishes from value-Cids in APIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeCid(pub Cid);

/// Hypergraph node identifier (sequential within a single graph).
pub type NodeId = u32;
/// Hypergraph hyperedge identifier (sequential within a single graph).
pub type EdgeId = u32;
/// Port index within a node (0-based).
pub type PortIdx = u16;

/// A node in the hypergraph : typed value with a label.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Node {
    pub id: NodeId,
    pub type_cid: TypeCid,
    pub label: NodeLabel,
}

/// Initial small label-set ; extensible via `Custom`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NodeLabel {
    /// A leaf node carrying an opaque payload string (interned externally).
    Leaf(String),
    /// Function-application node.
    App,
    /// λ-abstraction node.
    Lam,
    /// Custom label for domain extensions.
    Custom(String),
}

impl NodeLabel {
    fn tag(&self) -> u8 {
        match self {
            Self::Leaf(_) => 0,
            Self::App => 1,
            Self::Lam => 2,
            Self::Custom(_) => 3,
        }
    }
}

/// A typed port : `(node, idx)` attachment point with a type-Cid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Port {
    pub node: NodeId,
    pub idx: PortIdx,
    pub type_cid: TypeCid,
}

/// A hyperedge : kind + ordered list of ports.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HEdge {
    pub id: EdgeId,
    pub kind: EdgeKind,
    pub ports: SmallVec<[Port; 4]>,
}

/// Kinds of hyperedges this IR recognizes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    /// Data flows from port[0] to port[1..].
    DataFlow,
    /// Subterm relation : port[0] is the parent, port[1..] are children.
    Subterm,
    /// Effect-scope binding : port[0] is the handler, port[1..] are scoped operations.
    EffectScope,
    /// Grade annotation : port[0] is the bearer, payload identifies the grade.
    GradeAnnot(String),
    /// Custom edge for domain extensions.
    Custom(String),
}

impl EdgeKind {
    fn tag(&self) -> u8 {
        match self {
            Self::DataFlow => 0,
            Self::Subterm => 1,
            Self::EffectScope => 2,
            Self::GradeAnnot(_) => 3,
            Self::Custom(_) => 4,
        }
    }
}

/// Errors raised when constructing or mutating a hypergraph.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HGraphError {
    /// A port refers to a node that is not present in this graph.
    #[error("port references unknown node id {0}")]
    DanglingPort(NodeId),
    /// An edge was constructed with zero ports.
    #[error("hyperedge must have at least one port")]
    EmptyEdge,
}

/// A typed hypergraph.
#[derive(Clone, Debug, Default)]
pub struct HGraph {
    nodes: Vec<Node>,
    edges: Vec<HEdge>,
}

impl HGraph {
    /// Create an empty hypergraph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a node ; returns the new `NodeId`.
    pub fn add_node(&mut self, type_cid: TypeCid, label: NodeLabel) -> NodeId {
        let id = self.nodes.len() as NodeId;
        self.nodes.push(Node { id, type_cid, label });
        id
    }

    /// Insert a hyperedge ; validates port-node references.
    pub fn add_edge(&mut self, kind: EdgeKind, ports: &[Port]) -> Result<EdgeId, HGraphError> {
        if ports.is_empty() {
            return Err(HGraphError::EmptyEdge);
        }
        let node_count = self.nodes.len() as NodeId;
        for p in ports {
            if p.node >= node_count {
                return Err(HGraphError::DanglingPort(p.node));
            }
        }
        let id = self.edges.len() as EdgeId;
        self.edges.push(HEdge {
            id,
            kind,
            ports: SmallVec::from_slice(ports),
        });
        Ok(id)
    }

    /// Iterate nodes in insertion order.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> + '_ {
        self.nodes.iter()
    }

    /// Iterate hyperedges in insertion order.
    pub fn edges(&self) -> impl Iterator<Item = &HEdge> + '_ {
        self.edges.iter()
    }

    /// Number of nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of hyperedges.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Compute the canonical content-Cid of this hypergraph.
    ///
    /// Encoding is order-stable across insertion orders by sorting nodes and edges
    /// before encoding. Two hypergraphs with the same node-set and edge-set
    /// (up to the canonical sort) produce identical Cids.
    #[must_use]
    pub fn cid(&self) -> Cid {
        let mut buf = Vec::new();
        self.encode(&mut buf);
        cid_of_bytes(&buf)
    }
}

impl CanonicalEncode for HGraph {
    fn encode(&self, out: &mut Vec<u8>) {
        // Canonical encoding : nodes sorted by (type_cid, label-tag, label-payload, original-id) ;
        // node IDs are then RENUMBERED to sort-position so that edges (which reference
        // node IDs) are content-addressed independently of insertion order. Edges are
        // sorted lexicographically over (kind-tag, kind-payload, renumbered-port-tuple).
        let mut node_indices: Vec<usize> = (0..self.nodes.len()).collect();
        node_indices.sort_by(|&i, &j| {
            let a = &self.nodes[i];
            let b = &self.nodes[j];
            a.type_cid
                .cmp(&b.type_cid)
                .then_with(|| a.label.tag().cmp(&b.label.tag()))
                .then_with(|| label_payload(&a.label).cmp(label_payload(&b.label)))
                .then_with(|| a.id.cmp(&b.id))
        });
        // Build old-id → canonical-index permutation.
        let mut canon_id = vec![0u32; self.nodes.len()];
        for (canon, &orig_idx) in node_indices.iter().enumerate() {
            canon_id[self.nodes[orig_idx].id as usize] = canon as u32;
        }

        out.extend_from_slice(&(self.nodes.len() as u64).to_le_bytes());
        for &i in &node_indices {
            let n = &self.nodes[i];
            n.type_cid.0.encode(out);
            out.push(n.label.tag());
            label_payload(&n.label).encode(out);
        }

        // Renumber ports + sort edges by canonicalized content.
        let mut canon_edges: Vec<(u8, &str, Vec<(u32, u16, Cid)>)> = self
            .edges
            .iter()
            .map(|e| {
                let ports: Vec<(u32, u16, Cid)> = e
                    .ports
                    .iter()
                    .map(|p| (canon_id[p.node as usize], p.idx, p.type_cid.0))
                    .collect();
                (e.kind.tag(), edge_kind_payload(&e.kind), ports)
            })
            .collect();
        canon_edges.sort();

        out.extend_from_slice(&(canon_edges.len() as u64).to_le_bytes());
        for (tag, payload, ports) in canon_edges {
            out.push(tag);
            payload.encode(out);
            out.extend_from_slice(&(ports.len() as u64).to_le_bytes());
            for (node, idx, type_cid) in ports {
                out.extend_from_slice(&node.to_le_bytes());
                out.extend_from_slice(&idx.to_le_bytes());
                type_cid.encode(out);
            }
        }
    }
}

fn label_payload(l: &NodeLabel) -> &str {
    match l {
        NodeLabel::Leaf(s) | NodeLabel::Custom(s) => s.as_str(),
        _ => "",
    }
}

fn edge_kind_payload(k: &EdgeKind) -> &str {
    match k {
        EdgeKind::GradeAnnot(s) | EdgeKind::Custom(s) => s.as_str(),
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(seed: u8) -> TypeCid {
        TypeCid(cssl_cas::cid_of_bytes(&[seed]))
    }

    #[test]
    fn empty_graph_has_zero_nodes_and_edges() {
        let g = HGraph::new();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn add_node_returns_sequential_ids() {
        let mut g = HGraph::new();
        let a = g.add_node(t(1), NodeLabel::App);
        let b = g.add_node(t(2), NodeLabel::Lam);
        let c = g.add_node(t(3), NodeLabel::Leaf("x".into()));
        assert_eq!((a, b, c), (0, 1, 2));
    }

    #[test]
    fn add_edge_succeeds_when_ports_reference_valid_nodes() {
        let mut g = HGraph::new();
        let n0 = g.add_node(t(1), NodeLabel::App);
        let n1 = g.add_node(t(1), NodeLabel::App);
        let p0 = Port { node: n0, idx: 0, type_cid: t(1) };
        let p1 = Port { node: n1, idx: 0, type_cid: t(1) };
        let e = g.add_edge(EdgeKind::DataFlow, &[p0, p1]).unwrap();
        assert_eq!(e, 0);
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn add_edge_rejects_dangling_port() {
        let mut g = HGraph::new();
        let n0 = g.add_node(t(1), NodeLabel::App);
        let valid = Port { node: n0, idx: 0, type_cid: t(1) };
        let dangling = Port { node: 99, idx: 0, type_cid: t(1) };
        let err = g.add_edge(EdgeKind::DataFlow, &[valid, dangling]).unwrap_err();
        assert_eq!(err, HGraphError::DanglingPort(99));
    }

    #[test]
    fn add_edge_rejects_empty_port_list() {
        let mut g = HGraph::new();
        let err = g.add_edge(EdgeKind::DataFlow, &[]).unwrap_err();
        assert_eq!(err, HGraphError::EmptyEdge);
    }

    #[test]
    fn graph_cid_deterministic_under_same_construction() {
        let mut g1 = HGraph::new();
        g1.add_node(t(1), NodeLabel::App);
        let mut g2 = HGraph::new();
        g2.add_node(t(1), NodeLabel::App);
        assert_eq!(g1.cid(), g2.cid());
    }

    #[test]
    fn graph_cid_changes_when_node_added() {
        let mut g = HGraph::new();
        let c0 = g.cid();
        g.add_node(t(1), NodeLabel::App);
        let c1 = g.cid();
        assert_ne!(c0, c1);
    }

    #[test]
    fn isomorphic_graphs_via_canonical_sort_have_same_cid() {
        // Two graphs with same multiset of nodes/edges but different insertion
        // order should canonicalize to the same Cid.
        let mut g1 = HGraph::new();
        let a1 = g1.add_node(t(1), NodeLabel::App);
        let b1 = g1.add_node(t(2), NodeLabel::Lam);
        g1.add_edge(EdgeKind::DataFlow, &[
            Port { node: a1, idx: 0, type_cid: t(1) },
            Port { node: b1, idx: 0, type_cid: t(2) },
        ]).unwrap();

        let mut g2 = HGraph::new();
        let b2 = g2.add_node(t(2), NodeLabel::Lam);
        let a2 = g2.add_node(t(1), NodeLabel::App);
        g2.add_edge(EdgeKind::DataFlow, &[
            Port { node: a2, idx: 0, type_cid: t(1) },
            Port { node: b2, idx: 0, type_cid: t(2) },
        ]).unwrap();

        assert_eq!(g1.cid(), g2.cid(),
            "graphs with same canonical content must hash identically regardless of insertion order");
    }

    #[test]
    fn effect_scope_edge_round_trips() {
        let mut g = HGraph::new();
        let h = g.add_node(t(9), NodeLabel::Custom("handler".into()));
        let op = g.add_node(t(9), NodeLabel::Custom("op".into()));
        g.add_edge(EdgeKind::EffectScope, &[
            Port { node: h, idx: 0, type_cid: t(9) },
            Port { node: op, idx: 0, type_cid: t(9) },
        ]).unwrap();
        let e = g.edges().next().unwrap();
        assert!(matches!(e.kind, EdgeKind::EffectScope));
    }

    #[test]
    fn grade_annot_payload_distinguishes_cids() {
        let mut g1 = HGraph::new();
        let n1 = g1.add_node(t(1), NodeLabel::App);
        g1.add_edge(EdgeKind::GradeAnnot("linear".into()),
            &[Port { node: n1, idx: 0, type_cid: t(1) }]).unwrap();

        let mut g2 = HGraph::new();
        let n2 = g2.add_node(t(1), NodeLabel::App);
        g2.add_edge(EdgeKind::GradeAnnot("affine".into()),
            &[Port { node: n2, idx: 0, type_cid: t(1) }]).unwrap();

        assert_ne!(g1.cid(), g2.cid(),
            "different grade payloads must yield distinct Cids");
    }

    #[test]
    fn supports_one_thousand_nodes() {
        let mut g = HGraph::new();
        for _ in 0..1_000 {
            g.add_node(t(1), NodeLabel::App);
        }
        assert_eq!(g.node_count(), 1_000);
    }
}
