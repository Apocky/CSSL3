//! `MirModule` → DXIL container driver.
//!
//! § DESIGN
//!   The driver is intentionally thin at stage-0 : it walks the input MIR
//!   module, identifies the entry function, classifies the shader stage, and
//!   composes a [`DxbcContainer`] from the part-builders in [`crate::container`]
//!   + the bitcode-payload from [`crate::bitcode`].
//!
//!   Real per-op MIR-lowering into LLVM-bitcode instruction records is the
//!   W-G2-α follow-up slice. This slice ships the framing : every slice that
//!   adds an op-emission table appends records to the function-block in
//!   `bitcode.rs::emit_llvm_bitcode` ; the container assembly + part-ordering
//!   doesn't need to change.

use thiserror::Error;

use crate::bitcode::{emit_dxil_payload, ModuleConfig};
use crate::container::{
    build_empty_isg1, build_empty_osg1, build_minimal_root_signature, build_shex_chunk, part_tag,
    ContainerError, DxbcContainer, DxbcPart,
};
use cssl_mir::MirModule;

/// Shader stage classification — drives part-tag selection + SHEX
/// stage-class encoding + root-sig flag defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShaderStage {
    /// Compute shader (`cs_<sm>`).
    Compute,
    /// Vertex shader (`vs_<sm>`).
    Vertex,
    /// Pixel shader (`ps_<sm>`).
    Pixel,
    /// Geometry shader (`gs_<sm>`).
    Geometry,
    /// Hull / domain (tessellation) — kept for API completeness ; lowering
    /// path is the same as VS for stage-0 framing.
    Hull,
    Domain,
    /// Mesh shader (SM 6.5+).
    Mesh,
    /// Amplification shader (SM 6.5+).
    Amplification,
    /// Library — used for ray-tracing + linker inputs.
    Library,
}

impl ShaderStage {
    /// Stage-class value used in the SHEX version-token.
    /// Values match DXBC's `D3D11_SB_SHADER_TYPE` enumeration where
    /// applicable, and DXC's extension values for SM 6.x stages.
    #[must_use]
    pub const fn stage_class(self) -> u16 {
        match self {
            ShaderStage::Pixel         => 0x0000,
            ShaderStage::Vertex        => 0x0001,
            ShaderStage::Geometry      => 0x0002,
            ShaderStage::Hull          => 0x0003,
            ShaderStage::Domain        => 0x0004,
            ShaderStage::Compute       => 0x0005,
            ShaderStage::Library       => 0x0006,
            ShaderStage::Mesh          => 0x000D,
            ShaderStage::Amplification => 0x000E,
        }
    }

    /// HLSL-profile letter pair (e.g., `"cs"`, `"vs"`).
    #[must_use]
    pub const fn profile_prefix(self) -> &'static str {
        match self {
            ShaderStage::Pixel         => "ps",
            ShaderStage::Vertex        => "vs",
            ShaderStage::Geometry      => "gs",
            ShaderStage::Hull          => "hs",
            ShaderStage::Domain        => "ds",
            ShaderStage::Compute       => "cs",
            ShaderStage::Mesh          => "ms",
            ShaderStage::Amplification => "as",
            ShaderStage::Library       => "lib",
        }
    }
}

/// Shader-model selection : major.minor (e.g., 6.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShaderModel {
    pub major: u8,
    pub minor: u8,
}

impl ShaderModel {
    /// SM 6.0 — minimum DXIL.
    pub const SM_6_0: Self = Self { major: 6, minor: 0 };
    /// SM 6.5 — mesh shaders, RT 1.1, work-graphs.
    pub const SM_6_5: Self = Self { major: 6, minor: 5 };
    /// SM 6.6 — atomic ops on typed resources, dynamic resources.
    pub const SM_6_6: Self = Self { major: 6, minor: 6 };
    /// SM 6.7 — wave-matrix.
    pub const SM_6_7: Self = Self { major: 6, minor: 7 };
    /// SM 6.8 — work-graphs GA, SER.
    pub const SM_6_8: Self = Self { major: 6, minor: 8 };
}

/// Driver configuration for one MIR-module → DXIL-container compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxilLowerConfig {
    pub stage: ShaderStage,
    pub shader_model: ShaderModel,
    /// Entry-point function name as it appears in the MIR module.
    pub entry_point: String,
    /// Optional D3D12 root-signature flags. `0` = no flags. For VS+PS, set
    /// to `0x1` (`ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT`).
    pub root_signature_flags: u32,
    /// `true` to emit an `RTS0` part with a minimal serialized
    /// root-signature blob. Defaults to `true` ; D3D12 PSO creation
    /// requires either an embedded RTS0 or an externally-attached root sig.
    pub embed_root_signature: bool,
}

impl DxilLowerConfig {
    /// Default for a compute shader at SM 6.6 with embedded root-sig.
    #[must_use]
    pub fn compute_default(entry_point: impl Into<String>) -> Self {
        Self {
            stage: ShaderStage::Compute,
            shader_model: ShaderModel::SM_6_6,
            entry_point: entry_point.into(),
            root_signature_flags: 0,
            embed_root_signature: true,
        }
    }

    /// Default for a vertex shader at SM 6.6 with input-assembler flag.
    #[must_use]
    pub fn vertex_default(entry_point: impl Into<String>) -> Self {
        Self {
            stage: ShaderStage::Vertex,
            shader_model: ShaderModel::SM_6_6,
            entry_point: entry_point.into(),
            root_signature_flags: 0x1,
            embed_root_signature: true,
        }
    }

    /// Default for a pixel shader at SM 6.6.
    #[must_use]
    pub fn pixel_default(entry_point: impl Into<String>) -> Self {
        Self {
            stage: ShaderStage::Pixel,
            shader_model: ShaderModel::SM_6_6,
            entry_point: entry_point.into(),
            root_signature_flags: 0,
            embed_root_signature: true,
        }
    }
}

/// Errors produced during MIR → DXIL lowering.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DxilLowerError {
    /// The configured entry-point name doesn't appear in the MIR module's
    /// function list.
    #[error("DXIL lowering : entry-point '{name}' not found in MIR module ({available} fns available)")]
    EntryPointNotFound { name: String, available: usize },
    /// Container assembly produced a structural error (overflow, oversized
    /// part, etc.).
    #[error("DXIL container error : {0}")]
    Container(#[from] ContainerError),
}

/// Output of a successful MIR → DXIL lowering : the container bytes plus
/// the rendered shader-profile string for downstream PSO creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DxilArtifact {
    /// The DXBC container bytes — feed directly to
    /// `D3D12_SHADER_BYTECODE` / `CreateGraphicsPipelineState`.
    pub container_bytes: Vec<u8>,
    /// Rendered profile string (`"cs_6_6"`, `"vs_6_6"`, etc.).
    pub profile: String,
    /// Stage classification (informational ; matches `config.stage`).
    pub stage: ShaderStage,
}

/// Drive a `MirModule` through the from-scratch DXIL-bytecode pipeline.
///
/// # Errors
/// Returns [`DxilLowerError::EntryPointNotFound`] if the configured entry-
/// point name is absent from the MIR module ; [`DxilLowerError::Container`]
/// if container assembly fails.
pub fn lower_to_dxil(
    module: &MirModule,
    config: &DxilLowerConfig,
) -> Result<DxilArtifact, DxilLowerError> {
    // 1) verify the entry-point exists.
    let total_fns = module.funcs.len();
    let _entry = module
        .funcs
        .iter()
        .find(|f| f.name == config.entry_point)
        .ok_or_else(|| DxilLowerError::EntryPointNotFound {
            name: config.entry_point.clone(),
            available: total_fns,
        })?;

    // 2) emit DXIL bitcode payload (LLVM-bitstream).
    let bc_config = ModuleConfig::dxil_default();
    let dxil_payload = emit_dxil_payload(&bc_config, &config.entry_point);

    // 3) compose container parts. Canonical order matches DXC's output :
    //    RTS0 (if embedded) → ISG1 → OSG1 → SHEX → DXIL.
    let mut container = DxbcContainer::new();
    if config.embed_root_signature {
        container.push_part(DxbcPart::new(
            part_tag::RTS0,
            build_minimal_root_signature(config.root_signature_flags),
        ));
    }
    container.push_part(DxbcPart::new(part_tag::ISG1, build_empty_isg1()));
    container.push_part(DxbcPart::new(part_tag::OSG1, build_empty_osg1()));
    container.push_part(DxbcPart::new(
        part_tag::SHEX,
        build_shex_chunk(
            config.stage.stage_class(),
            config.shader_model.major,
            config.shader_model.minor,
        ),
    ));
    container.push_part(DxbcPart::new(part_tag::DXIL, dxil_payload));

    // 4) finish + render profile string.
    let container_bytes = container.finish()?;
    let profile = format!(
        "{}_{}_{}",
        config.stage.profile_prefix(),
        config.shader_model.major,
        config.shader_model.minor,
    );
    Ok(DxilArtifact {
        container_bytes,
        profile,
        stage: config.stage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_mir::{IntWidth, MirFunc, MirModule, MirOp, MirType, ValueId};

    fn module_with_entry(name: &str) -> MirModule {
        let mut m = MirModule::new();
        let mut f = MirFunc::new(name, vec![], vec![]);
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(ValueId(0), MirType::Int(IntWidth::I32))
                .with_attribute("value", "0"),
        );
        f.push_op(MirOp::std("func.return"));
        m.push_func(f);
        m
    }

    #[test]
    fn compute_shader_lowering_emits_dxbc_container() {
        let m = module_with_entry("main_cs");
        let cfg = DxilLowerConfig::compute_default("main_cs");
        let art = lower_to_dxil(&m, &cfg).unwrap();
        assert_eq!(art.profile, "cs_6_6");
        assert_eq!(art.stage, ShaderStage::Compute);
        // Container should start with 'DXBC' magic.
        assert_eq!(&art.container_bytes[0..4], b"DXBC");
    }

    #[test]
    fn vertex_shader_lowering_uses_vs_profile() {
        let m = module_with_entry("main_vs");
        let cfg = DxilLowerConfig::vertex_default("main_vs");
        let art = lower_to_dxil(&m, &cfg).unwrap();
        assert_eq!(art.profile, "vs_6_6");
        assert_eq!(art.stage, ShaderStage::Vertex);
    }

    #[test]
    fn pixel_shader_lowering_uses_ps_profile() {
        let m = module_with_entry("main_ps");
        let cfg = DxilLowerConfig::pixel_default("main_ps");
        let art = lower_to_dxil(&m, &cfg).unwrap();
        assert_eq!(art.profile, "ps_6_6");
        assert_eq!(art.stage, ShaderStage::Pixel);
    }

    #[test]
    fn missing_entry_point_returns_error() {
        let m = module_with_entry("main_cs");
        let cfg = DxilLowerConfig::compute_default("does_not_exist");
        let err = lower_to_dxil(&m, &cfg).unwrap_err();
        match err {
            DxilLowerError::EntryPointNotFound { name, .. } => {
                assert_eq!(name, "does_not_exist");
            }
            DxilLowerError::Container(c) => {
                panic!("expected EntryPointNotFound, got Container error: {c:?}")
            }
        }
    }

    #[test]
    fn root_signature_embedded_when_requested() {
        let m = module_with_entry("main_cs");
        let mut cfg = DxilLowerConfig::compute_default("main_cs");
        cfg.embed_root_signature = true;
        let art = lower_to_dxil(&m, &cfg).unwrap();
        // RTS0 fourcc 'RTS0' should appear in the bytes.
        let bytes = &art.container_bytes;
        let mut found = false;
        for window in bytes.windows(4) {
            if window == b"RTS0" {
                found = true;
                break;
            }
        }
        assert!(found, "RTS0 part not embedded in container");
    }

    #[test]
    fn root_signature_skipped_when_not_requested() {
        let m = module_with_entry("main_cs");
        let mut cfg = DxilLowerConfig::compute_default("main_cs");
        cfg.embed_root_signature = false;
        let art = lower_to_dxil(&m, &cfg).unwrap();
        let bytes = &art.container_bytes;
        let mut found = false;
        for window in bytes.windows(4) {
            if window == b"RTS0" {
                found = true;
                break;
            }
        }
        assert!(!found, "RTS0 part should not be embedded when disabled");
    }
}
