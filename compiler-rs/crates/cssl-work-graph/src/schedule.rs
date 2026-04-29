//! `Schedule` — compiled, immutable work-graph artifact.
//!
//! § DESIGN
//!   A [`Schedule`] is the output of [`crate::WorkGraphBuilder::build`]. It
//!   carries :
//!     - the chosen [`crate::Backend`]
//!     - the topo-sorted node-id list (`order`)
//!     - the original node store (`nodes`) + index (`by_id`)
//!     - a [`ScheduleStats`] summary
//!     - an optional [`crate::FrameBudget`] honored during compile
//!
//!   Dispatching a schedule is up to the backend module : the schedule
//!   itself does not bind D3D12 or Vulkan — it's a pure Rust artifact.
//!
//! § THREAD-SAFETY
//!   `Schedule` is `Send + Sync` ; multiple threads can dispatch the same
//!   schedule against different command-lists (so long as the backend
//!   implementation makes the per-call dispatch thread-safe — typically
//!   one cmd-list per thread).

use std::collections::HashMap;

use crate::backend::{Backend, BackendDescriptor};
use crate::cost_model::{CostModel, FrameBudget};
use crate::node::{NodeId, WorkGraphNode};

/// Compile-time stats for a [`Schedule`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScheduleStats {
    /// Total nodes in the graph.
    pub node_count: usize,
    /// Compute-flavor nodes.
    pub compute_node_count: usize,
    /// Mesh-flavor nodes.
    pub mesh_node_count: usize,
    /// Consumer-flavor nodes.
    pub consumer_node_count: usize,
    /// Aggregated estimated dispatch cost in microseconds.
    pub est_cost_us: u32,
}

/// Compiled, immutable work-graph schedule.
#[derive(Debug, Clone)]
pub struct Schedule {
    /// Optional debug label (forwarded to telemetry).
    pub(crate) label: Option<String>,
    /// Chosen backend.
    pub(crate) backend: Backend,
    /// Long-form descriptor (for telemetry + UI).
    pub(crate) descriptor: BackendDescriptor,
    /// Topo-sorted dispatch order.
    pub(crate) order: Vec<NodeId>,
    /// Original node store.
    pub(crate) nodes: Vec<WorkGraphNode>,
    /// Index from id → position in `nodes`.
    pub(crate) by_id: HashMap<NodeId, usize>,
    /// Optional frame-budget honored during compile.
    pub(crate) budget: Option<FrameBudget>,
    /// Compile-time stats.
    pub(crate) stats: ScheduleStats,
    /// Cost-model the schedule was gated against.
    pub(crate) cost_model: CostModel,
}

impl Schedule {
    /// Optional debug label.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Chosen backend.
    #[must_use]
    pub const fn backend(&self) -> Backend {
        self.backend
    }

    /// Long-form descriptor.
    #[must_use]
    pub const fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    /// Topo-sorted dispatch order.
    #[must_use]
    pub fn order(&self) -> &[NodeId] {
        &self.order
    }

    /// All nodes (insertion order, NOT dispatch order ; use `order()` for dispatch).
    #[must_use]
    pub fn nodes(&self) -> &[WorkGraphNode] {
        &self.nodes
    }

    /// Look-up node by id.
    #[must_use]
    pub fn node(&self, id: &NodeId) -> Option<&WorkGraphNode> {
        self.by_id.get(id).map(|&i| &self.nodes[i])
    }

    /// Compile-time stats.
    #[must_use]
    pub const fn stats(&self) -> &ScheduleStats {
        &self.stats
    }

    /// Optional frame-budget the schedule was gated against.
    #[must_use]
    pub const fn budget(&self) -> Option<FrameBudget> {
        self.budget
    }

    /// Number of nodes (alias for `stats().node_count`).
    #[must_use]
    pub const fn len(&self) -> usize {
        self.stats.node_count
    }

    /// Empty? (always false for a built schedule, kept for symmetry).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.stats.node_count == 0
    }

    /// Iterate the nodes in dispatch order.
    pub fn iter_in_order(&self) -> impl Iterator<Item = &WorkGraphNode> {
        self.order.iter().map(move |id| {
            let i = self.by_id.get(id).copied().expect("id in topo order");
            &self.nodes[i]
        })
    }

    /// Estimated total cost in microseconds (post backend-perf-factor).
    #[must_use]
    pub const fn est_cost_us(&self) -> u32 {
        self.stats.est_cost_us
    }

    /// Is the schedule's est-cost within its frame-budget?
    #[must_use]
    pub fn within_budget(&self) -> bool {
        self.budget.map_or(true, |b| {
            u64::from(self.stats.est_cost_us) <= u64::from(b.frame_us())
        })
    }

    /// Adjust the entity-count this schedule will see, capped by the
    /// backend's [`Backend::entity_ceiling`].
    ///
    /// Returns the capped count. Used by per-frame entity-tier bookkeeping
    /// to avoid over-committing on a fallback backend.
    #[must_use]
    pub fn entity_count_for_backend(&self, requested: u64) -> u64 {
        requested.min(self.backend.entity_ceiling())
    }

    /// Cost-model used for this schedule.
    #[must_use]
    pub const fn cost_model(&self) -> &CostModel {
        &self.cost_model
    }
}

#[cfg(test)]
mod tests {
    use super::ScheduleStats;
    use crate::backend::{Backend, FeatureMatrix};
    use crate::dispatch::DispatchArgs;
    use crate::node::WorkGraphNode;
    use crate::stage_layout::StageId;
    use crate::WorkGraphBuilder;

    fn build_two_node_schedule() -> super::Schedule {
        WorkGraphBuilder::new()
            .auto_select(FeatureMatrix::ultimate())
            .node(WorkGraphNode::compute(
                "A",
                StageId::WaveSolver,
                DispatchArgs::new(2, 2, 1),
            ))
            .unwrap()
            .node(
                WorkGraphNode::compute("B", StageId::SdfRaymarch, DispatchArgs::new(2, 2, 1))
                    .with_input("A"),
            )
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn order_matches_topo() {
        let s = build_two_node_schedule();
        assert_eq!(s.order().len(), 2);
        assert_eq!(s.order()[0].as_str(), "A");
        assert_eq!(s.order()[1].as_str(), "B");
    }

    #[test]
    fn node_lookup_by_id() {
        let s = build_two_node_schedule();
        assert!(s.node(&"B".into()).is_some());
        assert!(s.node(&"missing".into()).is_none());
    }

    #[test]
    fn iter_in_order_dispatches_a_first() {
        let s = build_two_node_schedule();
        let names: Vec<_> = s
            .iter_in_order()
            .map(|n| n.id.as_str().to_owned())
            .collect();
        assert_eq!(names, vec!["A".to_string(), "B".to_string()]);
    }

    #[test]
    fn within_budget_no_budget_set_is_true() {
        let s = build_two_node_schedule();
        assert!(s.within_budget());
    }

    #[test]
    fn entity_count_capped_by_backend() {
        let s = build_two_node_schedule();
        assert_eq!(
            s.entity_count_for_backend(2_000_000),
            Backend::D3d12WorkGraph.entity_ceiling()
        );
        assert_eq!(s.entity_count_for_backend(500), 500);
    }

    #[test]
    fn schedule_stats_default_zero() {
        let z = ScheduleStats::default();
        assert_eq!(z.node_count, 0);
        assert_eq!(z.compute_node_count, 0);
    }

    #[test]
    fn descriptor_carries_backend_tag() {
        let s = build_two_node_schedule();
        assert_eq!(s.descriptor().backend, Backend::D3d12WorkGraph);
    }

    #[test]
    fn len_and_empty_match_stats() {
        let s = build_two_node_schedule();
        assert_eq!(s.len(), s.stats().node_count);
        assert!(!s.is_empty());
    }
}
