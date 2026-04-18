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

use crate::lower::lower_op;
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
/// Returns [`CpuCodegenError::NonScalarParam`] if any fn param has a non-scalar
/// type, or [`CpuCodegenError::TooManyResults`] if a fn has >1 result. Body
/// ops are lowered via [`crate::lower::lower_op`] ; unrecognized ops emit
/// comment placeholders rather than errors.
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
    // T11-D18 : Entry block carries the params ; body ops are lowered via
    // `lower::lower_op`. Unrecognized ops emit a `;` comment placeholder so
    // the output remains valid CLIF-ish text.
    writeln!(out, "block0({}):", clif_params.join(", ")).unwrap();
    let op_count: usize = f.body.blocks.iter().map(|b| b.ops.len()).sum();
    if op_count == 0 {
        if ret.is_some() {
            writeln!(out, "    ; stage-0 skeleton — empty body").unwrap();
        } else {
            writeln!(out, "    ; stage-0 skeleton — no-result empty body").unwrap();
        }
        writeln!(out, "    return").unwrap();
    } else {
        // Lower each op in the entry block. Additional blocks are phase-2 work.
        let entry = f.body.blocks.first().expect("at least one block exists");
        let mut saw_return = false;
        for op in &entry.ops {
            match lower_op(op) {
                Some(insns) => {
                    for insn in insns {
                        writeln!(out, "{}", insn.text).unwrap();
                    }
                    if op.name == "func.return" {
                        saw_return = true;
                    }
                }
                None => {
                    writeln!(
                        out,
                        "    ; unlowered : {} (stage-0 recognizes arith/func/math only)",
                        op.name
                    )
                    .unwrap();
                }
            }
        }
        if !saw_return {
            writeln!(out, "    return").unwrap();
        }
    }
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
    use cssl_mir::{FloatWidth, IntWidth, MirFunc, MirModule, MirOp, MirType, ValueId};

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

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D18 : body-lowering end-to-end : simple add(i32, i32) -> i32.
    // ─────────────────────────────────────────────────────────────────────

    /// Build a hand-rolled MIR fn : `fn add(v0: i32, v1: i32) -> i32 { v0 + v1 }`.
    fn hand_built_add() -> MirFunc {
        let i32_ty = MirType::Int(IntWidth::I32);
        let mut f = MirFunc::new(
            "add",
            vec![i32_ty.clone(), i32_ty.clone()],
            vec![i32_ty.clone()],
        );
        // Advance next_value_id past the block-args (v0, v1) before allocating v2.
        f.next_value_id = 2;
        // Entry block : ensure block-args are registered for v0 / v1.
        {
            let entry = f.body.entry_mut().expect("entry block exists");
            entry.args = vec![
                cssl_mir::MirValue::new(ValueId(0), i32_ty.clone()),
                cssl_mir::MirValue::new(ValueId(1), i32_ty.clone()),
            ];
            entry.ops.push(
                MirOp::std("arith.addi")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), i32_ty),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(2)));
        }
        f
    }

    #[test]
    fn emit_add_lowers_body_to_iadd_plus_return() {
        let mut module = MirModule::new();
        module.push_func(hand_built_add());
        let art = emit_module(&module, &CpuTargetProfile::linux_default()).unwrap();
        assert!(art
            .clif_text
            .contains("function %add(v0: i32, v1: i32) -> i32"));
        assert!(art.clif_text.contains("v2 = iadd v0, v1"));
        assert!(art.clif_text.contains("return v2"));
        // Regression guard : no body-is-empty placeholder should leak through.
        assert!(!art.clif_text.contains("stage-0 skeleton — empty body"));
    }

    #[test]
    fn emit_constant_plus_arith_lowers_to_iconst_plus_iadd() {
        // fn answer() -> i32 { 40 + 2 }
        let i32_ty = MirType::Int(IntWidth::I32);
        let mut f = MirFunc::new("answer", vec![], vec![i32_ty.clone()]);
        f.next_value_id = 3;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.ops.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(0), i32_ty.clone())
                    .with_attribute("value", "40"),
            );
            entry.ops.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(1), i32_ty.clone())
                    .with_attribute("value", "2"),
            );
            entry.ops.push(
                MirOp::std("arith.addi")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), i32_ty),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(2)));
        }
        let mut module = MirModule::new();
        module.push_func(f);
        let art = emit_module(&module, &CpuTargetProfile::linux_default()).unwrap();
        assert!(art.clif_text.contains("v0 = iconst.i32 40"));
        assert!(art.clif_text.contains("v1 = iconst.i32 2"));
        assert!(art.clif_text.contains("v2 = iadd v0, v1"));
        assert!(art.clif_text.contains("return v2"));
    }

    #[test]
    fn emit_float_mul_lowers_to_fmul() {
        let f32_ty = MirType::Float(FloatWidth::F32);
        let mut f = MirFunc::new(
            "scale",
            vec![f32_ty.clone(), f32_ty.clone()],
            vec![f32_ty.clone()],
        );
        f.next_value_id = 2;
        {
            let entry = f.body.entry_mut().unwrap();
            entry.args = vec![
                cssl_mir::MirValue::new(ValueId(0), f32_ty.clone()),
                cssl_mir::MirValue::new(ValueId(1), f32_ty.clone()),
            ];
            entry.ops.push(
                MirOp::std("arith.mulf")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), f32_ty),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(2)));
        }
        let mut module = MirModule::new();
        module.push_func(f);
        let art = emit_module(&module, &CpuTargetProfile::linux_default()).unwrap();
        assert!(art.clif_text.contains("v2 = fmul v0, v1"));
    }

    #[test]
    fn emit_unrecognized_op_emits_unlowered_comment() {
        let i32_ty = MirType::Int(IntWidth::I32);
        let mut f = MirFunc::new("mystery", vec![], vec![i32_ty.clone()]);
        f.next_value_id = 1;
        {
            let entry = f.body.entry_mut().unwrap();
            entry
                .ops
                .push(MirOp::std("cssl.unknown").with_result(ValueId(0), i32_ty));
        }
        let mut module = MirModule::new();
        module.push_func(f);
        let art = emit_module(&module, &CpuTargetProfile::linux_default()).unwrap();
        assert!(art.clif_text.contains("; unlowered : cssl.unknown"));
        // Auto-appended trailing return because the body had no func.return.
        assert!(art.clif_text.contains("    return"));
    }
}
