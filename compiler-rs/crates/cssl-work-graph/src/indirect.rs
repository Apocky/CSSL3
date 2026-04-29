//! Indirect-dispatch fallback chain.
//!
//! § DESIGN
//!   When neither DX12 work-graphs nor VK_NV_DGC are available, we fall
//!   back to a chain of `ExecuteIndirect` calls (Vulkan
//!   `vkCmdDispatchIndirect` / D3D12 `ExecuteIndirect`). The dispatch-args
//!   are written by a sibling pipeline into a buffer ; we record one
//!   `ExecuteIndirect` per topo-sorted node.
//!
//!   This is the FB-of-FB-of-FB path from `density_budget § XI.B EDGE-7`.
//!   Entity-count is reduced to 100K per the spec to preserve the rest of
//!   the budget.

use crate::dispatch::DispatchArgs;
use crate::error::Result;
use crate::node::NodeKind;
use crate::schedule::Schedule;

/// One indirect-dispatch entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndirectEntry {
    /// Node id (for diagnostics).
    pub node_id: String,
    /// Dispatch args (the GPU may overwrite this from a sibling pipeline).
    pub args: DispatchArgs,
    /// Whether this entry is a mesh-task indirect (else compute).
    pub is_mesh: bool,
}

/// Compiled indirect-chain for the fallback backend.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndirectChain {
    entries: Vec<IndirectEntry>,
    label: Option<String>,
}

impl IndirectChain {
    /// Empty.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            label: None,
        }
    }

    /// With label.
    #[must_use]
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = Some(l.into());
        self
    }

    /// Optional label.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Slice.
    #[must_use]
    pub fn entries(&self) -> &[IndirectEntry] {
        &self.entries
    }

    /// Length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Build from a schedule (intended use).
    pub fn from_schedule(schedule: &Schedule) -> Result<Self> {
        let mut chain = Self::new().with_label(format!(
            "indirect-chain[{}]",
            schedule.label().unwrap_or("unlabeled")
        ));
        for node in schedule.iter_in_order() {
            match node.kind {
                NodeKind::Compute => chain.entries.push(IndirectEntry {
                    node_id: node.id.as_str().to_owned(),
                    args: node.dispatch,
                    is_mesh: false,
                }),
                NodeKind::Mesh => chain.entries.push(IndirectEntry {
                    node_id: node.id.as_str().to_owned(),
                    args: node.dispatch,
                    is_mesh: true,
                }),
                NodeKind::Consumer => {
                    // No indirect emission for consumer nodes — they're pure
                    // dataflow merges.
                }
            }
        }
        Ok(chain)
    }

    /// Total dispatch count.
    #[must_use]
    pub fn total_dispatch_groups(&self) -> u64 {
        self.entries.iter().map(|e| e.args.total_groups()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::IndirectChain;
    use crate::backend::FeatureMatrix;
    use crate::dispatch::DispatchArgs;
    use crate::node::WorkGraphNode;
    use crate::stage_layout::StageId;
    use crate::WorkGraphBuilder;

    fn build_indirect_schedule() -> super::Schedule {
        WorkGraphBuilder::new()
            .auto_select(FeatureMatrix::none())
            .node(WorkGraphNode::compute(
                "A",
                StageId::WaveSolver,
                DispatchArgs::new(2, 2, 1),
            ))
            .unwrap()
            .node(
                WorkGraphNode::compute("B", StageId::SdfRaymarch, DispatchArgs::new(4, 4, 1))
                    .with_input("A"),
            )
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn from_schedule_emits_one_per_compute_node() {
        let s = build_indirect_schedule();
        let chain = IndirectChain::from_schedule(&s).unwrap();
        assert_eq!(chain.len(), 2);
    }

    #[test]
    fn total_dispatch_groups_sums() {
        let s = build_indirect_schedule();
        let chain = IndirectChain::from_schedule(&s).unwrap();
        assert_eq!(chain.total_dispatch_groups(), 4 + 16);
    }

    #[test]
    fn empty_chain_is_empty() {
        let c = IndirectChain::new();
        assert!(c.is_empty());
        assert_eq!(c.total_dispatch_groups(), 0);
    }

    #[test]
    fn label_round_trips() {
        let c = IndirectChain::new().with_label("indirect-test");
        assert_eq!(c.label(), Some("indirect-test"));
    }
}
