//! Metal feature-set enumeration (subset CSSLv3 checks).

use core::fmt;

/// `MTLFeatureSet` enumeration (subset : Apple-Silicon-era + Metal-3 families).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MetalFeatureSet {
    /// macOS-catalyst + macOS-GPUFamily1 (pre-M1).
    MacOsGpuFamily1V1,
    /// macOS-GPUFamily2 (M1 baseline).
    MacOsGpuFamily2V1,
    /// Metal-2.4 / iOS-GPU-Family6 (A13).
    IosGpuFamily6,
    /// Metal-3.0 / Apple7 (A14 / M1).
    Metal3Apple7,
    /// Metal-3.1 / Apple8 (A15 / M2).
    Metal3_1Apple8,
    /// Metal-3.1 / Apple9 (A17 / M3).
    Metal3_1Apple9,
    /// Metal-3.2 / macOS 15 / iOS 18.
    Metal3_2,
}

impl MetalFeatureSet {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MacOsGpuFamily1V1 => "macos.gpu.family1.v1",
            Self::MacOsGpuFamily2V1 => "macos.gpu.family2.v1",
            Self::IosGpuFamily6 => "ios.gpu.family6",
            Self::Metal3Apple7 => "metal3.apple7",
            Self::Metal3_1Apple8 => "metal3.1.apple8",
            Self::Metal3_1Apple9 => "metal3.1.apple9",
            Self::Metal3_2 => "metal3.2",
        }
    }

    /// Supports ray-tracing pipelines.
    #[must_use]
    pub const fn supports_raytracing(self) -> bool {
        matches!(
            self,
            Self::Metal3Apple7 | Self::Metal3_1Apple8 | Self::Metal3_1Apple9 | Self::Metal3_2
        )
    }

    /// Supports mesh shaders.
    #[must_use]
    pub const fn supports_mesh_shaders(self) -> bool {
        matches!(
            self,
            Self::Metal3Apple7 | Self::Metal3_1Apple8 | Self::Metal3_1Apple9 | Self::Metal3_2
        )
    }

    /// Supports cooperative-matrix ops (Apple-Silicon AMX-style).
    #[must_use]
    pub const fn supports_cooperative_matrix(self) -> bool {
        matches!(
            self,
            Self::Metal3_1Apple8 | Self::Metal3_1Apple9 | Self::Metal3_2
        )
    }

    /// All 7 feature sets.
    pub const ALL_FEATURE_SETS: [Self; 7] = [
        Self::MacOsGpuFamily1V1,
        Self::MacOsGpuFamily2V1,
        Self::IosGpuFamily6,
        Self::Metal3Apple7,
        Self::Metal3_1Apple8,
        Self::Metal3_1Apple9,
        Self::Metal3_2,
    ];
}

impl fmt::Display for MetalFeatureSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::MetalFeatureSet;

    #[test]
    fn feature_set_count() {
        assert_eq!(MetalFeatureSet::ALL_FEATURE_SETS.len(), 7);
    }

    #[test]
    fn feature_set_names() {
        assert_eq!(MetalFeatureSet::Metal3_1Apple9.as_str(), "metal3.1.apple9");
        assert_eq!(MetalFeatureSet::Metal3_2.as_str(), "metal3.2");
    }

    #[test]
    fn metal3_supports_raytracing() {
        assert!(MetalFeatureSet::Metal3Apple7.supports_raytracing());
        assert!(MetalFeatureSet::Metal3_1Apple9.supports_raytracing());
        assert!(MetalFeatureSet::Metal3_2.supports_raytracing());
    }

    #[test]
    fn pre_metal3_no_raytracing() {
        assert!(!MetalFeatureSet::MacOsGpuFamily1V1.supports_raytracing());
        assert!(!MetalFeatureSet::IosGpuFamily6.supports_raytracing());
    }

    #[test]
    fn only_apple8_plus_has_coop_matrix() {
        assert!(!MetalFeatureSet::Metal3Apple7.supports_cooperative_matrix());
        assert!(MetalFeatureSet::Metal3_1Apple8.supports_cooperative_matrix());
        assert!(MetalFeatureSet::Metal3_1Apple9.supports_cooperative_matrix());
    }

    #[test]
    fn mesh_shaders_metal3_plus() {
        assert!(MetalFeatureSet::Metal3Apple7.supports_mesh_shaders());
        assert!(!MetalFeatureSet::MacOsGpuFamily1V1.supports_mesh_shaders());
    }
}
