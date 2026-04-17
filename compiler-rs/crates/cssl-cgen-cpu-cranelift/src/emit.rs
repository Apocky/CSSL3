//! Stage-0 text-CLIF emitter : `MirModule` → CLIF-like textual artifact.
//!
//! § STRATEGY
//!   Phase-1 emits a CLIF-flavoured textual representation that mirrors what
//!   `cranelift-codegen`'s `ir::Function::display()` would produce for simple skeleton
//!   functions. Phase-2 swaps this for the real `cranelift-frontend::FunctionBuilder`
//!   + `cranelift-object::ObjectModule` pipeline.
//!
//!   This keeps the stage-0 artifact inspectable + diffable without pulling the
//!   cranelift build-chain into the scaffold commit.

use core::fmt::Write as _;

use cssl_mir::{MirFunc, MirModule};
use thiserror::Error;

use crate::target::CpuTargetProfile;
use crate::types::clif_type_for;

/// Failure modes for codegen emission.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CpuCodegenError {
    /// A MIR fn signature referenced a type that has no CLIF scalar lowering.
    #[error(
        "fn `{fn_name}` param #{param_idx} has non-scalar MIR type `{ty}` ; stage-0 scalars-only"
    )]
    NonScalarParam {
        fn_name: String,
        param_idx: usize,
        ty: String,
    },
    /// A MIR fn signature had more than one non-scalar result ; stage-0 only supports 0/1-scalar results.
    #[error("fn `{fn_name}` has {count} results ; stage-0 supports ≤ 1 scalar result")]
    TooManyResults { fn_name: String, count: usize },
    /// Stage-0 requires an empty body (no ops) since MIR → CLIF body lowering is T10-phase-2 work.
    #[error(
        "fn `{fn_name}` body has {count} ops ; stage-0 emits skeleton only (T10-phase-2 lowers bodies)"
    )]
    BodyNotEmpty { fn_name: String, count: usize },
}

/// Emitted artifact bundle — stage-0 holds the CLIF-ish text + profile metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmittedArtifact {
    /// The target profile the artifact was emitted for.
    pub profile: CpuTargetProfile,
    /// Textual CLIF-like output (one `function` decl per MIR-fn).
    pub clif_text: String,
    /// Number of functions emitted.
    pub fn_count: usize,
}

impl EmittedArtifact {
    /// Render profile + fn-count + first-line preview for diagnostics.
    #[must_use]
    pub fn summary(&self) -> String {
        let preview = self
            .clif_text
            .lines()
            .next()
            .map_or("", str::trim)
            .to_string();
        format!(
            "cssl-cgen-cpu-cranelift : profile={} fns={} first-line=`{}`",
            self.profile.summary(),
            self.fn_count,
            preview,
        )
    }
}

/// Emit a full MIR-module to a stage-0 CLIF-like text artifact.
///
/// # Errors
/// Returns [`CpuCodegenError::NonScalarParam`] if any fn param has a non-scalar type,
/// [`CpuCodegenError::TooManyResults`] if a fn has >1 result, or
/// [`CpuCodegenError::BodyNotEmpty`] if a fn has ops (stage-0 emits sigs only).
pub fn emit_module(
    module: &MirModule,
    profile: &CpuTargetProfile,
) -> Result<EmittedArtifact, CpuCodegenError> {
    let mut out = String::new();
    // Header banner : records profile + target-features string.
    writeln!(
        out,
        "; cssl-cgen-cpu-cranelift stage-0 artifact\n\
         ; target = {}\n\
         ; target-features = {}\n\
         ; abi = {} / object = {} / debug = {}",
        profile.target.triple(),
        profile.features.render_target_features(),
        profile.abi.as_str(),
        profile.object_format.as_str(),
        profile.debug_format.as_str(),
    )
    .unwrap();

    let mut fn_count = 0usize;
    for f in &module.funcs {
        emit_function(f, &mut out)?;
        fn_count += 1;
    }

    Ok(EmittedArtifact {
        profile: profile.clone(),
        clif_text: out,
        fn_count,
    })
}

fn emit_function(f: &MirFunc, out: &mut String) -> Result<(), CpuCodegenError> {
    // Stage-0 : only accept fn bodies that are empty (no ops in any block).
    let op_count: usize = f.body.blocks.iter().map(|b| b.ops.len()).sum();
    if op_count > 0 {
        return Err(CpuCodegenError::BodyNotEmpty {
            fn_name: f.name.clone(),
            count: op_count,
        });
    }
    if f.results.len() > 1 {
        return Err(CpuCodegenError::TooManyResults {
            fn_name: f.name.clone(),
            count: f.results.len(),
        });
    }

    // Convert params ; bail on non-scalar.
    let mut clif_params: Vec<String> = Vec::with_capacity(f.params.len());
    for (i, p_ty) in f.params.iter().enumerate() {
        let Some(c) = clif_type_for(p_ty) else {
            return Err(CpuCodegenError::NonScalarParam {
                fn_name: f.name.clone(),
                param_idx: i,
                ty: format!("{p_ty}"),
            });
        };
        clif_params.push(format!("v{i}: {}", c.as_str()));
    }
    let ret = f.results.first().and_then(clif_type_for);

    writeln!(
        out,
        "\nfunction %{}({}) -> {} {{",
        f.name,
        clif_params.join(", "),
        ret_text(ret)
    )
    .unwrap();
    // Single empty block labeled `block0`.
    writeln!(out, "block0({}):", clif_params.join(", ")).unwrap();
    if ret.is_some() {
        writeln!(out, "    ; stage-0 skeleton — body lowered in T10-phase-2").unwrap();
    } else {
        writeln!(out, "    ; stage-0 skeleton — no-result return").unwrap();
    }
    writeln!(out, "    return").unwrap();
    writeln!(out, "}}").unwrap();
    Ok(())
}

fn ret_text(r: Option<crate::types::ClifType>) -> &'static str {
    r.map_or("()", crate::types::ClifType::as_str)
}

#[cfg(test)]
mod tests {
    use super::{emit_module, CpuCodegenError};
    use crate::target::CpuTargetProfile;
    use cssl_mir::{FloatWidth, IntWidth, MirFunc, MirModule, MirType};

    #[test]
    fn emit_empty_module_header_only() {
        let module = MirModule::new();
        let profile = CpuTargetProfile::windows_default();
        let art = emit_module(&module, &profile).unwrap();
        assert_eq!(art.fn_count, 0);
        assert!(art
            .clif_text
            .contains("cssl-cgen-cpu-cranelift stage-0 artifact"));
        assert!(art.clif_text.contains("intel-alder-lake"));
        assert!(art.clif_text.contains("+fma"));
    }

    #[test]
    fn emit_single_empty_fn() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("noop", vec![], vec![]));
        let profile = CpuTargetProfile::linux_default();
        let art = emit_module(&module, &profile).unwrap();
        assert_eq!(art.fn_count, 1);
        assert!(art.clif_text.contains("function %noop"));
        assert!(art.clif_text.contains("return"));
    }

    #[test]
    fn emit_i32_to_i32_fn() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new(
            "id32",
            vec![MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        ));
        let profile = CpuTargetProfile::windows_default();
        let art = emit_module(&module, &profile).unwrap();
        assert!(art.clif_text.contains("function %id32(v0: i32) -> i32"));
    }

    #[test]
    fn emit_rejects_non_scalar_param() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new(
            "takes_tuple",
            vec![MirType::Tuple(vec![
                MirType::Bool,
                MirType::Int(IntWidth::I32),
            ])],
            vec![],
        ));
        let err = emit_module(&module, &CpuTargetProfile::linux_default()).unwrap_err();
        assert!(matches!(err, CpuCodegenError::NonScalarParam { .. }));
    }

    #[test]
    fn emit_rejects_multi_result() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new(
            "multi",
            vec![],
            vec![MirType::Int(IntWidth::I32), MirType::Float(FloatWidth::F32)],
        ));
        let err = emit_module(&module, &CpuTargetProfile::linux_default()).unwrap_err();
        assert!(matches!(err, CpuCodegenError::TooManyResults { .. }));
    }

    #[test]
    fn summary_has_shape() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("noop", vec![], vec![]));
        let profile = CpuTargetProfile::windows_default();
        let art = emit_module(&module, &profile).unwrap();
        let s = art.summary();
        assert!(s.contains("cssl-cgen-cpu-cranelift"));
        assert!(s.contains("fns=1"));
    }
}
