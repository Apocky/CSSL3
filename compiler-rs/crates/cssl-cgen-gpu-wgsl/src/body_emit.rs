//! § body_emit — MIR-op → WGSL statement-list emitter (S6-D4 / T11-D75).
//!
//! § ROLE
//!   Walks a `MirFunc` body region and produces the WGSL statements that go
//!   inside the entry-point fn (or helper fn). Operates as a pure
//!   functional translator : MIR-op-stream in, `Vec<String>` of WGSL source
//!   lines out. Diagnostics surface as [`BodyEmitError`] variants ; no
//!   panics on user-facing input.
//!
//! § CONTRACT — D5 / T11-D70 fanout marker
//!   The structured-CFG validator ([`cssl_mir::validate_and_mark`]) MUST run
//!   before this emitter sees a module. Per the D5 marker contract, the
//!   `emit_wgsl` orchestrator checks `cssl_mir::has_structured_cfg_marker`
//!   and rejects an unmarked module with `WgslError::StructuredCfgMarkerMissing`.
//!   This emitter trusts the invariant : any `cf.br` / `cf.cond_br` / orphan
//!   `scf.yield` is reachable here only if the validator was bypassed —
//!   diagnostic-codes `WGSL0010..` surface those as emitter errors (defense
//!   in depth) rather than producing malformed WGSL that naga would reject.
//!
//! § MAPPING TABLE  (MIR → WGSL)
//!   The mapping table mirrors the cranelift-side helpers in
//!   `cssl-cgen-cpu-cranelift::scf` + `cssl-cgen-cpu-cranelift::jit` ; same
//!   shape, different target language. Specifically :
//!
//!   - `arith.constant`            → `let v<id> : <ty> = <value>;`
//!   - `arith.{addi,subi,muli,divsi,remsi}` → integer infix on `i32`
//!   - `arith.{addf,subf,mulf,divf,remf}`   → float infix on `f32`
//!   - `arith.{andi,ori,xori,shli,shrsi}`   → bitwise / shift on `i32`
//!   - `arith.cmpi_{eq,ne,slt,sle,sgt,sge}` → `let v : bool = a OP b;`
//!   - `arith.cmpf_{oeq,one,olt,ole,ogt,oge}` → float compare → bool
//!   - `arith.{negf, negi}`        → unary minus
//!   - `arith.bitcast`             → `bitcast<T>(v)` (rejected if dest type
//!                                   not yet inferable)
//!   - `scf.if  cond then else`    → `if (cond) { then-block } else { else-block }`,
//!                                   yielded value lifted to a pre-declared
//!                                   `var<function>` so both branches assign
//!                                   into it (WGSL's structured-CFG shape)
//!   - `scf.for / scf.while / scf.loop` → emitted as `loop { ... break-on-cond ... }`
//!                                       structured-CFG bodies. Stage-0
//!                                       lowering matches the cranelift
//!                                       brif-shape so scf-body invariants
//!                                       are identical.
//!   - `scf.yield`                 → assignment into the parent's yield-var
//!                                   (handled at the parent level — orphan
//!                                   yields are emitter-rejected even though
//!                                   D5 should already have caught them).
//!   - `memref.load  buf idx`      → `buf\[idx\]` load
//!   - `memref.store v buf idx`    → `buf\[idx\] = v` store
//!   - `func.return` / void-tail   → `return ...;`  /  no-op for entry fns
//!   - `cssl.heap.{alloc,dealloc,realloc}`   → REJECT (HeapNotSupported)
//!   - `cssl.closure*`             → REJECT (ClosureNotSupported)
//!
//! § TYPE LAW
//!   WGSL's type system is strict : `i32` and `u32` are distinct, no
//!   implicit numeric conversion. We map :
//!     `MirType::Int(IntWidth::I1)`  → `bool`
//!     `MirType::Int(IntWidth::I8 / I16 / I32 / Index)`  → `i32`
//!     `MirType::Int(IntWidth::I64)` → `i32`  (stage-0 narrowing — WGSL has
//!                                              no native i64 ; future slices
//!                                              add `enable f16;`-style
//!                                              extension or struct-pair
//!                                              emulation)
//!     `MirType::Float(FloatWidth::F32 / F16 / Bf16)` → `f32`  /  `f16`  /  emu
//!     `MirType::Float(FloatWidth::F64)` → `f32`  (WGSL has no f64)
//!     `MirType::Bool`               → `bool`
//!     `MirType::Vec(N, F)`          → `vec<N, fN>`
//!     `MirType::None`               → `()` (statement-only)
//!     `MirType::Memref / Ptr / Handle / Tuple / Function / Opaque` → REJECT
//!
//!   Stage-0 narrowing (i64 → i32, f64 → f32) matches the existing
//!   cranelift-lowering policy at `body_lower::lower_path`'s opaque
//!   placeholders ; future slices add 32-bit-pair emulation or push the
//!   user toward `i32`-only sources. Per the slice handoff landmines the
//!   pre-existing AD/cmp tests should not change semantics — those tests
//!   exercise i32 + f32 paths exclusively.

use core::fmt::Write as _;

use cssl_mir::{FloatWidth, IntWidth, MirFunc, MirOp, MirRegion, MirType, ValueId};
use thiserror::Error;

/// Failure modes for body emission. Stable diagnostic codes carried in each
/// `Display` rendering ; emitter callers may pattern-match the variant for
/// programmatic recovery.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BodyEmitError {
    /// **WGSL0001** — heap-alloc op encountered (heap not supported on GPU).
    #[error(
        "WGSL0001: fn `{fn_name}` contains heap-op `{op}` ; heap allocation is unsupported on \
         GPU shader paths (per slice handoff landmines)"
    )]
    HeapOpRejected { fn_name: String, op: String },

    /// **WGSL0002** — closure op encountered.
    #[error(
        "WGSL0002: fn `{fn_name}` contains closure op `{op}` ; closures unsupported on GPU \
         shader paths (per slice handoff landmines)"
    )]
    ClosureOpRejected { fn_name: String, op: String },

    /// **WGSL0003** — unsupported MIR type encountered.
    #[error("WGSL0003: fn `{fn_name}` value `%{value_id}` has unsupported WGSL type `{ty}`")]
    UnsupportedType {
        fn_name: String,
        value_id: u32,
        ty: String,
    },

    /// **WGSL0004** — `arith.constant` op without a `value` attribute.
    #[error("WGSL0004: fn `{fn_name}` arith.constant op missing required `value` attribute")]
    ConstantMissingValueAttr { fn_name: String },

    /// **WGSL0005** — operand count mismatch for a known op shape.
    #[error("WGSL0005: fn `{fn_name}` op `{op}` expects {expected} operand(s) ; got {actual}")]
    OperandCountMismatch {
        fn_name: String,
        op: String,
        expected: usize,
        actual: usize,
    },

    /// **WGSL0006** — bare `scf.yield` outside a structured parent. This is
    /// a defense-in-depth check : D5 should have rejected it already.
    #[error(
        "WGSL0006: fn `{fn_name}` has orphan `scf.yield` reachable in body emission \
         (D5 marker bypassed ?)"
    )]
    OrphanScfYield { fn_name: String },

    /// **WGSL0007** — unstructured `cf.br` / `cf.cond_br`. Defense in depth
    /// against D5 bypass.
    #[error(
        "WGSL0007: fn `{fn_name}` contains unstructured op `{op}` ; \
         the structured-CFG validator (D5) must run before WGSL emission"
    )]
    UnstructuredOp { fn_name: String, op: String },

    /// **WGSL0008** — unrecognized op-name we don't yet have a lowering for.
    /// Distinct from heap/closure/cf-br rejections so callers can grep.
    #[error("WGSL0008: fn `{fn_name}` contains unsupported op `{op}` (no WGSL lowering)")]
    UnsupportedOp { fn_name: String, op: String },

    /// **WGSL0009** — `scf.if` with the wrong region count. D5 should have
    /// rejected this ; mirrored here so the emitter never produces ill-formed
    /// WGSL on a malformed module.
    #[error("WGSL0009: fn `{fn_name}` `scf.if` has {actual} regions ; expected exactly 2")]
    ScfIfWrongRegions { fn_name: String, actual: usize },

    /// **WGSL0010** — loop-shape (`scf.for / scf.while / scf.loop`) with the
    /// wrong region count.
    #[error(
        "WGSL0010: fn `{fn_name}` `scf.{loop_form}` has {actual} regions ; expected exactly 1"
    )]
    LoopWrongRegions {
        fn_name: String,
        loop_form: String,
        actual: usize,
    },
}

impl BodyEmitError {
    /// Stable diagnostic code (e.g. `"WGSL0001"`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::HeapOpRejected { .. } => "WGSL0001",
            Self::ClosureOpRejected { .. } => "WGSL0002",
            Self::UnsupportedType { .. } => "WGSL0003",
            Self::ConstantMissingValueAttr { .. } => "WGSL0004",
            Self::OperandCountMismatch { .. } => "WGSL0005",
            Self::OrphanScfYield { .. } => "WGSL0006",
            Self::UnstructuredOp { .. } => "WGSL0007",
            Self::UnsupportedOp { .. } => "WGSL0008",
            Self::ScfIfWrongRegions { .. } => "WGSL0009",
            Self::LoopWrongRegions { .. } => "WGSL0010",
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Per-fn lowering entry-point.
// ───────────────────────────────────────────────────────────────────────

/// Lower the body of `f` into a WGSL statement list ready to drop into the
/// `body: Vec<String>` field of [`crate::wgsl::WgslStatement::EntryFunction`]
/// or `HelperFunction`.
///
/// `entry_point_void_return` controls whether a trailing `return;` is
/// elided : compute-stage entry-points return void, so we emit no explicit
/// terminator ; vertex/fragment + helper fns return a value, so we emit
/// `return <last-value>;` when one is reachable.
///
/// # Errors
/// Returns the first [`BodyEmitError`] encountered during the walk. Errors
/// are emitted eagerly (first-fail) — there is no useful "more-bodies-to-
/// emit" continuation since downstream WGSL would be ill-formed.
pub fn lower_fn_body(
    f: &MirFunc,
    entry_point_void_return: bool,
) -> Result<Vec<String>, BodyEmitError> {
    let mut ctx = Ctx::new(&f.name);
    walk_region(&mut ctx, &f.body)?;

    // Trailing-value handling : the last `cssl.assign / arith.constant /
    // arith.<binop> / call` op's result becomes the implicit return for
    // helper fns, *iff* the fn has a non-empty result list. We don't track
    // explicit `func.return` ops in stage-0 MIR (body_lower never emits
    // them) — the convention is that the last produced value is the result.
    if !entry_point_void_return {
        if let Some(last_id) = ctx.last_value_id {
            if !f.results.is_empty() {
                ctx.lines.push(format!("return v{};", last_id.0));
            }
        }
    }

    if ctx.lines.is_empty() {
        // body_emit always emits at least one line so the resulting WGSL
        // doesn't have a stray empty-block. This is naga-compatible — empty
        // fn bodies parse fine — but a trailing comment makes the output
        // human-readable.
        ctx.lines
            .push("// (body emitted empty — no MIR ops)".into());
    }

    Ok(ctx.lines)
}

// ───────────────────────────────────────────────────────────────────────
// § Walker context.
// ───────────────────────────────────────────────────────────────────────

/// Walker state for a single fn body emission.
struct Ctx {
    /// Owning fn name — threaded into every diagnostic for actionable text.
    fn_name: String,
    /// Output WGSL lines accumulated in source order.
    lines: Vec<String>,
    /// Last produced value-id (for implicit-return handling at the fn-body
    /// level + parent yield-var assignment for `scf.if` / loops).
    last_value_id: Option<ValueId>,
    /// Stack of yield-target var-names. When we recurse into an `scf.if`
    /// branch region, we push the target name ; a `scf.yield` inside that
    /// region resolves to `target = v<id>;`. The stack handles arbitrarily
    /// nested scf.if bodies.
    yield_targets: Vec<String>,
    /// Counter for synthetic var-names (yield targets + WGSL local vars
    /// distinct from MIR `v<id>` SSA names).
    synthetic_counter: u32,
}

impl Ctx {
    fn new(fn_name: &str) -> Self {
        Self {
            fn_name: fn_name.to_string(),
            lines: Vec::new(),
            last_value_id: None,
            yield_targets: Vec::new(),
            synthetic_counter: 0,
        }
    }

    fn next_synthetic(&mut self, prefix: &str) -> String {
        let n = self.synthetic_counter;
        self.synthetic_counter = self.synthetic_counter.saturating_add(1);
        format!("{prefix}{n}")
    }

    fn push(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    fn name(&self) -> String {
        self.fn_name.clone()
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Top-level region + op walkers.
// ───────────────────────────────────────────────────────────────────────

fn walk_region(ctx: &mut Ctx, region: &MirRegion) -> Result<(), BodyEmitError> {
    for block in &region.blocks {
        for op in &block.ops {
            walk_op(ctx, op)?;
        }
    }
    Ok(())
}

fn walk_op(ctx: &mut Ctx, op: &MirOp) -> Result<(), BodyEmitError> {
    match op.name.as_str() {
        // § Constants ────────────────────────────────────────────────────
        "arith.constant" => emit_constant(ctx, op)?,

        // § Integer arithmetic ──────────────────────────────────────────
        "arith.addi" => emit_int_binop(ctx, op, "+")?,
        "arith.subi" | "arith.subi_neg" => emit_int_binop(ctx, op, "-")?,
        "arith.muli" => emit_int_binop(ctx, op, "*")?,
        "arith.divsi" => emit_int_binop(ctx, op, "/")?,
        "arith.remsi" => emit_int_binop(ctx, op, "%")?,

        // § Float arithmetic ────────────────────────────────────────────
        "arith.addf" => emit_float_binop(ctx, op, "+")?,
        "arith.subf" => emit_float_binop(ctx, op, "-")?,
        "arith.mulf" => emit_float_binop(ctx, op, "*")?,
        "arith.divf" => emit_float_binop(ctx, op, "/")?,
        "arith.remf" => emit_float_binop(ctx, op, "%")?,

        // § Integer compare ─────────────────────────────────────────────
        "arith.cmpi_eq" => emit_cmp_binop(ctx, op, "==")?,
        "arith.cmpi_ne" => emit_cmp_binop(ctx, op, "!=")?,
        "arith.cmpi_slt" => emit_cmp_binop(ctx, op, "<")?,
        "arith.cmpi_sle" => emit_cmp_binop(ctx, op, "<=")?,
        "arith.cmpi_sgt" => emit_cmp_binop(ctx, op, ">")?,
        "arith.cmpi_sge" => emit_cmp_binop(ctx, op, ">=")?,

        // § Float compare ───────────────────────────────────────────────
        "arith.cmpf_oeq" => emit_cmp_binop(ctx, op, "==")?,
        "arith.cmpf_one" => emit_cmp_binop(ctx, op, "!=")?,
        "arith.cmpf_olt" => emit_cmp_binop(ctx, op, "<")?,
        "arith.cmpf_ole" => emit_cmp_binop(ctx, op, "<=")?,
        "arith.cmpf_ogt" => emit_cmp_binop(ctx, op, ">")?,
        "arith.cmpf_oge" => emit_cmp_binop(ctx, op, ">=")?,

        // § Bitwise + shifts (i32) ──────────────────────────────────────
        "arith.andi" => emit_int_binop(ctx, op, "&")?,
        "arith.ori" => emit_int_binop(ctx, op, "|")?,
        "arith.xori" => emit_int_binop(ctx, op, "^")?,
        "arith.shli" => emit_int_binop(ctx, op, "<<")?,
        "arith.shrsi" => emit_int_binop(ctx, op, ">>")?,

        // § Unary ───────────────────────────────────────────────────────
        "arith.negf" => emit_unary(ctx, op, "-", "f32")?,
        "arith.negi" => emit_unary(ctx, op, "-", "i32")?,

        // § Cast (best-effort) ──────────────────────────────────────────
        "arith.bitcast" => emit_bitcast(ctx, op)?,

        // § Structured control-flow ─────────────────────────────────────
        "scf.if" => emit_scf_if(ctx, op)?,
        "scf.for" => emit_scf_loop(ctx, op, "for")?,
        "scf.while" => emit_scf_loop(ctx, op, "while")?,
        "scf.loop" => emit_scf_loop(ctx, op, "loop")?,

        // § Yield ────────────────────────────────────────────────────────
        // A yield outside a structured parent should not be reachable —
        // D5 catches it. Defense-in-depth fault here.
        "scf.yield" => emit_yield(ctx, op)?,

        // § Memref ──────────────────────────────────────────────────────
        "memref.load" => emit_memref_load(ctx, op)?,
        "memref.store" => emit_memref_store(ctx, op)?,

        // § Heap (REJECT) ───────────────────────────────────────────────
        "cssl.heap.alloc" | "cssl.heap.dealloc" | "cssl.heap.realloc" => {
            return Err(BodyEmitError::HeapOpRejected {
                fn_name: ctx.name(),
                op: op.name.clone(),
            });
        }

        // § Closures (REJECT) ───────────────────────────────────────────
        // T11-D100 (J2) : `cssl.closure.call` + `cssl.closure.call.error`
        // join the rejected-set ; closures remain CPU-only at stage-0.
        "cssl.closure"
        | "cssl.closure.invoke"
        | "cssl.closure.env"
        | "cssl.closure.call"
        | "cssl.closure.call.error" => {
            return Err(BodyEmitError::ClosureOpRejected {
                fn_name: ctx.name(),
                op: op.name.clone(),
            });
        }

        // § Unstructured (REJECT) ───────────────────────────────────────
        "cf.br" | "cf.cond_br" => {
            return Err(BodyEmitError::UnstructuredOp {
                fn_name: ctx.name(),
                op: op.name.clone(),
            });
        }

        // § Function calls — best-effort passthrough as a comment.
        // A future slice will resolve callee names + emit real call-site
        // syntax. For now the call is left as a comment so naga sees a
        // syntactically-valid body.
        "func.call" | "func.return" => {
            // Trailing return : if the op carries an operand, treat it as
            // the implicit-return value-id so a downstream entry-point
            // emission picks it up.
            if op.name == "func.return" {
                if let Some(operand) = op.operands.first() {
                    ctx.last_value_id = Some(*operand);
                }
            }
            ctx.push(format!("// {}", op.name));
        }

        // § Source-loc + IFC + verify : emitted as comments, structurally
        // inert in the lowered shader body. These ops carry meaning at
        // earlier compile passes (IFC walker, verify-discharge) ; by the
        // time WGSL emission runs, they're already proven.
        "cssl.ifc.label" | "cssl.ifc.declassify" | "cssl.verify.assert" | "cssl.field" => {
            ctx.push(format!("// {} (passthrough)", op.name));
        }

        // § Default : everything else is a stage-0 hole.
        other => {
            return Err(BodyEmitError::UnsupportedOp {
                fn_name: ctx.name(),
                op: other.to_string(),
            });
        }
    }
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────
// § Lowering helpers — one per op-shape family.
// ───────────────────────────────────────────────────────────────────────

fn emit_constant(ctx: &mut Ctx, op: &MirOp) -> Result<(), BodyEmitError> {
    let result = result0(ctx, op)?;
    let value = op
        .attributes
        .iter()
        .find(|(k, _)| k == "value")
        .map(|(_, v)| v.as_str())
        .ok_or(BodyEmitError::ConstantMissingValueAttr {
            fn_name: ctx.name(),
        })?;
    let wgsl_ty = wgsl_type(&result.ty, &ctx.name(), result.id)?;
    let lit = render_literal(value, &result.ty);
    ctx.push(format!("let v{} : {wgsl_ty} = {lit};", result.id.0));
    ctx.last_value_id = Some(result.id);
    Ok(())
}

fn emit_int_binop(ctx: &mut Ctx, op: &MirOp, infix: &str) -> Result<(), BodyEmitError> {
    let result = result0(ctx, op)?;
    let (lhs, rhs) = operands_two(ctx, op)?;
    ctx.push(format!(
        "let v{} : i32 = v{} {infix} v{};",
        result.id.0, lhs.0, rhs.0,
    ));
    ctx.last_value_id = Some(result.id);
    Ok(())
}

fn emit_float_binop(ctx: &mut Ctx, op: &MirOp, infix: &str) -> Result<(), BodyEmitError> {
    let result = result0(ctx, op)?;
    let (lhs, rhs) = operands_two(ctx, op)?;
    ctx.push(format!(
        "let v{} : f32 = v{} {infix} v{};",
        result.id.0, lhs.0, rhs.0,
    ));
    ctx.last_value_id = Some(result.id);
    Ok(())
}

fn emit_cmp_binop(ctx: &mut Ctx, op: &MirOp, infix: &str) -> Result<(), BodyEmitError> {
    let result = result0(ctx, op)?;
    let (lhs, rhs) = operands_two(ctx, op)?;
    ctx.push(format!(
        "let v{} : bool = v{} {infix} v{};",
        result.id.0, lhs.0, rhs.0,
    ));
    ctx.last_value_id = Some(result.id);
    Ok(())
}

fn emit_unary(ctx: &mut Ctx, op: &MirOp, prefix: &str, ty: &str) -> Result<(), BodyEmitError> {
    let result = result0(ctx, op)?;
    let operand = operand0(ctx, op)?;
    ctx.push(format!(
        "let v{} : {ty} = {prefix}v{};",
        result.id.0, operand.0,
    ));
    ctx.last_value_id = Some(result.id);
    Ok(())
}

fn emit_bitcast(ctx: &mut Ctx, op: &MirOp) -> Result<(), BodyEmitError> {
    let result = result0(ctx, op)?;
    let operand = operand0(ctx, op)?;
    let wgsl_ty = wgsl_type(&result.ty, &ctx.name(), result.id)?;
    ctx.push(format!(
        "let v{} : {wgsl_ty} = bitcast<{wgsl_ty}>(v{});",
        result.id.0, operand.0,
    ));
    ctx.last_value_id = Some(result.id);
    Ok(())
}

// ── scf.if ───────────────────────────────────────────────────────────────
//
// WGSL has structured `if (cond) { ... } else { ... }`. The MIR scf.if op
// has 2 nested regions ; if both regions yield a value, we lift the result
// to a `var<function>` declared just before the `if` so each branch can
// assign into it. This matches the cranelift lowering's merge-block-param
// pattern but uses WGSL's local-var rather than block-arg.

fn emit_scf_if(ctx: &mut Ctx, op: &MirOp) -> Result<(), BodyEmitError> {
    if op.regions.len() != 2 {
        return Err(BodyEmitError::ScfIfWrongRegions {
            fn_name: ctx.name(),
            actual: op.regions.len(),
        });
    }
    let cond = operand0(ctx, op)?;
    let result = op.results.first();
    let yield_target = match result {
        Some(r) if !matches!(r.ty, MirType::None) => {
            let wgsl_ty = wgsl_type(&r.ty, &ctx.name(), r.id)?;
            let target_name = format!("v{}", r.id.0);
            ctx.push(format!("var {target_name} : {wgsl_ty};"));
            Some(target_name)
        }
        _ => None,
    };

    ctx.push(format!("if (v{}) {{", cond.0));
    ctx.yield_targets
        .push(yield_target.clone().unwrap_or_default());
    walk_region(ctx, &op.regions[0])?;
    ctx.yield_targets.pop();
    ctx.push("} else {");
    ctx.yield_targets.push(yield_target.unwrap_or_default());
    walk_region(ctx, &op.regions[1])?;
    ctx.yield_targets.pop();
    ctx.push("}");

    if let Some(r) = result {
        ctx.last_value_id = Some(r.id);
    }
    Ok(())
}

// ── scf.for / scf.while / scf.loop ───────────────────────────────────────
//
// Stage-0 emits the canonical structured-loop shape :
//   loop {
//     if (!cond) { break; }    ← only for scf.while
//     <body>
//   }
// scf.loop is unconditional ; scf.for desugars at HIR level so reaches us
// as scf.for without iter-state — for stage-0 we mirror scf.loop's shape
// (the iter + step + count desugaring is a future-slice transform).

fn emit_scf_loop(ctx: &mut Ctx, op: &MirOp, form: &str) -> Result<(), BodyEmitError> {
    if op.regions.len() != 1 {
        return Err(BodyEmitError::LoopWrongRegions {
            fn_name: ctx.name(),
            loop_form: form.to_string(),
            actual: op.regions.len(),
        });
    }
    ctx.push(format!("loop {{ // scf.{form}"));
    if form == "while" {
        if let Some(cond) = op.operands.first() {
            ctx.push(format!("    if (!v{}) {{ break; }}", cond.0));
        }
    }
    // Loops do not yield in stage-0 ; the empty yield-target stack frame
    // ensures any `scf.yield` inside the loop body becomes a no-op
    // assignment (per the D5 forward-compat policy in
    // `is_structured_parent_for_yield`).
    ctx.yield_targets.push(String::new());
    walk_region(ctx, &op.regions[0])?;
    ctx.yield_targets.pop();
    ctx.push("}");
    Ok(())
}

// ── scf.yield ────────────────────────────────────────────────────────────

fn emit_yield(ctx: &mut Ctx, op: &MirOp) -> Result<(), BodyEmitError> {
    let target =
        ctx.yield_targets
            .last()
            .cloned()
            .ok_or_else(|| BodyEmitError::OrphanScfYield {
                fn_name: ctx.name(),
            })?;
    if target.is_empty() {
        // The parent does not yield (statement-form scf.if or any loop).
        // No-op : the yield value is dropped on the floor structurally.
        if let Some(operand) = op.operands.first() {
            ctx.push(format!(
                "// scf.yield v{} (parent does not consume)",
                operand.0
            ));
        }
        return Ok(());
    }
    let operand = operand0(ctx, op)?;
    ctx.push(format!("    {target} = v{};", operand.0));
    Ok(())
}

// ── memref.load / memref.store ───────────────────────────────────────────
//
// Stage-0 lowers memref-backed loads/stores to a buffer-binding pattern.
// We emit the canonical WGSL shape `<buf>[<idx>]` ; the calling layer
// (S6-D4 emit.rs) is responsible for declaring the `var<storage>` binding
// itself. Since memref-backed buffer-binding inference from effect-row is
// a future-slice transform, we emit the load/store as a plain index
// expression and trust the binding to exist textually in the prelude.

fn emit_memref_load(ctx: &mut Ctx, op: &MirOp) -> Result<(), BodyEmitError> {
    let result = result0(ctx, op)?;
    let (buf, idx) = operands_two(ctx, op)?;
    let wgsl_ty = match &result.ty {
        MirType::None => "i32".to_string(), // fallback : memref load result type
        ty => wgsl_type(ty, &ctx.name(), result.id)?,
    };
    ctx.push(format!(
        "let v{} : {wgsl_ty} = v{}[v{}];",
        result.id.0, buf.0, idx.0,
    ));
    ctx.last_value_id = Some(result.id);
    Ok(())
}

fn emit_memref_store(ctx: &mut Ctx, op: &MirOp) -> Result<(), BodyEmitError> {
    if op.operands.len() < 2 {
        return Err(BodyEmitError::OperandCountMismatch {
            fn_name: ctx.name(),
            op: op.name.clone(),
            expected: 2,
            actual: op.operands.len(),
        });
    }
    // body_lower's lower_assign emits `cssl.assign rhs` ; the canonical
    // `memref.store buf idx value` shape from MLIR has 3 operands. We
    // accept both : if 3 operands present, treat as
    // `buf[idx] = value` ; if only 2, treat as compatibility shape with
    // implicit assignment of the value to the load-target.
    if op.operands.len() == 3 {
        let buf = op.operands[0];
        let idx = op.operands[1];
        let val = op.operands[2];
        ctx.push(format!("v{}[v{}] = v{};", buf.0, idx.0, val.0));
    } else {
        let _ = ctx.next_synthetic("storeloc"); // counter-stir for tests + future slots
        let target = op.operands[0];
        let value = op.operands[1];
        ctx.push(format!("v{} = v{};", target.0, value.0));
    }
    Ok(())
}

// ───────────────────────────────────────────────────────────────────────
// § Helpers : operand / result accessors, type translation, literal rendering.
// ───────────────────────────────────────────────────────────────────────

fn result0<'op>(ctx: &Ctx, op: &'op MirOp) -> Result<&'op cssl_mir::MirValue, BodyEmitError> {
    op.results
        .first()
        .ok_or_else(|| BodyEmitError::OperandCountMismatch {
            fn_name: ctx.name(),
            op: op.name.clone(),
            expected: 1,
            actual: 0,
        })
}

fn operand0(ctx: &Ctx, op: &MirOp) -> Result<ValueId, BodyEmitError> {
    op.operands
        .first()
        .copied()
        .ok_or_else(|| BodyEmitError::OperandCountMismatch {
            fn_name: ctx.name(),
            op: op.name.clone(),
            expected: 1,
            actual: 0,
        })
}

fn operands_two(ctx: &Ctx, op: &MirOp) -> Result<(ValueId, ValueId), BodyEmitError> {
    if op.operands.len() < 2 {
        return Err(BodyEmitError::OperandCountMismatch {
            fn_name: ctx.name(),
            op: op.name.clone(),
            expected: 2,
            actual: op.operands.len(),
        });
    }
    Ok((op.operands[0], op.operands[1]))
}

/// Map a `MirType` to its WGSL source-form. Stage-0 narrows `i64 → i32` and
/// `f64 → f32` to fit WebGPU's MVP type-set ; future slices add 32-bit
/// pair emulation or push diagnostics back through HIR. Non-scalar types
/// (`Tuple` / `Function` / `Memref` / `Ptr` / `Handle` / `Opaque`) reject
/// with [`BodyEmitError::UnsupportedType`].
pub fn wgsl_type(ty: &MirType, fn_name: &str, value_id: ValueId) -> Result<String, BodyEmitError> {
    let mapped = match ty {
        MirType::Bool | MirType::Int(IntWidth::I1) => "bool".to_string(),
        MirType::Int(_) => "i32".to_string(),
        MirType::Float(FloatWidth::F16) => "f16".to_string(),
        MirType::Float(FloatWidth::Bf16 | FloatWidth::F32 | FloatWidth::F64) => "f32".to_string(),
        MirType::Vec(lanes, FloatWidth::F32) => format!("vec{lanes}<f32>"),
        MirType::Vec(lanes, FloatWidth::F16) => format!("vec{lanes}<f16>"),
        MirType::Vec(lanes, FloatWidth::F64 | FloatWidth::Bf16) => {
            // f64 / bf16 vectors narrow to f32-vectors at stage-0 (per type-law).
            format!("vec{lanes}<f32>")
        }
        MirType::None => "()".to_string(),
        MirType::Memref { .. }
        | MirType::Ptr
        | MirType::Handle
        | MirType::Tuple(_)
        | MirType::Function { .. }
        | MirType::Opaque(_) => {
            return Err(BodyEmitError::UnsupportedType {
                fn_name: fn_name.to_string(),
                value_id: value_id.0,
                ty: ty.to_string(),
            });
        }
    };
    Ok(mapped)
}

/// Render an `arith.constant` `value`-attribute as a WGSL literal. The
/// `value` attribute is the canonical source-form string (e.g., `"42"` /
/// `"-3.14"` / `"true"`) ; the WGSL-side rendering must match the chosen
/// WGSL type to avoid implicit-conversion errors. Per the slice landmines
/// : WGSL i32 and u32 are distinct, no implicit conversion. We produce :
///   - `true` / `false` for bool
///   - `42` / `-7` for i32 (no `i` suffix needed in WGSL)
///   - `3.5` / `0.0` for f32 (always include a `.` so naga doesn't parse
///     it as an integer)
fn render_literal(value: &str, ty: &MirType) -> String {
    match ty {
        MirType::Bool | MirType::Int(IntWidth::I1) => render_bool_literal(value),
        MirType::Int(_) => render_int_literal(value),
        MirType::Float(_) => render_float_literal(value),
        // Vec / aggregate constants : not produced by body_lower at stage-0 ;
        // future slices emit `vec3<f32>(0.0, 0.0, 0.0)` from a JSON-like attr.
        _ => value.to_string(),
    }
}

fn render_bool_literal(value: &str) -> String {
    match value.trim() {
        "true" | "1" => "true".into(),
        "false" | "0" => "false".into(),
        other => other.to_string(),
    }
}

fn render_int_literal(value: &str) -> String {
    // body_lower's lower_literal renders ints as decimal-form (`format!`'d
    // i64) ; some paths produce `"stage0_int"` placeholders for unparseable
    // literals. We pass through the digit-form ; placeholders fall through
    // as-is and fail naga validation early (which is the desired stage-0
    // behavior — better to surface "stage0_int is not valid WGSL" than to
    // silently emit garbage).
    let trimmed = value.trim();
    // Accept negative sign + digits (no underscore / radix prefix —
    // body_lower already canonicalized).
    let mut iter = trimmed.chars();
    if let Some(first) = iter.next() {
        if first == '-' || first.is_ascii_digit() {
            let rest_ok = iter.all(|c| c.is_ascii_digit());
            if rest_ok {
                return trimmed.to_string();
            }
        }
    }
    // Unparseable : fall through to raw — naga will reject.
    trimmed.to_string()
}

fn render_float_literal(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.contains('.')
        || trimmed.contains('e')
        || trimmed.contains('E')
        || trimmed.contains("inf")
        || trimmed.contains("nan")
    {
        return trimmed.to_string();
    }
    // body_lower may emit `"3"` for integer-shaped float literals ; WGSL
    // requires a `.` to distinguish f32 from i32. Append `.0`.
    let mut out = trimmed.to_string();
    out.push_str(".0");
    out
}

#[allow(dead_code)]
fn _format_unused(out: &mut String, msg: &str) {
    let _ = write!(out, "{msg}");
}

// ───────────────────────────────────────────────────────────────────────
// § Tests — body emitter.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{lower_fn_body, render_literal, wgsl_type, BodyEmitError};
    use cssl_mir::{FloatWidth, IntWidth, MirBlock, MirFunc, MirOp, MirRegion, MirType, ValueId};

    fn fn_with_body(name: &str, return_ty: MirType, ops: Vec<MirOp>) -> MirFunc {
        let mut f = MirFunc::new(name, vec![], vec![return_ty]);
        if let Some(entry) = f.body.entry_mut() {
            entry.ops = ops;
        }
        f
    }

    fn const_op(id: u32, ty: MirType, value: &str) -> MirOp {
        MirOp::std("arith.constant")
            .with_result(ValueId(id), ty)
            .with_attribute("value", value)
    }

    // ── Type translation ─────────────────────────────────────────────────

    #[test]
    fn wgsl_type_i32_canonical() {
        let ty = wgsl_type(&MirType::Int(IntWidth::I32), "f", ValueId(0)).unwrap();
        assert_eq!(ty, "i32");
    }

    #[test]
    fn wgsl_type_f32_canonical() {
        let ty = wgsl_type(&MirType::Float(FloatWidth::F32), "f", ValueId(0)).unwrap();
        assert_eq!(ty, "f32");
    }

    #[test]
    fn wgsl_type_bool_canonical() {
        let ty = wgsl_type(&MirType::Bool, "f", ValueId(0)).unwrap();
        assert_eq!(ty, "bool");
    }

    #[test]
    fn wgsl_type_i64_narrows_to_i32() {
        let ty = wgsl_type(&MirType::Int(IntWidth::I64), "f", ValueId(0)).unwrap();
        assert_eq!(ty, "i32");
    }

    #[test]
    fn wgsl_type_f64_narrows_to_f32() {
        let ty = wgsl_type(&MirType::Float(FloatWidth::F64), "f", ValueId(0)).unwrap();
        assert_eq!(ty, "f32");
    }

    #[test]
    fn wgsl_type_vec3_f32_canonical() {
        let ty = wgsl_type(&MirType::Vec(3, FloatWidth::F32), "f", ValueId(0)).unwrap();
        assert_eq!(ty, "vec3<f32>");
    }

    #[test]
    fn wgsl_type_ptr_rejected() {
        let err = wgsl_type(&MirType::Ptr, "f", ValueId(0)).unwrap_err();
        assert_eq!(err.code(), "WGSL0003");
    }

    #[test]
    fn wgsl_type_memref_rejected() {
        let mr = MirType::Memref {
            shape: vec![Some(16)],
            elem: Box::new(MirType::Float(FloatWidth::F32)),
        };
        let err = wgsl_type(&mr, "f", ValueId(0)).unwrap_err();
        assert_eq!(err.code(), "WGSL0003");
    }

    // ── Constants ────────────────────────────────────────────────────────

    #[test]
    fn constant_i32_renders() {
        let f = fn_with_body(
            "k",
            MirType::Int(IntWidth::I32),
            vec![const_op(0, MirType::Int(IntWidth::I32), "42")],
        );
        let lines = lower_fn_body(&f, false).unwrap();
        let body = lines.join("\n");
        assert!(body.contains("let v0 : i32 = 42;"));
        assert!(body.contains("return v0;"));
    }

    #[test]
    fn constant_f32_appends_dot_when_missing() {
        let f = fn_with_body(
            "k",
            MirType::Float(FloatWidth::F32),
            vec![const_op(0, MirType::Float(FloatWidth::F32), "3")],
        );
        let lines = lower_fn_body(&f, false).unwrap();
        let body = lines.join("\n");
        assert!(body.contains("let v0 : f32 = 3.0;"));
    }

    #[test]
    fn constant_f32_keeps_existing_decimal_point() {
        let f = fn_with_body(
            "k",
            MirType::Float(FloatWidth::F32),
            vec![const_op(0, MirType::Float(FloatWidth::F32), "2.5")],
        );
        let lines = lower_fn_body(&f, false).unwrap();
        let body = lines.join("\n");
        assert!(body.contains("let v0 : f32 = 2.5;"));
    }

    #[test]
    fn constant_bool_normalizes() {
        assert_eq!(render_literal("true", &MirType::Bool), "true");
        assert_eq!(render_literal("1", &MirType::Bool), "true");
        assert_eq!(render_literal("false", &MirType::Bool), "false");
    }

    #[test]
    fn constant_missing_value_attr_errors() {
        let op = MirOp::std("arith.constant").with_result(ValueId(0), MirType::Int(IntWidth::I32));
        let f = fn_with_body("k", MirType::Int(IntWidth::I32), vec![op]);
        let err = lower_fn_body(&f, false).unwrap_err();
        assert_eq!(err.code(), "WGSL0004");
    }

    // ── Arithmetic ───────────────────────────────────────────────────────

    #[test]
    fn integer_addition_renders_infix() {
        let f = fn_with_body(
            "add",
            MirType::Int(IntWidth::I32),
            vec![
                const_op(0, MirType::Int(IntWidth::I32), "1"),
                const_op(1, MirType::Int(IntWidth::I32), "2"),
                MirOp::std("arith.addi")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), MirType::Int(IntWidth::I32)),
            ],
        );
        let lines = lower_fn_body(&f, false).unwrap();
        let body = lines.join("\n");
        assert!(body.contains("let v2 : i32 = v0 + v1;"));
    }

    #[test]
    fn float_multiplication_renders_infix() {
        let f = fn_with_body(
            "fma_lite",
            MirType::Float(FloatWidth::F32),
            vec![
                const_op(0, MirType::Float(FloatWidth::F32), "1.5"),
                const_op(1, MirType::Float(FloatWidth::F32), "2.5"),
                MirOp::std("arith.mulf")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), MirType::Float(FloatWidth::F32)),
            ],
        );
        let body = lower_fn_body(&f, false).unwrap().join("\n");
        assert!(body.contains("let v2 : f32 = v0 * v1;"));
    }

    #[test]
    fn integer_compare_emits_bool_result() {
        let f = fn_with_body(
            "lt",
            MirType::Bool,
            vec![
                const_op(0, MirType::Int(IntWidth::I32), "5"),
                const_op(1, MirType::Int(IntWidth::I32), "9"),
                MirOp::std("arith.cmpi_slt")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), MirType::Bool),
            ],
        );
        let body = lower_fn_body(&f, false).unwrap().join("\n");
        assert!(body.contains("let v2 : bool = v0 < v1;"));
    }

    #[test]
    fn bitwise_and_emits_amp() {
        let f = fn_with_body(
            "mask",
            MirType::Int(IntWidth::I32),
            vec![
                const_op(0, MirType::Int(IntWidth::I32), "15"),
                const_op(1, MirType::Int(IntWidth::I32), "9"),
                MirOp::std("arith.andi")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), MirType::Int(IntWidth::I32)),
            ],
        );
        let body = lower_fn_body(&f, false).unwrap().join("\n");
        assert!(body.contains("let v2 : i32 = v0 & v1;"));
    }

    #[test]
    fn unary_negf_emits_minus() {
        let f = fn_with_body(
            "neg",
            MirType::Float(FloatWidth::F32),
            vec![
                const_op(0, MirType::Float(FloatWidth::F32), "1.5"),
                MirOp::std("arith.negf")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), MirType::Float(FloatWidth::F32)),
            ],
        );
        let body = lower_fn_body(&f, false).unwrap().join("\n");
        assert!(body.contains("let v1 : f32 = -v0;"));
    }

    // ── scf.if ───────────────────────────────────────────────────────────

    #[test]
    fn scf_if_with_yields_emits_var_and_branches() {
        let then_region = {
            let mut r = MirRegion::with_entry(Vec::new());
            if let Some(b) = r.entry_mut() {
                b.push(const_op(1, MirType::Int(IntWidth::I32), "1"));
                b.push(MirOp::std("scf.yield").with_operand(ValueId(1)));
            }
            r
        };
        let else_region = {
            let mut r = MirRegion::with_entry(Vec::new());
            if let Some(b) = r.entry_mut() {
                b.push(const_op(2, MirType::Int(IntWidth::I32), "2"));
                b.push(MirOp::std("scf.yield").with_operand(ValueId(2)));
            }
            r
        };
        let if_op = MirOp::std("scf.if")
            .with_operand(ValueId(0))
            .with_region(then_region)
            .with_region(else_region)
            .with_result(ValueId(3), MirType::Int(IntWidth::I32));

        let f = fn_with_body(
            "choose",
            MirType::Int(IntWidth::I32),
            vec![const_op(0, MirType::Bool, "true"), if_op],
        );
        let body = lower_fn_body(&f, false).unwrap().join("\n");
        assert!(body.contains("var v3 : i32;"));
        assert!(body.contains("if (v0) {"));
        assert!(body.contains("} else {"));
        assert!(body.contains("v3 = v1;"));
        assert!(body.contains("v3 = v2;"));
    }

    #[test]
    fn scf_if_wrong_region_count_errors() {
        let if_op = MirOp::std("scf.if")
            .with_operand(ValueId(0))
            .with_result(ValueId(1), MirType::None);
        let f = fn_with_body(
            "broken",
            MirType::None,
            vec![const_op(0, MirType::Bool, "true"), if_op],
        );
        let err = lower_fn_body(&f, true).unwrap_err();
        assert_eq!(err.code(), "WGSL0009");
    }

    // ── scf.loop / scf.while ─────────────────────────────────────────────

    #[test]
    fn scf_loop_emits_loop_block() {
        let body_region = {
            let mut r = MirRegion::with_entry(Vec::new());
            if let Some(b) = r.entry_mut() {
                b.push(const_op(1, MirType::Int(IntWidth::I32), "1"));
            }
            r
        };
        let loop_op = MirOp::std("scf.loop")
            .with_region(body_region)
            .with_result(ValueId(2), MirType::None);
        let f = fn_with_body("looper", MirType::None, vec![loop_op]);
        let body = lower_fn_body(&f, true).unwrap().join("\n");
        assert!(body.contains("loop { // scf.loop"));
        assert!(body.contains("let v1 : i32 = 1;"));
    }

    #[test]
    fn scf_while_emits_break_condition() {
        let body_region = {
            let mut r = MirRegion::with_entry(Vec::new());
            if let Some(b) = r.entry_mut() {
                b.push(const_op(1, MirType::Int(IntWidth::I32), "1"));
            }
            r
        };
        let f = fn_with_body(
            "loopy",
            MirType::None,
            vec![
                const_op(0, MirType::Bool, "true"),
                MirOp::std("scf.while")
                    .with_operand(ValueId(0))
                    .with_region(body_region)
                    .with_result(ValueId(2), MirType::None),
            ],
        );
        let body = lower_fn_body(&f, true).unwrap().join("\n");
        assert!(body.contains("loop { // scf.while"));
        assert!(body.contains("if (!v0) { break; }"));
    }

    #[test]
    fn scf_loop_wrong_region_count_errors() {
        let f = fn_with_body(
            "broken",
            MirType::None,
            vec![MirOp::std("scf.for").with_result(ValueId(0), MirType::None)],
        );
        let err = lower_fn_body(&f, true).unwrap_err();
        assert_eq!(err.code(), "WGSL0010");
    }

    // ── memref.load / memref.store ───────────────────────────────────────

    #[test]
    fn memref_load_emits_index_expression() {
        // Hand-build a memref op : two operands (buf, idx), one result.
        let f = fn_with_body(
            "ld",
            MirType::Int(IntWidth::I32),
            vec![
                const_op(0, MirType::Int(IntWidth::I32), "0"), // buf
                const_op(1, MirType::Int(IntWidth::I32), "0"), // idx
                MirOp::std("memref.load")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), MirType::Int(IntWidth::I32)),
            ],
        );
        let body = lower_fn_body(&f, false).unwrap().join("\n");
        assert!(body.contains("let v2 : i32 = v0[v1];"));
    }

    #[test]
    fn memref_store_3operand_emits_index_assignment() {
        let f = fn_with_body(
            "st",
            MirType::None,
            vec![
                const_op(0, MirType::Int(IntWidth::I32), "0"), // buf
                const_op(1, MirType::Int(IntWidth::I32), "0"), // idx
                const_op(2, MirType::Int(IntWidth::I32), "9"), // value
                MirOp::std("memref.store")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_operand(ValueId(2)),
            ],
        );
        let body = lower_fn_body(&f, true).unwrap().join("\n");
        assert!(body.contains("v0[v1] = v2;"));
    }

    // ── Heap REJECT ──────────────────────────────────────────────────────

    #[test]
    fn heap_alloc_rejected() {
        let f = fn_with_body(
            "h",
            MirType::None,
            vec![MirOp::std("cssl.heap.alloc")
                .with_operand(ValueId(0))
                .with_operand(ValueId(0))
                .with_result(ValueId(1), MirType::Ptr)],
        );
        let err = lower_fn_body(&f, true).unwrap_err();
        assert_eq!(err.code(), "WGSL0001");
    }

    #[test]
    fn heap_dealloc_rejected() {
        let f = fn_with_body(
            "h",
            MirType::None,
            vec![MirOp::std("cssl.heap.dealloc")
                .with_operand(ValueId(0))
                .with_operand(ValueId(0))
                .with_operand(ValueId(0))],
        );
        let err = lower_fn_body(&f, true).unwrap_err();
        assert_eq!(err.code(), "WGSL0001");
    }

    // ── Closure REJECT ───────────────────────────────────────────────────

    #[test]
    fn closure_op_rejected() {
        let f = fn_with_body(
            "c",
            MirType::None,
            vec![MirOp::std("cssl.closure").with_result(ValueId(0), MirType::None)],
        );
        let err = lower_fn_body(&f, true).unwrap_err();
        assert_eq!(err.code(), "WGSL0002");
    }

    // ── Unstructured CFG REJECT ──────────────────────────────────────────

    #[test]
    fn cf_br_rejected() {
        let f = fn_with_body("u", MirType::None, vec![MirOp::std("cf.br")]);
        let err = lower_fn_body(&f, true).unwrap_err();
        assert_eq!(err.code(), "WGSL0007");
    }

    #[test]
    fn cf_cond_br_rejected() {
        let f = fn_with_body("u", MirType::None, vec![MirOp::std("cf.cond_br")]);
        let err = lower_fn_body(&f, true).unwrap_err();
        assert_eq!(err.code(), "WGSL0007");
    }

    #[test]
    fn unsupported_op_rejected() {
        let f = fn_with_body("u", MirType::None, vec![MirOp::std("totally.fictional")]);
        let err = lower_fn_body(&f, true).unwrap_err();
        assert_eq!(err.code(), "WGSL0008");
    }

    // ── Empty body ───────────────────────────────────────────────────────

    #[test]
    fn empty_body_emits_marker_comment() {
        let f = fn_with_body("empty", MirType::None, vec![]);
        let body = lower_fn_body(&f, true).unwrap().join("\n");
        assert!(body.contains("body emitted empty"));
    }

    // ── Multi-region nested scf.if inside loop ───────────────────────────

    #[test]
    fn nested_scf_if_in_loop_lowers() {
        // Outer scf.loop with scf.if inside — exercises the recursion path
        // through walk_op → emit_scf_loop → walk_region → emit_scf_if.
        let inner_then = {
            let mut r = MirRegion::with_entry(Vec::new());
            if let Some(b) = r.entry_mut() {
                b.push(const_op(2, MirType::Int(IntWidth::I32), "1"));
                b.push(MirOp::std("scf.yield").with_operand(ValueId(2)));
            }
            r
        };
        let inner_else = {
            let mut r = MirRegion::with_entry(Vec::new());
            if let Some(b) = r.entry_mut() {
                b.push(const_op(3, MirType::Int(IntWidth::I32), "2"));
                b.push(MirOp::std("scf.yield").with_operand(ValueId(3)));
            }
            r
        };
        let inner_if = MirOp::std("scf.if")
            .with_operand(ValueId(0))
            .with_region(inner_then)
            .with_region(inner_else)
            .with_result(ValueId(4), MirType::Int(IntWidth::I32));
        let outer_loop_body = {
            let mut r = MirRegion::with_entry(Vec::new());
            if let Some(b) = r.entry_mut() {
                b.push(inner_if);
            }
            r
        };
        let f = fn_with_body(
            "nested",
            MirType::None,
            vec![
                const_op(0, MirType::Bool, "true"),
                MirOp::std("scf.loop")
                    .with_region(outer_loop_body)
                    .with_result(ValueId(5), MirType::None),
            ],
        );
        let body = lower_fn_body(&f, true).unwrap().join("\n");
        assert!(body.contains("loop { // scf.loop"));
        assert!(body.contains("var v4 : i32;"));
        assert!(body.contains("if (v0) {"));
    }

    // ── Bitcast ──────────────────────────────────────────────────────────

    #[test]
    fn bitcast_emits_wgsl_bitcast() {
        let f = fn_with_body(
            "bc",
            MirType::Float(FloatWidth::F32),
            vec![
                const_op(0, MirType::Int(IntWidth::I32), "1065353216"),
                MirOp::std("arith.bitcast")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), MirType::Float(FloatWidth::F32)),
            ],
        );
        let body = lower_fn_body(&f, false).unwrap().join("\n");
        assert!(body.contains("bitcast<f32>(v0)"));
    }

    // ── Stable code coverage ─────────────────────────────────────────────

    #[test]
    fn all_body_emit_codes_unique_and_well_formed() {
        let codes = [
            BodyEmitError::HeapOpRejected {
                fn_name: "a".into(),
                op: "x".into(),
            }
            .code(),
            BodyEmitError::ClosureOpRejected {
                fn_name: "a".into(),
                op: "x".into(),
            }
            .code(),
            BodyEmitError::UnsupportedType {
                fn_name: "a".into(),
                value_id: 0,
                ty: "x".into(),
            }
            .code(),
            BodyEmitError::ConstantMissingValueAttr {
                fn_name: "a".into(),
            }
            .code(),
            BodyEmitError::OperandCountMismatch {
                fn_name: "a".into(),
                op: "x".into(),
                expected: 1,
                actual: 0,
            }
            .code(),
            BodyEmitError::OrphanScfYield {
                fn_name: "a".into(),
            }
            .code(),
            BodyEmitError::UnstructuredOp {
                fn_name: "a".into(),
                op: "x".into(),
            }
            .code(),
            BodyEmitError::UnsupportedOp {
                fn_name: "a".into(),
                op: "x".into(),
            }
            .code(),
            BodyEmitError::ScfIfWrongRegions {
                fn_name: "a".into(),
                actual: 1,
            }
            .code(),
            BodyEmitError::LoopWrongRegions {
                fn_name: "a".into(),
                loop_form: "for".into(),
                actual: 0,
            }
            .code(),
        ];
        let mut sorted: Vec<&'static str> = codes.to_vec();
        sorted.sort_unstable();
        let mut unique = sorted.clone();
        unique.dedup();
        assert_eq!(sorted, unique, "duplicate WGSL codes : {sorted:?}");
        for c in &codes {
            assert!(c.starts_with("WGSL"), "non-WGSL code : {c}");
            assert_eq!(c.len(), 8, "wrong code format : {c}");
        }
    }

    // Suppress unused imports / locals if compiled without certain test
    // groups (defense-in-depth in case test config is filtered).
    #[test]
    fn _ctx_reachable_from_tests() {
        let _ = MirBlock::new("entry");
    }
}
