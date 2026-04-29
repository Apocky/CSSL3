//! MIR → MSL emitter.
//!
//! § ROLE — D3 (T11-D74)
//!   T10's [`emit_msl`] produced skeleton-only MSL : entry-point signature
//!   plus a placeholder `// stage-0 skeleton — MIR body lowered @ T10-phase-2`
//!   line. T11-D74 extends the emitter to splice in real MSL body text from
//!   the [`crate::body::emit_body`] op-emission table when the entry fn has
//!   a body. Modules without a structured-CFG marker (D5, T11-D70) are
//!   rejected at the body-emission boundary with [`MslError::BodyEmission`]
//!   carrying the underlying [`crate::body::BodyError`].
//!
//!   The skeleton path is preserved for empty bodies (e.g., interface-only
//!   GPU fns produced by the cssl-mir signature-only lowering when monomorph
//!   has not yet inflated the impl).

use cssl_mir::{MirFunc, MirModule};
use thiserror::Error;

use crate::body::{emit_body, has_body, BodyError};
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
    /// Body emission failed — the underlying [`BodyError`] carries the cause
    /// (missing structured-CFG marker, unsupported op, heap rejection, etc.).
    #[error("MSL body emission failed : {0}")]
    BodyEmission(#[from] BodyError),
}

/// Emit a `MirModule` as a stage-0 MSL translation unit.
///
/// # Errors
/// Returns [`MslError::EntryPointMissing`] if the entry-point fn is absent,
/// or [`MslError::BodyEmission`] when [`emit_body`] surfaces a structural
/// or unsupported-op error. The wrapped [`BodyError`] codes are documented
/// at the [`crate::body`] module level.
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

    let mut out = MslModule::new();
    out.header = Some(format!(
        "// cssl-cgen-gpu-msl stage-0 emission\n\
         // profile : {}\n\
         // entry = {}",
        profile.summary(),
        entry_name,
    ));
    out.seed_prelude();

    // Entry fn — real body when present, skeleton otherwise.
    let (ret_ty, params) = stage_signature(profile.stage);
    let body = if has_body(entry_fn) {
        // emit_body returns one MSL line per Vec element, already
        // indented one level (4 spaces) inside the entry fn body. The
        // MslStatement::Function renderer adds one more level of indent
        // when printing each body line, so we strip our leading 4-space
        // pad here to avoid double-indenting.
        let lines = emit_body(module, entry_name)?;
        lines
            .into_iter()
            .map(|s| s.strip_prefix("    ").map_or(s.clone(), str::to_string))
            .collect()
    } else {
        vec![
            "// stage-0 skeleton — entry-point body deferred (no MIR ops)".into(),
            format!("// profile : {}", profile.summary()),
        ]
    };
    out.push(MslStatement::Function {
        stage_attribute: Some(profile.stage.attribute().to_string()),
        return_type: ret_ty.into(),
        name: entry_fn.name.clone(),
        params: params.iter().map(|s| (*s).to_string()).collect(),
        body,
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

    // ── T11-D74 / S6-D3 integration tests : real body emission ───────────

    #[test]
    fn body_emission_requires_structured_cfg_marker() {
        // Module with body ops but no D5 marker → BodyEmission error.
        use cssl_mir::MirOp;
        let mut module = MirModule::new();
        let mut f = MirFunc::new("main_cs", vec![], vec![]);
        f.push_op(MirOp::std("func.return"));
        module.push_func(f);
        let err = emit_msl(&module, &MslTargetProfile::kernel_default(), "main_cs").unwrap_err();
        assert!(matches!(err, MslError::BodyEmission(_)), "got {err:?}");
    }

    #[test]
    fn body_emission_splices_arith_const_return_into_kernel_body() {
        use cssl_mir::{validate_and_mark, IntWidth, MirOp, MirType, ValueId};
        let mut module = MirModule::new();
        let mut f = MirFunc::new("main_cs", vec![], vec![MirType::Int(IntWidth::I32)]);
        f.push_op(
            MirOp::std("arith.constant")
                .with_result(ValueId(0), MirType::Int(IntWidth::I32))
                .with_attribute("value", "7"),
        );
        f.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        module.push_func(f);
        validate_and_mark(&mut module).unwrap();
        let msl = emit_msl(&module, &MslTargetProfile::kernel_default(), "main_cs").unwrap();
        let text = msl.render();
        assert!(text.contains("[[kernel]]"), "got : {text}");
        assert!(text.contains("int v0 = 7;"), "got : {text}");
        assert!(text.contains("return v0;"), "got : {text}");
    }

    #[test]
    fn body_emission_rejects_heap_alloc_with_clear_error() {
        use cssl_mir::{validate_and_mark, MirOp, MirType, ValueId};
        let mut module = MirModule::new();
        let mut f = MirFunc::new("main_cs", vec![], vec![]);
        f.push_op(
            MirOp::std("cssl.heap.alloc")
                .with_result(ValueId(0), MirType::Ptr)
                .with_attribute("size", "16"),
        );
        module.push_func(f);
        validate_and_mark(&mut module).unwrap();
        let err = emit_msl(&module, &MslTargetProfile::kernel_default(), "main_cs").unwrap_err();
        // Wrapped in MslError::BodyEmission.
        let msg = format!("{err}");
        assert!(msg.contains("Metal compute kernels"), "got : {msg}");
    }

    #[test]
    fn empty_body_falls_back_to_skeleton_path() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        // No D5 marker — but the path doesn't run body emission since the
        // fn body has no ops, so this still succeeds via the skeleton path.
        let msl = emit_msl(&module, &MslTargetProfile::kernel_default(), "main_cs").unwrap();
        let text = msl.render();
        assert!(
            text.contains("stage-0 skeleton — entry-point body deferred"),
            "got : {text}"
        );
    }
}
