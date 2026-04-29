//! `WorkGraphBuilder` — incremental DAG construction + validation.
//!
//! § DESIGN
//!   The builder accepts [`crate::WorkGraphNode`]s one-at-a-time, then
//!   performs DAG-validation on `build()` (each-node-id-unique,
//!   inputs-reference-prior-nodes, no-cycles via Kahn topo-sort,
//!   dispatches-valid, mesh-nodes-rejected-on-DGC-backend).
//!
//! § OUTPUT
//!   `build()` returns a [`crate::Schedule`] : the topo-sorted node list
//!   bound to a chosen [`crate::Backend`]. A schedule is the immutable
//!   compiled artifact dispatched at frame-time.

#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

use std::collections::{HashMap, HashSet};

use crate::backend::{Backend, BackendDescriptor, FeatureMatrix};
use crate::cost_model::{CostModel, FrameBudget};
use crate::error::{Result, WorkGraphError};
use crate::node::{NodeId, NodeKind, WorkGraphNode};
use crate::schedule::{Schedule, ScheduleStats};

/// Incremental builder for a work-graph DAG.
#[derive(Debug, Clone, Default)]
pub struct WorkGraphBuilder {
    nodes: Vec<WorkGraphNode>,
    by_id: HashMap<NodeId, usize>,
    label: Option<String>,
    target_backend: Option<Backend>,
    features: FeatureMatrix,
    budget: Option<FrameBudget>,
    selection_reason: Option<String>,
}

impl WorkGraphBuilder {
    /// Empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            by_id: HashMap::new(),
            label: None,
            target_backend: None,
            features: FeatureMatrix::none(),
            budget: None,
            selection_reason: None,
        }
    }

    /// Optional debug label (carried to telemetry).
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the target backend explicitly. Most callers use [`Self::auto_select`]
    /// instead.
    #[must_use]
    pub fn with_backend(mut self, backend: Backend) -> Self {
        self.target_backend = Some(backend);
        self
    }

    /// Auto-select backend from a [`FeatureMatrix`] (cite `crate::detect_backend`).
    #[must_use]
    pub fn auto_select(mut self, features: FeatureMatrix) -> Self {
        let chosen = crate::detect_backend(&features);
        let reason = match chosen {
            Backend::D3d12WorkGraph => "WorkGraphsTier ≥ 1.0 detected",
            Backend::VulkanDgc => "VK_NV_device_generated_commands present",
            Backend::IndirectFallback => "no autonomous-dispatch backend present ⇒ ExecuteIndirect",
        };
        self.target_backend = Some(chosen);
        self.features = features;
        self.selection_reason = Some(reason.to_owned());
        self
    }

    /// Attach a frame-budget for cost-model gating.
    #[must_use]
    pub fn with_budget(mut self, budget: FrameBudget) -> Self {
        self.budget = Some(budget);
        self
    }

    /// Add one node. Returns error on duplicate id or invalid dispatch.
    pub fn add(&mut self, node: WorkGraphNode) -> Result<()> {
        if self.by_id.contains_key(&node.id) {
            return Err(WorkGraphError::duplicate(node.id.as_str()));
        }
        if !node.dispatch.is_valid() {
            return Err(WorkGraphError::invalid_args(
                node.id.as_str(),
                "dispatch dimension is zero",
            ));
        }
        let idx = self.nodes.len();
        self.by_id.insert(node.id.clone(), idx);
        self.nodes.push(node);
        Ok(())
    }

    /// Builder convenience : chain `.add` returning Self.
    pub fn node(mut self, node: WorkGraphNode) -> Result<Self> {
        self.add(node)?;
        Ok(self)
    }

    /// Number of nodes added so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Look-up a node by id.
    #[must_use]
    pub fn get(&self, id: &NodeId) -> Option<&WorkGraphNode> {
        self.by_id.get(id).map(|&i| &self.nodes[i])
    }

    /// Validate inputs : every input-node must already be in the graph.
    fn validate_inputs(&self) -> Result<()> {
        for node in &self.nodes {
            for input in &node.inputs {
                if !self.by_id.contains_key(input) {
                    return Err(WorkGraphError::undeclared(node.id.as_str(), input.as_str()));
                }
            }
        }
        Ok(())
    }

    /// Validate dispatch limits against the target backend.
    fn validate_dispatch_limits(&self, backend: Backend) -> Result<()> {
        for node in &self.nodes {
            let ok = match backend {
                Backend::D3d12WorkGraph => node.dispatch.fits_d3d12_limit(),
                Backend::VulkanDgc | Backend::IndirectFallback => node.dispatch.fits_vulkan_limit(),
            };
            if !ok {
                return Err(WorkGraphError::CapacityExceeded {
                    what: format!("dispatch group on {backend}"),
                    got: node.dispatch.x.max(node.dispatch.y).max(node.dispatch.z),
                    limit: 65_535,
                });
            }
        }
        Ok(())
    }

    /// Validate mesh-node compatibility with the target backend.
    fn validate_mesh_nodes(&self, backend: Backend) -> Result<()> {
        if backend.supports_mesh_nodes() {
            return Ok(());
        }
        for node in &self.nodes {
            if matches!(node.kind, NodeKind::Mesh) {
                return Err(WorkGraphError::MeshNodeUnsupported {
                    node: node.id.as_str().to_owned(),
                    backend: backend.tag().to_owned(),
                });
            }
        }
        Ok(())
    }

    /// Topological sort via Kahn's algorithm. Returns the id list in
    /// dispatch order, or `Cycle` on cycle-detection.
    fn topo_sort(&self) -> Result<Vec<NodeId>> {
        // Compute reverse-adjacency for in-degree.
        let mut in_degree: HashMap<&NodeId, usize> = HashMap::new();
        let mut adj: HashMap<&NodeId, Vec<&NodeId>> = HashMap::new();
        for node in &self.nodes {
            in_degree.entry(&node.id).or_insert(0);
            for input in &node.inputs {
                *in_degree.entry(&node.id).or_insert(0) += 1;
                adj.entry(input).or_default().push(&node.id);
            }
        }
        // Initialize queue with zero-in-degree nodes (preserve insertion order
        // for stable, debuggable output).
        let mut queue: Vec<&NodeId> = self
            .nodes
            .iter()
            .map(|n| &n.id)
            .filter(|id| in_degree.get(id).copied().unwrap_or(0) == 0)
            .collect();
        let mut out: Vec<NodeId> = Vec::with_capacity(self.nodes.len());
        let mut consumed: HashSet<NodeId> = HashSet::new();
        while let Some(head) = queue.first().copied() {
            queue.remove(0);
            out.push(head.clone());
            consumed.insert(head.clone());
            if let Some(succs) = adj.get(head) {
                for s in succs {
                    let entry = in_degree.entry(s).or_insert(0);
                    if *entry > 0 {
                        *entry -= 1;
                    }
                    if *entry == 0 && !consumed.contains(*s) && !queue.contains(s) {
                        queue.push(s);
                    }
                }
            }
        }
        if out.len() != self.nodes.len() {
            // Find any node still with positive in-degree to name in the cycle
            // diagnostic.
            let cyc = self
                .nodes
                .iter()
                .find(|n| in_degree.get(&n.id).copied().unwrap_or(0) > 0)
                .map_or_else(|| NodeId::new("<unknown>"), |n| n.id.clone());
            return Err(WorkGraphError::cycle(cyc.as_str()));
        }
        Ok(out)
    }

    /// Compile to a [`Schedule`].
    ///
    /// Validates the DAG, picks the backend (or honors the explicit one),
    /// gates against the frame-budget if attached, and returns the immutable
    /// [`Schedule`] artifact.
    pub fn build(self) -> Result<Schedule> {
        if self.nodes.is_empty() {
            return Err(WorkGraphError::Empty);
        }
        let backend = self.target_backend.ok_or_else(|| {
            WorkGraphError::backend_unsupported(
                "auto-select",
                "no FeatureMatrix supplied ; call .auto_select() or .with_backend()",
            )
        })?;
        self.validate_inputs()?;
        self.validate_dispatch_limits(backend)?;
        self.validate_mesh_nodes(backend)?;
        let order = self.topo_sort()?;
        let cost_us: u64 = self.nodes.iter().map(|n| u64::from(n.est_cost_us)).sum();
        // Apply backend perf-factor : autonomous backends are faster.
        let scaled_us = if backend.is_autonomous() {
            cost_us
        } else {
            // 75% perf ⇒ 1/0.75 = 1.333× wall-clock cost.
            (cost_us as f32 / backend.perf_factor()) as u64
        };
        if let Some(budget) = self.budget {
            if scaled_us > u64::from(budget.frame_us()) {
                return Err(WorkGraphError::FrameBudget {
                    projected_us: u32::try_from(scaled_us).unwrap_or(u32::MAX),
                    budget_us: budget.frame_us(),
                    target_hz: budget.target_hz(),
                });
            }
        }
        let mesh_node_count = self
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Mesh))
            .count();
        let compute_node_count = self
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Compute))
            .count();
        let consumer_node_count = self
            .nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Consumer))
            .count();
        let stats = ScheduleStats {
            node_count: self.nodes.len(),
            compute_node_count,
            mesh_node_count,
            consumer_node_count,
            est_cost_us: u32::try_from(scaled_us).unwrap_or(u32::MAX),
        };
        let descriptor = BackendDescriptor::new(
            backend,
            self.features,
            self.selection_reason
                .unwrap_or_else(|| "explicit backend".to_owned()),
        );
        Ok(Schedule {
            label: self.label,
            backend,
            descriptor,
            order,
            nodes: self.nodes,
            by_id: self.by_id,
            budget: self.budget,
            stats,
            cost_model: CostModel::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::WorkGraphBuilder;
    use crate::backend::{Backend, FeatureMatrix};
    use crate::dispatch::DispatchArgs;
    use crate::error::WorkGraphError;
    use crate::node::WorkGraphNode;
    use crate::stage_layout::StageId;

    fn cn(name: &str, stage: StageId, x: u32, y: u32, z: u32) -> WorkGraphNode {
        WorkGraphNode::compute(name, stage, DispatchArgs::new(x, y, z))
    }

    #[test]
    fn empty_builder_refused() {
        let e = WorkGraphBuilder::new()
            .with_backend(Backend::D3d12WorkGraph)
            .build()
            .unwrap_err();
        assert!(matches!(e, WorkGraphError::Empty));
    }

    #[test]
    fn duplicate_id_refused() {
        let mut b = WorkGraphBuilder::new();
        b.add(cn("X", StageId::WaveSolver, 1, 1, 1)).unwrap();
        let r = b.add(cn("X", StageId::WaveSolver, 1, 1, 1));
        assert!(matches!(r, Err(WorkGraphError::DuplicateNode { .. })));
    }

    #[test]
    fn invalid_dispatch_zero_x_refused() {
        let mut b = WorkGraphBuilder::new();
        let r = b.add(cn("Z", StageId::WaveSolver, 0, 1, 1));
        assert!(matches!(r, Err(WorkGraphError::InvalidArgs { .. })));
    }

    #[test]
    fn undeclared_input_refused() {
        let s = WorkGraphBuilder::new()
            .with_backend(Backend::D3d12WorkGraph)
            .node(cn("A", StageId::WaveSolver, 1, 1, 1).with_input("missing"))
            .unwrap()
            .build();
        assert!(matches!(s, Err(WorkGraphError::UndeclaredInput { .. })));
    }

    #[test]
    fn cycle_refused() {
        // A → B → C → A (cycle) — the validator should return Cycle.
        let s = WorkGraphBuilder::new()
            .with_backend(Backend::D3d12WorkGraph)
            .node(cn("A", StageId::WaveSolver, 1, 1, 1).with_input("C"))
            .unwrap()
            .node(cn("B", StageId::SdfRaymarch, 1, 1, 1).with_input("A"))
            .unwrap()
            .node(cn("C", StageId::KanBrdfEval, 1, 1, 1).with_input("B"))
            .unwrap()
            .build();
        assert!(matches!(s, Err(WorkGraphError::Cycle { .. })));
    }

    #[test]
    fn linear_chain_topo_orders_correctly() {
        let s = WorkGraphBuilder::new()
            .with_backend(Backend::D3d12WorkGraph)
            .node(cn("A", StageId::WaveSolver, 1, 1, 1))
            .unwrap()
            .node(cn("B", StageId::SdfRaymarch, 1, 1, 1).with_input("A"))
            .unwrap()
            .node(cn("C", StageId::KanBrdfEval, 1, 1, 1).with_input("B"))
            .unwrap()
            .build()
            .unwrap();
        let names: Vec<_> = s.order().iter().map(|n| n.as_str().to_owned()).collect();
        assert_eq!(names, vec!["A", "B", "C"]);
        assert_eq!(s.stats().node_count, 3);
    }

    #[test]
    fn mesh_node_refused_on_dgc_backend() {
        let s = WorkGraphBuilder::new()
            .with_backend(Backend::VulkanDgc)
            .node(WorkGraphNode::mesh(
                "MeshN",
                StageId::SdfRaymarch,
                DispatchArgs::new(1, 1, 1),
            ))
            .unwrap()
            .build();
        assert!(matches!(s, Err(WorkGraphError::MeshNodeUnsupported { .. })));
    }

    #[test]
    fn mesh_node_accepted_on_dx12_backend() {
        let s = WorkGraphBuilder::new()
            .with_backend(Backend::D3d12WorkGraph)
            .node(WorkGraphNode::mesh(
                "MeshN",
                StageId::SdfRaymarch,
                DispatchArgs::new(1, 1, 1),
            ))
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(s.stats().mesh_node_count, 1);
    }

    #[test]
    fn auto_select_picks_work_graph_when_supported() {
        let s = WorkGraphBuilder::new()
            .auto_select(FeatureMatrix::ultimate())
            .node(cn("A", StageId::WaveSolver, 1, 1, 1))
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(s.backend(), Backend::D3d12WorkGraph);
    }

    #[test]
    fn auto_select_dgc_only_picks_vulkan_dgc() {
        let s = WorkGraphBuilder::new()
            .auto_select(FeatureMatrix::dgc_only())
            .node(cn("A", StageId::WaveSolver, 1, 1, 1))
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(s.backend(), Backend::VulkanDgc);
    }

    #[test]
    fn auto_select_none_falls_back_to_indirect() {
        let s = WorkGraphBuilder::new()
            .auto_select(FeatureMatrix::none())
            .node(cn("A", StageId::WaveSolver, 1, 1, 1))
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(s.backend(), Backend::IndirectFallback);
    }

    #[test]
    fn frame_budget_overrun_refused() {
        let r = WorkGraphBuilder::new()
            .with_backend(Backend::D3d12WorkGraph)
            .with_budget(crate::cost_model::FrameBudget::hz_120())
            .node(cn("Heavy", StageId::WaveSolver, 1, 1, 1).with_cost_us(20_000))
            .unwrap()
            .build();
        assert!(matches!(r, Err(WorkGraphError::FrameBudget { .. })));
    }

    #[test]
    fn frame_budget_under_passes() {
        let s = WorkGraphBuilder::new()
            .with_backend(Backend::D3d12WorkGraph)
            .with_budget(crate::cost_model::FrameBudget::hz_60())
            .node(cn("Light", StageId::WaveSolver, 1, 1, 1).with_cost_us(2_000))
            .unwrap()
            .build()
            .unwrap();
        assert!(s.stats().est_cost_us <= 16_000);
    }

    #[test]
    fn fan_out_topo_visits_a_then_b_then_c_or_d() {
        // A → B → D ; A → C → D (diamond)
        let s = WorkGraphBuilder::new()
            .with_backend(Backend::D3d12WorkGraph)
            .node(cn("A", StageId::WaveSolver, 1, 1, 1))
            .unwrap()
            .node(cn("B", StageId::SdfRaymarch, 1, 1, 1).with_input("A"))
            .unwrap()
            .node(cn("C", StageId::KanBrdfEval, 1, 1, 1).with_input("A"))
            .unwrap()
            .node(
                cn("D", StageId::FractalAmplifier, 1, 1, 1)
                    .with_input("B")
                    .with_input("C"),
            )
            .unwrap()
            .build()
            .unwrap();
        let names: Vec<_> = s.order().iter().map(|n| n.as_str().to_owned()).collect();
        let pos = |k: &str| names.iter().position(|n| n == k).unwrap();
        assert!(pos("A") < pos("B"));
        assert!(pos("A") < pos("C"));
        assert!(pos("B") < pos("D"));
        assert!(pos("C") < pos("D"));
    }

    #[test]
    fn label_round_trips() {
        let s = WorkGraphBuilder::new()
            .with_label("hello-graph")
            .with_backend(Backend::D3d12WorkGraph)
            .node(cn("A", StageId::WaveSolver, 1, 1, 1))
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(s.label(), Some("hello-graph"));
    }

    #[test]
    fn missing_backend_refused() {
        let s = WorkGraphBuilder::new()
            .node(cn("A", StageId::WaveSolver, 1, 1, 1))
            .unwrap()
            .build();
        assert!(matches!(s, Err(WorkGraphError::BackendUnsupported { .. })));
    }
}
