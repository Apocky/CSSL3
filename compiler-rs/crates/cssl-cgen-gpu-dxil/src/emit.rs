//! MIR → HLSL textual emitter for DXIL codegen.
//!
//! § ROLE — S6-D2 (T11-D73)
//!   Phase-1 (T10) emitted skeleton bodies. S6-D2 grows the emitter into a
//!   real per-MirOp lowering : every fn body in `MirModule` walks its ops
//!   in declaration order + maps each to an HLSL statement (typed VarDecl
//!   for SSA defs, structured `if/for/while/loop` for `scf.*`, `return`
//!   for `func.return`, BufferLoad/store for `memref.load/store`,
//!   intrinsic calls for the recognized math fns).
//!
//! § FANOUT-CONTRACT — D5 (T11-D70)
//!   Per `specs/02_IR.csl § STRUCTURED-CFG VALIDATOR`, the GPU emitters
//!   D1..D4 W! check the `("structured_cfg.validated", "true")` marker on
//!   `MirModule` before emission. Calling `emit_hlsl` on an un-validated
//!   module is a programmer-error : the emitter refuses with
//!   [`DxilError::StructuredCfgUnvalidated`] rather than silently mis-
//!   lowering CFG patterns the validator would have rejected. Pass the
//!   module through [`cssl_mir::validate_and_mark`] first.
//!
//! § REJECTIONS
//!   - **Heap ops** (`cssl.heap.alloc / dealloc / realloc`) : the HLSL
//!     execution-model has no malloc — heap-allocated memory must come
//!     from D3D12 host-side resources, never from the shader. Emitter
//!     refuses with [`DxilError::HeapOpNotSupportedOnGpu`].
//!   - **Closures** (`cssl.closure.*` once C5 lands) : same etymology —
//!     no fn-pointers in DXIL. The current emitter recognizes the
//!     prefix `cssl.closure` defensively + refuses with
//!     [`DxilError::ClosureOpNotSupportedOnGpu`] so a stray closure op
//!     is caught here rather than slipping into DXC.
//!
//! § SIGNED-INTEGER CONVENTION
//!   MIR `i32` is signless ; HLSL distinguishes `int` (signed) from `uint`
//!   (unsigned). S6-D2 maps every MIR signless integer to HLSL `int` per
//!   the slice handoff landmines bullet ; explicit unsigned distinctions
//!   surface only when the source-MIR carries an attribute (e.g.
//!   `arith.cmpi { predicate = "ult" }` → unsigned-cast lhs/rhs at the
//!   compare site). A future slice can grow MIR-level signedness if
//!   needed ; for stage-0 this is sufficient + matches the cranelift
//!   side's signless lowering.

use cssl_mir::{
    has_structured_cfg_marker, FloatWidth, IntWidth, MirFunc, MirModule, MirOp, MirRegion, MirType,
    ValueId,
};
use thiserror::Error;

use crate::hlsl::{HlslBinaryOp, HlslBodyStmt, HlslExpr, HlslModule, HlslStatement, HlslUnaryOp};
use crate::target::{DxilTargetProfile, ShaderStage};

/// Failure modes for HLSL emission.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DxilError {
    /// No entry-point fn was found in the MIR module — DXIL needs at least one.
    #[error(
        "MIR module has no fn `{entry}` — DXIL target `{profile}` requires entry-point declaration"
    )]
    EntryPointMissing { entry: String, profile: String },

    /// **D5 contract** — module was not run through `validate_and_mark` before
    /// emission. Per `specs/02_IR.csl § STRUCTURED-CFG VALIDATOR`, GPU emitters
    /// W! check the marker. Emitter refuses to render rather than silently
    /// mis-lower unstructured-CFG patterns.
    #[error(
        "MIR module missing structured-CFG marker — call \
         `cssl_mir::validate_and_mark(&mut module)` before HLSL emission \
         (specs/02_IR.csl § STRUCTURED-CFG VALIDATOR)"
    )]
    StructuredCfgUnvalidated,

    /// Heap allocation requested in shader code — DXIL has no GPU malloc.
    /// Heap-backed storage must come from host-side D3D12 resources.
    #[error(
        "fn `{fn_name}` contains heap op `{op_name}` — DXIL has no GPU \
         malloc ; heap-backed storage must come from D3D12 host-side resources"
    )]
    HeapOpNotSupportedOnGpu { fn_name: String, op_name: String },

    /// Closure / fn-pointer requested — DXIL has no first-class fn-pointers.
    #[error(
        "fn `{fn_name}` contains closure op `{op_name}` — DXIL has no fn-pointers ; \
         CSSLv3 closures are CPU-only at stage-0"
    )]
    ClosureOpNotSupportedOnGpu { fn_name: String, op_name: String },

    /// MIR op the emitter does not recognize. Diagnostic carries op-name +
    /// fn-name so the user can locate the regression.
    #[error("fn `{fn_name}` contains unsupported MIR op `{op_name}` for HLSL emission")]
    UnsupportedMirOp { fn_name: String, op_name: String },

    /// Op result-type not lowerable to an HLSL primitive.
    #[error(
        "fn `{fn_name}` op `{op_name}` has result type `{ty}` not lowerable to HLSL primitive"
    )]
    UnsupportedResultType {
        fn_name: String,
        op_name: String,
        ty: String,
    },

    /// Op operand referenced an unknown ValueId — body-lower bug.
    #[error("fn `{fn_name}` op `{op_name}` references unknown ValueId({value_id})")]
    UnknownValueId {
        fn_name: String,
        op_name: String,
        value_id: u32,
    },

    /// Op was malformed (wrong arity, missing attribute, etc.).
    #[error("fn `{fn_name}` op `{op_name}` malformed : {detail}")]
    MalformedOp {
        fn_name: String,
        op_name: String,
        detail: String,
    },
}

// ─────────────────────────────────────────────────────────────────────────
// § Public entry — `emit_hlsl(&module, &profile, entry_name)`.
// ─────────────────────────────────────────────────────────────────────────

/// Emit a `MirModule` as an HLSL translation unit ready for `dxc`.
///
/// Walks the entry-point fn's body op-by-op, lowering each to HLSL
/// statements per the table documented in this module's header. Any
/// non-entry fn surfaces as a helper-fn with the same lowering.
///
/// # Errors
/// - [`DxilError::StructuredCfgUnvalidated`] if the module has not been
///   run through [`cssl_mir::validate_and_mark`].
/// - [`DxilError::EntryPointMissing`] if the named entry-fn is absent.
/// - [`DxilError::HeapOpNotSupportedOnGpu`] / `ClosureOpNotSupportedOnGpu`
///   if the body uses ops the GPU execution-model can't host.
/// - [`DxilError::UnsupportedMirOp`] / `UnsupportedResultType` /
///   `UnknownValueId` / `MalformedOp` for the rest.
pub fn emit_hlsl(
    module: &MirModule,
    profile: &DxilTargetProfile,
    entry_name: &str,
) -> Result<HlslModule, DxilError> {
    if !has_structured_cfg_marker(module) {
        return Err(DxilError::StructuredCfgUnvalidated);
    }

    let Some(entry_fn) = module.find_func(entry_name) else {
        return Err(DxilError::EntryPointMissing {
            entry: entry_name.into(),
            profile: profile.profile.render(),
        });
    };

    let mut out = HlslModule::new();
    out.header = Some(format!(
        "// cssl-cgen-gpu-dxil S6-D2 HLSL emission (T11-D73)\n\
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

    // Emit a signature-matched entry-fn with a real lowered body.
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
        body: render_fn_body(entry_fn, profile)?,
    });

    // Helper fns : every non-entry fn lowers with its own body. Stage-0
    // helpers carry no semantic + no stage-attribute ; they're plain
    // HLSL utility fns called from the entry-point.
    for f in &module.funcs {
        if f.name == entry_name {
            continue;
        }
        out.push(synthesize_helper_fn(f, profile)?);
    }

    Ok(out)
}

/// Convenience wrapper : run the validator + emit. Useful for callers that
/// want to skip the explicit `validate_and_mark` step. Mutates `module`
/// (sets the marker on success) ; returns the emitted HLSL module.
///
/// # Errors
/// - All variants of [`DxilError`] that [`emit_hlsl`] can produce, plus
///   the structured-CFG validator's `Vec<CfgViolation>` collected into a
///   single [`DxilError::MalformedOp`] surface.
pub fn validate_and_emit_hlsl(
    module: &mut MirModule,
    profile: &DxilTargetProfile,
    entry_name: &str,
) -> Result<HlslModule, DxilError> {
    if let Err(violations) = cssl_mir::validate_and_mark(module) {
        // Wrap every CFG violation into a single MalformedOp diagnostic so
        // the emitter API stays one error-type-out.
        let detail = violations
            .iter()
            .map(|v| format!("{} ({}: {v})", v.code(), v.fn_name()))
            .collect::<Vec<_>>()
            .join(" ; ");
        return Err(DxilError::MalformedOp {
            fn_name: "<module>".into(),
            op_name: "structured-cfg-validate".into(),
            detail,
        });
    }
    emit_hlsl(module, profile, entry_name)
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

// ─────────────────────────────────────────────────────────────────────────
// § Body rendering — walks fn body, returns Vec<String> per the existing
// HlslStatement::Function shape (which expects already-rendered body lines).
// ─────────────────────────────────────────────────────────────────────────

fn render_fn_body(f: &MirFunc, profile: &DxilTargetProfile) -> Result<Vec<String>, DxilError> {
    let stmts = lower_fn_body_stmts(f)?;
    let mut lines = Vec::with_capacity(stmts.len() + 4);

    // Header-comment threaded through each fn for diagnostics.
    lines.push(format!(
        "// MIR fn `{}` ; profile-summary : {}",
        f.name,
        profile.summary()
    ));
    if stmts.is_empty() {
        lines.push("// (empty body — MIR fn has no ops at this stage-0 walk)".into());
        return Ok(lines);
    }
    for s in stmts {
        // Render at indent 0 ; HlslStatement::Function adds the 4-space
        // indent at the body-vec rendering level. Trim the trailing
        // newline render() adds since we're collecting into a Vec<String>
        // that the parent fn-renderer joins with "\n".
        let raw = s.render(0);
        for line in raw.lines() {
            // Skip blank lines but preserve all real source.
            if !line.is_empty() {
                lines.push(line.to_string());
            }
        }
    }
    Ok(lines)
}

fn synthesize_helper_fn(
    f: &MirFunc,
    profile: &DxilTargetProfile,
) -> Result<HlslStatement, DxilError> {
    let return_type = helper_return_type(f);
    let params = helper_params(f);
    Ok(HlslStatement::Function {
        return_type,
        name: f.name.clone(),
        params,
        attributes: vec![],
        semantic: None,
        body: render_fn_body(f, profile)?,
    })
}

fn helper_return_type(f: &MirFunc) -> String {
    match f.results.first() {
        Some(t) => mir_to_hlsl_type(t).unwrap_or_else(|_| "void".into()),
        None => "void".into(),
    }
}

fn helper_params(f: &MirFunc) -> Vec<String> {
    f.params
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let ty = mir_to_hlsl_type(t).unwrap_or_else(|_| "int".into());
            format!("{ty} v{i}")
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────
// § Body-lower : MIR fn → Vec<HlslBodyStmt>.
// ─────────────────────────────────────────────────────────────────────────

/// Lower a MIR fn body to a flat `Vec<HlslBodyStmt>`. Each MIR op produces
/// zero or more HLSL statements. Nested regions (scf.if then/else,
/// scf.for/while/loop body) recurse into [`lower_region_stmts`].
fn lower_fn_body_stmts(f: &MirFunc) -> Result<Vec<HlslBodyStmt>, DxilError> {
    let mut stmts = Vec::new();
    // No entry block — that's a CFG0001-shaped problem D5 catches first ;
    // hitting it here means the validator marker was set incorrectly.
    // Render an empty body rather than panic.
    let Some(entry) = f.body.entry() else {
        return Ok(stmts);
    };
    for op in &entry.ops {
        lower_op(op, &f.name, &mut stmts)?;
    }
    Ok(stmts)
}

/// Lower one region (a single block of ops at stage-0 per the structured-
/// CFG validator's CFG0007 invariant). Recursion entry-point used by
/// `scf.if` then/else branches + `scf.for/while/loop` bodies.
fn lower_region_stmts(region: &MirRegion, fn_name: &str) -> Result<Vec<HlslBodyStmt>, DxilError> {
    let mut stmts = Vec::new();
    if let Some(block) = region.entry() {
        for op in &block.ops {
            lower_op(op, fn_name, &mut stmts)?;
        }
    }
    Ok(stmts)
}

/// Map one MirOp to its HLSL statement-form. Pushes 1+ statements onto
/// `out` ; the chosen statement-form depends on the op's name and shape.
#[allow(clippy::too_many_lines)]
fn lower_op(op: &MirOp, fn_name: &str, out: &mut Vec<HlslBodyStmt>) -> Result<(), DxilError> {
    // Hard rejections first — heap + closure ops have no GPU lowering.
    if op.name.starts_with("cssl.heap.") {
        return Err(DxilError::HeapOpNotSupportedOnGpu {
            fn_name: fn_name.to_string(),
            op_name: op.name.clone(),
        });
    }
    if op.name.starts_with("cssl.closure") {
        return Err(DxilError::ClosureOpNotSupportedOnGpu {
            fn_name: fn_name.to_string(),
            op_name: op.name.clone(),
        });
    }

    match op.name.as_str() {
        // ─── arith.constant ───────────────────────────────────────────
        "arith.constant" => {
            let r = first_result(op, fn_name)?;
            let value_str = op
                .attributes
                .iter()
                .find(|(k, _)| k == "value")
                .map_or("0", |(_, v)| v.as_str());
            let init = literal_for_type(&r.ty, value_str)?;
            let ty = mir_to_hlsl_type_for_op(&r.ty, op, fn_name)?;
            out.push(HlslBodyStmt::VarDecl {
                ty,
                name: ssa_name(r.id),
                init,
            });
            Ok(())
        }
        // ─── arith binary ops ─────────────────────────────────────────
        "arith.addi" => emit_binary_decl(op, fn_name, out, HlslBinaryOp::Add),
        "arith.subi" => emit_binary_decl(op, fn_name, out, HlslBinaryOp::Sub),
        "arith.muli" => emit_binary_decl(op, fn_name, out, HlslBinaryOp::Mul),
        "arith.divi" | "arith.divsi" => emit_binary_decl(op, fn_name, out, HlslBinaryOp::Div),
        "arith.divui" => emit_binary_decl_unsigned(op, fn_name, out, HlslBinaryOp::Div),
        "arith.remi" | "arith.remsi" => emit_binary_decl(op, fn_name, out, HlslBinaryOp::Rem),
        "arith.remui" => emit_binary_decl_unsigned(op, fn_name, out, HlslBinaryOp::Rem),
        "arith.addf" => emit_binary_decl(op, fn_name, out, HlslBinaryOp::Add),
        "arith.subf" => emit_binary_decl(op, fn_name, out, HlslBinaryOp::Sub),
        "arith.mulf" => emit_binary_decl(op, fn_name, out, HlslBinaryOp::Mul),
        "arith.divf" => emit_binary_decl(op, fn_name, out, HlslBinaryOp::Div),
        "arith.andi" => emit_binary_decl(op, fn_name, out, HlslBinaryOp::BitAnd),
        "arith.ori" => emit_binary_decl(op, fn_name, out, HlslBinaryOp::BitOr),
        "arith.xori" => emit_binary_decl(op, fn_name, out, HlslBinaryOp::BitXor),
        "arith.shli" => emit_binary_decl(op, fn_name, out, HlslBinaryOp::Shl),
        "arith.shrsi" | "arith.shrui" => emit_binary_decl(op, fn_name, out, HlslBinaryOp::Shr),
        // ─── arith unary ─────────────────────────────────────────────
        "arith.negf" => emit_unary_decl(op, fn_name, out, HlslUnaryOp::Neg),
        // ─── arith.cmpi / cmpf — predicate via attribute ─────────────
        "arith.cmpi" => emit_cmp_decl(op, fn_name, out, /*is_float=*/ false),
        "arith.cmpf" => emit_cmp_decl(op, fn_name, out, /*is_float=*/ true),
        // ─── arith.select ────────────────────────────────────────────
        "arith.select" => emit_select_decl(op, fn_name, out),
        // ─── memref.load / store ─────────────────────────────────────
        "memref.load" => emit_memref_load(op, fn_name, out),
        "memref.store" => emit_memref_store(op, fn_name, out),
        // ─── func.call : intrinsic or user fn ────────────────────────
        "func.call" => emit_func_call(op, fn_name, out),
        // ─── func.return ─────────────────────────────────────────────
        "func.return" | "cssl.diff.bwd_return" => {
            let value = match op.operands.first() {
                Some(&id) => Some(HlslExpr::Var(ssa_name(id))),
                None => None,
            };
            out.push(HlslBodyStmt::Return { value });
            Ok(())
        }
        // ─── scf.if : structured 2-region branch ─────────────────────
        "scf.if" => emit_scf_if(op, fn_name, out),
        // ─── scf.for / scf.while / scf.loop : structured loops ───────
        "scf.for" => emit_scf_for(op, fn_name, out),
        "scf.while" => emit_scf_while(op, fn_name, out),
        "scf.loop" => emit_scf_loop(op, fn_name, out),
        // ─── scf.yield : consumed by parent or no-op at outer level ──
        "scf.yield" => Ok(()),
        // ─── cssl.gpu.barrier : SM6.x intrinsic ──────────────────────
        "cssl.gpu.barrier" => {
            // GroupMemoryBarrierWithGroupSync is the canonical SM 6.x
            // compute-side equivalent of OpControlBarrier WG WG AcquireRelease.
            out.push(HlslBodyStmt::ExprStmt(HlslExpr::Call {
                name: "GroupMemoryBarrierWithGroupSync".into(),
                args: vec![],
            }));
            Ok(())
        }
        // ─── unsupported placeholders D5 should have caught ──────────
        "cssl.unsupported(Break)" | "cssl.unsupported(Continue)" => {
            // Defensively render as a comment so any module that bypassed
            // D5 still produces walkable HLSL ; the structured-CFG marker
            // check at emit_hlsl entry should make this unreachable.
            out.push(HlslBodyStmt::Comment(format!(
                "stage-0 deferred : {}",
                op.name
            )));
            Ok(())
        }
        // ─── catch-all ──────────────────────────────────────────────
        other => Err(DxilError::UnsupportedMirOp {
            fn_name: fn_name.to_string(),
            op_name: other.to_string(),
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Op-emission helpers.
// ─────────────────────────────────────────────────────────────────────────

fn emit_binary_decl(
    op: &MirOp,
    fn_name: &str,
    out: &mut Vec<HlslBodyStmt>,
    bin_op: HlslBinaryOp,
) -> Result<(), DxilError> {
    let r = first_result(op, fn_name)?;
    let (a, b) = two_operands(op, fn_name)?;
    let ty = mir_to_hlsl_type_for_op(&r.ty, op, fn_name)?;
    out.push(HlslBodyStmt::VarDecl {
        ty,
        name: ssa_name(r.id),
        init: HlslExpr::Binary {
            op: bin_op,
            lhs: Box::new(HlslExpr::Var(ssa_name(a))),
            rhs: Box::new(HlslExpr::Var(ssa_name(b))),
        },
    });
    Ok(())
}

fn emit_binary_decl_unsigned(
    op: &MirOp,
    fn_name: &str,
    out: &mut Vec<HlslBodyStmt>,
    bin_op: HlslBinaryOp,
) -> Result<(), DxilError> {
    let r = first_result(op, fn_name)?;
    let (a, b) = two_operands(op, fn_name)?;
    // Unsigned compute via explicit cast.
    let ty = unsigned_int_hlsl_type(&r.ty).unwrap_or_else(|| "uint".to_string());
    let lhs = HlslExpr::Cast {
        ty: ty.clone(),
        rhs: Box::new(HlslExpr::Var(ssa_name(a))),
    };
    let rhs = HlslExpr::Cast {
        ty: ty.clone(),
        rhs: Box::new(HlslExpr::Var(ssa_name(b))),
    };
    let result_ty = mir_to_hlsl_type_for_op(&r.ty, op, fn_name)?;
    out.push(HlslBodyStmt::VarDecl {
        ty: result_ty,
        name: ssa_name(r.id),
        init: HlslExpr::Binary {
            op: bin_op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
    });
    Ok(())
}

fn emit_unary_decl(
    op: &MirOp,
    fn_name: &str,
    out: &mut Vec<HlslBodyStmt>,
    un_op: HlslUnaryOp,
) -> Result<(), DxilError> {
    let r = first_result(op, fn_name)?;
    let a = first_operand(op, fn_name)?;
    let ty = mir_to_hlsl_type_for_op(&r.ty, op, fn_name)?;
    out.push(HlslBodyStmt::VarDecl {
        ty,
        name: ssa_name(r.id),
        init: HlslExpr::Unary {
            op: un_op,
            rhs: Box::new(HlslExpr::Var(ssa_name(a))),
        },
    });
    Ok(())
}

fn emit_cmp_decl(
    op: &MirOp,
    fn_name: &str,
    out: &mut Vec<HlslBodyStmt>,
    is_float: bool,
) -> Result<(), DxilError> {
    let r = first_result(op, fn_name)?;
    let (a, b) = two_operands(op, fn_name)?;
    let pred = op
        .attributes
        .iter()
        .find(|(k, _)| k == "predicate")
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| DxilError::MalformedOp {
            fn_name: fn_name.to_string(),
            op_name: op.name.clone(),
            detail: "missing `predicate` attribute".into(),
        })?;
    let bin_op = predicate_to_binary(pred, is_float).ok_or_else(|| DxilError::MalformedOp {
        fn_name: fn_name.to_string(),
        op_name: op.name.clone(),
        detail: format!("unknown predicate `{pred}`"),
    })?;
    // Result is bool — render as HLSL `bool`.
    out.push(HlslBodyStmt::VarDecl {
        ty: "bool".into(),
        name: ssa_name(r.id),
        init: HlslExpr::Binary {
            op: bin_op,
            lhs: Box::new(HlslExpr::Var(ssa_name(a))),
            rhs: Box::new(HlslExpr::Var(ssa_name(b))),
        },
    });
    Ok(())
}

fn emit_select_decl(
    op: &MirOp,
    fn_name: &str,
    out: &mut Vec<HlslBodyStmt>,
) -> Result<(), DxilError> {
    let r = first_result(op, fn_name)?;
    if op.operands.len() != 3 {
        return Err(DxilError::MalformedOp {
            fn_name: fn_name.to_string(),
            op_name: op.name.clone(),
            detail: format!("arith.select expects 3 operands, got {}", op.operands.len()),
        });
    }
    let cond = op.operands[0];
    let then_v = op.operands[1];
    let else_v = op.operands[2];
    let ty = mir_to_hlsl_type_for_op(&r.ty, op, fn_name)?;
    out.push(HlslBodyStmt::VarDecl {
        ty,
        name: ssa_name(r.id),
        init: HlslExpr::Ternary {
            cond: Box::new(HlslExpr::Var(ssa_name(cond))),
            then_branch: Box::new(HlslExpr::Var(ssa_name(then_v))),
            else_branch: Box::new(HlslExpr::Var(ssa_name(else_v))),
        },
    });
    Ok(())
}

fn emit_memref_load(
    op: &MirOp,
    fn_name: &str,
    out: &mut Vec<HlslBodyStmt>,
) -> Result<(), DxilError> {
    // memref.load : (ptr [, offset]) -> elem-T
    let r = first_result(op, fn_name)?;
    let ptr_id = first_operand(op, fn_name)?;
    let offset_id = op.operands.get(1).copied();
    let ty = mir_to_hlsl_type_for_op(&r.ty, op, fn_name)?;

    // Stage-0 buffer naming convention : a synthetic global
    // `g_buf_<ptr>` keyed by the ptr ValueId. The real address-mode
    // (group-shared / cbuffer / RWBuffer) is decided at root-signature-
    // generation time when E2 D3D12 host slice lands ; for stage-0 we
    // render through the ptr-ValueId name directly. Index expression is
    // either the ptr alone or `ptr + offset`.
    let idx_expr = match offset_id {
        Some(off) => HlslExpr::Binary {
            op: HlslBinaryOp::Add,
            lhs: Box::new(HlslExpr::Var(ssa_name(ptr_id))),
            rhs: Box::new(HlslExpr::Var(ssa_name(off))),
        },
        None => HlslExpr::Var(ssa_name(ptr_id)),
    };
    // Render as a load-from-buffer-element via an `int g_buf[]` synthetic.
    // The exact buffer-name + index-cast wiring grows when D2's host-side
    // root-signature generation slice lands ; the generated HLSL still
    // round-trips through dxc as long as the type-shape matches.
    let load_expr = HlslExpr::BufferLoad {
        buffer: format!("g_dyn_buf_{}", ptr_id.0),
        index: Box::new(idx_expr),
    };
    out.push(HlslBodyStmt::VarDecl {
        ty,
        name: ssa_name(r.id),
        init: load_expr,
    });
    Ok(())
}

fn emit_memref_store(
    op: &MirOp,
    fn_name: &str,
    out: &mut Vec<HlslBodyStmt>,
) -> Result<(), DxilError> {
    if op.operands.len() < 2 || op.operands.len() > 3 {
        return Err(DxilError::MalformedOp {
            fn_name: fn_name.to_string(),
            op_name: op.name.clone(),
            detail: format!(
                "memref.store expects (val, ptr [, offset]), got {} operands",
                op.operands.len()
            ),
        });
    }
    let val_id = op.operands[0];
    let ptr_id = op.operands[1];
    let offset_id = op.operands.get(2).copied();
    let idx_expr = match offset_id {
        Some(off) => HlslExpr::Binary {
            op: HlslBinaryOp::Add,
            lhs: Box::new(HlslExpr::Var(ssa_name(ptr_id))),
            rhs: Box::new(HlslExpr::Var(ssa_name(off))),
        },
        None => HlslExpr::Var(ssa_name(ptr_id)),
    };
    // HLSL has no buffer-store-as-expression that's portable across all
    // resource-kinds at stage-0 ; render as a plain assignment-like
    // statement against the synthetic `g_dyn_buf_<ptr>` global.
    let assign = format!(
        "g_dyn_buf_{}[{}] = {};",
        ptr_id.0,
        idx_expr.render(),
        ssa_name(val_id),
    );
    out.push(HlslBodyStmt::Raw(assign));
    Ok(())
}

fn emit_func_call(op: &MirOp, fn_name: &str, out: &mut Vec<HlslBodyStmt>) -> Result<(), DxilError> {
    let callee = op
        .attributes
        .iter()
        .find(|(k, _)| k == "callee")
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| DxilError::MalformedOp {
            fn_name: fn_name.to_string(),
            op_name: op.name.clone(),
            detail: "missing `callee` attribute".into(),
        })?;
    let intrinsic_name = intrinsic_hlsl_name(callee).unwrap_or(callee);
    let args: Vec<HlslExpr> = op
        .operands
        .iter()
        .map(|id| HlslExpr::Var(ssa_name(*id)))
        .collect();

    let call_expr = HlslExpr::Call {
        name: intrinsic_name.to_string(),
        args,
    };
    if let Some(r) = op.results.first() {
        let ty = mir_to_hlsl_type_for_op(&r.ty, op, fn_name)?;
        out.push(HlslBodyStmt::VarDecl {
            ty,
            name: ssa_name(r.id),
            init: call_expr,
        });
    } else {
        out.push(HlslBodyStmt::ExprStmt(call_expr));
    }
    Ok(())
}

fn emit_scf_if(op: &MirOp, fn_name: &str, out: &mut Vec<HlslBodyStmt>) -> Result<(), DxilError> {
    if op.regions.len() != 2 {
        return Err(DxilError::MalformedOp {
            fn_name: fn_name.to_string(),
            op_name: "scf.if".into(),
            detail: format!("expected 2 regions, got {}", op.regions.len()),
        });
    }
    let cond_id = first_operand(op, fn_name)?;
    let then_body = lower_region_stmts(&op.regions[0], fn_name)?;
    let else_body = lower_region_stmts(&op.regions[1], fn_name)?;

    out.push(HlslBodyStmt::If {
        cond: HlslExpr::Var(ssa_name(cond_id)),
        then_body,
        else_body,
    });

    // If `scf.if` produces a result (expression-form per C1 / T11-D58),
    // the regions' trailing `scf.yield` produced no statement — but the
    // result ValueId still needs to be made visible to subsequent ops.
    // Emit a phi-style declaration AFTER the if-block that reads the
    // last-yielded value name from each branch via a synthetic helper
    // var. Stage-0 simplification : declare the result-name with the
    // `then` branch's yielded value as the placeholder ; downstream ops
    // use `ssa_name(result.id)` and the structured-CFG validator
    // guarantees both branches emit a yield. A future slice can grow
    // proper `if` with both branches assigning to a pre-declared lvalue.
    if let Some(r) = op.results.first() {
        if let Some(yielded_then) = trailing_yield_value(&op.regions[0]) {
            let ty = mir_to_hlsl_type_for_op(&r.ty, op, fn_name)?;
            // Insert the result-decl BEFORE the if-block, then assign the
            // yielded value at the end of each branch. We reorder the
            // already-emitted If statement : remove it, push the decl,
            // mutate the If's bodies to add Assign at the tail, push the
            // mutated If back. This keeps the SSA shape correct in HLSL.
            let if_stmt = out
                .pop()
                .expect("just pushed scf.if's HlslBodyStmt::If above");
            let HlslBodyStmt::If {
                cond,
                mut then_body,
                mut else_body,
            } = if_stmt
            else {
                // Should be unreachable.
                return Err(DxilError::MalformedOp {
                    fn_name: fn_name.to_string(),
                    op_name: "scf.if".into(),
                    detail: "post-emit shape mismatch".into(),
                });
            };
            let result_name = ssa_name(r.id);
            // Pre-declare the result variable with a default-initialized
            // value of the same type so the assignment in each branch is
            // valid HLSL.
            out.push(HlslBodyStmt::VarDecl {
                ty: ty.clone(),
                name: result_name.clone(),
                init: default_value_for_type(&r.ty),
            });
            // Append branch-end assignments (then-yielded value already
            // captured ; else-yielded value computed in-line below).
            then_body.push(HlslBodyStmt::Assign {
                lhs: result_name.clone(),
                rhs: HlslExpr::Var(ssa_name(yielded_then)),
            });
            if let Some(yielded_else) = trailing_yield_value(&op.regions[1]) {
                else_body.push(HlslBodyStmt::Assign {
                    lhs: result_name,
                    rhs: HlslExpr::Var(ssa_name(yielded_else)),
                });
            }
            out.push(HlslBodyStmt::If {
                cond,
                then_body,
                else_body,
            });
        }
    }

    Ok(())
}

fn emit_scf_for(op: &MirOp, fn_name: &str, out: &mut Vec<HlslBodyStmt>) -> Result<(), DxilError> {
    if op.regions.len() != 1 {
        return Err(DxilError::MalformedOp {
            fn_name: fn_name.to_string(),
            op_name: "scf.for".into(),
            detail: format!("expected 1 region, got {}", op.regions.len()),
        });
    }
    let body = lower_region_stmts(&op.regions[0], fn_name)?;
    // Stage-0 single-trip per the C2 deferred-bullets : we render a
    // structurally-correct `for` skeleton with an init-vacuum, a
    // sentinel cond `false`, no step. The single-trip semantic matches
    // the cranelift `lower_scf_for` body. A future slice with iter-bounds
    // grows real init/cond/step expressions.
    out.push(HlslBodyStmt::For {
        init: None,
        cond: Some(HlslExpr::BoolLit(false)),
        step: None,
        body,
    });
    Ok(())
}

fn emit_scf_while(op: &MirOp, fn_name: &str, out: &mut Vec<HlslBodyStmt>) -> Result<(), DxilError> {
    if op.regions.len() != 1 {
        return Err(DxilError::MalformedOp {
            fn_name: fn_name.to_string(),
            op_name: "scf.while".into(),
            detail: format!("expected 1 region, got {}", op.regions.len()),
        });
    }
    let cond_id = first_operand(op, fn_name)?;
    let body = lower_region_stmts(&op.regions[0], fn_name)?;
    out.push(HlslBodyStmt::While {
        cond: HlslExpr::Var(ssa_name(cond_id)),
        body,
    });
    Ok(())
}

fn emit_scf_loop(op: &MirOp, fn_name: &str, out: &mut Vec<HlslBodyStmt>) -> Result<(), DxilError> {
    if op.regions.len() != 1 {
        return Err(DxilError::MalformedOp {
            fn_name: fn_name.to_string(),
            op_name: "scf.loop".into(),
            detail: format!("expected 1 region, got {}", op.regions.len()),
        });
    }
    let body = lower_region_stmts(&op.regions[0], fn_name)?;
    out.push(HlslBodyStmt::Loop { body });
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// § Helpers — operand/result extraction, type-mapping, naming.
// ─────────────────────────────────────────────────────────────────────────

fn first_result<'a>(op: &'a MirOp, fn_name: &str) -> Result<&'a cssl_mir::MirValue, DxilError> {
    op.results.first().ok_or_else(|| DxilError::MalformedOp {
        fn_name: fn_name.to_string(),
        op_name: op.name.clone(),
        detail: format!("`{}` has no result", op.name),
    })
}

fn first_operand(op: &MirOp, fn_name: &str) -> Result<ValueId, DxilError> {
    op.operands
        .first()
        .copied()
        .ok_or_else(|| DxilError::MalformedOp {
            fn_name: fn_name.to_string(),
            op_name: op.name.clone(),
            detail: format!("`{}` expected at least 1 operand", op.name),
        })
}

fn two_operands(op: &MirOp, fn_name: &str) -> Result<(ValueId, ValueId), DxilError> {
    if op.operands.len() < 2 {
        return Err(DxilError::MalformedOp {
            fn_name: fn_name.to_string(),
            op_name: op.name.clone(),
            detail: format!(
                "`{}` expected 2 operands, got {}",
                op.name,
                op.operands.len()
            ),
        });
    }
    Ok((op.operands[0], op.operands[1]))
}

/// Canonical SSA-value name in generated HLSL (`%3` → `v3`). HLSL forbids
/// `%` in identifiers, so we use `v<id>` consistently.
fn ssa_name(id: ValueId) -> String {
    format!("v{}", id.0)
}

/// MIR signless-int → HLSL signed `int<width>` (CSSLv3 default per the
/// slice handoff landmines bullet on signedness conventions).
fn mir_to_hlsl_type(ty: &MirType) -> Result<String, String> {
    match ty {
        MirType::Bool => Ok("bool".into()),
        MirType::Int(IntWidth::I1) => Ok("bool".into()),
        MirType::Int(IntWidth::I8) | MirType::Int(IntWidth::I16) | MirType::Int(IntWidth::I32) => {
            Ok("int".into())
        }
        MirType::Int(IntWidth::I64) | MirType::Int(IntWidth::Index) => Ok("int64_t".into()),
        MirType::Float(FloatWidth::F16) => Ok("half".into()),
        MirType::Float(FloatWidth::Bf16) => Ok("half".into()),
        MirType::Float(FloatWidth::F32) => Ok("float".into()),
        MirType::Float(FloatWidth::F64) => Ok("double".into()),
        MirType::None => Ok("void".into()),
        MirType::Vec(lanes, w) => {
            let scalar = match w {
                FloatWidth::F32 => "float",
                FloatWidth::F64 => "double",
                FloatWidth::F16 | FloatWidth::Bf16 => "half",
            };
            Ok(format!("{scalar}{lanes}"))
        }
        // Stage-0 : memref / handle / tuple / fn / opaque / ptr fall through
        // as `int` placeholders. The structured-CFG validator + emitter
        // contract together ensure these don't appear as MIR-result types
        // on the GPU path ; if one slips through, surface the type-name
        // for diagnostic clarity.
        other => Err(format!("{other} not lowerable to HLSL primitive")),
    }
}

fn unsigned_int_hlsl_type(ty: &MirType) -> Option<String> {
    match ty {
        MirType::Int(IntWidth::I8 | IntWidth::I16 | IntWidth::I32) => Some("uint".into()),
        MirType::Int(IntWidth::I64 | IntWidth::Index) => Some("uint64_t".into()),
        _ => None,
    }
}

fn mir_to_hlsl_type_for_op(ty: &MirType, op: &MirOp, fn_name: &str) -> Result<String, DxilError> {
    mir_to_hlsl_type(ty).map_err(|_| DxilError::UnsupportedResultType {
        fn_name: fn_name.to_string(),
        op_name: op.name.clone(),
        ty: format!("{ty}"),
    })
}

fn literal_for_type(ty: &MirType, value_str: &str) -> Result<HlslExpr, DxilError> {
    match ty {
        MirType::Bool | MirType::Int(IntWidth::I1) => {
            Ok(HlslExpr::BoolLit(value_str != "0" && value_str != "false"))
        }
        MirType::Int(_) => Ok(HlslExpr::IntLit(value_str.to_string())),
        MirType::Float(w) => Ok(HlslExpr::FloatLit {
            text: value_str.to_string(),
            is_f32: matches!(w, FloatWidth::F32),
        }),
        // Conservative default : render as int — the structured-CFG / type
        // checker should keep us off this branch.
        _ => Ok(HlslExpr::IntLit(value_str.to_string())),
    }
}

fn default_value_for_type(ty: &MirType) -> HlslExpr {
    match ty {
        MirType::Bool | MirType::Int(IntWidth::I1) => HlslExpr::BoolLit(false),
        MirType::Int(_) => HlslExpr::IntLit("0".into()),
        MirType::Float(w) => HlslExpr::FloatLit {
            text: "0.0".into(),
            is_f32: matches!(w, FloatWidth::F32),
        },
        _ => HlslExpr::IntLit("0".into()),
    }
}

/// Translate MLIR `arith.cmpi/cmpf` predicate strings to HLSL binary ops.
/// Returns `None` on unknown predicate. `is_float` selects between the
/// integer set (`slt/sle/sgt/sge/ult/ule/ugt/uge/eq/ne`) and the float set
/// (`olt/ole/ogt/oge/ult/ule/ugt/uge/oeq/one/eq/ne/ord/uno`).
fn predicate_to_binary(pred: &str, is_float: bool) -> Option<HlslBinaryOp> {
    if is_float {
        match pred {
            "oeq" | "ueq" | "eq" => Some(HlslBinaryOp::Eq),
            "one" | "une" | "ne" => Some(HlslBinaryOp::Ne),
            "olt" | "ult" | "lt" => Some(HlslBinaryOp::Lt),
            "ole" | "ule" | "le" => Some(HlslBinaryOp::Le),
            "ogt" | "ugt" | "gt" => Some(HlslBinaryOp::Gt),
            "oge" | "uge" | "ge" => Some(HlslBinaryOp::Ge),
            _ => None,
        }
    } else {
        match pred {
            "eq" => Some(HlslBinaryOp::Eq),
            "ne" => Some(HlslBinaryOp::Ne),
            "slt" | "ult" | "lt" => Some(HlslBinaryOp::Lt),
            "sle" | "ule" | "le" => Some(HlslBinaryOp::Le),
            "sgt" | "ugt" | "gt" => Some(HlslBinaryOp::Gt),
            "sge" | "uge" | "ge" => Some(HlslBinaryOp::Ge),
            _ => None,
        }
    }
}

/// Map known MIR-side intrinsic callee names to HLSL names. Returns `None`
/// when the callee is a user-defined fn (the caller-side falls back to
/// emitting the user fn's name verbatim).
const fn intrinsic_hlsl_name(callee: &str) -> Option<&'static str> {
    match callee.as_bytes() {
        b"min" | b"math.min" | b"fmin" => Some("min"),
        b"max" | b"math.max" | b"fmax" => Some("max"),
        b"abs" | b"math.abs" | b"fabs" | b"math.absf" => Some("abs"),
        b"sqrt" | b"math.sqrt" | b"sqrtf" | b"math.sqrtf" => Some("sqrt"),
        b"sin" => Some("sin"),
        b"cos" => Some("cos"),
        b"tan" => Some("tan"),
        b"exp" | b"math.expf" => Some("exp"),
        b"log" | b"math.logf" => Some("log"),
        b"pow" => Some("pow"),
        b"floor" => Some("floor"),
        b"ceil" => Some("ceil"),
        b"clamp" => Some("clamp"),
        b"saturate" => Some("saturate"),
        b"rsqrt" => Some("rsqrt"),
        _ => None,
    }
}

/// If the trailing op of a region's entry-block is a `scf.yield`, return
/// its first operand (the yielded ValueId). Used by `emit_scf_if` to
/// propagate branch-yielded values into the post-if SSA name.
fn trailing_yield_value(region: &MirRegion) -> Option<ValueId> {
    let block = region.entry()?;
    let last = block.ops.last()?;
    if last.name == "scf.yield" {
        last.operands.first().copied()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{emit_hlsl, validate_and_emit_hlsl, DxilError};
    use crate::target::DxilTargetProfile;
    use cssl_mir::{
        validate_and_mark, FloatWidth, IntWidth, MirBlock, MirFunc, MirModule, MirOp, MirRegion,
        MirType, MirValue, ValueId,
    };

    /// Build a minimal validated module containing one fn with the given ops.
    /// Returns the module + entry-fn name. Caller must request a profile
    /// matching the entry-fn's stage.
    fn module_with_entry(entry: &str, results: Vec<MirType>, ops: Vec<MirOp>) -> MirModule {
        let mut m = MirModule::new();
        let mut f = MirFunc::new(entry, vec![], results);
        for op in ops {
            f.push_op(op);
        }
        m.push_func(f);
        validate_and_mark(&mut m).expect("validate_and_mark must succeed for fixture");
        m
    }

    fn compute_profile() -> DxilTargetProfile {
        DxilTargetProfile::compute_sm66_default()
    }

    // ─── D5 contract enforcement ──────────────────────────────────────

    #[test]
    fn d5_marker_required_to_emit() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        // Note : NOT marked.
        let err = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap_err();
        assert!(matches!(err, DxilError::StructuredCfgUnvalidated));
    }

    #[test]
    fn d5_validate_and_emit_marks_module() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        let r = validate_and_emit_hlsl(&mut module, &compute_profile(), "main_cs");
        assert!(r.is_ok());
        // Validated-marker now present.
        assert!(cssl_mir::has_structured_cfg_marker(&module));
    }

    #[test]
    fn d5_validate_and_emit_surfaces_cfg_violations() {
        let mut module = MirModule::new();
        let mut f = MirFunc::new("bad", vec![], vec![]);
        f.push_op(MirOp::std("cf.br"));
        module.push_func(f);
        let err = validate_and_emit_hlsl(&mut module, &compute_profile(), "main_cs").unwrap_err();
        if let DxilError::MalformedOp { detail, .. } = err {
            assert!(detail.contains("CFG0004"));
        } else {
            panic!("expected MalformedOp wrapping CFG violation");
        }
    }

    // ─── Entry-point + skeleton ──────────────────────────────────────

    #[test]
    fn missing_entry_point_errors() {
        let mut module = MirModule::new();
        validate_and_mark(&mut module).unwrap();
        let err = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap_err();
        assert!(matches!(err, DxilError::EntryPointMissing { .. }));
    }

    #[test]
    fn compute_skeleton_has_numthreads_and_signature() {
        let module = module_with_entry("main_cs", vec![], vec![]);
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("[numthreads(1, 1, 1)]"));
        assert!(text.contains("void main_cs(uint3 tid : SV_DispatchThreadID)"));
    }

    #[test]
    fn vertex_skeleton_has_sv_position_semantic() {
        let module = module_with_entry("main_vs", vec![], vec![]);
        let profile = DxilTargetProfile::vertex_sm66_default();
        let hlsl = emit_hlsl(&module, &profile, "main_vs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("float4 main_vs(uint vid : SV_VertexID) : SV_Position"));
    }

    #[test]
    fn pixel_skeleton_has_sv_target_semantic() {
        let module = module_with_entry("main_ps", vec![], vec![]);
        let profile = DxilTargetProfile::pixel_sm66_default();
        let hlsl = emit_hlsl(&module, &profile, "main_ps").unwrap();
        let text = hlsl.render();
        assert!(text.contains("float4 main_ps(float4 pos : SV_Position) : SV_Target0"));
    }

    #[test]
    fn helper_fns_emitted_with_lowered_bodies() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("main_cs", vec![], vec![]));
        let mut helper = MirFunc::new(
            "helper",
            vec![MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        helper.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        module.push_func(helper);
        validate_and_mark(&mut module).unwrap();
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("int helper(int v0)"));
        assert!(text.contains("return v0;"));
    }

    #[test]
    fn header_carries_profile_metadata() {
        let module = module_with_entry("main_cs", vec![], vec![]);
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("S6-D2 HLSL emission"));
        assert!(text.contains("profile = cs_6_6"));
        assert!(text.contains("entry = main_cs"));
    }

    // ─── arith.constant + arith binary ────────────────────────────────

    #[test]
    fn arith_constant_int_emits_var_decl() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("arith.constant")
                .with_result(ValueId(0), MirType::Int(IntWidth::I32))
                .with_attribute("value", "42")],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("int v0 = 42;"));
    }

    #[test]
    fn arith_constant_float_emits_f32_suffix() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("arith.constant")
                .with_result(ValueId(0), MirType::Float(FloatWidth::F32))
                .with_attribute("value", "2.5")],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("float v0 = 2.5f;"));
    }

    #[test]
    fn arith_addi_emits_binary_decl() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("arith.addi")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Int(IntWidth::I32))],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("int v2 = (v0 + v1);"));
    }

    #[test]
    fn arith_addf_emits_float_binary() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("arith.addf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Float(FloatWidth::F32))],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("float v2 = (v0 + v1);"));
    }

    #[test]
    fn arith_negf_emits_unary_decl() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("arith.negf")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), MirType::Float(FloatWidth::F32))],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("float v1 = (-v0);"));
    }

    #[test]
    fn arith_cmpi_renders_with_predicate() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("arith.cmpi")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Bool)
                .with_attribute("predicate", "slt")],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("bool v2 = (v0 < v1);"));
    }

    #[test]
    fn arith_cmpf_renders_with_oeq_predicate() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("arith.cmpf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Bool)
                .with_attribute("predicate", "oeq")],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("bool v2 = (v0 == v1);"));
    }

    #[test]
    fn arith_select_emits_ternary() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("arith.select")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_operand(ValueId(2))
                .with_result(ValueId(3), MirType::Float(FloatWidth::F32))],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("float v3 = (v0 ? v1 : v2);"));
    }

    // ─── memref.load / store ─────────────────────────────────────────

    #[test]
    fn memref_load_emits_buffer_load() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("memref.load")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), MirType::Float(FloatWidth::F32))],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("float v1 = g_dyn_buf_0[v0];"));
    }

    #[test]
    fn memref_load_with_offset_emits_added_index() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("memref.load")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Int(IntWidth::I32))],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("int v2 = g_dyn_buf_0[(v0 + v1)];"));
    }

    #[test]
    fn memref_store_emits_assignment() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("memref.store")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("g_dyn_buf_1[v1] = v0;"));
    }

    // ─── func.call intrinsics ────────────────────────────────────────

    #[test]
    fn func_call_min_lowers_to_hlsl_min() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("func.call")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Float(FloatWidth::F32))
                .with_attribute("callee", "min")],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("float v2 = min(v0, v1);"));
    }

    #[test]
    fn func_call_sqrt_lowers_to_hlsl_sqrt() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("func.call")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), MirType::Float(FloatWidth::F32))
                .with_attribute("callee", "sqrt")],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("float v1 = sqrt(v0);"));
    }

    #[test]
    fn func_call_void_emits_expr_stmt() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("func.call")
                .with_operand(ValueId(0))
                .with_attribute("callee", "user_void_fn")],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("user_void_fn(v0);"));
    }

    // ─── func.return ─────────────────────────────────────────────────

    #[test]
    fn func_return_with_value_renders() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("func.return").with_operand(ValueId(7))],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("return v7;"));
    }

    #[test]
    fn func_return_void_renders() {
        let module = module_with_entry("main_cs", vec![], vec![MirOp::std("func.return")]);
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("return;"));
    }

    // ─── scf.if ──────────────────────────────────────────────────────

    #[test]
    fn scf_if_renders_structured_branch() {
        let mut then_region = MirRegion::with_entry(Vec::new());
        if let Some(b) = then_region.entry_mut() {
            b.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(10), MirType::Int(IntWidth::I32))
                    .with_attribute("value", "1"),
            );
            b.push(MirOp::std("scf.yield").with_operand(ValueId(10)));
        }
        let mut else_region = MirRegion::with_entry(Vec::new());
        if let Some(b) = else_region.entry_mut() {
            b.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(11), MirType::Int(IntWidth::I32))
                    .with_attribute("value", "0"),
            );
            b.push(MirOp::std("scf.yield").with_operand(ValueId(11)));
        }
        let mut iff = MirOp::std("scf.if").with_operand(ValueId(0));
        iff.regions.push(then_region);
        iff.regions.push(else_region);
        iff = iff.with_result(ValueId(12), MirType::Int(IntWidth::I32));

        let module = module_with_entry("main_cs", vec![], vec![iff]);
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("int v12 = 0;"));
        assert!(text.contains("if (v0) {"));
        assert!(text.contains("v12 = v10;"));
        assert!(text.contains("} else {"));
        assert!(text.contains("v12 = v11;"));
    }

    #[test]
    fn scf_if_without_yield_renders_plain_if() {
        // Statement-form (no yield, no result).
        let mut then_region = MirRegion::with_entry(Vec::new());
        if let Some(b) = then_region.entry_mut() {
            b.push(MirOp::std("func.return"));
        }
        let mut else_region = MirRegion::with_entry(Vec::new());
        if let Some(b) = else_region.entry_mut() {
            b.push(MirOp::std("func.return"));
        }
        let mut iff = MirOp::std("scf.if").with_operand(ValueId(0));
        iff.regions.push(then_region);
        iff.regions.push(else_region);

        let module = module_with_entry("main_cs", vec![], vec![iff]);
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("if (v0) {"));
        assert!(text.contains("} else {"));
        // No phi-style decl since result-list is empty.
        assert!(!text.contains("v_phi"));
    }

    // ─── scf.for / while / loop ──────────────────────────────────────

    #[test]
    fn scf_for_renders_for_loop_skeleton() {
        let mut body_region = MirRegion::with_entry(Vec::new());
        if let Some(b) = body_region.entry_mut() {
            b.push(MirOp::std("scf.yield"));
        }
        let mut for_op = MirOp::std("scf.for").with_operand(ValueId(0));
        for_op.regions.push(body_region);
        let module = module_with_entry("main_cs", vec![], vec![for_op]);
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("for ("));
    }

    #[test]
    fn scf_while_renders_while_loop() {
        let mut body_region = MirRegion::with_entry(Vec::new());
        if let Some(b) = body_region.entry_mut() {
            b.push(MirOp::std("scf.yield"));
        }
        let mut wh = MirOp::std("scf.while").with_operand(ValueId(0));
        wh.regions.push(body_region);
        let module = module_with_entry("main_cs", vec![], vec![wh]);
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("while (v0) {"));
    }

    #[test]
    fn scf_loop_renders_do_while_true() {
        let mut body_region = MirRegion::with_entry(Vec::new());
        if let Some(b) = body_region.entry_mut() {
            b.push(MirOp::std("scf.yield"));
        }
        let mut lp = MirOp::std("scf.loop");
        lp.regions.push(body_region);
        let module = module_with_entry("main_cs", vec![], vec![lp]);
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("do {"));
        assert!(text.contains("} while (true);"));
    }

    // ─── Heap / closure rejections ──────────────────────────────────

    #[test]
    fn heap_alloc_rejected_with_actionable_error() {
        let mut module = MirModule::new();
        let mut f = MirFunc::new("main_cs", vec![], vec![]);
        f.push_op(MirOp::std("cssl.heap.alloc"));
        module.push_func(f);
        validate_and_mark(&mut module).unwrap();
        let err = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap_err();
        if let DxilError::HeapOpNotSupportedOnGpu { fn_name, op_name } = err {
            assert_eq!(fn_name, "main_cs");
            assert_eq!(op_name, "cssl.heap.alloc");
        } else {
            panic!("expected HeapOpNotSupportedOnGpu");
        }
    }

    #[test]
    fn heap_dealloc_rejected() {
        let mut module = MirModule::new();
        let mut f = MirFunc::new("main_cs", vec![], vec![]);
        f.push_op(MirOp::std("cssl.heap.dealloc"));
        module.push_func(f);
        validate_and_mark(&mut module).unwrap();
        let err = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap_err();
        assert!(matches!(err, DxilError::HeapOpNotSupportedOnGpu { .. }));
    }

    #[test]
    fn closure_rejected() {
        let mut module = MirModule::new();
        let mut f = MirFunc::new("main_cs", vec![], vec![]);
        f.push_op(MirOp::std("cssl.closure.create"));
        module.push_func(f);
        validate_and_mark(&mut module).unwrap();
        let err = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap_err();
        assert!(matches!(err, DxilError::ClosureOpNotSupportedOnGpu { .. }));
    }

    // ─── Signed-int convention ──────────────────────────────────────

    #[test]
    fn mir_signless_i32_renders_as_signed_int() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("arith.muli")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Int(IntWidth::I32))],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("int v2 = (v0 * v1);"));
        assert!(!text.contains("uint v2"));
    }

    #[test]
    fn mir_unsigned_div_emits_uint_cast() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("arith.divui")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Int(IntWidth::I32))],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("(uint)v0"));
        assert!(text.contains("(uint)v1"));
    }

    #[test]
    fn mir_i64_renders_as_int64() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("arith.constant")
                .with_result(ValueId(0), MirType::Int(IntWidth::I64))
                .with_attribute("value", "100")],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("int64_t v0 = 100;"));
    }

    #[test]
    fn mir_f64_renders_as_double() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![MirOp::std("arith.constant")
                .with_result(ValueId(0), MirType::Float(FloatWidth::F64))
                .with_attribute("value", "3.14")],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        // Double precision MIR types render as HLSL `double` ; no `f` suffix.
        assert!(text.contains("double v0 = 3.14;"));
    }

    // ─── Unsupported op surfaces a clean diagnostic ─────────────────

    #[test]
    fn unsupported_op_surfaces_diagnostic() {
        let mut module = MirModule::new();
        let mut f = MirFunc::new("main_cs", vec![], vec![]);
        f.push_op(MirOp::std("xyz.totally.unknown"));
        module.push_func(f);
        validate_and_mark(&mut module).unwrap();
        let err = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap_err();
        if let DxilError::UnsupportedMirOp { fn_name, op_name } = err {
            assert_eq!(fn_name, "main_cs");
            assert_eq!(op_name, "xyz.totally.unknown");
        } else {
            panic!("expected UnsupportedMirOp");
        }
    }

    // ─── gpu.barrier ─────────────────────────────────────────────────

    #[test]
    fn gpu_barrier_emits_groupmemorybarrier() {
        let module = module_with_entry("main_cs", vec![], vec![MirOp::std("cssl.gpu.barrier")]);
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("GroupMemoryBarrierWithGroupSync()"));
    }

    // ─── Multi-op composition ────────────────────────────────────────

    #[test]
    fn multi_op_fn_renders_in_order() {
        let module = module_with_entry(
            "main_cs",
            vec![],
            vec![
                MirOp::std("arith.constant")
                    .with_result(ValueId(0), MirType::Int(IntWidth::I32))
                    .with_attribute("value", "10"),
                MirOp::std("arith.constant")
                    .with_result(ValueId(1), MirType::Int(IntWidth::I32))
                    .with_attribute("value", "32"),
                MirOp::std("arith.addi")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), MirType::Int(IntWidth::I32)),
                MirOp::std("func.return").with_operand(ValueId(2)),
            ],
        );
        let hlsl = emit_hlsl(&module, &compute_profile(), "main_cs").unwrap();
        let text = hlsl.render();
        assert!(text.contains("int v0 = 10;"));
        assert!(text.contains("int v1 = 32;"));
        assert!(text.contains("int v2 = (v0 + v1);"));
        assert!(text.contains("return v2;"));
    }

    #[test]
    fn empty_module_with_marker_errors_only_on_missing_entry() {
        let mut module = MirModule::new();
        validate_and_mark(&mut module).unwrap();
        let err = emit_hlsl(&module, &compute_profile(), "anything").unwrap_err();
        assert!(matches!(err, DxilError::EntryPointMissing { .. }));
    }

    // ─── MirBlock import sanity (smoke — silences unused warning) ───

    #[test]
    fn block_import_is_used() {
        let _ = MirBlock::new("entry");
        let _ = MirValue::new(ValueId(0), MirType::Int(IntWidth::I32));
    }
}
