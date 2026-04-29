//! Mesh-node primitive (DX12-Ultimate) + Vulkan-equivalent fallback.
//!
//! § DESIGN
//!   `D3D12_NODE_TYPE_MESH` lets a work-graph dispatch a mesh-shader directly
//!   from another work-graph node — without going through the graphics
//!   pipeline state machine. Use cases :
//!     - per-cell mesh-tessellation triggered by SDF-raymarch hit events
//!     - per-region MERA-summary mesh-instance output
//!     - per-Sovereign body-presence-volume mesh-emission
//!
//!   On non-DX12 backends, the mesh-node lowers to an `ExecuteIndirect`
//!   call that issues `vkCmdDrawMeshTasksIndirectEXT` (Vulkan 1.3 +
//!   `VK_EXT_mesh_shader`). This module exposes the parameters in a
//!   backend-neutral form ; the actual emission lives in
//!   [`crate::work_graph_d3d12`] / [`crate::dgc`].

use crate::dispatch::DispatchArgs;

/// Per-mesh-node arguments.
///
/// § FIELDS
///   - `groups`        : mesh-task thread-group grid (x,y,z)
///   - `vertex_budget` : max vertices output per group (DX12 caps at 256)
///   - `prim_budget`   : max primitives output per group (DX12 caps at 256)
///   - `payload_bytes` : payload size from amplification-shader (≤ 16K)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshNodeArgs {
    /// Mesh-task thread-group grid.
    pub groups: DispatchArgs,
    /// Max vertices per group (DX12 spec : ≤ 256).
    pub vertex_budget: u32,
    /// Max primitives per group (DX12 spec : ≤ 256).
    pub prim_budget: u32,
    /// Payload size in bytes from amplification (≤ 16384).
    pub payload_bytes: u32,
}

impl MeshNodeArgs {
    /// Construct.
    #[must_use]
    pub const fn new(
        groups: DispatchArgs,
        vertex_budget: u32,
        prim_budget: u32,
        payload_bytes: u32,
    ) -> Self {
        Self {
            groups,
            vertex_budget,
            prim_budget,
            payload_bytes,
        }
    }

    /// Default conservative budgets matching DX12 mesh-shader caps.
    #[must_use]
    pub const fn default_caps(groups: DispatchArgs) -> Self {
        Self::new(groups, 256, 256, 0)
    }

    /// Returns true iff the args are within DX12 mesh-shader caps :
    ///   vertex_budget ≤ 256 ; prim_budget ≤ 256 ; payload_bytes ≤ 16K.
    #[must_use]
    pub const fn within_dx12_caps(self) -> bool {
        self.vertex_budget <= 256 && self.prim_budget <= 256 && self.payload_bytes <= 16_384
    }

    /// Returns true iff the args are within Vulkan VK_EXT_mesh_shader caps :
    ///   maxMeshOutputVertices ≥ 256 ; maxMeshOutputPrimitives ≥ 256 ;
    ///   maxMeshPayloadAndOutputMemorySize ≥ 47K (typical NV) or ≥ 16K (AMD/Adreno).
    #[must_use]
    pub const fn within_vulkan_caps(self) -> bool {
        self.vertex_budget <= 256 && self.prim_budget <= 256 && self.payload_bytes <= 16_384
    }

    /// Estimated bandwidth in bytes/frame at the given dispatch-rate.
    #[must_use]
    pub const fn estimated_bandwidth_bytes(self) -> u64 {
        // 32 bytes/vertex (pos+normal+uv+color) is a reasonable upper bound.
        let per_group = 32_u64 * (self.vertex_budget as u64);
        per_group * self.groups.total_groups()
    }
}

/// Mesh-node descriptor — combines the args with a label + the shader tag
/// the backend will resolve to a real mesh-shader blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshNode {
    /// Unique label.
    pub label: String,
    /// Args.
    pub args: MeshNodeArgs,
    /// Shader-blob tag (looked-up by the backend ; opaque to this crate).
    pub shader_tag: String,
}

impl MeshNode {
    /// Construct.
    #[must_use]
    pub fn new(label: impl Into<String>, args: MeshNodeArgs) -> Self {
        Self {
            label: label.into(),
            args,
            shader_tag: String::new(),
        }
    }

    /// Builder : set shader-blob tag.
    #[must_use]
    pub fn with_shader_tag(mut self, t: impl Into<String>) -> Self {
        self.shader_tag = t.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{MeshNode, MeshNodeArgs};
    use crate::dispatch::DispatchArgs;

    #[test]
    fn args_constructor() {
        let a = MeshNodeArgs::new(DispatchArgs::new(2, 2, 1), 128, 64, 1024);
        assert_eq!(a.vertex_budget, 128);
        assert_eq!(a.prim_budget, 64);
    }

    #[test]
    fn default_caps_match_dx12() {
        let a = MeshNodeArgs::default_caps(DispatchArgs::new(1, 1, 1));
        assert_eq!(a.vertex_budget, 256);
        assert_eq!(a.prim_budget, 256);
        assert!(a.within_dx12_caps());
    }

    #[test]
    fn over_vertex_budget_refused() {
        let a = MeshNodeArgs::new(DispatchArgs::new(1, 1, 1), 257, 256, 0);
        assert!(!a.within_dx12_caps());
    }

    #[test]
    fn over_payload_refused() {
        let a = MeshNodeArgs::new(DispatchArgs::new(1, 1, 1), 256, 256, 32_000);
        assert!(!a.within_dx12_caps());
    }

    #[test]
    fn vulkan_caps_match_dx12_for_default() {
        let a = MeshNodeArgs::default_caps(DispatchArgs::new(1, 1, 1));
        assert!(a.within_vulkan_caps());
    }

    #[test]
    fn estimated_bandwidth_scales_with_groups() {
        let a1 = MeshNodeArgs::default_caps(DispatchArgs::new(2, 2, 1));
        let a2 = MeshNodeArgs::default_caps(DispatchArgs::new(4, 4, 1));
        assert!(a2.estimated_bandwidth_bytes() > a1.estimated_bandwidth_bytes());
    }

    #[test]
    fn mesh_node_round_trips() {
        let n = MeshNode::new(
            "MeshTess",
            MeshNodeArgs::default_caps(DispatchArgs::new(1, 1, 1)),
        )
        .with_shader_tag("tess_v3");
        assert_eq!(n.label, "MeshTess");
        assert_eq!(n.shader_tag, "tess_v3");
    }
}
