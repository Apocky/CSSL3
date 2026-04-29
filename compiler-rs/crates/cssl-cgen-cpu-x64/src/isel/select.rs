//! § select — MIR → [`X64Func`] instruction-selection.
//!
//! § ROLE
//!   Foundation slice S7-G1. Walks a [`MirFunc`] body and emits virtual-
//!   register-based [`X64Inst`]s + [`X64Term`]s. Subset-matched to the same
//!   D5-validated MIR op-set the cranelift JIT consumes.
//!
//! § ENTRY-POINT
//!   - [`select_function`] — single fn (asserts the parent module's D5
//!     marker is set).
//!   - [`select_module`]   — convenience over [`MirModule`] : checks the
//!     marker once, then selects every fn into a `Vec<X64Func>`.
//!
//! § INVARIANTS
//!   - **D5 marker required** : the parent module must carry the
//!     `("structured_cfg.validated", "true")` attribute. Calling this on an
//!     unmarked module is a programmer-error and surfaces as
//!     [`SelectError::StructuredCfgMarkerMissing`].
//!   - **Closures are rejected** : `cssl.closure.*` ops are a Phase-H concern ;
//!     this slice rejects them with [`SelectError::ClosureRejected`].
//!   - **Unstructured CFG is rejected** : `cf.br` / `cf.cond_br` /
//!     `cssl.unsupported(Break|Continue)` are rejected even if the marker is
//!     present (defense-in-depth).
//!   - **vreg id 0 is sentinel** : the selector starts allocating at 1.
//!
//! § SCF SCAFFOLDING
//!   The structured-control-flow ops drive the only place this slice creates
//!   new blocks :
//!     - **scf.if** : `then_block` + `else_block` + `merge_block` ; entry
//!       terminator becomes `Jcc(cond) then, else` ; both branches end with
//!       `Jmp merge` (carrying the yield value through a merge-vreg).
//!     - **scf.loop / scf.for** : `header` + `body` + `exit` ; entry → header,
//!       header → body (unconditional at G1), body → header (back-edge).
//!     - **scf.while** : `header` + `body` + `exit` ; header carries the
//!       cond test ; cond-true → body, cond-false → exit ; body → header
//!       (back-edge).
//!
//!   Per the slice handoff the iter-counter / IV-block-arg machinery for
//!   `scf.for` is the same deferred-shape as cranelift's `scf.rs` : at G1
//!   we run the body once (single-trip) and document the future plumbing.
//!
//! § COVERAGE
//!   See `crate` doc-block for the full MIR-op → X64Inst table. Anything
//!   not in that table surfaces [`SelectError::UnsupportedOp`].

use std::collections::HashMap;

use cssl_mir::{
    has_structured_cfg_marker, FloatWidth, IntWidth, MirFunc, MirModule, MirOp, MirRegion, MirType,
    ValueId,
};

use super::func::{X64Func, X64Signature};
use super::inst::{
    BlockId, FpCmpKind, IntCmpKind, MemAddr, X64Imm, X64Inst, X64SetCondCode, X64Term,
};
use super::vreg::{X64VReg, X64Width};

/// Errors surfaced by [`select_function`] / [`select_module`].
///
/// § STABILITY
///   These error variants correspond to stable diagnostic codes prefixed
///   `X64-`. Mirroring the GPU emitters' code-allocation discipline (per
///   `SESSION_6_DISPATCH_PLAN.md § 3 escalation #4`), this initial allocation
///   covers every reject-shape the current MIR dialect produces ; future
///   shapes get new codes via a follow-up DECISIONS sub-entry.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SelectError {
    /// **X64-D5** : parent module is missing the structured-CFG validator
    /// marker. Per the D5 fanout-contract the GPU emitters honor, the CPU
    /// backend rejects calls that bypass the validator.
    #[error(
        "X64-D5: parent module is missing the structured_cfg.validated marker; \
         run cssl_mir::validate_and_mark before select_function (T11-D70 fanout-contract)"
    )]
    StructuredCfgMarkerMissing,

    /// **X64-0001** : MIR fn signature uses a type the selector can't lower.
    #[error(
        "X64-0001: fn `{fn_name}` signature contains unsupported type `{ty}` \
         (param/result type not stage-0 scalar)"
    )]
    UnsupportedSignatureType { fn_name: String, ty: String },

    /// **X64-0002** : MIR op result/operand type isn't a stage-0 scalar.
    #[error("X64-0002: fn `{fn_name}` op `{op}` references unsupported type `{ty}`")]
    UnsupportedType {
        fn_name: String,
        op: String,
        ty: String,
    },

    /// **X64-0003** : MIR fn body has no entry block.
    #[error("X64-0003: fn `{fn_name}` has empty body (no entry block)")]
    EmptyBody { fn_name: String },

    /// **X64-0004** : MIR op references a `ValueId` not in scope.
    #[error("X64-0004: fn `{fn_name}` op `{op}` references unknown ValueId({value_id})")]
    UnknownValueId {
        fn_name: String,
        op: String,
        value_id: u32,
    },

    /// **X64-0005** : MIR `arith.constant` is missing its `value` attribute.
    #[error("X64-0005: fn `{fn_name}` arith.constant is missing the `value` attribute")]
    ConstantMissingValue { fn_name: String },

    /// **X64-0006** : `arith.cmpi` / `arith.cmpf` is missing the `predicate`
    /// attribute, or the predicate is unrecognized.
    #[error(
        "X64-0006: fn `{fn_name}` op `{op}` has bad predicate `{predicate}` \
         (missing or unrecognized)"
    )]
    BadComparisonPredicate {
        fn_name: String,
        op: String,
        predicate: String,
    },

    /// **X64-0007** : MIR op had the wrong operand count for its known shape.
    #[error("X64-0007: fn `{fn_name}` op `{op}` expected {expected} operands, got {actual}")]
    OperandCountMismatch {
        fn_name: String,
        op: String,
        expected: usize,
        actual: usize,
    },

    /// **X64-0008** : MIR op had the wrong result count.
    #[error("X64-0008: fn `{fn_name}` op `{op}` expected {expected} results, got {actual}")]
    ResultCountMismatch {
        fn_name: String,
        op: String,
        expected: usize,
        actual: usize,
    },

    /// **X64-0009** : `scf.if` had a region count ≠ 2.
    #[error("X64-0009: fn `{fn_name}` scf.if has {actual} regions ; expected 2 (then + else)")]
    ScfIfWrongRegionCount { fn_name: String, actual: usize },

    /// **X64-0010** : loop op (`scf.for` / `scf.while` / `scf.loop`) had a
    /// region count ≠ 1.
    #[error("X64-0010: fn `{fn_name}` scf.{op_name} has {actual} regions ; expected 1 (body)")]
    LoopWrongRegionCount {
        fn_name: String,
        op_name: String,
        actual: usize,
    },

    /// **X64-0011** : nested region had ≠ 1 block (stage-0 expects exactly
    /// one entry-block per nested region).
    #[error(
        "X64-0011: fn `{fn_name}` scf.{op_name} nested region has {block_count} blocks ; \
         expected 1"
    )]
    ScfRegionMultiBlock {
        fn_name: String,
        op_name: String,
        block_count: usize,
    },

    /// **X64-0012** : `cf.br` / `cf.cond_br` reached the selector. Defense-in-
    /// depth against D5 bypass.
    #[error(
        "X64-0012: fn `{fn_name}` contains unstructured `{op}` op ; \
         CSSLv3 emits structured scf.* (defense-in-depth)"
    )]
    UnstructuredOp { fn_name: String, op: String },

    /// **X64-0013** : `cssl.closure.*` reached the selector. Closures are a
    /// Phase-H concern and explicitly out of S7-G1 scope.
    #[error(
        "X64-0013: fn `{fn_name}` contains closure op `{op}` ; \
         closures are deferred to Phase-H"
    )]
    ClosureRejected { fn_name: String, op: String },

    /// **X64-0014** : `cssl.unsupported(Break)` / `cssl.unsupported(Continue)`
    /// placeholder reached the selector.
    #[error(
        "X64-0014: fn `{fn_name}` contains unsupported `{op}` op ; \
         break/continue lowering is deferred"
    )]
    UnsupportedBreakContinue { fn_name: String, op: String },

    /// **X64-0015** : an op-name has no handler in the selector. Distinct from
    /// the explicit-rejection variants above so callers can grep.
    #[error("X64-0015: fn `{fn_name}` has unsupported op `{op}` (no selector entry)")]
    UnsupportedOp { fn_name: String, op: String },
}

impl SelectError {
    /// Stable diagnostic-code (e.g. `"X64-D5"`, `"X64-0001"`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::StructuredCfgMarkerMissing => "X64-D5",
            Self::UnsupportedSignatureType { .. } => "X64-0001",
            Self::UnsupportedType { .. } => "X64-0002",
            Self::EmptyBody { .. } => "X64-0003",
            Self::UnknownValueId { .. } => "X64-0004",
            Self::ConstantMissingValue { .. } => "X64-0005",
            Self::BadComparisonPredicate { .. } => "X64-0006",
            Self::OperandCountMismatch { .. } => "X64-0007",
            Self::ResultCountMismatch { .. } => "X64-0008",
            Self::ScfIfWrongRegionCount { .. } => "X64-0009",
            Self::LoopWrongRegionCount { .. } => "X64-0010",
            Self::ScfRegionMultiBlock { .. } => "X64-0011",
            Self::UnstructuredOp { .. } => "X64-0012",
            Self::ClosureRejected { .. } => "X64-0013",
            Self::UnsupportedBreakContinue { .. } => "X64-0014",
            Self::UnsupportedOp { .. } => "X64-0015",
        }
    }
}

/// Translate a [`MirType`] to an [`X64Width`]. Returns `None` for non-scalar
/// types ; callers turn the `None` into [`SelectError::UnsupportedSignatureType`]
/// or [`SelectError::UnsupportedType`] depending on context.
#[must_use]
pub fn mir_to_x64_width(ty: &MirType) -> Option<X64Width> {
    match ty {
        MirType::Bool => Some(X64Width::Bool),
        MirType::Int(IntWidth::I1) => Some(X64Width::Bool),
        MirType::Int(IntWidth::I8) => Some(X64Width::I8),
        MirType::Int(IntWidth::I16) => Some(X64Width::I16),
        MirType::Int(IntWidth::I32) => Some(X64Width::I32),
        MirType::Int(IntWidth::I64 | IntWidth::Index) => Some(X64Width::I64),
        MirType::Float(FloatWidth::F32) => Some(X64Width::F32),
        MirType::Float(FloatWidth::F64) => Some(X64Width::F64),
        MirType::Ptr | MirType::Handle => Some(X64Width::Ptr),
        // Half-floats + non-scalars deferred — caller surfaces a typed error.
        MirType::Float(FloatWidth::F16 | FloatWidth::Bf16)
        | MirType::None
        | MirType::Tuple(_)
        | MirType::Function { .. }
        | MirType::Memref { .. }
        | MirType::Vec(_, _)
        | MirType::Opaque(_) => None,
    }
}

/// Selection context — threads the value-id → vreg map + the function under
/// construction through nested-region walks.
struct Ctx<'a> {
    /// Source MIR fn — used for diagnostic threading + region walks.
    src: &'a MirFunc,
    /// Output X64Func.
    out: X64Func,
    /// MIR `ValueId` → x64 vreg mapping.
    val_map: HashMap<ValueId, X64VReg>,
    /// Block currently being filled with instructions. Updated as we walk
    /// into / out of scf nested regions.
    current_block: BlockId,
    /// `true` once a func.return / cssl.diff.bwd_return terminated the body
    /// at the outer level. Used to skip auto-emit of a default Ret.
    saw_return: bool,
    /// Stack of yield-target vregs : when entering a scf.if branch region,
    /// we push the merge-vreg (or `None` if scf.if has no result) ; an
    /// scf.yield inside resolves to a `Mov merge_vreg, yield_vreg`. Loops
    /// push `None` (yield inside a loop body is a no-op at G1).
    yield_target_stack: Vec<Option<X64VReg>>,
}

/// Select one [`MirFunc`] into an [`X64Func`].
///
/// # Errors
///   - [`SelectError::StructuredCfgMarkerMissing`] if `parent` lacks the D5
///     marker.
///   - One of the X64-0001..X64-0015 variants on a per-op selection failure.
pub fn select_function(parent: &MirModule, src: &MirFunc) -> Result<X64Func, SelectError> {
    if !has_structured_cfg_marker(parent) {
        return Err(SelectError::StructuredCfgMarkerMissing);
    }
    select_function_unmarked(src)
}

/// Internal entry that skips the marker check ; used by [`select_module`]
/// after a single up-front check, avoiding the per-fn re-check.
fn select_function_unmarked(src: &MirFunc) -> Result<X64Func, SelectError> {
    // Translate signature.
    let mut param_widths = Vec::with_capacity(src.params.len());
    for p in &src.params {
        let w = mir_to_x64_width(p).ok_or_else(|| SelectError::UnsupportedSignatureType {
            fn_name: src.name.clone(),
            ty: p.to_string(),
        })?;
        param_widths.push(w);
    }
    let mut result_widths = Vec::with_capacity(src.results.len());
    for r in &src.results {
        let w = mir_to_x64_width(r).ok_or_else(|| SelectError::UnsupportedSignatureType {
            fn_name: src.name.clone(),
            ty: r.to_string(),
        })?;
        result_widths.push(w);
    }
    let sig = X64Signature::new(param_widths.clone(), result_widths);
    let out = X64Func::new(&src.name, sig);

    // Wire entry-block param vregs into the value map.
    let mut val_map: HashMap<ValueId, X64VReg> = HashMap::new();
    let entry_block = src.body.entry().ok_or_else(|| SelectError::EmptyBody {
        fn_name: src.name.clone(),
    })?;
    for (idx, mv) in entry_block.args.iter().enumerate() {
        if idx >= param_widths.len() {
            // Body has more args than fn signature — bail out.
            return Err(SelectError::OperandCountMismatch {
                fn_name: src.name.clone(),
                op: "<entry-args>".to_string(),
                expected: param_widths.len(),
                actual: entry_block.args.len(),
            });
        }
        // Use `out.param_vreg(idx)` so the convention "params occupy ids
        // 1..=N" is enforced in one place.
        let v = out.param_vreg(idx);
        val_map.insert(mv.id, v);
    }

    let mut ctx = Ctx {
        src,
        out,
        val_map,
        current_block: BlockId::ENTRY,
        saw_return: false,
        yield_target_stack: Vec::new(),
    };

    // Walk top-level ops in the body.
    walk_region(&mut ctx, &src.body)?;

    // Auto-emit a default Ret if the body didn't terminate.
    if !ctx.saw_return {
        if ctx.src.results.is_empty() {
            ctx.out
                .set_terminator(ctx.current_block, X64Term::Ret { operands: vec![] });
        } else {
            // Body has a result but no func.return — diagnostic.
            return Err(SelectError::EmptyBody {
                fn_name: ctx.src.name.clone(),
            });
        }
    }

    Ok(ctx.out)
}

/// Convenience : select every fn in `module` into `Vec<X64Func>`.
///
/// # Errors
/// Same as [`select_function`] ; first-failure short-circuits.
pub fn select_module(module: &MirModule) -> Result<Vec<X64Func>, SelectError> {
    if !has_structured_cfg_marker(module) {
        return Err(SelectError::StructuredCfgMarkerMissing);
    }
    let mut out = Vec::with_capacity(module.funcs.len());
    for f in &module.funcs {
        out.push(select_function_unmarked(f)?);
    }
    Ok(out)
}

// ────────────────────────────────────────────────────────────────────────
// § Region + op walker.
// ────────────────────────────────────────────────────────────────────────

/// Walk a region (single-block at stage-0). Used both for the fn body and
/// for nested scf-region branches.
fn walk_region(ctx: &mut Ctx<'_>, region: &MirRegion) -> Result<(), SelectError> {
    let Some(block) = region.blocks.first() else {
        return Err(SelectError::EmptyBody {
            fn_name: ctx.src.name.clone(),
        });
    };
    if region.blocks.len() > 1 {
        // Stage-0 expects exactly one block per region.
        return Err(SelectError::ScfRegionMultiBlock {
            fn_name: ctx.src.name.clone(),
            op_name: "<region>".to_string(),
            block_count: region.blocks.len(),
        });
    }
    for op in &block.ops {
        if walk_op(ctx, op)? {
            // saw_return at outer level breaks
            break;
        }
    }
    Ok(())
}

/// Walk one op. Returns `Ok(true)` if the op was a terminator (`func.return`)
/// at the current block ; the caller stops walking subsequent ops.
fn walk_op(ctx: &mut Ctx<'_>, op: &MirOp) -> Result<bool, SelectError> {
    match op.name.as_str() {
        // ─── Constants ─────────────────────────────────────────────────
        "arith.constant" => {
            select_constant(ctx, op)?;
            Ok(false)
        }
        // ─── Integer arithmetic ────────────────────────────────────────
        "arith.addi" => select_int_binary(ctx, op, IntBinOp::Add).map(|()| false),
        "arith.subi" => select_int_binary(ctx, op, IntBinOp::Sub).map(|()| false),
        "arith.muli" => select_int_binary(ctx, op, IntBinOp::Mul).map(|()| false),
        "arith.sdivi" => select_int_div(ctx, op, /*signed=*/ true).map(|()| false),
        "arith.udivi" => select_int_div(ctx, op, /*signed=*/ false).map(|()| false),
        // ─── Float arithmetic ──────────────────────────────────────────
        "arith.addf" => select_fp_binary(ctx, op, FpBinOp::Add).map(|()| false),
        "arith.subf" => select_fp_binary(ctx, op, FpBinOp::Sub).map(|()| false),
        "arith.mulf" => select_fp_binary(ctx, op, FpBinOp::Mul).map(|()| false),
        "arith.divf" => select_fp_binary(ctx, op, FpBinOp::Div).map(|()| false),
        "arith.negf" => select_fp_neg(ctx, op).map(|()| false),
        // ─── Comparisons ────────────────────────────────────────────────
        "arith.cmpi" => select_cmpi(ctx, op).map(|()| false),
        "arith.cmpf" => select_cmpf(ctx, op).map(|()| false),
        "arith.select" => select_select(ctx, op).map(|()| false),
        // ─── Memory ─────────────────────────────────────────────────────
        "memref.load" => select_memref_load(ctx, op).map(|()| false),
        "memref.store" => select_memref_store(ctx, op).map(|()| false),
        // ─── Function call ──────────────────────────────────────────────
        "func.call" => select_func_call(ctx, op).map(|()| false),
        // ─── Heap (FFI to cssl-rt) ──────────────────────────────────────
        "cssl.heap.alloc" => {
            select_heap_call(ctx, op, "__cssl_alloc", X64Width::Ptr).map(|()| false)
        }
        "cssl.heap.dealloc" => select_heap_call_void(ctx, op, "__cssl_free").map(|()| false),
        "cssl.heap.realloc" => {
            select_heap_call(ctx, op, "__cssl_realloc", X64Width::Ptr).map(|()| false)
        }
        // ─── Structured CFG ────────────────────────────────────────────
        "scf.if" => select_scf_if(ctx, op).map(|()| false),
        "scf.for" => select_scf_loop(ctx, op, "for").map(|()| false),
        "scf.while" => select_scf_while(ctx, op).map(|()| false),
        "scf.loop" => select_scf_loop(ctx, op, "loop").map(|()| false),
        // scf.yield outside a parent scf.if is consumed at the outer level.
        // Inside an scf branch the parent walker reads it as the yield-source
        // and emits the Mov-to-merge-vreg ; here we only see it if a fn body
        // has a stray scf.yield, in which case (matching cranelift's policy)
        // we tolerate it as a no-op so legacy hand-built MIR keeps lowering.
        "scf.yield" => {
            // If we're inside a nested scf.if region, the yield resolves to
            // a Mov merge_vreg <- yield_value. The yield_target_stack tells
            // us the merge vreg.
            if let Some(Some(target)) = ctx.yield_target_stack.last().copied() {
                let yield_id = op.operands.first().copied();
                if let Some(yid) = yield_id {
                    let src = lookup_vreg(ctx, &op.name, yid)?;
                    ctx.out
                        .push_inst(ctx.current_block, X64Inst::Mov { dst: target, src });
                }
            }
            Ok(false)
        }
        // ─── Returns ────────────────────────────────────────────────────
        "func.return" | "cssl.diff.bwd_return" => {
            let mut operands = Vec::with_capacity(op.operands.len());
            for vid in &op.operands {
                operands.push(lookup_vreg(ctx, &op.name, *vid)?);
            }
            ctx.out
                .set_terminator(ctx.current_block, X64Term::Ret { operands });
            ctx.saw_return = true;
            Ok(true)
        }
        // ─── Reject closures (Phase-H) ──────────────────────────────────
        n if n.starts_with("cssl.closure.") => Err(SelectError::ClosureRejected {
            fn_name: ctx.src.name.clone(),
            op: n.to_string(),
        }),
        // ─── Reject unstructured CFG (defense-in-depth) ────────────────
        "cf.br" | "cf.cond_br" => Err(SelectError::UnstructuredOp {
            fn_name: ctx.src.name.clone(),
            op: op.name.clone(),
        }),
        // ─── Reject break/continue placeholders ────────────────────────
        n if n.starts_with("cssl.unsupported")
            && (n.contains("Break") || n.contains("Continue")) =>
        {
            Err(SelectError::UnsupportedBreakContinue {
                fn_name: ctx.src.name.clone(),
                op: n.to_string(),
            })
        }
        // ─── Comment-passthrough ops (preserved for diagnostics) ──────
        // IFC labels / declassify / verify-assert / cssl.field carry meaning
        // at earlier compile passes ; by selection time they're already
        // proven and have no x86-64 emission. Mirror the WGSL emitter's
        // policy : accept silently, emit nothing.
        "cssl.ifc.label"
        | "cssl.ifc.declassify"
        | "cssl.verify.assert"
        | "cssl.field"
        | "cssl.region.enter"
        | "cssl.region.exit"
        | "cssl.telemetry.probe" => Ok(false),
        // ─── Default reject ────────────────────────────────────────────
        n => Err(SelectError::UnsupportedOp {
            fn_name: ctx.src.name.clone(),
            op: n.to_string(),
        }),
    }
}

// ────────────────────────────────────────────────────────────────────────
// § Per-op selection helpers.
// ────────────────────────────────────────────────────────────────────────

/// Select `arith.constant` : `MovImm(dst, imm)`.
fn select_constant(ctx: &mut Ctx<'_>, op: &MirOp) -> Result<(), SelectError> {
    let r = single_result(ctx, op)?;
    let value_str = op
        .attributes
        .iter()
        .find(|(k, _)| k == "value")
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| SelectError::ConstantMissingValue {
            fn_name: ctx.src.name.clone(),
        })?;
    let width = result_width(ctx, op, &r.ty)?;
    let dst = ctx.out.fresh_vreg(width);
    ctx.val_map.insert(r.id, dst);
    let imm = parse_imm(value_str, width).ok_or_else(|| SelectError::UnsupportedType {
        fn_name: ctx.src.name.clone(),
        op: op.name.clone(),
        ty: r.ty.to_string(),
    })?;
    ctx.out
        .push_inst(ctx.current_block, X64Inst::MovImm { dst, imm });
    Ok(())
}

/// Parse a constant value-string into an [`X64Imm`] of the given width.
fn parse_imm(raw: &str, width: X64Width) -> Option<X64Imm> {
    let s = raw.trim();
    match width {
        X64Width::Bool => match s {
            "true" | "1" => Some(X64Imm::Bool(true)),
            "false" | "0" => Some(X64Imm::Bool(false)),
            _ => None,
        },
        X64Width::I8 | X64Width::I16 | X64Width::I32 => {
            let v: i64 = s.parse().ok()?;
            Some(X64Imm::I32(v as i32))
        }
        X64Width::I64 | X64Width::Ptr => {
            let v: i64 = s.parse().ok()?;
            Some(X64Imm::I64(v))
        }
        X64Width::F32 => {
            let v: f32 = s.parse().ok()?;
            Some(X64Imm::F32(v.to_bits()))
        }
        X64Width::F64 => {
            let v: f64 = s.parse().ok()?;
            Some(X64Imm::F64(v.to_bits()))
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum IntBinOp {
    Add,
    Sub,
    Mul,
}

/// Select an integer binary arithmetic op : `addi` / `subi` / `muli`.
/// Pattern : `Mov dst, lhs ; Add/Sub/IMul dst, rhs`.
fn select_int_binary(ctx: &mut Ctx<'_>, op: &MirOp, kind: IntBinOp) -> Result<(), SelectError> {
    let (lhs_id, rhs_id) = two_operands(ctx, op)?;
    let r = single_result(ctx, op)?;
    let width = result_width(ctx, op, &r.ty)?;
    let lhs = lookup_vreg(ctx, &op.name, lhs_id)?;
    let rhs = lookup_vreg(ctx, &op.name, rhs_id)?;
    let dst = ctx.out.fresh_vreg(width);
    ctx.val_map.insert(r.id, dst);
    // dst <- mov lhs ; dst <- op dst, rhs
    ctx.out
        .push_inst(ctx.current_block, X64Inst::Mov { dst, src: lhs });
    let inst = match kind {
        IntBinOp::Add => X64Inst::Add { dst, src: rhs },
        IntBinOp::Sub => X64Inst::Sub { dst, src: rhs },
        IntBinOp::Mul => X64Inst::IMul { dst, src: rhs },
    };
    ctx.out.push_inst(ctx.current_block, inst);
    Ok(())
}

/// Select integer division (`sdivi` / `udivi`) :
///   - signed   : `mov dst, lhs ; cdq/cqo ; idiv rhs`
///   - unsigned : `mov dst, lhs ; xor rdx, rdx ; div rhs`
/// The selector emits the abstract sequence ; G2 (regalloc) is responsible
/// for pinning the dividend to eax/rax and the result back from eax/rax.
fn select_int_div(ctx: &mut Ctx<'_>, op: &MirOp, signed: bool) -> Result<(), SelectError> {
    let (lhs_id, rhs_id) = two_operands(ctx, op)?;
    let r = single_result(ctx, op)?;
    let width = result_width(ctx, op, &r.ty)?;
    let lhs = lookup_vreg(ctx, &op.name, lhs_id)?;
    let rhs = lookup_vreg(ctx, &op.name, rhs_id)?;
    let dst = ctx.out.fresh_vreg(width);
    ctx.val_map.insert(r.id, dst);
    ctx.out
        .push_inst(ctx.current_block, X64Inst::Mov { dst, src: lhs });
    if signed {
        // cdq for i32, cqo for i64 — explicit upper-half sign-extension per
        // x86-64 idiv ABI (see crate doc-block § INTEGER DIVISION DETAIL).
        match width {
            X64Width::I64 => ctx.out.push_inst(ctx.current_block, X64Inst::Cqo),
            _ => ctx.out.push_inst(ctx.current_block, X64Inst::Cdq),
        }
        ctx.out
            .push_inst(ctx.current_block, X64Inst::Idiv { divisor: rhs });
    } else {
        ctx.out
            .push_inst(ctx.current_block, X64Inst::XorRdx { width });
        ctx.out
            .push_inst(ctx.current_block, X64Inst::Div { divisor: rhs });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum FpBinOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// Select an SSE2 float binary op : `addf` / `subf` / `mulf` / `divf`.
fn select_fp_binary(ctx: &mut Ctx<'_>, op: &MirOp, kind: FpBinOp) -> Result<(), SelectError> {
    let (lhs_id, rhs_id) = two_operands(ctx, op)?;
    let r = single_result(ctx, op)?;
    let width = result_width(ctx, op, &r.ty)?;
    let lhs = lookup_vreg(ctx, &op.name, lhs_id)?;
    let rhs = lookup_vreg(ctx, &op.name, rhs_id)?;
    let dst = ctx.out.fresh_vreg(width);
    ctx.val_map.insert(r.id, dst);
    ctx.out
        .push_inst(ctx.current_block, X64Inst::Mov { dst, src: lhs });
    let inst = match kind {
        FpBinOp::Add => X64Inst::FpAdd { dst, src: rhs },
        FpBinOp::Sub => X64Inst::FpSub { dst, src: rhs },
        FpBinOp::Mul => X64Inst::FpMul { dst, src: rhs },
        FpBinOp::Div => X64Inst::FpDiv { dst, src: rhs },
    };
    ctx.out.push_inst(ctx.current_block, inst);
    Ok(())
}

/// Select `arith.negf` : `xorps dst, sign_bit_mask` (sign-bit flip).
fn select_fp_neg(ctx: &mut Ctx<'_>, op: &MirOp) -> Result<(), SelectError> {
    let src_id = single_operand(ctx, op)?;
    let r = single_result(ctx, op)?;
    let width = result_width(ctx, op, &r.ty)?;
    let src = lookup_vreg(ctx, &op.name, src_id)?;
    let dst = ctx.out.fresh_vreg(width);
    ctx.val_map.insert(r.id, dst);
    ctx.out
        .push_inst(ctx.current_block, X64Inst::Mov { dst, src });
    ctx.out
        .push_inst(ctx.current_block, X64Inst::FpNeg { dst, width });
    Ok(())
}

/// Select `arith.cmpi` : `Cmp lhs, rhs ; Setcc dst`. Result is a boolean vreg.
fn select_cmpi(ctx: &mut Ctx<'_>, op: &MirOp) -> Result<(), SelectError> {
    let (lhs_id, rhs_id) = two_operands(ctx, op)?;
    let r = single_result(ctx, op)?;
    let pred = predicate(ctx, op)?;
    let kind = parse_int_cmp(pred).ok_or_else(|| SelectError::BadComparisonPredicate {
        fn_name: ctx.src.name.clone(),
        op: op.name.clone(),
        predicate: pred.to_string(),
    })?;
    let lhs = lookup_vreg(ctx, &op.name, lhs_id)?;
    let rhs = lookup_vreg(ctx, &op.name, rhs_id)?;
    let dst = ctx.out.fresh_vreg(X64Width::Bool);
    ctx.val_map.insert(r.id, dst);
    ctx.out
        .push_inst(ctx.current_block, X64Inst::Cmp { lhs, rhs });
    ctx.out.push_inst(
        ctx.current_block,
        X64Inst::Setcc {
            dst,
            cond_kind: X64SetCondCode::Int(kind),
        },
    );
    Ok(())
}

/// Select `arith.cmpf` : `Ucomi/Comi lhs, rhs ; Setcc dst`.
fn select_cmpf(ctx: &mut Ctx<'_>, op: &MirOp) -> Result<(), SelectError> {
    let (lhs_id, rhs_id) = two_operands(ctx, op)?;
    let r = single_result(ctx, op)?;
    let pred = predicate(ctx, op)?;
    let kind = parse_fp_cmp(pred).ok_or_else(|| SelectError::BadComparisonPredicate {
        fn_name: ctx.src.name.clone(),
        op: op.name.clone(),
        predicate: pred.to_string(),
    })?;
    let lhs = lookup_vreg(ctx, &op.name, lhs_id)?;
    let rhs = lookup_vreg(ctx, &op.name, rhs_id)?;
    let dst = ctx.out.fresh_vreg(X64Width::Bool);
    ctx.val_map.insert(r.id, dst);
    if kind.is_ordered() {
        ctx.out
            .push_inst(ctx.current_block, X64Inst::Ucomi { lhs, rhs });
    } else {
        ctx.out
            .push_inst(ctx.current_block, X64Inst::Comi { lhs, rhs });
    }
    ctx.out.push_inst(
        ctx.current_block,
        X64Inst::Setcc {
            dst,
            cond_kind: X64SetCondCode::Float(kind),
        },
    );
    Ok(())
}

/// Select `arith.select` : keep as a high-level [`X64Inst::Select`] so G2's
/// coalescer can pick the best concrete shape (cmov vs branch-and-mov).
fn select_select(ctx: &mut Ctx<'_>, op: &MirOp) -> Result<(), SelectError> {
    if op.operands.len() != 3 {
        return Err(SelectError::OperandCountMismatch {
            fn_name: ctx.src.name.clone(),
            op: op.name.clone(),
            expected: 3,
            actual: op.operands.len(),
        });
    }
    let r = single_result(ctx, op)?;
    let width = result_width(ctx, op, &r.ty)?;
    let cond = lookup_vreg(ctx, &op.name, op.operands[0])?;
    let if_true = lookup_vreg(ctx, &op.name, op.operands[1])?;
    let if_false = lookup_vreg(ctx, &op.name, op.operands[2])?;
    let dst = ctx.out.fresh_vreg(width);
    ctx.val_map.insert(r.id, dst);
    ctx.out.push_inst(
        ctx.current_block,
        X64Inst::Select {
            dst,
            cond,
            if_true,
            if_false,
        },
    );
    Ok(())
}

/// Select `memref.load` : `Load { dst, addr }`.
/// Operand shape per `specs/02_IR.csl § MEMORY-OPS` :
///   `(ptr : i64 [, offset : i64]) -> elem-T`.
fn select_memref_load(ctx: &mut Ctx<'_>, op: &MirOp) -> Result<(), SelectError> {
    if op.operands.is_empty() || op.operands.len() > 2 {
        return Err(SelectError::OperandCountMismatch {
            fn_name: ctx.src.name.clone(),
            op: op.name.clone(),
            expected: 1,
            actual: op.operands.len(),
        });
    }
    let r = single_result(ctx, op)?;
    let width = result_width(ctx, op, &r.ty)?;
    let ptr = lookup_vreg(ctx, &op.name, op.operands[0])?;
    let addr = if op.operands.len() == 2 {
        let idx = lookup_vreg(ctx, &op.name, op.operands[1])?;
        MemAddr::base_plus_index(ptr, idx)
    } else {
        MemAddr::base(ptr)
    };
    let dst = ctx.out.fresh_vreg(width);
    ctx.val_map.insert(r.id, dst);
    ctx.out
        .push_inst(ctx.current_block, X64Inst::Load { dst, addr });
    Ok(())
}

/// Select `memref.store` : `Store { src, addr }`.
/// Operand shape : `(val : T, ptr : i64 [, offset : i64]) -> ()`.
fn select_memref_store(ctx: &mut Ctx<'_>, op: &MirOp) -> Result<(), SelectError> {
    if op.operands.len() < 2 || op.operands.len() > 3 {
        return Err(SelectError::OperandCountMismatch {
            fn_name: ctx.src.name.clone(),
            op: op.name.clone(),
            expected: 2,
            actual: op.operands.len(),
        });
    }
    if !op.results.is_empty() {
        return Err(SelectError::ResultCountMismatch {
            fn_name: ctx.src.name.clone(),
            op: op.name.clone(),
            expected: 0,
            actual: op.results.len(),
        });
    }
    let val = lookup_vreg(ctx, &op.name, op.operands[0])?;
    let ptr = lookup_vreg(ctx, &op.name, op.operands[1])?;
    let addr = if op.operands.len() == 3 {
        let idx = lookup_vreg(ctx, &op.name, op.operands[2])?;
        MemAddr::base_plus_index(ptr, idx)
    } else {
        MemAddr::base(ptr)
    };
    ctx.out
        .push_inst(ctx.current_block, X64Inst::Store { src: val, addr });
    Ok(())
}

/// Select `func.call` : abstract `Call { callee, args, results }`. Args + results
/// are mapped into vregs. G3 lowers the abstract call into the System-V or
/// MS-x64 ABI ; G1 just records the operand list.
fn select_func_call(ctx: &mut Ctx<'_>, op: &MirOp) -> Result<(), SelectError> {
    let callee = op
        .attributes
        .iter()
        .find(|(k, _)| k == "callee")
        .map(|(_, v)| v.clone())
        .ok_or_else(|| SelectError::UnsupportedOp {
            fn_name: ctx.src.name.clone(),
            op: "func.call (missing `callee` attribute)".to_string(),
        })?;
    let mut args = Vec::with_capacity(op.operands.len());
    for vid in &op.operands {
        args.push(lookup_vreg(ctx, &op.name, *vid)?);
    }
    let mut results = Vec::with_capacity(op.results.len());
    for r in &op.results {
        let width = result_width(ctx, op, &r.ty)?;
        let dst = ctx.out.fresh_vreg(width);
        ctx.val_map.insert(r.id, dst);
        results.push(dst);
    }
    ctx.out.push_inst(
        ctx.current_block,
        X64Inst::Call {
            callee,
            args,
            results,
        },
    );
    Ok(())
}

/// Select `cssl.heap.alloc` / `cssl.heap.realloc` : abstract call into the
/// cssl-rt FFI symbol with the same argument shape as the MIR op carries.
fn select_heap_call(
    ctx: &mut Ctx<'_>,
    op: &MirOp,
    callee_sym: &str,
    result_width: X64Width,
) -> Result<(), SelectError> {
    let r = single_result(ctx, op)?;
    let mut args = Vec::with_capacity(op.operands.len());
    for vid in &op.operands {
        args.push(lookup_vreg(ctx, &op.name, *vid)?);
    }
    // Result type sanity-check : heap-alloc / realloc must produce a Ptr-shape.
    let mir_w = mir_to_x64_width(&r.ty).ok_or_else(|| SelectError::UnsupportedType {
        fn_name: ctx.src.name.clone(),
        op: op.name.clone(),
        ty: r.ty.to_string(),
    })?;
    if mir_w != result_width {
        // Defense : the heap ops should produce ptr-typed results. If not,
        // surface the mismatch cleanly rather than emitting a typed-mismatched
        // call.
        return Err(SelectError::UnsupportedType {
            fn_name: ctx.src.name.clone(),
            op: op.name.clone(),
            ty: r.ty.to_string(),
        });
    }
    let dst = ctx.out.fresh_vreg(result_width);
    ctx.val_map.insert(r.id, dst);
    ctx.out.push_inst(
        ctx.current_block,
        X64Inst::Call {
            callee: callee_sym.to_string(),
            args,
            results: vec![dst],
        },
    );
    Ok(())
}

/// Select `cssl.heap.dealloc` : abstract call producing no result.
fn select_heap_call_void(
    ctx: &mut Ctx<'_>,
    op: &MirOp,
    callee_sym: &str,
) -> Result<(), SelectError> {
    if !op.results.is_empty() {
        return Err(SelectError::ResultCountMismatch {
            fn_name: ctx.src.name.clone(),
            op: op.name.clone(),
            expected: 0,
            actual: op.results.len(),
        });
    }
    let mut args = Vec::with_capacity(op.operands.len());
    for vid in &op.operands {
        args.push(lookup_vreg(ctx, &op.name, *vid)?);
    }
    ctx.out.push_inst(
        ctx.current_block,
        X64Inst::Call {
            callee: callee_sym.to_string(),
            args,
            results: vec![],
        },
    );
    Ok(())
}

/// Select `scf.if` : create then/else/merge blocks ; entry → Jcc(cond) then,
/// else ; both branches end with Jmp merge ; if there's a result, declare a
/// merge-vreg + push it on the yield-target stack so scf.yield in either
/// branch resolves to a Mov.
fn select_scf_if(ctx: &mut Ctx<'_>, op: &MirOp) -> Result<(), SelectError> {
    if op.regions.len() != 2 {
        return Err(SelectError::ScfIfWrongRegionCount {
            fn_name: ctx.src.name.clone(),
            actual: op.regions.len(),
        });
    }
    if op.operands.len() != 1 {
        return Err(SelectError::OperandCountMismatch {
            fn_name: ctx.src.name.clone(),
            op: op.name.clone(),
            expected: 1,
            actual: op.operands.len(),
        });
    }
    let cond_id = op.operands[0];
    let cond = lookup_vreg(ctx, &op.name, cond_id)?;
    let then_block = ctx.out.fresh_block();
    let else_block = ctx.out.fresh_block();
    let merge_block = ctx.out.fresh_block();

    // Result merge-vreg — only allocated if the scf.if has a result.
    let merge_vreg: Option<X64VReg> = if let Some(r) = op.results.first() {
        let width = result_width(ctx, op, &r.ty)?;
        let v = ctx.out.fresh_vreg(width);
        ctx.val_map.insert(r.id, v);
        Some(v)
    } else {
        None
    };

    // Entry-block terminator : Jcc(cond) then_block, else_block.
    ctx.out.set_terminator(
        ctx.current_block,
        X64Term::Jcc {
            cond_kind: X64SetCondCode::Int(IntCmpKind::Ne), // tests cond != 0
            cond_vreg: cond,
            then_block,
            else_block,
        },
    );

    // Walk then-region.
    ctx.yield_target_stack.push(merge_vreg);
    let then_region = &op.regions[0];
    let saved_block = ctx.current_block;
    ctx.current_block = then_block;
    walk_region(ctx, then_region)?;
    // If the body didn't terminate (no func.return inside), close with Jmp merge.
    if matches!(
        ctx.out.blocks[then_block.0 as usize].terminator,
        X64Term::Unreachable
    ) {
        ctx.out.set_terminator(
            then_block,
            X64Term::Jmp {
                target: merge_block,
            },
        );
    }
    ctx.yield_target_stack.pop();

    // Walk else-region.
    ctx.yield_target_stack.push(merge_vreg);
    let else_region = &op.regions[1];
    ctx.current_block = else_block;
    walk_region(ctx, else_region)?;
    if matches!(
        ctx.out.blocks[else_block.0 as usize].terminator,
        X64Term::Unreachable
    ) {
        ctx.out.set_terminator(
            else_block,
            X64Term::Jmp {
                target: merge_block,
            },
        );
    }
    ctx.yield_target_stack.pop();

    // Continue selection at the merge block.
    ctx.current_block = merge_block;
    let _ = saved_block;
    Ok(())
}

/// Select `scf.loop` / `scf.for` : header / body / exit triplet.
/// At G1 these are emitted as an unconditional infinite loop ; an inner
/// `func.return` terminates. The iter-counter / IV-block-arg machinery for
/// `scf.for` is a deferred-shape (matches cranelift's `scf.rs` policy).
fn select_scf_loop(ctx: &mut Ctx<'_>, op: &MirOp, op_name: &str) -> Result<(), SelectError> {
    if op.regions.len() != 1 {
        return Err(SelectError::LoopWrongRegionCount {
            fn_name: ctx.src.name.clone(),
            op_name: op_name.to_string(),
            actual: op.regions.len(),
        });
    }
    let header = ctx.out.fresh_block();
    let body = ctx.out.fresh_block();
    let exit = ctx.out.fresh_block();
    // Entry → header (unconditional).
    ctx.out
        .set_terminator(ctx.current_block, X64Term::Jmp { target: header });
    // Header → body (unconditional at G1 ; iter-bounds gating is deferred).
    ctx.out
        .set_terminator(header, X64Term::Jmp { target: body });
    // Walk body.
    ctx.yield_target_stack.push(None); // yields inside loop bodies are no-ops
    let saved = ctx.current_block;
    ctx.current_block = body;
    walk_region(ctx, &op.regions[0])?;
    // Body back-edge → header. If the body terminated early (func.return),
    // its terminator is already set ; we only patch when still Unreachable.
    if matches!(
        ctx.out.blocks[body.0 as usize].terminator,
        X64Term::Unreachable
    ) {
        ctx.out
            .set_terminator(body, X64Term::Jmp { target: header });
    }
    ctx.yield_target_stack.pop();
    // Continue at exit (selection beyond the loop op resumes here).
    ctx.current_block = exit;
    let _ = saved;
    Ok(())
}

/// Select `scf.while` : header tests cond ; cond-true → body, cond-false → exit ;
/// body → header (back-edge).
fn select_scf_while(ctx: &mut Ctx<'_>, op: &MirOp) -> Result<(), SelectError> {
    if op.regions.len() != 1 {
        return Err(SelectError::LoopWrongRegionCount {
            fn_name: ctx.src.name.clone(),
            op_name: "while".to_string(),
            actual: op.regions.len(),
        });
    }
    if op.operands.len() != 1 {
        return Err(SelectError::OperandCountMismatch {
            fn_name: ctx.src.name.clone(),
            op: op.name.clone(),
            expected: 1,
            actual: op.operands.len(),
        });
    }
    let cond_id = op.operands[0];
    let cond = lookup_vreg(ctx, &op.name, cond_id)?;
    let header = ctx.out.fresh_block();
    let body = ctx.out.fresh_block();
    let exit = ctx.out.fresh_block();
    // Entry → header.
    ctx.out
        .set_terminator(ctx.current_block, X64Term::Jmp { target: header });
    // Header tests cond : Jcc(cond_ne_zero) body, exit.
    ctx.out.set_terminator(
        header,
        X64Term::Jcc {
            cond_kind: X64SetCondCode::Int(IntCmpKind::Ne),
            cond_vreg: cond,
            then_block: body,
            else_block: exit,
        },
    );
    // Walk body.
    ctx.yield_target_stack.push(None);
    let saved = ctx.current_block;
    ctx.current_block = body;
    walk_region(ctx, &op.regions[0])?;
    // Body back-edge → header (if not already terminated by a func.return).
    if matches!(
        ctx.out.blocks[body.0 as usize].terminator,
        X64Term::Unreachable
    ) {
        ctx.out
            .set_terminator(body, X64Term::Jmp { target: header });
    }
    ctx.yield_target_stack.pop();
    ctx.current_block = exit;
    let _ = saved;
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────
// § Tiny helpers — operand / result / attribute lookup with diagnostics.
// ────────────────────────────────────────────────────────────────────────

fn lookup_vreg(ctx: &Ctx<'_>, op_name: &str, vid: ValueId) -> Result<X64VReg, SelectError> {
    ctx.val_map
        .get(&vid)
        .copied()
        .ok_or_else(|| SelectError::UnknownValueId {
            fn_name: ctx.src.name.clone(),
            op: op_name.to_string(),
            value_id: vid.0,
        })
}

fn single_result(ctx: &Ctx<'_>, op: &MirOp) -> Result<cssl_mir::MirValue, SelectError> {
    if op.results.len() != 1 {
        return Err(SelectError::ResultCountMismatch {
            fn_name: ctx.src.name.clone(),
            op: op.name.clone(),
            expected: 1,
            actual: op.results.len(),
        });
    }
    Ok(op.results[0].clone())
}

fn single_operand(ctx: &Ctx<'_>, op: &MirOp) -> Result<ValueId, SelectError> {
    if op.operands.len() != 1 {
        return Err(SelectError::OperandCountMismatch {
            fn_name: ctx.src.name.clone(),
            op: op.name.clone(),
            expected: 1,
            actual: op.operands.len(),
        });
    }
    Ok(op.operands[0])
}

fn two_operands(ctx: &Ctx<'_>, op: &MirOp) -> Result<(ValueId, ValueId), SelectError> {
    if op.operands.len() != 2 {
        return Err(SelectError::OperandCountMismatch {
            fn_name: ctx.src.name.clone(),
            op: op.name.clone(),
            expected: 2,
            actual: op.operands.len(),
        });
    }
    Ok((op.operands[0], op.operands[1]))
}

fn result_width(ctx: &Ctx<'_>, op: &MirOp, ty: &MirType) -> Result<X64Width, SelectError> {
    mir_to_x64_width(ty).ok_or_else(|| SelectError::UnsupportedType {
        fn_name: ctx.src.name.clone(),
        op: op.name.clone(),
        ty: ty.to_string(),
    })
}

fn predicate<'a>(ctx: &Ctx<'_>, op: &'a MirOp) -> Result<&'a str, SelectError> {
    op.attributes
        .iter()
        .find(|(k, _)| k == "predicate")
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| SelectError::BadComparisonPredicate {
            fn_name: ctx.src.name.clone(),
            op: op.name.clone(),
            predicate: "<missing>".to_string(),
        })
}

fn parse_int_cmp(s: &str) -> Option<IntCmpKind> {
    Some(match s {
        "eq" => IntCmpKind::Eq,
        "ne" => IntCmpKind::Ne,
        "slt" => IntCmpKind::Slt,
        "sle" => IntCmpKind::Sle,
        "sgt" => IntCmpKind::Sgt,
        "sge" => IntCmpKind::Sge,
        "ult" => IntCmpKind::Ult,
        "ule" => IntCmpKind::Ule,
        "ugt" => IntCmpKind::Ugt,
        "uge" => IntCmpKind::Uge,
        _ => return None,
    })
}

fn parse_fp_cmp(s: &str) -> Option<FpCmpKind> {
    Some(match s {
        "oeq" | "eq" => FpCmpKind::Oeq,
        "one" => FpCmpKind::One,
        "olt" | "lt" => FpCmpKind::Olt,
        "ole" | "le" => FpCmpKind::Ole,
        "ogt" | "gt" => FpCmpKind::Ogt,
        "oge" | "ge" => FpCmpKind::Oge,
        "une" | "ne" => FpCmpKind::Une,
        "ult" => FpCmpKind::Ult,
        "ule" => FpCmpKind::Ule,
        "ugt" => FpCmpKind::Ugt,
        "uge" => FpCmpKind::Uge,
        "ord" => FpCmpKind::Ord,
        "uno" => FpCmpKind::Uno,
        _ => return None,
    })
}

// ════════════════════════════════════════════════════════════════════════
// § Tests.
// ════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::{select_function, select_module, SelectError};
    use crate::isel::display::format_func;
    use cssl_mir::{
        validate_and_mark, FloatWidth, IntWidth, MirFunc, MirModule, MirOp, MirRegion, MirType,
        MirValue, ValueId,
    };

    fn i32_ty() -> MirType {
        MirType::Int(IntWidth::I32)
    }
    fn i64_ty() -> MirType {
        MirType::Int(IntWidth::I64)
    }
    fn f32_ty() -> MirType {
        MirType::Float(FloatWidth::F32)
    }
    fn ptr_ty() -> MirType {
        MirType::Ptr
    }

    /// Wrap a single fn in a fresh marked module — D5 marker is required.
    fn marked_module(f: MirFunc) -> MirModule {
        let mut m = MirModule::with_name("test");
        m.push_func(f);
        validate_and_mark(&mut m).expect("test fixtures must be D5-validatable");
        m
    }

    /// Wrap a single fn + write the D5 marker MANUALLY without running the
    /// validator. Used for selector tests that need to verify the per-op
    /// reject paths (`cf.br`, `cssl.unsupported.Break`, malformed scf.* shapes)
    /// fire as defense-in-depth even when the marker is bypassed. In real use
    /// these shapes never reach the selector because D5 catches them first.
    fn marker_only_module(f: MirFunc) -> MirModule {
        let mut m = MirModule::with_name("test");
        m.push_func(f);
        m.attributes.push((
            cssl_mir::STRUCTURED_CFG_VALIDATED_KEY.to_string(),
            cssl_mir::STRUCTURED_CFG_VALIDATED_VALUE.to_string(),
        ));
        m
    }

    fn add_i32_fn() -> MirFunc {
        let mut f = MirFunc::new("add", vec![i32_ty(), i32_ty()], vec![i32_ty()]);
        f.next_value_id = 2;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), i32_ty()),
            MirValue::new(ValueId(1), i32_ty()),
        ];
        entry.ops.push(
            MirOp::std("arith.addi")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), i32_ty()),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(2)));
        f
    }

    // ─── D5 marker fanout-contract ────────────────────────────────────

    #[test]
    fn missing_d5_marker_is_rejected() {
        let mut m = MirModule::with_name("test");
        m.push_func(add_i32_fn());
        // Don't call validate_and_mark — selector should reject.
        let err = select_function(&m, &m.funcs[0]).unwrap_err();
        assert_eq!(err, SelectError::StructuredCfgMarkerMissing);
        assert_eq!(err.code(), "X64-D5");
    }

    #[test]
    fn marker_present_allows_selection() {
        let m = marked_module(add_i32_fn());
        let f = select_function(&m, &m.funcs[0]).expect("D5 marker present");
        assert_eq!(f.name, "add");
        assert_eq!(f.sig.params.len(), 2);
        assert_eq!(f.sig.results.len(), 1);
    }

    #[test]
    fn select_module_walks_all_fns() {
        let mut m = MirModule::with_name("test");
        m.push_func(add_i32_fn());
        m.push_func({
            let mut f = MirFunc::new("answer", vec![], vec![i32_ty()]);
            f.next_value_id = 1;
            let entry = f.body.entry_mut().unwrap();
            entry.ops.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(0), i32_ty())
                    .with_attribute("value", "42"),
            );
            entry
                .ops
                .push(MirOp::std("func.return").with_operand(ValueId(0)));
            f
        });
        validate_and_mark(&mut m).expect("module passes D5");
        let funcs = select_module(&m).unwrap();
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "add");
        assert_eq!(funcs[1].name, "answer");
    }

    #[test]
    fn select_module_rejects_when_marker_missing() {
        let mut m = MirModule::with_name("test");
        m.push_func(add_i32_fn());
        let err = select_module(&m).unwrap_err();
        assert_eq!(err, SelectError::StructuredCfgMarkerMissing);
    }

    // ─── arith.constant ───────────────────────────────────────────────

    #[test]
    fn constant_i32_lowers_to_movimm() {
        let mut f = MirFunc::new("answer", vec![], vec![i32_ty()]);
        f.next_value_id = 1;
        let entry = f.body.entry_mut().unwrap();
        entry.ops.push(
            MirOp::std("arith.constant")
                .with_result(ValueId(0), i32_ty())
                .with_attribute("value", "42"),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(0)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        assert!(s.contains("mov.imm 42i32"), "got: {s}");
        assert!(s.contains("ret v1:i32"), "got: {s}");
    }

    #[test]
    fn constant_f32_lowers_to_movimm_with_bit_pattern() {
        let mut f = MirFunc::new("pi", vec![], vec![f32_ty()]);
        f.next_value_id = 1;
        let entry = f.body.entry_mut().unwrap();
        entry.ops.push(
            MirOp::std("arith.constant")
                .with_result(ValueId(0), f32_ty())
                .with_attribute("value", "3.14"),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(0)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        assert!(s.contains("mov.imm f32:0x"));
    }

    #[test]
    fn constant_missing_value_attr_errors() {
        let mut f = MirFunc::new("bad", vec![], vec![i32_ty()]);
        f.next_value_id = 1;
        let entry = f.body.entry_mut().unwrap();
        entry
            .ops
            .push(MirOp::std("arith.constant").with_result(ValueId(0), i32_ty()));
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(0)));
        let m = marked_module(f);
        let err = select_function(&m, &m.funcs[0]).unwrap_err();
        assert!(matches!(err, SelectError::ConstantMissingValue { .. }));
        assert_eq!(err.code(), "X64-0005");
    }

    // ─── Integer arithmetic ───────────────────────────────────────────

    #[test]
    fn add_i32_round_trip_text_form() {
        let m = marked_module(add_i32_fn());
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        // Expected shape : Mov dst, p0 ; Add dst, p1 ; ret dst.
        assert!(s.contains("v3:i32 <- mov v1:i32"));
        assert!(s.contains("v3:i32 <- add v3:i32, v2:i32"));
        assert!(s.contains("ret v3:i32"));
    }

    #[test]
    fn sub_and_mul_i32_select_canonical() {
        let mut f = MirFunc::new("smul", vec![i32_ty(), i32_ty()], vec![i32_ty()]);
        f.next_value_id = 2;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), i32_ty()),
            MirValue::new(ValueId(1), i32_ty()),
        ];
        entry.ops.push(
            MirOp::std("arith.subi")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), i32_ty()),
        );
        entry.ops.push(
            MirOp::std("arith.muli")
                .with_operand(ValueId(2))
                .with_operand(ValueId(0))
                .with_result(ValueId(3), i32_ty()),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(3)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        assert!(s.contains("<- sub"));
        assert!(s.contains("<- imul"));
    }

    #[test]
    fn signed_div_i32_emits_cdq_then_idiv() {
        let mut f = MirFunc::new("sdiv", vec![i32_ty(), i32_ty()], vec![i32_ty()]);
        f.next_value_id = 2;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), i32_ty()),
            MirValue::new(ValueId(1), i32_ty()),
        ];
        entry.ops.push(
            MirOp::std("arith.sdivi")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), i32_ty()),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(2)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        // Per slice handoff landmines : signed div MUST emit cdq before idiv.
        let cdq_pos = s.find("cdq").expect("cdq must precede idiv");
        let idiv_pos = s.find("idiv").expect("idiv must follow cdq");
        assert!(cdq_pos < idiv_pos, "cdq must come before idiv");
    }

    #[test]
    fn signed_div_i64_emits_cqo_then_idiv() {
        let mut f = MirFunc::new("sdiv64", vec![i64_ty(), i64_ty()], vec![i64_ty()]);
        f.next_value_id = 2;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), i64_ty()),
            MirValue::new(ValueId(1), i64_ty()),
        ];
        entry.ops.push(
            MirOp::std("arith.sdivi")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), i64_ty()),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(2)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        let cqo_pos = s.find("cqo").expect("cqo for i64 div");
        let idiv_pos = s.find("idiv").expect("idiv after cqo");
        assert!(cqo_pos < idiv_pos);
    }

    #[test]
    fn unsigned_div_emits_xor_rdx_then_div() {
        let mut f = MirFunc::new("udiv", vec![i32_ty(), i32_ty()], vec![i32_ty()]);
        f.next_value_id = 2;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), i32_ty()),
            MirValue::new(ValueId(1), i32_ty()),
        ];
        entry.ops.push(
            MirOp::std("arith.udivi")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), i32_ty()),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(2)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        // Print on failure so we can see the actual shape.
        let xor_pos = s
            .find("xor.rdx")
            .unwrap_or_else(|| panic!("expected `xor.rdx` for unsigned div ; got:\n{s}"));
        // ‼ Look for `div v` (followed by a vreg id) to disambiguate from
        //   any other token containing the substring "div" — e.g. "idiv"
        //   or future addr-mode literals.
        let div_pos = s
            .find("div v")
            .unwrap_or_else(|| panic!("expected `div v...` after xor.rdx ; got:\n{s}"));
        assert!(
            xor_pos < div_pos,
            "xor.rdx must come before div ; got:\n{s}"
        );
    }

    // ─── Float arithmetic ─────────────────────────────────────────────

    #[test]
    fn fadd_f32_lowers_to_fpadd() {
        let mut f = MirFunc::new("fadd", vec![f32_ty(), f32_ty()], vec![f32_ty()]);
        f.next_value_id = 2;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), f32_ty()),
            MirValue::new(ValueId(1), f32_ty()),
        ];
        entry.ops.push(
            MirOp::std("arith.addf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), f32_ty()),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(2)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        assert!(s.contains("<- fadd"));
        assert!(s.contains("ret v3:f32"));
    }

    #[test]
    fn fneg_f32_lowers_to_fpneg() {
        let mut f = MirFunc::new("fneg_f", vec![f32_ty()], vec![f32_ty()]);
        f.next_value_id = 1;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![MirValue::new(ValueId(0), f32_ty())];
        entry.ops.push(
            MirOp::std("arith.negf")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), f32_ty()),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(1)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        assert!(s.contains("<- fneg.f32"));
    }

    // ─── Comparisons + select ────────────────────────────────────────

    #[test]
    fn cmpi_slt_lowers_to_cmp_setl() {
        let mut f = MirFunc::new("lt", vec![i32_ty(), i32_ty()], vec![MirType::Bool]);
        f.next_value_id = 2;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), i32_ty()),
            MirValue::new(ValueId(1), i32_ty()),
        ];
        entry.ops.push(
            MirOp::std("arith.cmpi")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Bool)
                .with_attribute("predicate", "slt"),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(2)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        assert!(s.contains("cmp v1:i32, v2:i32"));
        assert!(s.contains("<- setl"));
    }

    #[test]
    fn cmpf_ole_uses_ucomi() {
        let mut f = MirFunc::new("le", vec![f32_ty(), f32_ty()], vec![MirType::Bool]);
        f.next_value_id = 2;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), f32_ty()),
            MirValue::new(ValueId(1), f32_ty()),
        ];
        entry.ops.push(
            MirOp::std("arith.cmpf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Bool)
                .with_attribute("predicate", "ole"),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(2)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        // Per slice handoff landmines : ordered (`o*`) predicates use ucomi.
        assert!(
            s.contains("ucomi"),
            "ordered predicate should use ucomi : {s}"
        );
    }

    #[test]
    fn cmpf_ult_uses_comi() {
        let mut f = MirFunc::new("ult", vec![f32_ty(), f32_ty()], vec![MirType::Bool]);
        f.next_value_id = 2;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), f32_ty()),
            MirValue::new(ValueId(1), f32_ty()),
        ];
        entry.ops.push(
            MirOp::std("arith.cmpf")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Bool)
                .with_attribute("predicate", "ult"),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(2)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        // Per slice handoff landmines : unordered `u*` predicate uses comi (signaling).
        assert!(s.contains("comi"));
        assert!(!s.contains("ucomi"));
    }

    #[test]
    fn select_emits_high_level_select_inst() {
        // arith.select cond, t, f → one X64Inst::Select.
        let mut f = MirFunc::new(
            "sel",
            vec![MirType::Bool, i32_ty(), i32_ty()],
            vec![i32_ty()],
        );
        f.next_value_id = 3;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), MirType::Bool),
            MirValue::new(ValueId(1), i32_ty()),
            MirValue::new(ValueId(2), i32_ty()),
        ];
        entry.ops.push(
            MirOp::std("arith.select")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_operand(ValueId(2))
                .with_result(ValueId(3), i32_ty()),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(3)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        assert!(s.contains("<- select"));
    }

    #[test]
    fn cmpi_bad_predicate_errors() {
        let mut f = MirFunc::new("bad", vec![i32_ty(), i32_ty()], vec![MirType::Bool]);
        f.next_value_id = 2;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), i32_ty()),
            MirValue::new(ValueId(1), i32_ty()),
        ];
        entry.ops.push(
            MirOp::std("arith.cmpi")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), MirType::Bool)
                .with_attribute("predicate", "bogus"),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(2)));
        let m = marked_module(f);
        let err = select_function(&m, &m.funcs[0]).unwrap_err();
        assert!(matches!(err, SelectError::BadComparisonPredicate { .. }));
        assert_eq!(err.code(), "X64-0006");
    }

    // ─── memref.load / memref.store ──────────────────────────────────

    #[test]
    fn memref_load_produces_load_inst() {
        let mut f = MirFunc::new("ld", vec![ptr_ty()], vec![i32_ty()]);
        f.next_value_id = 1;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![MirValue::new(ValueId(0), ptr_ty())];
        entry.ops.push(
            MirOp::std("memref.load")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), i32_ty()),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(1)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        assert!(s.contains("<- load [v1:ptr]"));
    }

    #[test]
    fn memref_store_3operand_uses_indexed_addr() {
        // store val, ptr, offset
        let mut f = MirFunc::new("st", vec![i32_ty(), ptr_ty(), i64_ty()], vec![]);
        f.next_value_id = 3;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), i32_ty()),
            MirValue::new(ValueId(1), ptr_ty()),
            MirValue::new(ValueId(2), i64_ty()),
        ];
        entry.ops.push(
            MirOp::std("memref.store")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_operand(ValueId(2)),
        );
        entry.ops.push(MirOp::std("func.return"));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        assert!(
            s.contains("store [v2:ptr + v3:i64 * 1], v1:i32"),
            "got: {s}"
        );
    }

    // ─── func.return / multi-return ──────────────────────────────────

    #[test]
    fn empty_void_fn_auto_terminates_with_ret() {
        let f = MirFunc::new("noop", vec![], vec![]);
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        assert!(s.contains("ret"));
    }

    // ─── func.call ─────────────────────────────────────────────────────

    #[test]
    fn func_call_emits_abstract_call() {
        let mut f = MirFunc::new("caller", vec![i32_ty()], vec![i32_ty()]);
        f.next_value_id = 1;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![MirValue::new(ValueId(0), i32_ty())];
        entry.ops.push(
            MirOp::std("func.call")
                .with_operand(ValueId(0))
                .with_result(ValueId(1), i32_ty())
                .with_attribute("callee", "double"),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(1)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        assert!(s.contains("<- call double(v1:i32)"));
    }

    // ─── Heap FFI ───────────────────────────────────────────────────────

    #[test]
    fn heap_alloc_lowers_to_cssl_alloc_call() {
        let mut f = MirFunc::new("alloc1", vec![i64_ty(), i64_ty()], vec![ptr_ty()]);
        f.next_value_id = 2;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), i64_ty()),
            MirValue::new(ValueId(1), i64_ty()),
        ];
        entry.ops.push(
            MirOp::std("cssl.heap.alloc")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_result(ValueId(2), ptr_ty()),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(2)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        assert!(s.contains("call __cssl_alloc"));
        assert!(s.contains("ret v3:ptr"));
    }

    #[test]
    fn heap_dealloc_lowers_to_cssl_free_call() {
        let mut f = MirFunc::new("dealloc1", vec![ptr_ty(), i64_ty(), i64_ty()], vec![]);
        f.next_value_id = 3;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), ptr_ty()),
            MirValue::new(ValueId(1), i64_ty()),
            MirValue::new(ValueId(2), i64_ty()),
        ];
        entry.ops.push(
            MirOp::std("cssl.heap.dealloc")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_operand(ValueId(2)),
        );
        entry.ops.push(MirOp::std("func.return"));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        assert!(s.contains("call __cssl_free"));
    }

    #[test]
    fn heap_realloc_lowers_to_cssl_realloc_call() {
        let mut f = MirFunc::new(
            "realloc1",
            vec![ptr_ty(), i64_ty(), i64_ty(), i64_ty()],
            vec![ptr_ty()],
        );
        f.next_value_id = 4;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), ptr_ty()),
            MirValue::new(ValueId(1), i64_ty()),
            MirValue::new(ValueId(2), i64_ty()),
            MirValue::new(ValueId(3), i64_ty()),
        ];
        entry.ops.push(
            MirOp::std("cssl.heap.realloc")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_operand(ValueId(2))
                .with_operand(ValueId(3))
                .with_result(ValueId(4), ptr_ty()),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(4)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        assert!(s.contains("call __cssl_realloc"));
    }

    // ─── scf.if ────────────────────────────────────────────────────────

    #[test]
    fn scf_if_creates_three_blocks_then_else_merge() {
        // fn cond(b: i1, t: i32, f: i32) -> i32 { if b { t } else { f } }
        let mut f = MirFunc::new(
            "branch",
            vec![MirType::Bool, i32_ty(), i32_ty()],
            vec![i32_ty()],
        );
        f.next_value_id = 3;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![
            MirValue::new(ValueId(0), MirType::Bool),
            MirValue::new(ValueId(1), i32_ty()),
            MirValue::new(ValueId(2), i32_ty()),
        ];
        // then-region : yield v1
        let then_region = MirRegion::with_entry(vec![]);
        let mut then_region_filled = then_region;
        then_region_filled
            .blocks
            .first_mut()
            .unwrap()
            .ops
            .push(MirOp::std("scf.yield").with_operand(ValueId(1)));
        // else-region : yield v2
        let mut else_region_filled = MirRegion::with_entry(vec![]);
        else_region_filled
            .blocks
            .first_mut()
            .unwrap()
            .ops
            .push(MirOp::std("scf.yield").with_operand(ValueId(2)));
        entry.ops.push(
            MirOp::std("scf.if")
                .with_operand(ValueId(0))
                .with_result(ValueId(3), i32_ty())
                .with_region(then_region_filled)
                .with_region(else_region_filled),
        );
        entry
            .ops
            .push(MirOp::std("func.return").with_operand(ValueId(3)));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        // Should have at least 4 blocks : entry + then + else + merge.
        assert!(xf.blocks.len() >= 4);
        let s = format_func(&xf);
        assert!(s.contains("jcc"));
        assert!(s.contains("ret"));
    }

    #[test]
    fn scf_if_with_wrong_region_count_errors() {
        // ‼ This shape is rejected by D5 (CFG0005) ; we use marker_only_module
        // to bypass the D5 walk and verify the selector's defense-in-depth
        // reject path also fires.
        let mut f = MirFunc::new("bad_if", vec![MirType::Bool], vec![]);
        f.next_value_id = 1;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![MirValue::new(ValueId(0), MirType::Bool)];
        // scf.if with only one region (should be 2).
        entry.ops.push(
            MirOp::std("scf.if")
                .with_operand(ValueId(0))
                .with_region(MirRegion::with_entry(vec![])),
        );
        entry.ops.push(MirOp::std("func.return"));
        let m = marker_only_module(f);
        let err = select_function(&m, &m.funcs[0]).unwrap_err();
        assert!(matches!(err, SelectError::ScfIfWrongRegionCount { .. }));
        assert_eq!(err.code(), "X64-0009");
    }

    // ─── scf loops ─────────────────────────────────────────────────────

    #[test]
    fn scf_loop_creates_header_body_exit() {
        let mut f = MirFunc::new("forever", vec![], vec![]);
        f.next_value_id = 0;
        let entry = f.body.entry_mut().unwrap();
        let body_region = MirRegion::with_entry(vec![]);
        entry
            .ops
            .push(MirOp::std("scf.loop").with_region(body_region));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        // Entry + header + body + exit = 4 blocks.
        assert!(xf.blocks.len() >= 4);
    }

    #[test]
    fn scf_for_creates_header_body_exit() {
        let mut f = MirFunc::new("for1", vec![], vec![]);
        let body_region = MirRegion::with_entry(vec![]);
        let entry = f.body.entry_mut().unwrap();
        entry
            .ops
            .push(MirOp::std("scf.for").with_region(body_region));
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        assert!(xf.blocks.len() >= 4);
    }

    #[test]
    fn scf_while_emits_jcc_at_header() {
        // fn loop_until(cond: bool) { while cond { } }
        let mut f = MirFunc::new("wh", vec![MirType::Bool], vec![]);
        f.next_value_id = 1;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![MirValue::new(ValueId(0), MirType::Bool)];
        let body_region = MirRegion::with_entry(vec![]);
        entry.ops.push(
            MirOp::std("scf.while")
                .with_operand(ValueId(0))
                .with_region(body_region),
        );
        let m = marked_module(f);
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        // Header should have a jcc terminator gating body vs exit.
        assert!(s.contains("jcc"));
    }

    #[test]
    fn scf_loop_wrong_region_count_errors() {
        // ‼ Bypass D5 (CFG0006) — verify selector's defense-in-depth.
        let mut f = MirFunc::new("badloop", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        // No regions.
        entry.ops.push(MirOp::std("scf.loop"));
        let m = marker_only_module(f);
        let err = select_function(&m, &m.funcs[0]).unwrap_err();
        assert!(matches!(err, SelectError::LoopWrongRegionCount { .. }));
        assert_eq!(err.code(), "X64-0010");
    }

    // ─── Closures + cf.* + break/continue rejects ────────────────────

    #[test]
    fn closure_op_is_rejected_with_x64_0013() {
        let mut f = MirFunc::new("clos", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        entry.ops.push(MirOp::std("cssl.closure.create"));
        let m = marked_module(f);
        let err = select_function(&m, &m.funcs[0]).unwrap_err();
        assert!(matches!(err, SelectError::ClosureRejected { .. }));
        assert_eq!(err.code(), "X64-0013");
    }

    #[test]
    fn cf_br_is_rejected_with_x64_0012() {
        // ‼ D5 catches `cf.br` (CFG0004) ; bypass via marker_only_module so we
        // verify the selector's defense-in-depth path also fires. Mirrors
        // WGSL's `unstructured_op_after_marker_bypass` test pattern.
        let mut f = MirFunc::new("br", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        entry.ops.push(MirOp::std("cf.br"));
        let m = marker_only_module(f);
        let err = select_function(&m, &m.funcs[0]).unwrap_err();
        assert!(matches!(err, SelectError::UnstructuredOp { .. }));
        assert_eq!(err.code(), "X64-0012");
    }

    #[test]
    fn cf_cond_br_is_rejected_with_x64_0012() {
        let mut f = MirFunc::new("br2", vec![MirType::Bool], vec![]);
        f.next_value_id = 1;
        let entry = f.body.entry_mut().unwrap();
        entry.args = vec![MirValue::new(ValueId(0), MirType::Bool)];
        entry
            .ops
            .push(MirOp::std("cf.cond_br").with_operand(ValueId(0)));
        let m = marker_only_module(f);
        let err = select_function(&m, &m.funcs[0]).unwrap_err();
        assert!(matches!(err, SelectError::UnstructuredOp { .. }));
    }

    #[test]
    fn break_placeholder_is_rejected_with_x64_0014() {
        let mut f = MirFunc::new("br_h", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        entry.ops.push(MirOp::std("cssl.unsupported.Break"));
        let m = marked_module(f);
        let err = select_function(&m, &m.funcs[0]).unwrap_err();
        assert!(matches!(err, SelectError::UnsupportedBreakContinue { .. }));
        assert_eq!(err.code(), "X64-0014");
    }

    // ─── Unsupported op ───────────────────────────────────────────────

    #[test]
    fn unknown_op_is_rejected_with_x64_0015() {
        let mut f = MirFunc::new("weird", vec![], vec![]);
        let entry = f.body.entry_mut().unwrap();
        entry.ops.push(MirOp::std("cssl.mystery"));
        let m = marked_module(f);
        let err = select_function(&m, &m.funcs[0]).unwrap_err();
        assert!(matches!(err, SelectError::UnsupportedOp { .. }));
        assert_eq!(err.code(), "X64-0015");
    }

    // ─── Round-trip text-form for the canonical example ──────────────

    #[test]
    fn add_i32_round_trip_full_text_form() {
        // The canonical `fn add(a: i32, b: i32) -> i32 { a + b }` example.
        let m = marked_module(add_i32_fn());
        let xf = select_function(&m, &m.funcs[0]).unwrap();
        let s = format_func(&xf);
        // Verify the structure : signature line + bb0 label + 3 ops + ret.
        assert!(s.starts_with("fn add (i32, i32) -> i32 {"));
        assert!(s.contains("bb0:"));
        assert!(s.ends_with("}\n"));
    }

    // ─── Stable diagnostic codes are unique ──────────────────────────

    #[test]
    fn select_error_codes_are_unique() {
        // Build one of every error variant via dummy fields so we can
        // collect their codes.
        let codes = [
            SelectError::StructuredCfgMarkerMissing.code(),
            SelectError::UnsupportedSignatureType {
                fn_name: String::new(),
                ty: String::new(),
            }
            .code(),
            SelectError::UnsupportedType {
                fn_name: String::new(),
                op: String::new(),
                ty: String::new(),
            }
            .code(),
            SelectError::EmptyBody {
                fn_name: String::new(),
            }
            .code(),
            SelectError::UnknownValueId {
                fn_name: String::new(),
                op: String::new(),
                value_id: 0,
            }
            .code(),
            SelectError::ConstantMissingValue {
                fn_name: String::new(),
            }
            .code(),
            SelectError::BadComparisonPredicate {
                fn_name: String::new(),
                op: String::new(),
                predicate: String::new(),
            }
            .code(),
            SelectError::OperandCountMismatch {
                fn_name: String::new(),
                op: String::new(),
                expected: 0,
                actual: 0,
            }
            .code(),
            SelectError::ResultCountMismatch {
                fn_name: String::new(),
                op: String::new(),
                expected: 0,
                actual: 0,
            }
            .code(),
            SelectError::ScfIfWrongRegionCount {
                fn_name: String::new(),
                actual: 0,
            }
            .code(),
            SelectError::LoopWrongRegionCount {
                fn_name: String::new(),
                op_name: String::new(),
                actual: 0,
            }
            .code(),
            SelectError::ScfRegionMultiBlock {
                fn_name: String::new(),
                op_name: String::new(),
                block_count: 0,
            }
            .code(),
            SelectError::UnstructuredOp {
                fn_name: String::new(),
                op: String::new(),
            }
            .code(),
            SelectError::ClosureRejected {
                fn_name: String::new(),
                op: String::new(),
            }
            .code(),
            SelectError::UnsupportedBreakContinue {
                fn_name: String::new(),
                op: String::new(),
            }
            .code(),
            SelectError::UnsupportedOp {
                fn_name: String::new(),
                op: String::new(),
            }
            .code(),
        ];
        let mut sorted: Vec<&'static str> = codes.to_vec();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), before, "stable codes must be unique");
    }
}
