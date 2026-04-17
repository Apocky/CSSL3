//! WebGPU feature + limits catalog.

use core::fmt;
use std::collections::BTreeSet;

/// WebGPU optional features (matches the spec `GPUFeatureName` enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WebGpuFeature {
    /// `depth-clip-control`.
    DepthClipControl,
    /// `depth32float-stencil8`.
    Depth32FloatStencil8,
    /// `texture-compression-bc`.
    TextureCompressionBc,
    /// `texture-compression-etc2`.
    TextureCompressionEtc2,
    /// `texture-compression-astc`.
    TextureCompressionAstc,
    /// `timestamp-query` — R18 telemetry.
    TimestampQuery,
    /// `indirect-first-instance`.
    IndirectFirstInstance,
    /// `shader-f16`.
    ShaderF16,
    /// `rg11b10ufloat-renderable`.
    Rg11b10UfloatRenderable,
    /// `bgra8unorm-storage`.
    Bgra8UnormStorage,
    /// `float32-filterable`.
    Float32Filterable,
    /// `dual-source-blending`.
    DualSourceBlending,
    /// `clip-distances`.
    ClipDistances,
    /// `subgroups` — Chrome flag.
    Subgroups,
}

impl WebGpuFeature {
    /// Canonical feature-string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DepthClipControl => "depth-clip-control",
            Self::Depth32FloatStencil8 => "depth32float-stencil8",
            Self::TextureCompressionBc => "texture-compression-bc",
            Self::TextureCompressionEtc2 => "texture-compression-etc2",
            Self::TextureCompressionAstc => "texture-compression-astc",
            Self::TimestampQuery => "timestamp-query",
            Self::IndirectFirstInstance => "indirect-first-instance",
            Self::ShaderF16 => "shader-f16",
            Self::Rg11b10UfloatRenderable => "rg11b10ufloat-renderable",
            Self::Bgra8UnormStorage => "bgra8unorm-storage",
            Self::Float32Filterable => "float32-filterable",
            Self::DualSourceBlending => "dual-source-blending",
            Self::ClipDistances => "clip-distances",
            Self::Subgroups => "subgroups",
        }
    }

    /// All 14 feature flags.
    pub const ALL_FEATURES: [Self; 14] = [
        Self::DepthClipControl,
        Self::Depth32FloatStencil8,
        Self::TextureCompressionBc,
        Self::TextureCompressionEtc2,
        Self::TextureCompressionAstc,
        Self::TimestampQuery,
        Self::IndirectFirstInstance,
        Self::ShaderF16,
        Self::Rg11b10UfloatRenderable,
        Self::Bgra8UnormStorage,
        Self::Float32Filterable,
        Self::DualSourceBlending,
        Self::ClipDistances,
        Self::Subgroups,
    ];
}

impl fmt::Display for WebGpuFeature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Set of enabled features on an adapter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupportedFeatureSet {
    features: BTreeSet<WebGpuFeature>,
}

impl SupportedFeatureSet {
    /// Empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a feature.
    pub fn add(&mut self, f: WebGpuFeature) {
        self.features.insert(f);
    }

    /// Present check.
    #[must_use]
    pub fn contains(&self, f: WebGpuFeature) -> bool {
        self.features.contains(&f)
    }

    /// Iter sorted.
    pub fn iter(&self) -> impl Iterator<Item = WebGpuFeature> + '_ {
        self.features.iter().copied()
    }

    /// Size.
    #[must_use]
    pub fn len(&self) -> usize {
        self.features.len()
    }

    /// Empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}

impl FromIterator<WebGpuFeature> for SupportedFeatureSet {
    fn from_iter<I: IntoIterator<Item = WebGpuFeature>>(iter: I) -> Self {
        let mut s = Self::new();
        for f in iter {
            s.add(f);
        }
        s
    }
}

/// WebGPU `GPUSupportedLimits` snapshot (subset CSSLv3 probes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebGpuLimits {
    pub max_texture_dimension_1d: u32,
    pub max_texture_dimension_2d: u32,
    pub max_texture_dimension_3d: u32,
    pub max_texture_array_layers: u32,
    pub max_bind_groups: u32,
    pub max_bindings_per_bind_group: u32,
    pub max_dynamic_uniform_buffers_per_pipeline_layout: u32,
    pub max_dynamic_storage_buffers_per_pipeline_layout: u32,
    pub max_sampled_textures_per_shader_stage: u32,
    pub max_samplers_per_shader_stage: u32,
    pub max_storage_buffers_per_shader_stage: u32,
    pub max_storage_textures_per_shader_stage: u32,
    pub max_uniform_buffers_per_shader_stage: u32,
    pub max_uniform_buffer_binding_size: u32,
    pub max_storage_buffer_binding_size: u32,
    pub max_vertex_buffers: u32,
    pub max_buffer_size: u64,
    pub max_vertex_attributes: u32,
    pub max_vertex_buffer_array_stride: u32,
    pub max_inter_stage_shader_components: u32,
    pub max_compute_workgroup_storage_size: u32,
    pub max_compute_invocations_per_workgroup: u32,
    pub max_compute_workgroup_size_x: u32,
    pub max_compute_workgroup_size_y: u32,
    pub max_compute_workgroup_size_z: u32,
    pub max_compute_workgroups_per_dimension: u32,
}

impl WebGpuLimits {
    /// Canonical WebGPU default-limits (from the spec required-defaults table).
    #[must_use]
    pub const fn webgpu_default() -> Self {
        Self {
            max_texture_dimension_1d: 8192,
            max_texture_dimension_2d: 8192,
            max_texture_dimension_3d: 2048,
            max_texture_array_layers: 256,
            max_bind_groups: 4,
            max_bindings_per_bind_group: 1000,
            max_dynamic_uniform_buffers_per_pipeline_layout: 8,
            max_dynamic_storage_buffers_per_pipeline_layout: 4,
            max_sampled_textures_per_shader_stage: 16,
            max_samplers_per_shader_stage: 16,
            max_storage_buffers_per_shader_stage: 8,
            max_storage_textures_per_shader_stage: 4,
            max_uniform_buffers_per_shader_stage: 12,
            max_uniform_buffer_binding_size: 64 * 1024,
            max_storage_buffer_binding_size: 128 * 1024 * 1024,
            max_vertex_buffers: 8,
            max_buffer_size: 256 * 1024 * 1024,
            max_vertex_attributes: 16,
            max_vertex_buffer_array_stride: 2048,
            max_inter_stage_shader_components: 60,
            max_compute_workgroup_storage_size: 16384,
            max_compute_invocations_per_workgroup: 256,
            max_compute_workgroup_size_x: 256,
            max_compute_workgroup_size_y: 256,
            max_compute_workgroup_size_z: 64,
            max_compute_workgroups_per_dimension: 65535,
        }
    }
}

impl Default for WebGpuLimits {
    fn default() -> Self {
        Self::webgpu_default()
    }
}

#[cfg(test)]
mod tests {
    use super::{SupportedFeatureSet, WebGpuFeature, WebGpuLimits};

    #[test]
    fn feature_count() {
        assert_eq!(WebGpuFeature::ALL_FEATURES.len(), 14);
    }

    #[test]
    fn feature_names() {
        assert_eq!(WebGpuFeature::TimestampQuery.as_str(), "timestamp-query");
        assert_eq!(WebGpuFeature::ShaderF16.as_str(), "shader-f16");
        assert_eq!(
            WebGpuFeature::TextureCompressionBc.as_str(),
            "texture-compression-bc"
        );
    }

    #[test]
    fn feature_set_ops() {
        let s = SupportedFeatureSet::from_iter([
            WebGpuFeature::TimestampQuery,
            WebGpuFeature::ShaderF16,
        ]);
        assert!(s.contains(WebGpuFeature::TimestampQuery));
        assert!(s.contains(WebGpuFeature::ShaderF16));
        assert!(!s.contains(WebGpuFeature::Subgroups));
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn webgpu_default_limits_have_canonical_values() {
        let l = WebGpuLimits::webgpu_default();
        assert_eq!(l.max_texture_dimension_2d, 8192);
        assert_eq!(l.max_bind_groups, 4);
        assert_eq!(l.max_compute_invocations_per_workgroup, 256);
        assert_eq!(l.max_buffer_size, 256 * 1024 * 1024);
    }
}
