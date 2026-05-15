#![forbid(unsafe_code)]
#![doc = "cssl-prov — provenance DAG.\n\n\
Spec: `specs/Upgrade/impl/IMPL_02_FOUNDATION.csl` § cssl-prov. \
Records `(output_cid, op, inputs[], ts)` for every Cid-bearing artifact. The \
acyclicity of the DAG is guaranteed by Cid-acyclicity (output Cid is a function \
of input Cids, so no cycle is constructible)."]

use cssl_cas::Cid;
use smallvec::SmallVec;
use std::collections::HashMap;

/// A single provenance record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvNode {
    pub output: Cid,
    pub op: String,
    pub inputs: SmallVec<[Cid; 4]>,
    pub ts: u64,
}

/// In-memory provenance DAG : `output Cid` → `ProvNode`.
#[derive(Clone, Debug, Default)]
pub struct ProvDag {
    nodes: HashMap<Cid, ProvNode>,
}

impl ProvDag {
    /// Construct an empty DAG.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a node ; idempotent (overwriting a record with identical content
    /// is a no-op ; overwriting with different content keeps the first record).
    pub fn record(&mut self, node: ProvNode) {
        self.nodes.entry(node.output).or_insert(node);
    }

    /// Look up the record for an output Cid.
    #[must_use]
    pub fn lookup(&self, output: &Cid) -> Option<&ProvNode> {
        self.nodes.get(output)
    }

    /// Iterate ancestors of an output (transitive closure of inputs).
    pub fn ancestors<'a>(&'a self, output: &Cid) -> Vec<&'a ProvNode> {
        let mut out = Vec::new();
        let mut stack = Vec::new();
        if let Some(start) = self.nodes.get(output) {
            stack.extend(start.inputs.iter().copied());
        }
        let mut seen = std::collections::HashSet::new();
        while let Some(cid) = stack.pop() {
            if !seen.insert(cid) {
                continue;
            }
            if let Some(n) = self.nodes.get(&cid) {
                out.push(n);
                stack.extend(n.inputs.iter().copied());
            }
        }
        out
    }

    /// Number of records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// `true` iff there are no records.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_cas::cid_of_bytes;

    fn node(out_seed: u8, op: &str, inputs: &[u8]) -> ProvNode {
        ProvNode {
            output: cid_of_bytes(&[out_seed]),
            op: op.into(),
            inputs: inputs.iter().map(|s| cid_of_bytes(&[*s])).collect(),
            ts: u64::from(out_seed),
        }
    }

    #[test]
    fn record_then_lookup_returns_same_node() {
        let mut d = ProvDag::new();
        let n = node(1, "add", &[2, 3]);
        d.record(n.clone());
        assert_eq!(d.lookup(&n.output), Some(&n));
    }

    #[test]
    fn lookup_missing_returns_none() {
        let d = ProvDag::new();
        assert!(d.lookup(&cid_of_bytes(b"missing")).is_none());
    }

    #[test]
    fn duplicate_record_is_idempotent() {
        let mut d = ProvDag::new();
        let n = node(1, "op", &[]);
        d.record(n.clone());
        d.record(n.clone());
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn ancestors_traverses_dag() {
        let mut d = ProvDag::new();
        // Build : root(1) ← a(2) ← b(3) ; root inputs = [a]; a inputs = [b]
        d.record(node(3, "leaf", &[]));
        d.record(node(2, "mid", &[3]));
        d.record(node(1, "root", &[2]));
        let root_cid = cid_of_bytes(&[1]);
        let anc = d.ancestors(&root_cid);
        assert_eq!(anc.len(), 2, "root has 2 transitive ancestors (a + b)");
    }

    #[test]
    fn ancestors_empty_for_leaf_node() {
        let mut d = ProvDag::new();
        d.record(node(5, "leaf", &[]));
        assert_eq!(d.ancestors(&cid_of_bytes(&[5])).len(), 0);
    }

    #[test]
    fn empty_dag_reports_empty() {
        let d = ProvDag::new();
        assert!(d.is_empty());
        assert_eq!(d.len(), 0);
    }
}
