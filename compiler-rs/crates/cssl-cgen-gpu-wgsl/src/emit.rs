//! MIR → WGSL emitter.

use cssl_mir::{MirFunc, MirModule};
use thiserror::Error;

use crate::target::{WebGpuFeature, WebGpuStage, WgslTargetProfile};
use crate::wgsl::{WgslModule, WgslStatement};

/// Failure modes for WGSL emission.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WgslError {
    /// No entry-point fn was found in the MIR module.
    #[error(
        "MIR module has no fn `{entry}` — WGSL target {stage} requires entry-point declaration"
    )]
    EntryPointMissing { entry: String, stage: String },
    /// The fn has a body but stage-0 only emits skeletons.
    #[error(
        "fn `{fn_name}` body has {count} ops ; stage-0 emits WGSL skeletons only \
         (T10-phase-2 lowers bodies)"
    )]
    BodyNotEmpty { fn_name: String, count: usize },
}

/// Emit a `MirModule` as a stage-0 WGSL translation unit.
///
/// # Errors
/// Returns [`WgslError::EntryPointMissing`] if the entry-point fn is absent, or
/// [`WgslError::BodyNotEmpty`] if the fn already has ops.
pub fn emit_wgsl(
    module: &MirModule,
    profile: &WgslTargetProfile,
    entry_name: &str,
) -> Result<WgslModule, WgslError> {
    let Some(entry_fn) = module.find_func(entry_name) else {
        return Err(WgslError::EntryPointMissing {
            entry: entry_name.into(),
            stage: profile.stage.attribute().to_string(),
        });
    };
    let op_count: usize = entry_fn.body.blocks.iter().map(|b| b.ops.len()).sum();
    if op_count > 0 {
        return Err(WgslError::BodyNotEmpty {
            fn_name: entry_fn.name.clone(),
            count: op_count,
        });
    }

    let mut out = WgslModule::new();
    out.header = Some(format!(
        "// cssl-cgen-gpu-wgsl stage-0 emission\n\
         // profile : {}\n\
         // entry = {}",
        profile.summary(),
        entry_name,
    ));

    // Enable-directives derived from features.
    if profile.features.contains(&WebGpuFeature::ShaderF16) {
        out.push(WgslStatement::Enable("f16".into()));
    }
    if profile.features.contains(&WebGpuFeature::Subgroups) {
        out.push(WgslStatement::Enable("subgroups".into()));
    }

    // Entry fn with stage attribute + optional workgroup-size.
    let (ret_ty, params, workgroup_size) = stage_signature(profile);
    out.push(WgslStatement::EntryFunction {
        stage_attribute: profile.stage.attribute().to_string(),
        workgroup_size,
        return_type: ret_ty.map(String::from),
        name: entry_fn.name.clone(),
        params: params.iter().map(|s| (*s).to_string()).collect(),
        body: vec![
            "// stage-0 skeleton — MIR body lowered @ T10-phase-2".into(),
            format!("// profile : {}", profile.summary()),
            stage_skeleton_return(profile.stage).into(),
        ],
    });

    // Helpers.
    for f in &module.funcs {
        if f.name == entry_name {
            continue;
        }
        out.push(synthesize_helper(f));
    }

    Ok(out)
}

type StageSignature = (
    Option<&'static str>,
    &'static [&'static str],
    Option<(u32, u32, u32)>,
);

fn stage_signature(profile: &WgslTargetProfile) -> StageSignature {
    match profile.stage {
        WebGpuStage::Compute => (
            None,
            &["@builtin(global_invocation_id) gid : vec3<u32>"],
            Some((profile.limits.max_workgroup_size_x.min(64), 1, 1)),
        ),
        WebGpuStage::Vertex => (
            Some("@builtin(position) vec4<f32>"),
            &["@builtin(vertex_index) vid : u32"],
            None,
        ),
        WebGpuStage::Fragment => (
            Some("@location(0) vec4<f32>"),
            &["@builtin(position) pos : vec4<f32>"],
            None,
        ),
    }
}

fn stage_skeleton_return(stage: WebGpuStage) -> &'static str {
    match stage {
        WebGpuStage::Compute => "// no return",
        WebGpuStage::Vertex | WebGpuStage::Fragment => "return vec4<f32>(0.0, 0.0, 0.0, 1.0);",
    }
}

fn synthesize_helper(f: &MirFunc) -> WgslStatement {
    WgslStatement::HelperFunction {
        return_type: None,
        name: f.name.clone(),
        params: vec![],
        body: vec![format!(
            "// helper fn (stage-0 skeleton) — MIR params : {} ; results : {}",
            f.params.len(),
            f.results.len()
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::{emit_wgsl, WgslError};
    use crate::target::WgslTargetProfile;
    use cssl_mir::{MirFunc, MirModule};

    #[test]
    fn missing_entry_errors() {
        let module = MirModule::new();
        let err = emit_wgsl(&module, &WgslTargetProfile::compute_default(), "main").unwrap_err();
        assert!(matches!(err, WgslError::EntryPointMissing { .. }));
    }

    #[test]
    fn compute_skeleton_has_workgroup_size() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        let wgsl = emit_wgsl(&module, &WgslTargetProfile::compute_default(), "main_cs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("@compute @workgroup_size(64, 1, 1)"));
        assert!(text.contains("fn main_cs"));
    }

    #[test]
    fn vertex_skeleton_returns_position() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_vs", vec![], vec![]));
        let wgsl = emit_wgsl(&module, &WgslTargetProfile::vertex_default(), "main_vs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("@vertex\n"));
        assert!(text.contains("@builtin(position) vec4<f32>"));
        assert!(text.contains("return vec4<f32>(0.0, 0.0, 0.0, 1.0);"));
    }

    #[test]
    fn fragment_skeleton_emits_location_0() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_fs", vec![], vec![]));
        let wgsl = emit_wgsl(&module, &WgslTargetProfile::fragment_default(), "main_fs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("@fragment\n"));
        assert!(text.contains("@location(0) vec4<f32>"));
    }

    #[test]
    fn shader_f16_feature_emits_enable_directive() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        let wgsl = emit_wgsl(&module, &WgslTargetProfile::compute_default(), "main_cs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("enable f16;"));
    }

    #[test]
    fn helpers_emitted_without_stage_attribute() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        module.push_func(MirFunc::new("util", vec![], vec![]));
        let wgsl = emit_wgsl(&module, &WgslTargetProfile::compute_default(), "main_cs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("fn util()"));
    }

    #[test]
    fn header_records_profile() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        let wgsl = emit_wgsl(&module, &WgslTargetProfile::compute_default(), "main_cs").unwrap();
        let text = wgsl.render();
        assert!(text.contains("cssl-cgen-gpu-wgsl stage-0 emission"));
        assert!(text.contains("timestamp-query"));
        assert!(text.contains("entry = main_cs"));
    }
}
