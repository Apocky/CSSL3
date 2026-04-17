//! MIR → MSL emitter.

use cssl_mir::{MirFunc, MirModule};
use thiserror::Error;

use crate::msl::{MslModule, MslStatement};
use crate::target::{MetalStage, MslTargetProfile};

/// Failure modes for MSL emission.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MslError {
    /// No entry-point fn was found in the MIR module.
    #[error(
        "MIR module has no fn `{entry}` — MSL target {stage} requires entry-point declaration"
    )]
    EntryPointMissing { entry: String, stage: String },
    /// The fn has a body but stage-0 only emits skeletons.
    #[error(
        "fn `{fn_name}` body has {count} ops ; stage-0 emits MSL skeletons only \
         (T10-phase-2 lowers bodies)"
    )]
    BodyNotEmpty { fn_name: String, count: usize },
}

/// Emit a `MirModule` as a stage-0 MSL translation unit.
///
/// # Errors
/// Returns [`MslError::EntryPointMissing`] if the entry-point fn is absent, or
/// [`MslError::BodyNotEmpty`] if the fn already has ops.
pub fn emit_msl(
    module: &MirModule,
    profile: &MslTargetProfile,
    entry_name: &str,
) -> Result<MslModule, MslError> {
    let Some(entry_fn) = module.find_func(entry_name) else {
        return Err(MslError::EntryPointMissing {
            entry: entry_name.into(),
            stage: profile.stage.attribute().to_string(),
        });
    };
    let op_count: usize = entry_fn.body.blocks.iter().map(|b| b.ops.len()).sum();
    if op_count > 0 {
        return Err(MslError::BodyNotEmpty {
            fn_name: entry_fn.name.clone(),
            count: op_count,
        });
    }

    let mut out = MslModule::new();
    out.header = Some(format!(
        "// cssl-cgen-gpu-msl stage-0 emission\n\
         // profile : {}\n\
         // entry = {}",
        profile.summary(),
        entry_name,
    ));
    out.seed_prelude();

    // Entry fn skeleton per stage.
    let (ret_ty, params) = stage_signature(profile.stage);
    out.push(MslStatement::Function {
        stage_attribute: Some(profile.stage.attribute().to_string()),
        return_type: ret_ty.into(),
        name: entry_fn.name.clone(),
        params: params.iter().map(|s| (*s).to_string()).collect(),
        body: vec![
            "// stage-0 skeleton — MIR body lowered @ T10-phase-2".into(),
            format!("// profile : {}", profile.summary()),
        ],
    });

    // Helper fn stubs.
    for f in &module.funcs {
        if f.name == entry_name {
            continue;
        }
        out.push(synthesize_helper(f));
    }

    Ok(out)
}

fn stage_signature(stage: MetalStage) -> (&'static str, &'static [&'static str]) {
    match stage {
        MetalStage::Kernel => (
            "void",
            &[
                "uint3 gid [[thread_position_in_grid]]",
                "device float* out [[buffer(0)]]",
            ],
        ),
        MetalStage::Vertex => ("float4", &["uint vid [[vertex_id]]"]),
        MetalStage::Fragment => ("float4", &["float4 pos [[position]]"]),
        MetalStage::Object | MetalStage::Mesh | MetalStage::Tile => ("void", &[]),
        MetalStage::VisibleFunction => ("void", &[]),
    }
}

fn synthesize_helper(f: &MirFunc) -> MslStatement {
    MslStatement::Function {
        stage_attribute: None,
        return_type: "void".into(),
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
    use super::{emit_msl, MslError};
    use crate::target::MslTargetProfile;
    use cssl_mir::{MirFunc, MirModule};

    #[test]
    fn missing_entry_point_errors() {
        let module = MirModule::new();
        let err =
            emit_msl(&module, &MslTargetProfile::kernel_default(), "compute_main").unwrap_err();
        assert!(matches!(err, MslError::EntryPointMissing { .. }));
    }

    #[test]
    fn kernel_skeleton_has_kernel_attribute() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("compute_main", vec![], vec![]));
        let msl = emit_msl(&module, &MslTargetProfile::kernel_default(), "compute_main").unwrap();
        let text = msl.render();
        assert!(text.contains("[[kernel]]"));
        assert!(text.contains("void compute_main("));
        assert!(text.contains("thread_position_in_grid"));
    }

    #[test]
    fn vertex_skeleton_returns_float4() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_vs", vec![], vec![]));
        let msl = emit_msl(&module, &MslTargetProfile::vertex_default(), "main_vs").unwrap();
        let text = msl.render();
        assert!(text.contains("[[vertex]]"));
        assert!(text.contains("float4 main_vs("));
        assert!(text.contains("[[vertex_id]]"));
    }

    #[test]
    fn fragment_skeleton_has_position_attribute() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_fs", vec![], vec![]));
        let msl = emit_msl(&module, &MslTargetProfile::fragment_default(), "main_fs").unwrap();
        let text = msl.render();
        assert!(text.contains("[[fragment]]"));
        assert!(text.contains("float4 main_fs(float4 pos [[position]])"));
    }

    #[test]
    fn prelude_is_first() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        let msl = emit_msl(&module, &MslTargetProfile::kernel_default(), "main_cs").unwrap();
        let text = msl.render();
        let include_pos = text.find("#include <metal_stdlib>").unwrap();
        let entry_pos = text.find("void main_cs(").unwrap();
        assert!(include_pos < entry_pos);
    }

    #[test]
    fn helper_fns_have_no_stage_attribute() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        module.push_func(MirFunc::new("util", vec![], vec![]));
        let msl = emit_msl(&module, &MslTargetProfile::kernel_default(), "main_cs").unwrap();
        let text = msl.render();
        assert!(text.contains("void util()"));
        // Ensure no [[kernel]] for util.
        let util_pos = text.find("void util()").unwrap();
        let slice = &text[util_pos.saturating_sub(40)..util_pos];
        assert!(!slice.contains("[[kernel]]"));
    }

    #[test]
    fn header_records_profile() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        let msl = emit_msl(&module, &MslTargetProfile::kernel_default(), "main_cs").unwrap();
        let text = msl.render();
        assert!(text.contains("cssl-cgen-gpu-msl stage-0 emission"));
        assert!(text.contains("MSL 3.0"));
        assert!(text.contains("entry = main_cs"));
    }
}
