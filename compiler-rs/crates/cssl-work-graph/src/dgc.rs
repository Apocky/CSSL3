//! Vulkan `VK_NV_device_generated_commands` fallback path.
//!
//! § REFERENCE
//!   - VK_NV_device_generated_commands (1.3 + extension)
//!     https://registry.khronos.org/vulkan/specs/1.3-extensions/man/html/VK_NV_device_generated_commands.html
//!   - Newer Khronos VK_EXT_device_generated_commands (2024) — same shape,
//!     same sequence-buffer protocol ; this module models the NV variant
//!     since it has wider device support.
//!
//! § DESIGN
//!   A `DgcSequence` is a stream of dispatch / draw / push-constant
//!   commands the GPU records and re-issues without a CPU round-trip.
//!   We produce the sequence directly from a [`crate::Schedule`] by
//!   iterating in topo-order and appending one command per node :
//!   `DgcCommand::Dispatch` for compute, `DispatchMeshIndirect` for
//!   mesh, `PushConstant` for per-node uniforms, and `PipelineBind`
//!   to switch shaders between nodes.
//!
//!   The resulting `Vec<DgcCommand>` is later serialized to the
//!   indirect-commands-layout buffer the Vulkan implementation will read
//!   (struct-of-args layout per `VkIndirectCommandsLayoutNV`).

use crate::dispatch::DispatchArgs;
use crate::error::{Result, WorkGraphError};
use crate::node::NodeKind;
use crate::schedule::Schedule;

/// One command in the DGC sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DgcCommand {
    /// Switch the active pipeline (`vkIndirectCommandsTokenNV_PIPELINE`).
    PipelineBind {
        /// Shader-tag the backend resolves to a pipeline handle.
        shader_tag: String,
    },
    /// Push-constant write (`vkIndirectCommandsTokenNV_PUSH_CONSTANT`).
    PushConstant {
        /// Bytes to push.
        data: Vec<u8>,
    },
    /// Compute dispatch (`vkIndirectCommandsTokenNV_DISPATCH`).
    Dispatch {
        /// Dispatch grid.
        args: DispatchArgs,
        /// Source node id (for diagnostics).
        node_id: String,
    },
    /// Mesh-task indirect (`vkIndirectCommandsTokenNV_DRAW_TASKS`).
    DispatchMeshIndirect {
        /// Mesh-task grid.
        args: DispatchArgs,
        /// Node id.
        node_id: String,
    },
    /// Sequence terminator (no-op for accounting).
    Terminator,
}

impl DgcCommand {
    /// Estimated wire size in bytes (used for indirect-commands-buffer sizing).
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub fn wire_size(&self) -> usize {
        match self {
            Self::PipelineBind { shader_tag } => 4 + shader_tag.len(),
            Self::PushConstant { data } => 4 + data.len(),
            Self::Dispatch { .. } => 12, // vkCmdDispatchIndirect args
            Self::DispatchMeshIndirect { .. } => 12,
            Self::Terminator => 0,
        }
    }
}

/// A compiled DGC command sequence ready to be uploaded to the
/// indirect-commands buffer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DgcSequence {
    cmds: Vec<DgcCommand>,
    label: Option<String>,
}

impl DgcSequence {
    /// Empty.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            cmds: Vec::new(),
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
    pub fn commands(&self) -> &[DgcCommand] {
        &self.cmds
    }

    /// Length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cmds.len()
    }

    /// Empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }

    /// Append.
    pub fn push(&mut self, c: DgcCommand) {
        self.cmds.push(c);
    }

    /// Aggregate wire size in bytes.
    #[must_use]
    pub fn wire_size(&self) -> usize {
        self.cmds.iter().map(DgcCommand::wire_size).sum()
    }

    /// Build a sequence from a schedule (intended use ; backend-neutral).
    ///
    /// # Errors
    ///   Returns `BackendMismatch` if the schedule was compiled for D3D12
    ///   work-graphs (mesh-nodes wouldn't lower correctly).
    pub fn from_schedule(schedule: &Schedule) -> Result<Self> {
        if schedule.backend() == crate::backend::Backend::D3d12WorkGraph {
            // Schedule is targeting work-graphs ; we should not lower this to
            // DGC. Caller must build a different schedule with `.auto_select(...)`
            // that picks `VulkanDgc` or `IndirectFallback`.
            return Err(WorkGraphError::BackendMismatch {
                compiled: crate::backend::Backend::D3d12WorkGraph.tag().to_owned(),
                live: crate::backend::Backend::VulkanDgc.tag().to_owned(),
            });
        }
        let mut seq = Self::new().with_label(format!(
            "dgc-seq[{}]",
            schedule.label().unwrap_or("unlabeled")
        ));
        let mut last_shader: Option<String> = None;
        for node in schedule.iter_in_order() {
            // Pipeline rebind only when the shader-tag changes (matches the
            // VK_NV_DGC token-stream sequencing rule).
            if Some(&node.shader_tag) != last_shader.as_ref() && !node.shader_tag.is_empty() {
                seq.push(DgcCommand::PipelineBind {
                    shader_tag: node.shader_tag.clone(),
                });
                last_shader = Some(node.shader_tag.clone());
            }
            match node.kind {
                NodeKind::Compute => {
                    seq.push(DgcCommand::Dispatch {
                        args: node.dispatch,
                        node_id: node.id.as_str().to_owned(),
                    });
                }
                NodeKind::Mesh => {
                    seq.push(DgcCommand::DispatchMeshIndirect {
                        args: node.dispatch,
                        node_id: node.id.as_str().to_owned(),
                    });
                }
                NodeKind::Consumer => {
                    // Consumer nodes are pure data-flow merges with no GPU
                    // dispatch ; we emit nothing for them in the DGC stream.
                    // The downstream stage will read their merged buffer.
                }
            }
        }
        seq.push(DgcCommand::Terminator);
        Ok(seq)
    }

    /// Iterate.
    #[allow(clippy::iter_without_into_iter)]
    pub fn iter(&self) -> std::slice::Iter<'_, DgcCommand> {
        self.cmds.iter()
    }

    /// Number of `Dispatch` commands.
    #[must_use]
    pub fn dispatch_count(&self) -> usize {
        self.cmds
            .iter()
            .filter(|c| matches!(c, DgcCommand::Dispatch { .. }))
            .count()
    }

    /// Number of mesh-indirect commands.
    #[must_use]
    pub fn mesh_count(&self) -> usize {
        self.cmds
            .iter()
            .filter(|c| matches!(c, DgcCommand::DispatchMeshIndirect { .. }))
            .count()
    }

    /// Number of pipeline-binds.
    #[must_use]
    pub fn pipeline_bind_count(&self) -> usize {
        self.cmds
            .iter()
            .filter(|c| matches!(c, DgcCommand::PipelineBind { .. }))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::{DgcCommand, DgcSequence};
    use crate::backend::{Backend, FeatureMatrix};
    use crate::dispatch::DispatchArgs;
    use crate::node::WorkGraphNode;
    use crate::stage_layout::StageId;
    use crate::WorkGraphBuilder;

    #[test]
    fn empty_sequence_is_empty() {
        assert!(DgcSequence::new().is_empty());
    }

    #[test]
    fn cmd_wire_size_dispatch_12() {
        let c = DgcCommand::Dispatch {
            args: DispatchArgs::new(1, 1, 1),
            node_id: "n".into(),
        };
        assert_eq!(c.wire_size(), 12);
    }

    #[test]
    fn cmd_wire_size_terminator_zero() {
        assert_eq!(DgcCommand::Terminator.wire_size(), 0);
    }

    #[test]
    fn cmd_wire_size_pipeline_bind_includes_tag() {
        let c = DgcCommand::PipelineBind {
            shader_tag: "abcd".into(),
        };
        assert_eq!(c.wire_size(), 4 + 4);
    }

    #[test]
    fn cmd_wire_size_push_constant_includes_data() {
        let c = DgcCommand::PushConstant {
            data: vec![0u8; 16],
        };
        assert_eq!(c.wire_size(), 4 + 16);
    }

    fn build_dgc_schedule() -> super::Schedule {
        WorkGraphBuilder::new()
            .auto_select(FeatureMatrix::dgc_only())
            .node(
                WorkGraphNode::compute("A", StageId::WaveSolver, DispatchArgs::new(2, 2, 1))
                    .with_shader_tag("wave_solver_v1"),
            )
            .unwrap()
            .node(
                WorkGraphNode::compute("B", StageId::SdfRaymarch, DispatchArgs::new(4, 4, 1))
                    .with_shader_tag("sdf_raymarch_v1")
                    .with_input("A"),
            )
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn from_schedule_emits_dispatch_per_compute_node() {
        let s = build_dgc_schedule();
        let seq = DgcSequence::from_schedule(&s).unwrap();
        assert_eq!(seq.dispatch_count(), 2);
    }

    #[test]
    fn from_schedule_emits_pipeline_binds_when_shaders_differ() {
        let s = build_dgc_schedule();
        let seq = DgcSequence::from_schedule(&s).unwrap();
        assert_eq!(seq.pipeline_bind_count(), 2);
    }

    #[test]
    fn from_schedule_terminates_with_terminator() {
        let s = build_dgc_schedule();
        let seq = DgcSequence::from_schedule(&s).unwrap();
        assert!(matches!(
            seq.commands().last().unwrap(),
            DgcCommand::Terminator
        ));
    }

    #[test]
    fn from_schedule_refuses_d3d12_work_graph_backend() {
        let s = WorkGraphBuilder::new()
            .auto_select(FeatureMatrix::ultimate())
            .node(WorkGraphNode::compute(
                "A",
                StageId::WaveSolver,
                DispatchArgs::new(1, 1, 1),
            ))
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(s.backend(), Backend::D3d12WorkGraph);
        let r = DgcSequence::from_schedule(&s);
        assert!(matches!(
            r,
            Err(crate::error::WorkGraphError::BackendMismatch { .. })
        ));
    }

    #[test]
    fn shader_tag_dedup_avoids_redundant_binds() {
        let s = WorkGraphBuilder::new()
            .auto_select(FeatureMatrix::dgc_only())
            .node(
                WorkGraphNode::compute("A", StageId::WaveSolver, DispatchArgs::new(1, 1, 1))
                    .with_shader_tag("same_shader"),
            )
            .unwrap()
            .node(
                WorkGraphNode::compute("B", StageId::SdfRaymarch, DispatchArgs::new(1, 1, 1))
                    .with_shader_tag("same_shader")
                    .with_input("A"),
            )
            .unwrap()
            .build()
            .unwrap();
        let seq = DgcSequence::from_schedule(&s).unwrap();
        // Only one pipeline-bind should appear since both nodes share a shader.
        assert_eq!(seq.pipeline_bind_count(), 1);
    }

    #[test]
    fn wire_size_aggregates() {
        let mut seq = DgcSequence::new();
        seq.push(DgcCommand::PipelineBind {
            shader_tag: "abcd".into(),
        });
        seq.push(DgcCommand::Dispatch {
            args: DispatchArgs::new(1, 1, 1),
            node_id: "n".into(),
        });
        seq.push(DgcCommand::Terminator);
        // 8 (bind) + 12 (dispatch) + 0 (terminator) = 20
        assert_eq!(seq.wire_size(), 20);
    }

    #[test]
    fn label_round_trips() {
        let s = DgcSequence::new().with_label("test-seq");
        assert_eq!(s.label(), Some("test-seq"));
    }
}
