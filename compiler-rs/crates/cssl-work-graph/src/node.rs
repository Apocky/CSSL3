//! Work-graph node definition.
//!
//! § DESIGN
//!   A [`WorkGraphNode`] is one node in the DAG that the GPU schedules
//!   autonomously. Three flavors exist :
//!     - **Compute** : a compute-shader dispatch with grid-shape (x,y,z).
//!     - **Mesh**    : a mesh-node primitive (DX12-only ; Vulkan emits an
//!                     ExecuteIndirect-equivalent).
//!     - **Consumer**: a tail node that fans-in producer-output buffers and
//!                     emits to the next render-graph stage's staging buffer.
//!
//! § DAG-INVARIANTS
//!   Each node lists `inputs` (other nodes' outputs it depends on) and
//!   `outputs` (handles it produces). The [`crate::WorkGraphBuilder`] checks
//!   the DAG on `build()` :
//!     - no cycles
//!     - each input ⊑ some prior node's output
//!     - each output uniquely-named
//!
//! § BUDGET-INTEGRATION
//!   Each node carries an `est_cost_us` field used by [`crate::CostModel`]
//!   to gate the schedule against the 8.33ms / 16.67ms frame-budget. A
//!   schedule whose total est-cost > budget is refused at compile-time.

use core::fmt;

use crate::dispatch::DispatchArgs;
use crate::stage_layout::StageId;

/// Stable identifier for a work-graph node.
///
/// Wraps a `String` rather than an integer so the diagnostic surface
/// preserves human-readable names like `"WaveSolver"` or `"Stage6/KAN-BRDF"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub String);

impl NodeId {
    /// Construct from any `Into<String>`.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Three flavors of work-graph node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// Compute-shader dispatch node.
    Compute,
    /// Mesh-shader node (DX12 mesh-node primitive).
    Mesh,
    /// Consumer / sink node.
    Consumer,
}

impl NodeKind {
    /// Stable string tag.
    #[must_use]
    pub const fn tag(&self) -> &'static str {
        match self {
            Self::Compute => "compute",
            Self::Mesh => "mesh",
            Self::Consumer => "consumer",
        }
    }
}

/// One node in a work-graph DAG.
///
/// § FIELDS
///   - `id`         : stable name (used in diagnostics + topo-sort)
///   - `kind`       : compute / mesh / consumer
///   - `stage`      : which render-pipeline stage this node belongs to (4..=7
///                    for the canonical work-graph fusion ; other stages are
///                    legal but rare)
///   - `inputs`     : DAG predecessors (other nodes' ids) — the builder
///                    rewrites these into actual buffer-handles when the
///                    schedule is compiled
///   - `outputs`    : output handles this node produces (named-strings)
///   - `dispatch`   : grid-shape for compute / mesh dispatch
///   - `est_cost_us`: estimated wall-clock cost in microseconds (used by
///                    [`crate::CostModel`] to project frame-cost)
///   - `shader_tag` : opaque tag identifying which shader this node binds
///                    (DXIL/SPIR-V blob lookup happens later)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkGraphNode {
    /// Stable identifier.
    pub id: NodeId,
    /// Compute / mesh / consumer.
    pub kind: NodeKind,
    /// Which stage this node belongs to.
    pub stage: StageId,
    /// Other nodes whose outputs this consumes.
    pub inputs: Vec<NodeId>,
    /// Named output handles this node produces.
    pub outputs: Vec<String>,
    /// Grid-shape for compute / mesh dispatch.
    pub dispatch: DispatchArgs,
    /// Estimated cost in microseconds (for cost-model gating).
    pub est_cost_us: u32,
    /// Shader-blob tag.
    pub shader_tag: String,
}

impl WorkGraphNode {
    /// Construct a compute-node.
    #[must_use]
    pub fn compute(id: impl Into<NodeId>, stage: StageId, dispatch: DispatchArgs) -> Self {
        Self {
            id: id.into(),
            kind: NodeKind::Compute,
            stage,
            inputs: Vec::new(),
            outputs: Vec::new(),
            dispatch,
            est_cost_us: 0,
            shader_tag: String::new(),
        }
    }

    /// Construct a mesh-node.
    #[must_use]
    pub fn mesh(id: impl Into<NodeId>, stage: StageId, dispatch: DispatchArgs) -> Self {
        Self {
            id: id.into(),
            kind: NodeKind::Mesh,
            stage,
            inputs: Vec::new(),
            outputs: Vec::new(),
            dispatch,
            est_cost_us: 0,
            shader_tag: String::new(),
        }
    }

    /// Construct a consumer-node.
    #[must_use]
    pub fn consumer(id: impl Into<NodeId>, stage: StageId) -> Self {
        Self {
            id: id.into(),
            kind: NodeKind::Consumer,
            stage,
            inputs: Vec::new(),
            outputs: Vec::new(),
            dispatch: DispatchArgs::single(),
            est_cost_us: 0,
            shader_tag: String::new(),
        }
    }

    /// Builder : add an input dependency.
    #[must_use]
    pub fn with_input(mut self, src: impl Into<NodeId>) -> Self {
        self.inputs.push(src.into());
        self
    }

    /// Builder : add a named output.
    #[must_use]
    pub fn with_output(mut self, name: impl Into<String>) -> Self {
        self.outputs.push(name.into());
        self
    }

    /// Builder : set estimated cost in microseconds.
    #[must_use]
    pub fn with_cost_us(mut self, est: u32) -> Self {
        self.est_cost_us = est;
        self
    }

    /// Builder : tag the shader blob.
    #[must_use]
    pub fn with_shader_tag(mut self, t: impl Into<String>) -> Self {
        self.shader_tag = t.into();
        self
    }

    /// Sum of dispatched thread groups (for capacity checks).
    #[must_use]
    pub fn dispatch_groups(&self) -> u64 {
        self.dispatch.total_groups()
    }
}

#[cfg(test)]
mod tests {
    use super::{NodeId, NodeKind, WorkGraphNode};
    use crate::dispatch::DispatchArgs;
    use crate::stage_layout::StageId;

    #[test]
    fn node_id_round_trips() {
        let n: NodeId = "WaveSolver".into();
        assert_eq!(n.as_str(), "WaveSolver");
    }

    #[test]
    fn compute_node_default_kind() {
        let n =
            WorkGraphNode::compute("KAN-BRDF", StageId::KanBrdfEval, DispatchArgs::new(8, 8, 1));
        assert_eq!(n.kind, NodeKind::Compute);
    }

    #[test]
    fn mesh_node_kind_marker() {
        let n = WorkGraphNode::mesh(
            "MeshTess",
            StageId::SdfRaymarch,
            DispatchArgs::new(64, 1, 1),
        );
        assert_eq!(n.kind, NodeKind::Mesh);
    }

    #[test]
    fn consumer_node_default_dispatch_single() {
        let n = WorkGraphNode::consumer("Tail", StageId::FractalAmplifier);
        assert_eq!(n.dispatch, DispatchArgs::single());
    }

    #[test]
    fn builder_adds_inputs() {
        let n = WorkGraphNode::compute("B", StageId::KanBrdfEval, DispatchArgs::new(1, 1, 1))
            .with_input("A1")
            .with_input("A2");
        assert_eq!(n.inputs.len(), 2);
        assert_eq!(n.inputs[0].as_str(), "A1");
    }

    #[test]
    fn builder_attaches_cost_us() {
        let n = WorkGraphNode::compute("C", StageId::WaveSolver, DispatchArgs::new(2, 2, 2))
            .with_cost_us(1_500);
        assert_eq!(n.est_cost_us, 1_500);
    }

    #[test]
    fn dispatch_groups_total() {
        let n = WorkGraphNode::compute("D", StageId::WaveSolver, DispatchArgs::new(4, 8, 16));
        assert_eq!(n.dispatch_groups(), 4 * 8 * 16);
    }

    #[test]
    fn shader_tag_round_trip() {
        let n = WorkGraphNode::compute("E", StageId::KanBrdfEval, DispatchArgs::new(1, 1, 1))
            .with_shader_tag("kan_brdf_eval_v3");
        assert_eq!(n.shader_tag, "kan_brdf_eval_v3");
    }

    #[test]
    fn node_kind_tags() {
        assert_eq!(NodeKind::Compute.tag(), "compute");
        assert_eq!(NodeKind::Mesh.tag(), "mesh");
        assert_eq!(NodeKind::Consumer.tag(), "consumer");
    }

    #[test]
    fn outputs_attach_in_order() {
        let n = WorkGraphNode::compute("X", StageId::WaveSolver, DispatchArgs::new(1, 1, 1))
            .with_output("psi_light")
            .with_output("psi_audio");
        assert_eq!(
            n.outputs,
            vec!["psi_light".to_string(), "psi_audio".to_string()]
        );
    }
}
