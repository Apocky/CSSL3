//! DXIL target enums : shader-model + shader-stage + HLSL profile string.
//!
//! § SPEC : `specs/07_CODEGEN.csl` § GPU BACKEND — DXIL path + `specs/10_HW.csl`.

use core::fmt;

/// HLSL shader-model version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShaderModel {
    /// SM 6.0 — DXIL baseline.
    Sm60,
    /// SM 6.1 — SV_* builtins.
    Sm61,
    /// SM 6.2 — FP16 + Int16.
    Sm62,
    /// SM 6.3 — Ray-tracing pipelines.
    Sm63,
    /// SM 6.4 — int packed-dot + low-precision.
    Sm64,
    /// SM 6.5 — inline ray-tracing (ray-query) + mesh-shaders.
    Sm65,
    /// SM 6.6 — 64-bit atomics + dynamic resources + wave-matrix.
    Sm66,
    /// SM 6.7 — advanced textures + sampler-feedback.
    Sm67,
    /// SM 6.8 — cooperative-matrix + work-graphs.
    Sm68,
}

impl ShaderModel {
    /// Short profile-form numeric suffix (`"6_6"`).
    #[must_use]
    pub const fn profile_suffix(self) -> &'static str {
        match self {
            Self::Sm60 => "6_0",
            Self::Sm61 => "6_1",
            Self::Sm62 => "6_2",
            Self::Sm63 => "6_3",
            Self::Sm64 => "6_4",
            Self::Sm65 => "6_5",
            Self::Sm66 => "6_6",
            Self::Sm67 => "6_7",
            Self::Sm68 => "6_8",
        }
    }

    /// Dotted form (`"6.6"`).
    #[must_use]
    pub const fn dotted(self) -> &'static str {
        match self {
            Self::Sm60 => "6.0",
            Self::Sm61 => "6.1",
            Self::Sm62 => "6.2",
            Self::Sm63 => "6.3",
            Self::Sm64 => "6.4",
            Self::Sm65 => "6.5",
            Self::Sm66 => "6.6",
            Self::Sm67 => "6.7",
            Self::Sm68 => "6.8",
        }
    }

    /// All 9 shader models.
    pub const ALL_MODELS: [Self; 9] = [
        Self::Sm60,
        Self::Sm61,
        Self::Sm62,
        Self::Sm63,
        Self::Sm64,
        Self::Sm65,
        Self::Sm66,
        Self::Sm67,
        Self::Sm68,
    ];
}

impl fmt::Display for ShaderModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.dotted())
    }
}

/// HLSL shader-stage (corresponds 1:1 to dxc `-T <profile>` prefixes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderStage {
    /// Vertex shader.
    Vertex,
    /// Pixel (fragment) shader.
    Pixel,
    /// Compute shader.
    Compute,
    /// Geometry shader (legacy).
    Geometry,
    /// Hull (tessellation control) shader.
    Hull,
    /// Domain (tessellation eval) shader.
    Domain,
    /// Mesh shader (SM 6.5+).
    Mesh,
    /// Amplification (task) shader.
    Amplification,
    /// Shader library (ray-tracing + DXR 1.0 + Work-Graphs).
    Lib,
    /// Ray-generation shader (DXR).
    RayGeneration,
    /// Closest-hit shader (DXR).
    ClosestHit,
    /// Any-hit shader (DXR).
    AnyHit,
    /// Miss shader (DXR).
    Miss,
    /// Intersection shader (DXR).
    Intersection,
    /// Callable shader (DXR).
    Callable,
}

impl ShaderStage {
    /// HLSL-profile stage prefix (`"cs"`, `"ps"`, `"lib"`, `"raygeneration"` etc.).
    #[must_use]
    pub const fn profile_prefix(self) -> &'static str {
        match self {
            Self::Vertex => "vs",
            Self::Pixel => "ps",
            Self::Compute => "cs",
            Self::Geometry => "gs",
            Self::Hull => "hs",
            Self::Domain => "ds",
            Self::Mesh => "ms",
            Self::Amplification => "as",
            Self::Lib => "lib",
            Self::RayGeneration => "raygeneration",
            Self::ClosestHit => "closesthit",
            Self::AnyHit => "anyhit",
            Self::Miss => "miss",
            Self::Intersection => "intersection",
            Self::Callable => "callable",
        }
    }

    /// Minimum shader model that supports this stage.
    #[must_use]
    pub const fn min_shader_model(self) -> ShaderModel {
        match self {
            Self::Vertex
            | Self::Pixel
            | Self::Compute
            | Self::Geometry
            | Self::Hull
            | Self::Domain => ShaderModel::Sm60,
            Self::Lib
            | Self::RayGeneration
            | Self::ClosestHit
            | Self::AnyHit
            | Self::Miss
            | Self::Intersection
            | Self::Callable => ShaderModel::Sm63,
            Self::Mesh | Self::Amplification => ShaderModel::Sm65,
        }
    }

    /// All 15 shader stages.
    pub const ALL_STAGES: [Self; 15] = [
        Self::Vertex,
        Self::Pixel,
        Self::Compute,
        Self::Geometry,
        Self::Hull,
        Self::Domain,
        Self::Mesh,
        Self::Amplification,
        Self::Lib,
        Self::RayGeneration,
        Self::ClosestHit,
        Self::AnyHit,
        Self::Miss,
        Self::Intersection,
        Self::Callable,
    ];
}

impl fmt::Display for ShaderStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.profile_prefix())
    }
}

/// Combined (stage + shader-model) profile string — what dxc `-T` expects (`"cs_6_6"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HlslProfile {
    pub stage: ShaderStage,
    pub model: ShaderModel,
}

impl HlslProfile {
    /// Build a profile and validate the stage/model pair.
    ///
    /// # Errors
    /// Returns `None` when `model < stage.min_shader_model()`.
    #[must_use]
    pub fn new(stage: ShaderStage, model: ShaderModel) -> Option<Self> {
        if model < stage.min_shader_model() {
            None
        } else {
            Some(Self { stage, model })
        }
    }

    /// Render as `"<prefix>_<suffix>"` (e.g., `"cs_6_6"`).
    #[must_use]
    pub fn render(self) -> String {
        format!(
            "{}_{}",
            self.stage.profile_prefix(),
            self.model.profile_suffix()
        )
    }
}

impl fmt::Display for HlslProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

/// D3D12 root-signature format version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RootSignatureVersion {
    /// Root-signature version 1.0 (DX12 launch).
    V1_0,
    /// Root-signature version 1.1 (DX12 2017 — adds static-descriptors + data-volatile hints).
    V1_1,
    /// Root-signature version 1.2 (DX12 Agility — adds static-samplers 2.0).
    V1_2,
}

impl RootSignatureVersion {
    /// Dotted form (`"1.1"`).
    #[must_use]
    pub const fn dotted(self) -> &'static str {
        match self {
            Self::V1_0 => "1.0",
            Self::V1_1 => "1.1",
            Self::V1_2 => "1.2",
        }
    }
}

/// Full DXIL target-profile bundle : HLSL profile + root-signature version + wave-size + flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxilTargetProfile {
    /// HLSL stage + shader-model profile.
    pub profile: HlslProfile,
    /// Root-signature version.
    pub root_sig: RootSignatureVersion,
    /// Wave-size (subgroup-size) hint — `None` = driver default (typically 32 on Intel).
    pub wave_size: Option<u32>,
    /// Whether to enable 16-bit floats (SM 6.2+).
    pub enable_16_bit_types: bool,
    /// Whether to enable dynamic-resources (SM 6.6+).
    pub enable_dynamic_resources: bool,
}

impl DxilTargetProfile {
    /// Default profile for compute @ SM 6.6 + root-sig 1.1.
    #[must_use]
    pub fn compute_sm66_default() -> Self {
        Self {
            profile: HlslProfile::new(ShaderStage::Compute, ShaderModel::Sm66).unwrap(),
            root_sig: RootSignatureVersion::V1_1,
            wave_size: None,
            enable_16_bit_types: true,
            enable_dynamic_resources: true,
        }
    }

    /// Default profile for vertex @ SM 6.6 + root-sig 1.1.
    #[must_use]
    pub fn vertex_sm66_default() -> Self {
        Self {
            profile: HlslProfile::new(ShaderStage::Vertex, ShaderModel::Sm66).unwrap(),
            root_sig: RootSignatureVersion::V1_1,
            wave_size: None,
            enable_16_bit_types: true,
            enable_dynamic_resources: true,
        }
    }

    /// Default profile for pixel @ SM 6.6 + root-sig 1.1.
    #[must_use]
    pub fn pixel_sm66_default() -> Self {
        Self {
            profile: HlslProfile::new(ShaderStage::Pixel, ShaderModel::Sm66).unwrap(),
            root_sig: RootSignatureVersion::V1_1,
            wave_size: None,
            enable_16_bit_types: true,
            enable_dynamic_resources: true,
        }
    }

    /// Diagnostic summary.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut flags: Vec<&str> = Vec::new();
        if self.enable_16_bit_types {
            flags.push("16bit");
        }
        if self.enable_dynamic_resources {
            flags.push("dyn-res");
        }
        if let Some(w) = self.wave_size {
            flags.push("wave");
            format!(
                "{} / rs{} / wave={} / {}",
                self.profile.render(),
                self.root_sig.dotted(),
                w,
                flags.join("+"),
            )
        } else {
            format!(
                "{} / rs{} / {}",
                self.profile.render(),
                self.root_sig.dotted(),
                flags.join("+"),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DxilTargetProfile, HlslProfile, RootSignatureVersion, ShaderModel, ShaderStage};

    #[test]
    fn shader_model_profile_suffix() {
        assert_eq!(ShaderModel::Sm60.profile_suffix(), "6_0");
        assert_eq!(ShaderModel::Sm66.profile_suffix(), "6_6");
        assert_eq!(ShaderModel::Sm68.profile_suffix(), "6_8");
    }

    #[test]
    fn shader_model_dotted() {
        assert_eq!(ShaderModel::Sm66.dotted(), "6.6");
    }

    #[test]
    fn shader_model_count() {
        assert_eq!(ShaderModel::ALL_MODELS.len(), 9);
    }

    #[test]
    fn shader_stage_profile_prefix() {
        assert_eq!(ShaderStage::Compute.profile_prefix(), "cs");
        assert_eq!(ShaderStage::Pixel.profile_prefix(), "ps");
        assert_eq!(ShaderStage::Lib.profile_prefix(), "lib");
        assert_eq!(ShaderStage::RayGeneration.profile_prefix(), "raygeneration");
    }

    #[test]
    fn shader_stage_count() {
        assert_eq!(ShaderStage::ALL_STAGES.len(), 15);
    }

    #[test]
    fn stage_min_shader_model() {
        assert_eq!(ShaderStage::Compute.min_shader_model(), ShaderModel::Sm60);
        assert_eq!(
            ShaderStage::RayGeneration.min_shader_model(),
            ShaderModel::Sm63
        );
        assert_eq!(ShaderStage::Mesh.min_shader_model(), ShaderModel::Sm65);
    }

    #[test]
    fn hlsl_profile_renders() {
        let p = HlslProfile::new(ShaderStage::Compute, ShaderModel::Sm66).unwrap();
        assert_eq!(p.render(), "cs_6_6");
        assert_eq!(format!("{p}"), "cs_6_6");
    }

    #[test]
    fn hlsl_profile_rejects_too_low_model() {
        assert!(HlslProfile::new(ShaderStage::Mesh, ShaderModel::Sm63).is_none());
        assert!(HlslProfile::new(ShaderStage::Mesh, ShaderModel::Sm65).is_some());
    }

    #[test]
    fn root_sig_dotted() {
        assert_eq!(RootSignatureVersion::V1_0.dotted(), "1.0");
        assert_eq!(RootSignatureVersion::V1_1.dotted(), "1.1");
    }

    #[test]
    fn compute_default_profile() {
        let p = DxilTargetProfile::compute_sm66_default();
        assert_eq!(p.profile.render(), "cs_6_6");
        assert_eq!(p.root_sig, RootSignatureVersion::V1_1);
        assert!(p.enable_16_bit_types);
        assert!(p.enable_dynamic_resources);
    }

    #[test]
    fn vertex_default_profile() {
        let p = DxilTargetProfile::vertex_sm66_default();
        assert_eq!(p.profile.render(), "vs_6_6");
    }

    #[test]
    fn pixel_default_profile() {
        let p = DxilTargetProfile::pixel_sm66_default();
        assert_eq!(p.profile.render(), "ps_6_6");
    }

    #[test]
    fn summary_contains_profile_and_rs() {
        let p = DxilTargetProfile::compute_sm66_default();
        let s = p.summary();
        assert!(s.contains("cs_6_6"));
        assert!(s.contains("rs1.1"));
    }
}
