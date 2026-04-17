//! MSL target enums.
//!
//! § SPEC : `specs/14_BACKEND.csl` § OWNED MSL EMITTER + `specs/07_CODEGEN.csl` § MSL path.

use core::fmt;

/// Metal Shading Language version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MslVersion {
    /// MSL 2.0 (macOS 10.13 + iOS 11) — pre-argument-buffer tier-2.
    V2_0,
    /// MSL 2.1 (macOS 10.14 + iOS 12).
    V2_1,
    /// MSL 2.2 (macOS 10.15 + iOS 13).
    V2_2,
    /// MSL 2.3 (macOS 11 + iOS 14) — baseline for Apple-Silicon.
    V2_3,
    /// MSL 2.4 (macOS 12 + iOS 15) — mesh-shaders + function-pointers.
    V2_4,
    /// MSL 3.0 (macOS 13 + iOS 16) — cooperative-matrix + inline-RT.
    V3_0,
    /// MSL 3.1 (macOS 14 + iOS 17) — visible-fn-tables + dynamic-libraries.
    V3_1,
    /// MSL 3.2 (macOS 15 + iOS 18) — ray-tracing acceleration-structure updates.
    V3_2,
}

impl MslVersion {
    /// Compiler-flag form (`"2.4"` / `"3.0"` / `"3.2"`).
    #[must_use]
    pub const fn dotted(self) -> &'static str {
        match self {
            Self::V2_0 => "2.0",
            Self::V2_1 => "2.1",
            Self::V2_2 => "2.2",
            Self::V2_3 => "2.3",
            Self::V2_4 => "2.4",
            Self::V3_0 => "3.0",
            Self::V3_1 => "3.1",
            Self::V3_2 => "3.2",
        }
    }

    /// Underscored form for identifier use (`"3_0"`).
    #[must_use]
    pub const fn underscored(self) -> &'static str {
        match self {
            Self::V2_0 => "2_0",
            Self::V2_1 => "2_1",
            Self::V2_2 => "2_2",
            Self::V2_3 => "2_3",
            Self::V2_4 => "2_4",
            Self::V3_0 => "3_0",
            Self::V3_1 => "3_1",
            Self::V3_2 => "3_2",
        }
    }

    /// All 8 versions.
    pub const ALL_VERSIONS: [Self; 8] = [
        Self::V2_0,
        Self::V2_1,
        Self::V2_2,
        Self::V2_3,
        Self::V2_4,
        Self::V3_0,
        Self::V3_1,
        Self::V3_2,
    ];
}

impl fmt::Display for MslVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.dotted())
    }
}

/// Metal shader-stage (uniformly-typed attribute `[[<stage>]]` in MSL source).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetalStage {
    /// Vertex function.
    Vertex,
    /// Fragment function.
    Fragment,
    /// Compute kernel.
    Kernel,
    /// Object (task / amplification) shader — Metal-3+.
    Object,
    /// Mesh shader — Metal-3+.
    Mesh,
    /// Tile-shader (on-tile compute on Apple-Silicon iOS/macOS-M).
    Tile,
    /// Visible function (intersection / callable shaders in ray-tracing pipelines).
    VisibleFunction,
}

impl MetalStage {
    /// MSL attribute form.
    #[must_use]
    pub const fn attribute(self) -> &'static str {
        match self {
            Self::Vertex => "[[vertex]]",
            Self::Fragment => "[[fragment]]",
            Self::Kernel => "[[kernel]]",
            Self::Object => "[[object]]",
            Self::Mesh => "[[mesh]]",
            Self::Tile => "[[tile]]",
            Self::VisibleFunction => "[[visible]]",
        }
    }

    /// Minimum MSL version for this stage.
    #[must_use]
    pub const fn min_msl_version(self) -> MslVersion {
        match self {
            Self::Vertex | Self::Fragment | Self::Kernel => MslVersion::V2_0,
            Self::Tile => MslVersion::V2_3,
            Self::Object | Self::Mesh => MslVersion::V2_4,
            Self::VisibleFunction => MslVersion::V3_1,
        }
    }

    /// All 7 stages.
    pub const ALL_STAGES: [Self; 7] = [
        Self::Vertex,
        Self::Fragment,
        Self::Kernel,
        Self::Object,
        Self::Mesh,
        Self::Tile,
        Self::VisibleFunction,
    ];
}

impl fmt::Display for MetalStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.attribute())
    }
}

/// Metal host platform (gates feature-availability).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetalPlatform {
    /// macOS (Intel + Apple-Silicon).
    MacOs,
    /// iOS / iPadOS (Apple-Silicon only).
    IOs,
    /// tvOS.
    TvOs,
    /// visionOS (Apple-Silicon).
    VisionOs,
}

impl MetalPlatform {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MacOs => "macos",
            Self::IOs => "ios",
            Self::TvOs => "tvos",
            Self::VisionOs => "visionos",
        }
    }
}

/// Argument-buffer tier declared in metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArgumentBufferTier {
    /// Tier 1 (no nested structures).
    Tier1,
    /// Tier 2 (nested + indirect-command-buffers + heap-resources).
    Tier2,
}

impl ArgumentBufferTier {
    /// MSL metadata string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tier1 => "tier1",
            Self::Tier2 => "tier2",
        }
    }
}

/// MSL target-profile bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MslTargetProfile {
    /// MSL version.
    pub version: MslVersion,
    /// Host platform.
    pub platform: MetalPlatform,
    /// Shader stage.
    pub stage: MetalStage,
    /// Argument-buffer tier declared.
    pub argument_buffer_tier: ArgumentBufferTier,
    /// Enable `-ffast-math` equivalent.
    pub fast_math: bool,
}

impl MslTargetProfile {
    /// Default kernel profile : MSL 3.0 + macOS + Tier-2 + fast-math.
    #[must_use]
    pub fn kernel_default() -> Self {
        Self {
            version: MslVersion::V3_0,
            platform: MetalPlatform::MacOs,
            stage: MetalStage::Kernel,
            argument_buffer_tier: ArgumentBufferTier::Tier2,
            fast_math: true,
        }
    }

    /// Default vertex profile.
    #[must_use]
    pub fn vertex_default() -> Self {
        Self {
            version: MslVersion::V3_0,
            platform: MetalPlatform::MacOs,
            stage: MetalStage::Vertex,
            argument_buffer_tier: ArgumentBufferTier::Tier2,
            fast_math: true,
        }
    }

    /// Default fragment profile.
    #[must_use]
    pub fn fragment_default() -> Self {
        Self {
            version: MslVersion::V3_0,
            platform: MetalPlatform::MacOs,
            stage: MetalStage::Fragment,
            argument_buffer_tier: ArgumentBufferTier::Tier2,
            fast_math: true,
        }
    }

    /// Diagnostic summary.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "MSL {} / {} / {} / arg-buffers={} / fast-math={}",
            self.version.dotted(),
            self.platform.as_str(),
            self.stage.attribute(),
            self.argument_buffer_tier.as_str(),
            self.fast_math,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{ArgumentBufferTier, MetalPlatform, MetalStage, MslTargetProfile, MslVersion};

    #[test]
    fn version_dotted_forms() {
        assert_eq!(MslVersion::V3_0.dotted(), "3.0");
        assert_eq!(MslVersion::V2_4.dotted(), "2.4");
    }

    #[test]
    fn version_underscored_forms() {
        assert_eq!(MslVersion::V3_0.underscored(), "3_0");
    }

    #[test]
    fn version_count() {
        assert_eq!(MslVersion::ALL_VERSIONS.len(), 8);
    }

    #[test]
    fn stage_attributes() {
        assert_eq!(MetalStage::Vertex.attribute(), "[[vertex]]");
        assert_eq!(MetalStage::Kernel.attribute(), "[[kernel]]");
        assert_eq!(MetalStage::Mesh.attribute(), "[[mesh]]");
    }

    #[test]
    fn stage_count() {
        assert_eq!(MetalStage::ALL_STAGES.len(), 7);
    }

    #[test]
    fn stage_min_version_ordering() {
        assert!(MetalStage::Mesh.min_msl_version() >= MslVersion::V2_4);
        assert!(MetalStage::VisibleFunction.min_msl_version() >= MslVersion::V3_1);
    }

    #[test]
    fn platform_names() {
        assert_eq!(MetalPlatform::MacOs.as_str(), "macos");
        assert_eq!(MetalPlatform::IOs.as_str(), "ios");
    }

    #[test]
    fn argument_buffer_tier_names() {
        assert_eq!(ArgumentBufferTier::Tier1.as_str(), "tier1");
        assert_eq!(ArgumentBufferTier::Tier2.as_str(), "tier2");
    }

    #[test]
    fn kernel_default_profile_summary() {
        let p = MslTargetProfile::kernel_default();
        let s = p.summary();
        assert!(s.contains("MSL 3.0"));
        assert!(s.contains("kernel"));
        assert!(s.contains("tier2"));
    }

    #[test]
    fn vertex_default_profile() {
        let p = MslTargetProfile::vertex_default();
        assert_eq!(p.stage, MetalStage::Vertex);
    }

    #[test]
    fn fragment_default_profile() {
        let p = MslTargetProfile::fragment_default();
        assert_eq!(p.stage, MetalStage::Fragment);
    }
}
