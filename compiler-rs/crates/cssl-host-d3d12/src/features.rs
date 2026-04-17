//! D3D12 feature-options catalog.

/// Wave-Matrix tier (D3D12_WAVE_MATRIX_TIER_*).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WaveMatrixTier {
    /// Tier 0 — not supported.
    NotSupported,
    /// Tier 1 — basic wave-matrix ops.
    Tier1,
    /// Tier 1.0 variant (SM 6.6+).
    Tier1_0,
}

impl WaveMatrixTier {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotSupported => "not-supported",
            Self::Tier1 => "tier1",
            Self::Tier1_0 => "tier1.0",
        }
    }
}

/// Subset of `D3D12_FEATURE_DATA_D3D12_OPTIONS*` that CSSLv3 checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct D3d12FeatureOptions {
    /// Supports Raytracing-Tier-1.1 (inline-RT / ray-query).
    pub raytracing_tier_1_1: bool,
    /// Supports Mesh-Shader-Tier-1.
    pub mesh_shader_tier_1: bool,
    /// Supports Sampler-Feedback-Tier-0.9+.
    pub sampler_feedback: bool,
    /// Supports Variable-Rate-Shading-Tier-2.
    pub vrs_tier_2: bool,
    /// Supports Atomic-Int64 on a uint64 buffer.
    pub atomic_int64: bool,
    /// Supports 16-bit float-arithmetic in shaders (SM 6.2+).
    pub shader_fp16: bool,
    /// Supports Int16 ops (SM 6.2+).
    pub shader_int16: bool,
    /// Supports Dynamic-Resources (SM 6.6+).
    pub dynamic_resources: bool,
    /// Wave-Matrix tier.
    pub wave_matrix: WaveMatrixTier,
    /// Supports Wave-Size specialization (SM 6.6+).
    pub wave_size_specialization: bool,
}

impl D3d12FeatureOptions {
    /// All-disabled baseline.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            raytracing_tier_1_1: false,
            mesh_shader_tier_1: false,
            sampler_feedback: false,
            vrs_tier_2: false,
            atomic_int64: false,
            shader_fp16: false,
            shader_int16: false,
            dynamic_resources: false,
            wave_matrix: WaveMatrixTier::NotSupported,
            wave_size_specialization: false,
        }
    }

    /// Arc A770 expected feature set (Alchemist / D3D12 ISV driver).
    #[must_use]
    pub const fn arc_a770() -> Self {
        Self {
            raytracing_tier_1_1: true,
            mesh_shader_tier_1: true,
            sampler_feedback: true,
            vrs_tier_2: true,
            atomic_int64: true,
            shader_fp16: true,
            shader_int16: true,
            dynamic_resources: true,
            wave_matrix: WaveMatrixTier::Tier1,
            wave_size_specialization: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{D3d12FeatureOptions, WaveMatrixTier};

    #[test]
    fn wave_matrix_names() {
        assert_eq!(WaveMatrixTier::NotSupported.as_str(), "not-supported");
        assert_eq!(WaveMatrixTier::Tier1.as_str(), "tier1");
    }

    #[test]
    fn none_all_off() {
        let f = D3d12FeatureOptions::none();
        assert!(!f.raytracing_tier_1_1);
        assert!(!f.mesh_shader_tier_1);
        assert_eq!(f.wave_matrix, WaveMatrixTier::NotSupported);
    }

    #[test]
    fn arc_a770_enables_rt_and_mesh() {
        let f = D3d12FeatureOptions::arc_a770();
        assert!(f.raytracing_tier_1_1);
        assert!(f.mesh_shader_tier_1);
        assert!(f.dynamic_resources);
        assert_eq!(f.wave_matrix, WaveMatrixTier::Tier1);
    }
}
