//! Backend selection + driver-feature matrix.
//!
//! § DRIVER-FEATURE TABLE (cite `06_RENDERING_PIPELINE.csl` § X.detection +
//! `density_budget § XI.B EDGE-7`):
//!
//! | Backend             | D3D12-Ultimate | DXIL≥6.6 | VK_NV_DGC | mesh-nodes | mesh-shader-fallback | last-resort |
//! |---------------------|---------------|----------|-----------|------------|----------------------|-------------|
//! | D3d12WorkGraph      | YES           | YES      | -         | YES        | -                    | -           |
//! | VulkanDgc           | -             | -        | YES       | -          | YES                  | -           |
//! | IndirectFallback    | -             | -        | -         | -          | -                    | YES         |
//!
//! § DEGRADATION POLICY
//!   `D3d12WorkGraph` ⇒ ~100% perf (target 1M ent / 8.3ms)
//!   `VulkanDgc`      ⇒ ~95%  perf (target 1M ent / 8.7ms)
//!   `IndirectFallback` ⇒ ~75% perf (drops to ~750K ent / 8.3ms)
//!
//! Per `density_budget § XI.B EDGE-7` the FB-of-FB-of-FB reduces entity
//! count to 100K when no autonomous-dispatch is available — this crate
//! exposes the knob via [`Schedule::entity_count_for_backend`].

use core::fmt;

/// Which backend a [`crate::Schedule`] was compiled for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    /// `D3D12_WORK_GRAPH_DESC` + `DispatchGraph` (Ultimate hardware).
    D3d12WorkGraph,
    /// `VK_NV_device_generated_commands` (Vulkan 1.3+ + extension).
    VulkanDgc,
    /// `ExecuteIndirect` chain (last-resort fallback).
    IndirectFallback,
}

impl Backend {
    /// Stable string tag.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::D3d12WorkGraph => "d3d12-work-graph",
            Self::VulkanDgc => "vulkan-dgc",
            Self::IndirectFallback => "indirect-fallback",
        }
    }

    /// Does this backend support mesh-node primitives?
    #[must_use]
    pub const fn supports_mesh_nodes(self) -> bool {
        matches!(self, Self::D3d12WorkGraph)
    }

    /// Does this backend allow GPU-issued dispatch (no-CPU-stall)?
    #[must_use]
    pub const fn is_autonomous(self) -> bool {
        matches!(self, Self::D3d12WorkGraph | Self::VulkanDgc)
    }

    /// Approximate perf vs fully-autonomous baseline (% of 1.0).
    ///
    /// Used by [`crate::cost_model::CostModel`] to project frame-cost when
    /// only a fallback backend is available.
    #[must_use]
    pub const fn perf_factor(self) -> f32 {
        match self {
            Self::D3d12WorkGraph => 1.00,
            Self::VulkanDgc => 0.95,
            Self::IndirectFallback => 0.75,
        }
    }

    /// 1M-entity ceiling at this backend (per `density_budget § XI.B EDGE-7`).
    ///
    /// FB-of-FB-of-FB drops entity-count to 100K to preserve other budgets.
    #[must_use]
    pub const fn entity_ceiling(self) -> u64 {
        match self {
            Self::D3d12WorkGraph => 1_000_000,
            Self::VulkanDgc => 950_000,
            Self::IndirectFallback => 100_000,
        }
    }

    /// Iterate all backends in preference order.
    pub fn all() -> impl Iterator<Item = Self> {
        [
            Self::D3d12WorkGraph,
            Self::VulkanDgc,
            Self::IndirectFallback,
        ]
        .into_iter()
    }
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

/// Driver-feature matrix snapshot for backend selection.
///
/// Populated at boot-time from the live D3D12 + Vulkan probes ; passed to
/// [`crate::detect_backend`] to pick the best available path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FeatureMatrix {
    /// `D3D12_FEATURE_D3D12_OPTIONS21.WorkGraphsTier ≥ 1.0`.
    pub d3d12_work_graphs_tier_1_0: bool,
    /// `D3D12_FEATURE_DATA_SHADER_MODEL ≥ 6.8`.
    pub d3d12_shader_model_6_8: bool,
    /// `D3D12_FEATURE_D3D12_OPTIONS7.MeshShaderTier ≥ Tier_1`.
    pub d3d12_mesh_shader_tier_1: bool,
    /// `VK_NV_device_generated_commands` extension present.
    pub vk_nv_device_generated_commands: bool,
    /// `VK_KHR_synchronization2` present.
    pub vk_synchronization2: bool,
    /// `VK_KHR_buffer_device_address` (for DGC indirect-args buffers).
    pub vk_buffer_device_address: bool,
    /// `VK_EXT_mesh_shader` present (used by mesh-node fallback path).
    pub vk_ext_mesh_shader: bool,
    /// `VK_KHR_cooperative_matrix` present (KAN-eval acceleration).
    pub vk_cooperative_matrix: bool,
}

impl FeatureMatrix {
    /// All-disabled baseline.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            d3d12_work_graphs_tier_1_0: false,
            d3d12_shader_model_6_8: false,
            d3d12_mesh_shader_tier_1: false,
            vk_nv_device_generated_commands: false,
            vk_synchronization2: false,
            vk_buffer_device_address: false,
            vk_ext_mesh_shader: false,
            vk_cooperative_matrix: false,
        }
    }

    /// "Ultimate hardware" baseline (RTX-40 / RX-7000 / Arc-A770 with WG-driver).
    #[must_use]
    pub const fn ultimate() -> Self {
        Self {
            d3d12_work_graphs_tier_1_0: true,
            d3d12_shader_model_6_8: true,
            d3d12_mesh_shader_tier_1: true,
            vk_nv_device_generated_commands: true,
            vk_synchronization2: true,
            vk_buffer_device_address: true,
            vk_ext_mesh_shader: true,
            vk_cooperative_matrix: true,
        }
    }

    /// "DGC-only" baseline : older NVIDIA / Quest-3 with VK_NV_DGC support.
    #[must_use]
    pub const fn dgc_only() -> Self {
        Self {
            d3d12_work_graphs_tier_1_0: false,
            d3d12_shader_model_6_8: true,
            d3d12_mesh_shader_tier_1: true,
            vk_nv_device_generated_commands: true,
            vk_synchronization2: true,
            vk_buffer_device_address: true,
            vk_ext_mesh_shader: true,
            vk_cooperative_matrix: false,
        }
    }
}

/// Long-form record describing the chosen backend + perf + fallback class.
///
/// Returned by [`crate::Schedule::descriptor`] for telemetry + UI.
#[derive(Debug, Clone, PartialEq)]
pub struct BackendDescriptor {
    /// Selected backend.
    pub backend: Backend,
    /// Driver-feature snapshot (frozen at compile-time of the schedule).
    pub features: FeatureMatrix,
    /// Human-readable reason for the selection.
    pub reason: String,
}

impl BackendDescriptor {
    /// Construct a descriptor.
    #[must_use]
    pub fn new(backend: Backend, features: FeatureMatrix, reason: impl Into<String>) -> Self {
        Self {
            backend,
            features,
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Backend, BackendDescriptor, FeatureMatrix};

    #[test]
    fn backend_tags_stable() {
        assert_eq!(Backend::D3d12WorkGraph.tag(), "d3d12-work-graph");
        assert_eq!(Backend::VulkanDgc.tag(), "vulkan-dgc");
        assert_eq!(Backend::IndirectFallback.tag(), "indirect-fallback");
    }

    #[test]
    fn only_dx12_supports_mesh_nodes() {
        assert!(Backend::D3d12WorkGraph.supports_mesh_nodes());
        assert!(!Backend::VulkanDgc.supports_mesh_nodes());
        assert!(!Backend::IndirectFallback.supports_mesh_nodes());
    }

    #[test]
    fn autonomy_class() {
        assert!(Backend::D3d12WorkGraph.is_autonomous());
        assert!(Backend::VulkanDgc.is_autonomous());
        assert!(!Backend::IndirectFallback.is_autonomous());
    }

    #[test]
    fn perf_factor_ordering() {
        assert!(Backend::D3d12WorkGraph.perf_factor() > Backend::VulkanDgc.perf_factor());
        assert!(Backend::VulkanDgc.perf_factor() > Backend::IndirectFallback.perf_factor());
    }

    #[test]
    fn entity_ceiling_drops_for_indirect() {
        assert_eq!(Backend::D3d12WorkGraph.entity_ceiling(), 1_000_000);
        assert!(Backend::VulkanDgc.entity_ceiling() < Backend::D3d12WorkGraph.entity_ceiling());
        assert_eq!(Backend::IndirectFallback.entity_ceiling(), 100_000);
    }

    #[test]
    fn feature_matrix_none_all_off() {
        let f = FeatureMatrix::none();
        assert!(!f.d3d12_work_graphs_tier_1_0);
        assert!(!f.vk_nv_device_generated_commands);
    }

    #[test]
    fn feature_matrix_ultimate_all_on() {
        let f = FeatureMatrix::ultimate();
        assert!(f.d3d12_work_graphs_tier_1_0);
        assert!(f.d3d12_mesh_shader_tier_1);
        assert!(f.vk_nv_device_generated_commands);
    }

    #[test]
    fn feature_matrix_dgc_only_off_for_work_graphs() {
        let f = FeatureMatrix::dgc_only();
        assert!(!f.d3d12_work_graphs_tier_1_0);
        assert!(f.vk_nv_device_generated_commands);
    }

    #[test]
    fn descriptor_carries_reason() {
        let d = BackendDescriptor::new(
            Backend::D3d12WorkGraph,
            FeatureMatrix::ultimate(),
            "WorkGraphsTier=1.0",
        );
        assert!(d.reason.contains("WorkGraphsTier"));
    }

    #[test]
    fn backend_iter_includes_all_three() {
        let n: usize = Backend::all().count();
        assert_eq!(n, 3);
    }
}
