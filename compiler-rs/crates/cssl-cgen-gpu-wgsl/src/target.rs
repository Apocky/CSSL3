//! WGSL target enums + limits.
//!
//! § SPEC : `specs/14_BACKEND.csl` § OWNED WGSL EMITTER +
//!         `specs/07_CODEGEN.csl` § WGSL path.

use core::fmt;
use std::collections::BTreeSet;

/// WebGPU pipeline stage (1:1 with `@vertex` / `@fragment` / `@compute` attributes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebGpuStage {
    /// Vertex stage.
    Vertex,
    /// Fragment stage.
    Fragment,
    /// Compute stage.
    Compute,
}

impl WebGpuStage {
    /// WGSL attribute form (`"@vertex"` / `"@fragment"` / `"@compute"`).
    #[must_use]
    pub const fn attribute(self) -> &'static str {
        match self {
            Self::Vertex => "@vertex",
            Self::Fragment => "@fragment",
            Self::Compute => "@compute",
        }
    }

    /// All 3 stages.
    pub const ALL_STAGES: [Self; 3] = [Self::Vertex, Self::Fragment, Self::Compute];
}

impl fmt::Display for WebGpuStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.attribute())
    }
}

/// Optional WebGPU feature flags that CSSLv3 codegen may need.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WebGpuFeature {
    /// `float32-filterable` — bilinear sampling on f32 textures.
    Float32Filterable,
    /// `shader-f16` — 16-bit float ops.
    ShaderF16,
    /// `timestamp-query` — R18 telemetry hook.
    TimestampQuery,
    /// `subgroups` — WGSL subgroup-op extension (Chrome flag).
    Subgroups,
    /// `dual-source-blending`.
    DualSourceBlending,
    /// `bgra8unorm-storage` — storage-texture on bgra8unorm.
    Bgra8UnormStorage,
    /// `clip-distances`.
    ClipDistances,
}

impl WebGpuFeature {
    /// Canonical WebGPU feature-name string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Float32Filterable => "float32-filterable",
            Self::ShaderF16 => "shader-f16",
            Self::TimestampQuery => "timestamp-query",
            Self::Subgroups => "subgroups",
            Self::DualSourceBlending => "dual-source-blending",
            Self::Bgra8UnormStorage => "bgra8unorm-storage",
            Self::ClipDistances => "clip-distances",
        }
    }
}

/// WebGPU workgroup-size limits + bind-group / storage-buffer / dispatch limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WgslLimits {
    /// Max workgroup-size X (WebGPU default : 256).
    pub max_workgroup_size_x: u32,
    /// Max workgroup-size Y (WebGPU default : 256).
    pub max_workgroup_size_y: u32,
    /// Max workgroup-size Z (WebGPU default : 64).
    pub max_workgroup_size_z: u32,
    /// Max total workgroup invocations (WebGPU default : 256).
    pub max_workgroup_invocations: u32,
    /// Max bind-groups per pipeline-layout (WebGPU default : 4).
    pub max_bind_groups: u32,
    /// Max storage-buffers per shader-stage (WebGPU default : 8).
    pub max_storage_buffers_per_stage: u32,
    /// Max storage-textures per shader-stage (WebGPU default : 4).
    pub max_storage_textures_per_stage: u32,
    /// Max uniform-buffers per shader-stage (WebGPU default : 12).
    pub max_uniform_buffers_per_stage: u32,
}

impl WgslLimits {
    /// Canonical WebGPU default limits.
    #[must_use]
    pub const fn webgpu_default() -> Self {
        Self {
            max_workgroup_size_x: 256,
            max_workgroup_size_y: 256,
            max_workgroup_size_z: 64,
            max_workgroup_invocations: 256,
            max_bind_groups: 4,
            max_storage_buffers_per_stage: 8,
            max_storage_textures_per_stage: 4,
            max_uniform_buffers_per_stage: 12,
        }
    }

    /// "Compat" preset (lowered limits for broader device compatibility).
    #[must_use]
    pub const fn compat() -> Self {
        Self {
            max_workgroup_size_x: 128,
            max_workgroup_size_y: 128,
            max_workgroup_size_z: 32,
            max_workgroup_invocations: 128,
            max_bind_groups: 2,
            max_storage_buffers_per_stage: 4,
            max_storage_textures_per_stage: 2,
            max_uniform_buffers_per_stage: 8,
        }
    }
}

impl Default for WgslLimits {
    fn default() -> Self {
        Self::webgpu_default()
    }
}

/// WGSL target-profile bundle : stage + limits + enabled-features.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgslTargetProfile {
    /// WebGPU pipeline stage.
    pub stage: WebGpuStage,
    /// WebGPU limits.
    pub limits: WgslLimits,
    /// Enabled WebGPU features.
    pub features: BTreeSet<WebGpuFeature>,
}

impl WgslTargetProfile {
    /// Default compute profile : webgpu-default limits + timestamp-query + shader-f16.
    #[must_use]
    pub fn compute_default() -> Self {
        let mut features = BTreeSet::new();
        features.insert(WebGpuFeature::TimestampQuery);
        features.insert(WebGpuFeature::ShaderF16);
        Self {
            stage: WebGpuStage::Compute,
            limits: WgslLimits::webgpu_default(),
            features,
        }
    }

    /// Default vertex profile.
    #[must_use]
    pub fn vertex_default() -> Self {
        Self {
            stage: WebGpuStage::Vertex,
            limits: WgslLimits::webgpu_default(),
            features: BTreeSet::new(),
        }
    }

    /// Default fragment profile.
    #[must_use]
    pub fn fragment_default() -> Self {
        let mut features = BTreeSet::new();
        features.insert(WebGpuFeature::Float32Filterable);
        Self {
            stage: WebGpuStage::Fragment,
            limits: WgslLimits::webgpu_default(),
            features,
        }
    }

    /// Diagnostic summary.
    #[must_use]
    pub fn summary(&self) -> String {
        let features: Vec<&str> = self.features.iter().map(|f| f.as_str()).collect();
        format!(
            "WGSL / {} / max-wg=({}x{}x{}) / bind-groups={} / features=[{}]",
            self.stage.attribute(),
            self.limits.max_workgroup_size_x,
            self.limits.max_workgroup_size_y,
            self.limits.max_workgroup_size_z,
            self.limits.max_bind_groups,
            features.join(","),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{WebGpuFeature, WebGpuStage, WgslLimits, WgslTargetProfile};

    #[test]
    fn stage_attributes() {
        assert_eq!(WebGpuStage::Vertex.attribute(), "@vertex");
        assert_eq!(WebGpuStage::Fragment.attribute(), "@fragment");
        assert_eq!(WebGpuStage::Compute.attribute(), "@compute");
    }

    #[test]
    fn stage_count() {
        assert_eq!(WebGpuStage::ALL_STAGES.len(), 3);
    }

    #[test]
    fn feature_names() {
        assert_eq!(WebGpuFeature::TimestampQuery.as_str(), "timestamp-query");
        assert_eq!(WebGpuFeature::ShaderF16.as_str(), "shader-f16");
        assert_eq!(WebGpuFeature::Subgroups.as_str(), "subgroups");
    }

    #[test]
    fn webgpu_default_limits() {
        let l = WgslLimits::webgpu_default();
        assert_eq!(l.max_workgroup_size_x, 256);
        assert_eq!(l.max_workgroup_invocations, 256);
        assert_eq!(l.max_bind_groups, 4);
    }

    #[test]
    fn compat_limits_lower_than_default() {
        let default_l = WgslLimits::webgpu_default();
        let compat_l = WgslLimits::compat();
        assert!(compat_l.max_workgroup_size_x <= default_l.max_workgroup_size_x);
        assert!(compat_l.max_bind_groups <= default_l.max_bind_groups);
    }

    #[test]
    fn compute_default_profile_has_timestamp_query() {
        let p = WgslTargetProfile::compute_default();
        assert!(p.features.contains(&WebGpuFeature::TimestampQuery));
        assert!(p.features.contains(&WebGpuFeature::ShaderF16));
        assert_eq!(p.stage, WebGpuStage::Compute);
    }

    #[test]
    fn vertex_default_profile() {
        let p = WgslTargetProfile::vertex_default();
        assert_eq!(p.stage, WebGpuStage::Vertex);
        assert!(p.features.is_empty());
    }

    #[test]
    fn fragment_default_profile_has_float32_filterable() {
        let p = WgslTargetProfile::fragment_default();
        assert_eq!(p.stage, WebGpuStage::Fragment);
        assert!(p.features.contains(&WebGpuFeature::Float32Filterable));
    }

    #[test]
    fn summary_shape() {
        let p = WgslTargetProfile::compute_default();
        let s = p.summary();
        assert!(s.contains("@compute"));
        assert!(s.contains("max-wg=(256x256x64)"));
        assert!(s.contains("timestamp-query"));
    }
}
