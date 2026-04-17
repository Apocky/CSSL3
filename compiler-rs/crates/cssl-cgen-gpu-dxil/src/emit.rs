//! MIR → HLSL textual emitter for DXIL codegen.
//!
//! § STRATEGY
//!   Phase-1 maps each MIR-fn to an empty HLSL function body with the entry-point
//!   semantic derived from the `DxilTargetProfile.profile.stage`. Phase-2 lowers
//!   actual MIR bodies to HLSL statements + wires `dxc` subprocess compilation.

use cssl_mir::{MirFunc, MirModule};
use thiserror::Error;

use crate::hlsl::{HlslModule, HlslStatement};
use crate::target::{DxilTargetProfile, ShaderStage};

/// Failure modes for HLSL emission.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DxilError {
    /// No entry-point fn was found in the MIR module — DXIL needs at least one.
    #[error(
        "MIR module has no fn `{entry}` — DXIL target `{profile}` requires entry-point declaration"
    )]
    EntryPointMissing { entry: String, profile: String },
    /// The fn has a body but stage-0 only emits skeletons.
    #[error(
        "fn `{fn_name}` body has {count} ops ; stage-0 emits HLSL skeletons only \
         (T10-phase-2 lowers bodies)"
    )]
    BodyNotEmpty { fn_name: String, count: usize },
}

/// Emit a `MirModule` as a stage-0 HLSL translation unit.
///
/// Requires a named entry-point that matches a fn in the module.
///
/// # Errors
/// Returns [`DxilError::EntryPointMissing`] if the entry-point fn is absent, or
/// [`DxilError::BodyNotEmpty`] if the fn already has ops.
pub fn emit_hlsl(
    module: &MirModule,
    profile: &DxilTargetProfile,
    entry_name: &str,
) -> Result<HlslModule, DxilError> {
    let Some(entry_fn) = module.find_func(entry_name) else {
        return Err(DxilError::EntryPointMissing {
            entry: entry_name.into(),
            profile: profile.profile.render(),
        });
    };
    let op_count: usize = entry_fn.body.blocks.iter().map(|b| b.ops.len()).sum();
    if op_count > 0 {
        return Err(DxilError::BodyNotEmpty {
            fn_name: entry_fn.name.clone(),
            count: op_count,
        });
    }

    let mut out = HlslModule::new();
    out.header = Some(format!(
        "// cssl-cgen-gpu-dxil stage-0 HLSL emission\n\
         // profile = {} / root-sig = {}\n\
         // entry = {}",
        profile.profile.render(),
        profile.root_sig.dotted(),
        entry_name,
    ));

    // Optional Compute-stage `[numthreads(...)]` attribute placeholder.
    let mut attributes = Vec::new();
    if profile.profile.stage == ShaderStage::Compute {
        attributes.push("[numthreads(1, 1, 1)]".into());
    }

    // Emit a signature-matched skeleton.
    let semantic = stage_entry_semantic(profile.profile.stage);
    out.push(HlslStatement::Function {
        return_type: stage_entry_return_type(profile.profile.stage).into(),
        name: entry_fn.name.clone(),
        params: stage_entry_params(profile.profile.stage)
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        attributes,
        semantic: semantic.map(String::from),
        body: vec![
            "// stage-0 skeleton — MIR body lowered at T10-phase-2".into(),
            "// profile-summary : ".to_string() + &profile.summary(),
        ],
    });

    // Also emit every other fn as a stub (non-entry helpers).
    for f in &module.funcs {
        if f.name == entry_name {
            continue;
        }
        out.push(synthesize_helper_fn(f));
    }

    Ok(out)
}

fn stage_entry_return_type(stage: ShaderStage) -> &'static str {
    match stage {
        ShaderStage::Vertex => "float4",
        ShaderStage::Pixel => "float4",
        ShaderStage::Compute | ShaderStage::Mesh | ShaderStage::Amplification => "void",
        _ => "void",
    }
}

fn stage_entry_params(stage: ShaderStage) -> &'static [&'static str] {
    match stage {
        ShaderStage::Vertex => &["uint vid : SV_VertexID"],
        ShaderStage::Pixel => &["float4 pos : SV_Position"],
        ShaderStage::Compute => &["uint3 tid : SV_DispatchThreadID"],
        _ => &[],
    }
}

fn stage_entry_semantic(stage: ShaderStage) -> Option<&'static str> {
    match stage {
        ShaderStage::Vertex => Some("SV_Position"),
        ShaderStage::Pixel => Some("SV_Target0"),
        _ => None,
    }
}

fn synthesize_helper_fn(f: &MirFunc) -> HlslStatement {
    HlslStatement::Function {
        return_type: "void".into(),
        name: f.name.clone(),
        params: vec![],
        attributes: vec![],
        semantic: None,
        body: vec![format!(
            "// helper fn (stage-0 skeleton) — MIR params : {} ; results : {}",
            f.params.len(),
            f.results.len()
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::{emit_hlsl, DxilError};
    use crate::target::DxilTargetProfile;
    use cssl_mir::{MirFunc, MirModule};

    #[test]
    fn missing_entry_point_errors() {
        let module = MirModule::new();
        let err = emit_hlsl(
            &module,
            &DxilTargetProfile::compute_sm66_default(),
            "main_cs",
        )
        .unwrap_err();
        assert!(matches!(err, DxilError::EntryPointMissing { .. }));
    }

    #[test]
    fn compute_skeleton_has_numthreads() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        let profile = DxilTargetProfile::compute_sm66_default();
        let hlsl = emit_hlsl(&module, &profile, "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("[numthreads(1, 1, 1)]"));
        assert!(text.contains("void main_cs(uint3 tid : SV_DispatchThreadID)"));
    }

    #[test]
    fn vertex_skeleton_has_sv_position_semantic() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_vs", vec![], vec![]));
        let profile = DxilTargetProfile::vertex_sm66_default();
        let hlsl = emit_hlsl(&module, &profile, "main_vs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("float4 main_vs(uint vid : SV_VertexID) : SV_Position"));
    }

    #[test]
    fn pixel_skeleton_has_sv_target_semantic() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_ps", vec![], vec![]));
        let profile = DxilTargetProfile::pixel_sm66_default();
        let hlsl = emit_hlsl(&module, &profile, "main_ps").unwrap();
        let text = hlsl.render();
        assert!(text.contains("float4 main_ps(float4 pos : SV_Position) : SV_Target0"));
    }

    #[test]
    fn helper_fns_emitted_as_stubs() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        module.push_func(MirFunc::new("helper", vec![], vec![]));
        let profile = DxilTargetProfile::compute_sm66_default();
        let hlsl = emit_hlsl(&module, &profile, "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("void helper()"));
        assert!(text.contains("void main_cs"));
    }

    #[test]
    fn header_carries_profile_metadata() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        let profile = DxilTargetProfile::compute_sm66_default();
        let hlsl = emit_hlsl(&module, &profile, "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("cssl-cgen-gpu-dxil stage-0 HLSL emission"));
        assert!(text.contains("profile = cs_6_6"));
        assert!(text.contains("entry = main_cs"));
    }
}
