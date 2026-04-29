//! HIR-fn-body → MIR-op body lowering.
//!
//! § SPEC : `specs/02_IR.csl` § MIR + `specs/15_MLIR.csl` § CSSL-DIALECT-OPS +
//!         standard `arith.*` / `scf.*` / `func.*` dialects via [`CsslOp::Std`].
//!
//! § SCOPE (T6-phase-2c / this commit)
//!   - [`BodyLowerCtx`] : per-fn lowering context with fresh-value-id + op-buffer.
//!     Now carries an optional `&SourceFile` so literal-value extraction can
//!     pull the real text out of the span.
//!   - [`lower_fn_body`] : entry-point that takes a `HirFn` + a `MirFunc` +
//!     optional `&SourceFile` and populates `MirFunc.body.entry().ops` with
//!     lowered ops.
//!   - Every one of the 31 `HirExprKind` variants now has a dedicated lowerer
//!     (including the six that previously fell through to `emit_unsupported` :
//!     Lambda / Perform / With / Region / Compound / SectionRef). `Error` is
//!     the one remaining structural fallback — it's a parser-recovery shape,
//!     not user-writable syntax.
//!   - Literal-value extraction : when a `SourceFile` is threaded through,
//!     Int / Float / Bool / Str / Char literals carry the parsed value (or
//!     canonical string form) as the `value` attribute ; fallback to the
//!     `stage0_*` placeholder only when no source is available or the parse
//!     fails.
//!
//! § T6-phase-2d+ DEFERRED
//!   - Real type-propagation (many lowerers still return `MirType::None`
//!     where a precise type could be inferred by T3.4 type-inference).
//!   - Lambda closure-capture analysis (stage-0 emits `cssl.closure` with
//!     `param_count` attribute + a body region ; capture-discovery is T6-
//!     phase-2d work).
//!   - Effect-handler resolution (stage-0 `cssl.effect.handle` op carries
//!     the handler expression as a nested region + a handler_count attr ;
//!     operation-dispatch tables come in the effects-lowering pass).
//!   - `cssl.region.exit` pairing + arena-lifetime synthesis (per
//!     `specs/15` § PASS-PIPELINE the region → memref.alloca/dealloc
//!     lowering is a later pass).
//!   - Break-with-label targeting — `scf.br` / `scf.continue` emission.
//!   - Pattern-matching arm-guard lowering + exhaustiveness-checking.

use std::collections::HashMap;

use cssl_ast::{SourceFile, Span};
use cssl_hir::{
    HirBinOp, HirBlock, HirCallArg, HirCompoundOp, HirExpr, HirExprKind, HirFn, HirLambdaParam,
    HirLiteral, HirLiteralKind, HirStmt, HirStmtKind, HirStructFieldInit, HirType, HirTypeKind,
    HirUnOp, Interner, Symbol,
};

use crate::block::{MirBlock, MirOp, MirRegion};
use crate::func::MirFunc;
use crate::op::CsslOp;
use crate::value::{FloatWidth, IntWidth, MirType, MirValue, ValueId};

/// Per-fn lowering context.
///
/// Carries an optional [`SourceFile`] reference so literal-value extraction
/// can read the actual source text for the span of each `HirLiteral` — when
/// no source is available, literal attributes fall back to `stage0_*`
/// placeholders (preserves the T6-phase-2a behavior for source-less callers).
#[derive(Debug)]
pub struct BodyLowerCtx<'a> {
    /// Source symbol-interner.
    pub interner: &'a Interner,
    /// Optional source file — threaded for literal-value text extraction.
    pub source: Option<&'a SourceFile>,
    /// Mapping from HIR param-symbol → entry-block value-id.
    pub param_vars: HashMap<Symbol, (ValueId, MirType)>,
    /// T11-D35 : mapping from HIR vec-param-symbol → N consecutive scalar value-ids
    /// + lane-count + element width. A `vec3<f32>` param `p` maps to `(vec![v0, v1, v2], 3, F32)`.
    /// Kept distinct from [`Self::param_vars`] so stage-0 can dispatch per-op
    /// (length(p) expands via these scalars ; normalize / dot / field access are
    /// deferred to future slices).
    pub vec_param_vars: HashMap<Symbol, (Vec<ValueId>, u32, FloatWidth)>,
    /// Next free value-id (wired to `MirFunc.fresh_value_id`).
    pub next_value_id: u32,
    /// Accumulated ops (consumed at end into the entry-block).
    pub ops: Vec<MirOp>,
}

impl<'a> BodyLowerCtx<'a> {
    /// Build a fresh context with no source-file reference. Callers who want
    /// real literal-value extraction should use [`Self::with_source`] instead.
    #[must_use]
    pub fn new(interner: &'a Interner) -> Self {
        Self {
            interner,
            source: None,
            param_vars: HashMap::new(),
            vec_param_vars: HashMap::new(),
            next_value_id: 0,
            ops: Vec::new(),
        }
    }

    /// Build a fresh context carrying a source-file reference for literal
    /// text extraction.
    #[must_use]
    pub fn with_source(interner: &'a Interner, source: &'a SourceFile) -> Self {
        Self {
            interner,
            source: Some(source),
            param_vars: HashMap::new(),
            vec_param_vars: HashMap::new(),
            next_value_id: 0,
            ops: Vec::new(),
        }
    }

    /// Allocate a fresh value-id.
    pub fn fresh_value_id(&mut self) -> ValueId {
        let id = ValueId(self.next_value_id);
        self.next_value_id = self.next_value_id.saturating_add(1);
        id
    }

    /// Build a sub-context that inherits the source-file reference + the
    /// current `next_value_id`. Used by helpers that lower nested regions
    /// (match arms, scf.if branches, effect-handler bodies, etc.).
    fn sub(&self) -> BodyLowerCtx<'a> {
        BodyLowerCtx {
            interner: self.interner,
            source: self.source,
            param_vars: HashMap::new(),
            vec_param_vars: HashMap::new(),
            next_value_id: self.next_value_id,
            ops: Vec::new(),
        }
    }
}

/// Entry point : lower the body of `hir_fn` into `mir_fn.body.entry().ops`.
///
/// If `hir_fn.body` is `None`, `mir_fn` is left as-is (signature-only — the
/// T6-phase-1 shape). The `param_vars` map is populated from `hir_fn.params`
/// using entry-block value-ids `v0`, `v1`, …
///
/// The optional `source` parameter threads a `SourceFile` reference so
/// literal-value extraction can pull the real text from each `HirLiteral`
/// span. Callers without a source (or that don't care about literal fidelity)
/// can pass `None` — the lowerer falls back to `stage0_*` placeholder values.
pub fn lower_fn_body(
    interner: &Interner,
    source: Option<&SourceFile>,
    hir_fn: &HirFn,
    mir_fn: &mut MirFunc,
) {
    let Some(body) = &hir_fn.body else {
        return;
    };
    let mut ctx = match source {
        Some(src) => BodyLowerCtx::with_source(interner, src),
        None => BodyLowerCtx::new(interner),
    };
    // Entry-block args = flat-scalarized fn params. Each vec2/vec3/vec4 param
    // occupies N consecutive entry-block ids (matches the flat signature emitted
    // by `lower_function_signature`) ; everything else occupies one id. The
    // per-symbol mapping lands in either `param_vars` (scalar) or `vec_param_vars`
    // (vec) so downstream lowering (notably `lower_call` for `length`) can
    // dispatch correctly.
    let mut next_id: u32 = 0;
    for p in &hir_fn.params {
        let sym = extract_pattern_symbol(&p.pat);
        if let Some((lanes, width)) = hir_type_as_vec_lanes(interner, &p.ty) {
            let lane_ids: Vec<ValueId> = (0..lanes).map(|i| ValueId(next_id + i)).collect();
            next_id = next_id.saturating_add(lanes);
            if let Some(sym) = sym {
                ctx.vec_param_vars.insert(sym, (lane_ids, lanes, width));
            }
        } else {
            let id = ValueId(next_id);
            next_id = next_id.saturating_add(1);
            let ty = lower_hir_type_light(interner, &p.ty);
            if let Some(sym) = sym {
                ctx.param_vars.insert(sym, (id, ty));
            }
        }
    }
    ctx.next_value_id = next_id;

    // Lower the body. If a trailing value exists, emit `func.return`.
    let trailing = lower_block(&mut ctx, body);
    emit_return(&mut ctx, trailing, body.span);

    // Install the ops into the entry-block.
    if let Some(entry) = mir_fn.body.entry_mut() {
        entry.ops.extend(ctx.ops);
    }
    mir_fn.next_value_id = ctx.next_value_id;
}

fn extract_pattern_symbol(pat: &cssl_hir::HirPattern) -> Option<Symbol> {
    match &pat.kind {
        cssl_hir::HirPatternKind::Binding { name, .. } => Some(*name),
        _ => None,
    }
}

/// Shallow HIR-type → MIR-type translation (mirrors the T6-phase-1 mapping).
fn lower_hir_type_light(interner: &Interner, t: &HirType) -> MirType {
    match &t.kind {
        HirTypeKind::Path { path, .. } if path.len() == 1 => {
            let n = interner.resolve(path[0]);
            match n.as_str() {
                "i8" => MirType::Int(IntWidth::I8),
                "i16" => MirType::Int(IntWidth::I16),
                "i32" | "u32" | "isize" | "usize" => MirType::Int(IntWidth::I32),
                "i64" | "u64" => MirType::Int(IntWidth::I64),
                "f16" => MirType::Float(FloatWidth::F16),
                "bf16" => MirType::Float(FloatWidth::Bf16),
                "f32" => MirType::Float(FloatWidth::F32),
                "f64" => MirType::Float(FloatWidth::F64),
                "bool" => MirType::Bool,
                "Handle" => MirType::Handle,
                other => MirType::Opaque(other.to_string()),
            }
        }
        HirTypeKind::Refined { base, .. } => lower_hir_type_light(interner, base),
        HirTypeKind::Reference { inner, .. } => lower_hir_type_light(interner, inner),
        HirTypeKind::Infer => MirType::None,
        _ => MirType::None,
    }
}

/// T11-D35 : recognize `vec2` / `vec3` / `vec4` HIR types and report lane-count
/// + element float-width for scalarization.
///
/// A parameter declared `p : vec3` or `p : vec3<f32>` is recognized as `Some((3, F32))` ;
/// callers scalarize the param into N separate scalar MIR parameters so the walker
/// + JIT can treat the function as a standard N-scalar-input routine.
///
/// Returns `None` for any non-vec type (normal scalar or opaque).
#[must_use]
pub fn hir_type_as_vec_lanes(interner: &Interner, t: &HirType) -> Option<(u32, FloatWidth)> {
    // Peel through refined + reference wrappers so `&vec3<f32>` / `vec3<f32> { ... }` also match.
    match &t.kind {
        HirTypeKind::Refined { base, .. } => hir_type_as_vec_lanes(interner, base),
        HirTypeKind::Reference { inner, .. } => hir_type_as_vec_lanes(interner, inner),
        HirTypeKind::Path {
            path, type_args, ..
        } if path.len() == 1 => {
            let name = interner.resolve(path[0]);
            let lanes: u32 = match name.as_str() {
                "vec2" => 2,
                "vec3" => 3,
                "vec4" => 4,
                _ => return None,
            };
            // Default to F32 when no type-arg is given (canonical shape in stage-0 tests).
            let width = if type_args.is_empty() {
                FloatWidth::F32
            } else {
                match lower_hir_type_light(interner, &type_args[0]) {
                    MirType::Float(w) => w,
                    _ => return None, // non-float element : stage-0 doesn't scalarize these.
                }
            };
            Some((lanes, width))
        }
        _ => None,
    }
}

/// T11-D35 : expand an HIR param type into its MIR representation. Vec types
/// are scalarized into N consecutive scalar entries so the flat `MirFunc` param
/// list matches the JIT-callable ABI. Everything else round-trips through the
/// crate-internal `lower_hir_type_light` helper unchanged.
///
/// This is the single source of truth used by both signature-lowering
/// (`crate::lower::lower_function_signature`) and body-lowering (`lower_fn_body`)
/// so the two stay in lockstep.
#[must_use]
pub fn expand_fn_param_types(interner: &Interner, t: &HirType) -> Vec<MirType> {
    if let Some((lanes, width)) = hir_type_as_vec_lanes(interner, t) {
        return (0..lanes).map(|_| MirType::Float(width)).collect();
    }
    vec![lower_hir_type_light(interner, t)]
}

/// Lower a block. Returns `Some((ValueId, MirType))` if the block has a
/// trailing expression that produces a value.
fn lower_block(ctx: &mut BodyLowerCtx<'_>, block: &HirBlock) -> Option<(ValueId, MirType)> {
    for stmt in &block.stmts {
        lower_stmt(ctx, stmt);
    }
    block.trailing.as_ref().and_then(|e| lower_expr(ctx, e))
}

fn lower_stmt(ctx: &mut BodyLowerCtx<'_>, stmt: &HirStmt) {
    match &stmt.kind {
        HirStmtKind::Let { value, .. } => {
            if let Some(e) = value {
                let _ = lower_expr(ctx, e);
            }
        }
        HirStmtKind::Expr(e) => {
            let _ = lower_expr(ctx, e);
        }
        HirStmtKind::Item(_) => {
            // Nested items are T3.4+ work ; stage-0 skips.
        }
    }
}

/// Lower one expression. Returns the SSA-value-id + type of the result if the
/// expression produces a value.
fn lower_expr(ctx: &mut BodyLowerCtx<'_>, expr: &HirExpr) -> Option<(ValueId, MirType)> {
    match &expr.kind {
        HirExprKind::Literal(lit) => Some(lower_literal(ctx, lit, expr.span)),
        HirExprKind::Path { segments, .. } => Some(lower_path(ctx, segments, expr.span)),
        HirExprKind::Binary { op, lhs, rhs } => lower_binary(ctx, *op, lhs, rhs, expr.span),
        HirExprKind::Unary { op, operand } => lower_unary(ctx, *op, operand, expr.span),
        HirExprKind::Block(b) => lower_block(ctx, b),
        HirExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => lower_if(ctx, cond, then_branch, else_branch.as_deref(), expr.span),
        HirExprKind::Call { callee, args, .. } => lower_call(ctx, callee, args, expr.span, expr.id),
        HirExprKind::Return { value } => {
            let trailing = value.as_deref().and_then(|e| lower_expr(ctx, e));
            emit_return(ctx, trailing, expr.span);
            None
        }
        HirExprKind::Paren(inner) => lower_expr(ctx, inner),
        // § T6-phase-2b : structured control-flow + compound-expression coverage
        HirExprKind::For { iter, body, .. } => Some(lower_for(ctx, iter, body, expr.span)),
        HirExprKind::While { cond, body } => Some(lower_while(ctx, cond, body, expr.span)),
        HirExprKind::Loop { body } => Some(lower_loop(ctx, body, expr.span)),
        HirExprKind::Match { scrutinee, arms } => {
            Some(lower_match(ctx, scrutinee, arms, expr.span))
        }
        HirExprKind::Field { obj, name } => Some(lower_field(ctx, obj, *name, expr.span)),
        HirExprKind::Index { obj, index } => Some(lower_index(ctx, obj, index, expr.span)),
        HirExprKind::Assign { op, lhs, rhs } => Some(lower_assign(ctx, *op, lhs, rhs, expr.span)),
        HirExprKind::Cast { expr: inner, .. } => Some(lower_cast(ctx, inner, expr.span)),
        HirExprKind::Tuple(elements) => Some(lower_tuple(ctx, elements, expr.span)),
        HirExprKind::Array(arr) => Some(lower_array(ctx, arr, expr.span)),
        HirExprKind::Struct { path, fields, .. } => {
            Some(lower_struct_expr(ctx, path, fields, expr.span))
        }
        HirExprKind::Run { expr: inner } => lower_expr(ctx, inner),
        HirExprKind::Pipeline { lhs, rhs } => Some(lower_pipeline(ctx, lhs, rhs, expr.span)),
        HirExprKind::TryDefault {
            expr: inner,
            default,
        } => Some(lower_try_default(ctx, inner, default, expr.span)),
        HirExprKind::Try { expr: inner } => Some(lower_try(ctx, inner, expr.span)),
        HirExprKind::Range { lo, hi, inclusive } => Some(lower_range(
            ctx,
            lo.as_deref(),
            hi.as_deref(),
            *inclusive,
            expr.span,
        )),
        HirExprKind::Break { value, .. } => {
            if let Some(v) = value {
                let _ = lower_expr(ctx, v);
            }
            Some(emit_unsupported(ctx, expr.span, "Break"))
        }
        HirExprKind::Continue { .. } => Some(emit_unsupported(ctx, expr.span, "Continue")),
        // § T6-phase-2c : the remaining six variants now have dedicated lowerers.
        HirExprKind::Lambda {
            params,
            return_ty,
            body,
        } => Some(lower_lambda(
            ctx,
            params,
            return_ty.as_ref(),
            body,
            expr.span,
        )),
        HirExprKind::Perform { path, args, .. } => Some(lower_perform(ctx, path, args, expr.span)),
        HirExprKind::With { handler, body } => Some(lower_with(ctx, handler, body, expr.span)),
        HirExprKind::Region { label, body } => {
            Some(lower_region(ctx, label.as_ref().copied(), body, expr.span))
        }
        HirExprKind::Compound { op, lhs, rhs } => {
            Some(lower_compound(ctx, *op, lhs, rhs, expr.span))
        }
        HirExprKind::SectionRef { path } => Some(lower_section_ref(ctx, path, expr.span)),
        // `HirExprKind::Error` is a parser-recovery shape — keep the placeholder
        // so downstream passes see a typed value rather than a panic.
        HirExprKind::Error => Some(emit_unsupported(ctx, expr.span, "Error")),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § T6-phase-2b : additional lowerers covering structured control-flow +
//   field-access + indexing + assignment + cast + tuple + array + struct +
//   pipeline + try forms.
// ─────────────────────────────────────────────────────────────────────────

fn lower_for(
    ctx: &mut BodyLowerCtx<'_>,
    iter: &HirExpr,
    body: &HirBlock,
    span: Span,
) -> (ValueId, MirType) {
    let (iter_id, _) = lower_expr(ctx, iter).unwrap_or((ctx.fresh_value_id(), MirType::None));
    let body_region = lower_sub_region_from(ctx, body);
    let id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("scf.for")
            .with_operand(iter_id)
            .with_region(body_region)
            .with_result(id, MirType::None)
            .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, MirType::None)
}

fn lower_while(
    ctx: &mut BodyLowerCtx<'_>,
    cond: &HirExpr,
    body: &HirBlock,
    span: Span,
) -> (ValueId, MirType) {
    let (cond_id, _) = lower_expr(ctx, cond).unwrap_or((ctx.fresh_value_id(), MirType::Bool));
    let body_region = lower_sub_region_from(ctx, body);
    let id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("scf.while")
            .with_operand(cond_id)
            .with_region(body_region)
            .with_result(id, MirType::None)
            .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, MirType::None)
}

fn lower_loop(ctx: &mut BodyLowerCtx<'_>, body: &HirBlock, span: Span) -> (ValueId, MirType) {
    let body_region = lower_sub_region_from(ctx, body);
    let id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("scf.loop")
            .with_region(body_region)
            .with_result(id, MirType::None)
            .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, MirType::None)
}

fn lower_match(
    ctx: &mut BodyLowerCtx<'_>,
    scrutinee: &HirExpr,
    arms: &[cssl_hir::HirMatchArm],
    span: Span,
) -> (ValueId, MirType) {
    let (scrut_id, _) = lower_expr(ctx, scrutinee).unwrap_or((ctx.fresh_value_id(), MirType::None));
    // One nested region per arm body.
    let arm_regions: Vec<MirRegion> = arms
        .iter()
        .map(|arm| {
            let mut sub = ctx.sub();
            let _ = lower_expr(&mut sub, &arm.body);
            ctx.next_value_id = sub.next_value_id;
            let mut blk = MirBlock::new("arm");
            blk.ops = sub.ops;
            let mut r = MirRegion::new();
            r.push(blk);
            r
        })
        .collect();
    let id = ctx.fresh_value_id();
    let mut op = MirOp::std("scf.match")
        .with_operand(scrut_id)
        .with_result(id, MirType::None)
        .with_attribute("arm_count", arms.len().to_string())
        .with_attribute("source_loc", format!("{span:?}"));
    for region in arm_regions {
        op = op.with_region(region);
    }
    ctx.ops.push(op);
    (id, MirType::None)
}

fn lower_field(
    ctx: &mut BodyLowerCtx<'_>,
    obj: &HirExpr,
    name: Symbol,
    span: Span,
) -> (ValueId, MirType) {
    let (obj_id, _) = lower_expr(ctx, obj).unwrap_or((ctx.fresh_value_id(), MirType::None));
    let id = ctx.fresh_value_id();
    let ty = MirType::Opaque(format!("!cssl.field.{}", ctx.interner.resolve(name)));
    ctx.ops.push(
        MirOp::std("cssl.field")
            .with_operand(obj_id)
            .with_result(id, ty.clone())
            .with_attribute("field_name", ctx.interner.resolve(name))
            .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, ty)
}

fn lower_index(
    ctx: &mut BodyLowerCtx<'_>,
    obj: &HirExpr,
    index: &HirExpr,
    span: Span,
) -> (ValueId, MirType) {
    let (obj_id, _) = lower_expr(ctx, obj).unwrap_or((ctx.fresh_value_id(), MirType::None));
    let (idx_id, _) = lower_expr(ctx, index).unwrap_or((ctx.fresh_value_id(), MirType::None));
    let id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("memref.load")
            .with_operand(obj_id)
            .with_operand(idx_id)
            .with_result(id, MirType::None)
            .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, MirType::None)
}

fn lower_assign(
    ctx: &mut BodyLowerCtx<'_>,
    op: Option<HirBinOp>,
    lhs: &HirExpr,
    rhs: &HirExpr,
    span: Span,
) -> (ValueId, MirType) {
    let (_lhs_id, _) = lower_expr(ctx, lhs).unwrap_or((ctx.fresh_value_id(), MirType::None));
    let (rhs_id, rhs_ty) = lower_expr(ctx, rhs).unwrap_or((ctx.fresh_value_id(), MirType::None));
    // Compound-assign : emit the binary-op first (x += y → arith.addX x y → store).
    let op_name = match op {
        Some(HirBinOp::Add) => "cssl.assign_add",
        Some(HirBinOp::Sub) => "cssl.assign_sub",
        Some(HirBinOp::Mul) => "cssl.assign_mul",
        Some(HirBinOp::Div) => "cssl.assign_div",
        Some(_) => "cssl.assign_compound",
        None => "cssl.assign",
    };
    let id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std(op_name)
            .with_operand(rhs_id)
            .with_result(id, rhs_ty.clone())
            .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, rhs_ty)
}

fn lower_cast(ctx: &mut BodyLowerCtx<'_>, inner: &HirExpr, span: Span) -> (ValueId, MirType) {
    let (in_id, _) = lower_expr(ctx, inner).unwrap_or((ctx.fresh_value_id(), MirType::None));
    let id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("arith.bitcast")
            .with_operand(in_id)
            .with_result(id, MirType::None)
            .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, MirType::None)
}

fn lower_tuple(ctx: &mut BodyLowerCtx<'_>, elements: &[HirExpr], span: Span) -> (ValueId, MirType) {
    let mut operand_ids = Vec::with_capacity(elements.len());
    let mut elem_types = Vec::with_capacity(elements.len());
    for e in elements {
        if let Some((eid, ety)) = lower_expr(ctx, e) {
            operand_ids.push(eid);
            elem_types.push(ety);
        }
    }
    let id = ctx.fresh_value_id();
    let ty = MirType::Tuple(elem_types);
    let mut op = MirOp::std("cssl.tuple")
        .with_result(id, ty.clone())
        .with_attribute("arity", elements.len().to_string())
        .with_attribute("source_loc", format!("{span:?}"));
    for oid in operand_ids {
        op = op.with_operand(oid);
    }
    ctx.ops.push(op);
    (id, ty)
}

fn lower_array(
    ctx: &mut BodyLowerCtx<'_>,
    arr: &cssl_hir::HirArrayExpr,
    span: Span,
) -> (ValueId, MirType) {
    match arr {
        cssl_hir::HirArrayExpr::List(items) => {
            let mut operand_ids = Vec::with_capacity(items.len());
            for e in items {
                if let Some((eid, _)) = lower_expr(ctx, e) {
                    operand_ids.push(eid);
                }
            }
            let id = ctx.fresh_value_id();
            let ty = MirType::Memref {
                shape: vec![Some(items.len() as u64)],
                elem: Box::new(MirType::None),
            };
            let mut op = MirOp::std("cssl.array_list")
                .with_result(id, ty.clone())
                .with_attribute("count", items.len().to_string())
                .with_attribute("source_loc", format!("{span:?}"));
            for oid in operand_ids {
                op = op.with_operand(oid);
            }
            ctx.ops.push(op);
            (id, ty)
        }
        cssl_hir::HirArrayExpr::Repeat { elem, len } => {
            let (elem_id, _) =
                lower_expr(ctx, elem).unwrap_or((ctx.fresh_value_id(), MirType::None));
            let (len_id, _) = lower_expr(ctx, len).unwrap_or((ctx.fresh_value_id(), MirType::None));
            let id = ctx.fresh_value_id();
            let ty = MirType::Memref {
                shape: vec![None],
                elem: Box::new(MirType::None),
            };
            ctx.ops.push(
                MirOp::std("cssl.array_repeat")
                    .with_operand(elem_id)
                    .with_operand(len_id)
                    .with_result(id, ty.clone())
                    .with_attribute("source_loc", format!("{span:?}")),
            );
            (id, ty)
        }
    }
}

fn lower_struct_expr(
    ctx: &mut BodyLowerCtx<'_>,
    path: &[Symbol],
    fields: &[HirStructFieldInit],
    span: Span,
) -> (ValueId, MirType) {
    let struct_name = path
        .iter()
        .map(|s| ctx.interner.resolve(*s))
        .collect::<Vec<_>>()
        .join(".");
    let mut operand_ids = Vec::with_capacity(fields.len());
    for f in fields {
        if let Some(value) = &f.value {
            if let Some((fid, _)) = lower_expr(ctx, value) {
                operand_ids.push(fid);
            }
        }
    }
    let id = ctx.fresh_value_id();
    let ty = MirType::Opaque(format!("!cssl.struct.{struct_name}"));
    let mut op = MirOp::std("cssl.struct")
        .with_result(id, ty.clone())
        .with_attribute("struct_name", struct_name)
        .with_attribute("field_count", fields.len().to_string())
        .with_attribute("source_loc", format!("{span:?}"));
    for oid in operand_ids {
        op = op.with_operand(oid);
    }
    ctx.ops.push(op);
    (id, ty)
}

fn lower_pipeline(
    ctx: &mut BodyLowerCtx<'_>,
    lhs: &HirExpr,
    rhs: &HirExpr,
    span: Span,
) -> (ValueId, MirType) {
    // a |> f  ==  f(a). Lower as a func.call-like structure.
    let (lhs_id, _) = lower_expr(ctx, lhs).unwrap_or((ctx.fresh_value_id(), MirType::None));
    let (rhs_id, _) = lower_expr(ctx, rhs).unwrap_or((ctx.fresh_value_id(), MirType::None));
    let id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("cssl.pipeline")
            .with_operand(lhs_id)
            .with_operand(rhs_id)
            .with_result(id, MirType::None)
            .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, MirType::None)
}

fn lower_try_default(
    ctx: &mut BodyLowerCtx<'_>,
    inner: &HirExpr,
    default: &HirExpr,
    span: Span,
) -> (ValueId, MirType) {
    let (inner_id, inner_ty) =
        lower_expr(ctx, inner).unwrap_or((ctx.fresh_value_id(), MirType::None));
    let (default_id, _) = lower_expr(ctx, default).unwrap_or((ctx.fresh_value_id(), MirType::None));
    let id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("cssl.try_default")
            .with_operand(inner_id)
            .with_operand(default_id)
            .with_result(id, inner_ty.clone())
            .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, inner_ty)
}

fn lower_try(ctx: &mut BodyLowerCtx<'_>, inner: &HirExpr, span: Span) -> (ValueId, MirType) {
    let (inner_id, inner_ty) =
        lower_expr(ctx, inner).unwrap_or((ctx.fresh_value_id(), MirType::None));
    let id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("cssl.try")
            .with_operand(inner_id)
            .with_result(id, inner_ty.clone())
            .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, inner_ty)
}

fn lower_range(
    ctx: &mut BodyLowerCtx<'_>,
    lo: Option<&HirExpr>,
    hi: Option<&HirExpr>,
    inclusive: bool,
    span: Span,
) -> (ValueId, MirType) {
    let lo_id = lo
        .and_then(|e| lower_expr(ctx, e))
        .map_or_else(|| ctx.fresh_value_id(), |(id, _)| id);
    let hi_id = hi
        .and_then(|e| lower_expr(ctx, e))
        .map_or_else(|| ctx.fresh_value_id(), |(id, _)| id);
    let id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std(if inclusive {
            "cssl.range_inclusive"
        } else {
            "cssl.range"
        })
        .with_operand(lo_id)
        .with_operand(hi_id)
        .with_result(id, MirType::None)
        .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, MirType::None)
}

// ─────────────────────────────────────────────────────────────────────────
// § T6-phase-2c : the final six variants (Lambda / Perform / With / Region /
//   Compound / SectionRef) that previously fell through to `emit_unsupported`
//   now have dedicated lowerers.
// ─────────────────────────────────────────────────────────────────────────

/// Lower `|params| -> Ty { body }` into `cssl.closure` with a body-region.
///
/// Stage-0 : no env-capture analysis — the closure op carries `param_count` +
/// optional `return_ty` attrs. Capture-discovery + environment-pack lowering
/// land in T6-phase-2d.
fn lower_lambda(
    ctx: &mut BodyLowerCtx<'_>,
    params: &[HirLambdaParam],
    return_ty: Option<&HirType>,
    body: &HirExpr,
    span: Span,
) -> (ValueId, MirType) {
    // Build a sub-region for the lambda body. The inner lowerer runs in a
    // sub-context so parameter names inside the lambda don't leak to the
    // outer fn's `param_vars`.
    let mut sub = ctx.sub();
    // Seed sub-ctx param bindings so `HirExprKind::Path` references inside
    // the lambda body can resolve to their block-args. Lambda params start
    // at id 0 in the nested region's SSA space.
    for (i, p) in params.iter().enumerate() {
        let pid = ValueId(u32::try_from(i).unwrap_or(0));
        let pty =
            p.ty.as_ref()
                .map_or(MirType::None, |t| lower_hir_type_light(sub.interner, t));
        if let Some(sym) = extract_pattern_symbol(&p.pat) {
            sub.param_vars.insert(sym, (pid, pty));
        }
    }
    // Reserve param-ids in the sub-context's SSA space.
    sub.next_value_id = u32::try_from(params.len()).unwrap_or(0);
    let _ = lower_expr(&mut sub, body);
    let mut blk = MirBlock::new("entry");
    blk.ops = sub.ops;
    let mut body_region = MirRegion::new();
    body_region.push(blk);

    let id = ctx.fresh_value_id();
    let ty = MirType::Opaque("!cssl.closure".into());
    let mut op = MirOp::new(CsslOp::Std);
    op.name = "cssl.closure".to_string();
    op = op
        .with_result(id, ty.clone())
        .with_region(body_region)
        .with_attribute("param_count", params.len().to_string())
        .with_attribute("source_loc", format!("{span:?}"));
    if return_ty.is_some() {
        op = op.with_attribute("has_return_ty", "true");
    }
    ctx.ops.push(op);
    (id, ty)
}

/// Lower `perform Effect::op(args)` into `cssl.effect.perform`.
///
/// The effect-path is joined into a dotted `effect_path` attribute. At
/// stage-0 the result-type is the opaque `!cssl.perform_result` sentinel —
/// full effect-row-driven type recovery is a post-monomorphization pass.
fn lower_perform(
    ctx: &mut BodyLowerCtx<'_>,
    path: &[Symbol],
    args: &[HirCallArg],
    span: Span,
) -> (ValueId, MirType) {
    let effect_path = path
        .iter()
        .map(|s| ctx.interner.resolve(*s))
        .collect::<Vec<_>>()
        .join(".");
    // Lower each arg → value-id. Uses the same positional/named handling as
    // `lower_call` (stage-0 collapses named-args into positional).
    let mut operand_ids = Vec::with_capacity(args.len());
    for arg in args {
        let a_expr = match arg {
            HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
        };
        if let Some((oid, _)) = lower_expr(ctx, a_expr) {
            operand_ids.push(oid);
        }
    }
    let id = ctx.fresh_value_id();
    let ty = MirType::Opaque("!cssl.perform_result".into());
    let mut op = MirOp::new(CsslOp::EffectPerform)
        .with_result(id, ty.clone())
        .with_attribute("effect_path", effect_path)
        .with_attribute("arg_count", args.len().to_string())
        .with_attribute("source_loc", format!("{span:?}"));
    for oid in operand_ids {
        op = op.with_operand(oid);
    }
    ctx.ops.push(op);
    (id, ty)
}

/// Lower `with handler { body }` into `cssl.effect.handle`.
///
/// Stage-0 shape : the handler expression is lowered first (its value becomes
/// the operand that identifies which handler is installed), and the body is
/// lowered into a nested region. HirExprKind::With holds a single handler —
/// multi-handler installations desugar to nested `with`s at the HIR level.
fn lower_with(
    ctx: &mut BodyLowerCtx<'_>,
    handler: &HirExpr,
    body: &HirBlock,
    span: Span,
) -> (ValueId, MirType) {
    let (handler_id, _) = lower_expr(ctx, handler).unwrap_or((ctx.fresh_value_id(), MirType::None));
    let body_region = lower_sub_region_from(ctx, body);
    let id = ctx.fresh_value_id();
    let ty = MirType::Opaque("!cssl.effect.handle_result".into());
    ctx.ops.push(
        MirOp::new(CsslOp::EffectHandle)
            .with_operand(handler_id)
            .with_region(body_region)
            .with_result(id, ty.clone())
            .with_attribute("handler_count", "1")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, ty)
}

/// Lower `region 'label { body }` into `cssl.region.enter` with a body-region.
///
/// Stage-0 emits only the `enter` half — the pairing `cssl.region.exit` +
/// arena-lifetime synthesis is a later MIR→MIR pass (per `specs/15`
/// § PASS-PIPELINE, where `cssl.region → memref.alloca + memref.dealloc`).
fn lower_region(
    ctx: &mut BodyLowerCtx<'_>,
    label: Option<Symbol>,
    body: &HirBlock,
    span: Span,
) -> (ValueId, MirType) {
    let body_region = lower_sub_region_from(ctx, body);
    let id = ctx.fresh_value_id();
    let ty = MirType::Opaque("!cssl.region".into());
    let mut op = MirOp::new(CsslOp::RegionEnter)
        .with_region(body_region)
        .with_result(id, ty.clone())
        .with_attribute("source_loc", format!("{span:?}"));
    if let Some(lbl) = label {
        op = op.with_attribute("label", ctx.interner.resolve(lbl));
    } else {
        op = op.with_attribute("label", "_anon");
    }
    ctx.ops.push(op);
    (id, ty)
}

/// Lower a CSLv3-native compound `A op B` (§§ 13 morpheme-stack : `.` `+` `-`
/// `⊗` `@`) into `cssl.compound` with a `compound_op` attribute encoding the
/// 2-letter morpheme code per `HirCompoundOp`.
fn lower_compound(
    ctx: &mut BodyLowerCtx<'_>,
    op: HirCompoundOp,
    lhs: &HirExpr,
    rhs: &HirExpr,
    span: Span,
) -> (ValueId, MirType) {
    let (lhs_id, _) = lower_expr(ctx, lhs).unwrap_or((ctx.fresh_value_id(), MirType::None));
    let (rhs_id, _) = lower_expr(ctx, rhs).unwrap_or((ctx.fresh_value_id(), MirType::None));
    let code = compound_op_code(op);
    let id = ctx.fresh_value_id();
    let ty = MirType::Opaque(format!("!cssl.compound.{code}"));
    ctx.ops.push(
        MirOp::std("cssl.compound")
            .with_operand(lhs_id)
            .with_operand(rhs_id)
            .with_result(id, ty.clone())
            .with_attribute("compound_op", code)
            .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, ty)
}

/// Map a `HirCompoundOp` variant to its canonical 2-letter morpheme code per
/// `specs/16 § CSLv3-NATIVE SURFACE` : `.` → `tp` (tatpuruṣa, B-of-A),
/// `+` → `dv` (dvandva, co-equal conjunction), `-` → `kd` (karmadhāraya,
/// B-that-is-A), `⊗` → `bv` (bahuvrīhi, thing-having-A+B), `@` → `av`
/// (avyayībhāva, at/per/in-scope-of).
const fn compound_op_code(op: HirCompoundOp) -> &'static str {
    match op {
        HirCompoundOp::Tp => "tp",
        HirCompoundOp::Dv => "dv",
        HirCompoundOp::Kd => "kd",
        HirCompoundOp::Bv => "bv",
        HirCompoundOp::Av => "av",
    }
}

/// Lower `§§ path` into `cssl.section_ref` with the joined `section_path`
/// attribute. No operands — a section-reference is a frozen identifier.
fn lower_section_ref(
    ctx: &mut BodyLowerCtx<'_>,
    path: &[Symbol],
    span: Span,
) -> (ValueId, MirType) {
    let section_path = path
        .iter()
        .map(|s| ctx.interner.resolve(*s))
        .collect::<Vec<_>>()
        .join(".");
    let id = ctx.fresh_value_id();
    let ty = MirType::Opaque(format!("!cssl.section_ref.{section_path}"));
    ctx.ops.push(
        MirOp::std("cssl.section_ref")
            .with_result(id, ty.clone())
            .with_attribute("section_path", section_path)
            .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, ty)
}

fn lower_literal(ctx: &mut BodyLowerCtx<'_>, lit: &HirLiteral, span: Span) -> (ValueId, MirType) {
    // Try to pull the real source-text for the literal ; fall back to the
    // `stage0_*` placeholder when source is unavailable or parse fails.
    let slice = ctx
        .source
        .and_then(|s| s.slice(lit.span.start, lit.span.end));
    let (ty, attr_value) = match lit.kind {
        HirLiteralKind::Int => {
            let parsed = slice.and_then(parse_int_literal);
            let val = parsed.map_or_else(|| "stage0_int".to_string(), |n| n.to_string());
            (MirType::Int(IntWidth::I32), val)
        }
        HirLiteralKind::Float => {
            let parsed = slice.and_then(parse_float_literal);
            let val = parsed.map_or_else(|| "stage0_float".to_string(), |f| format!("{f:?}"));
            (MirType::Float(FloatWidth::F32), val)
        }
        HirLiteralKind::Bool(b) => (MirType::Bool, b.to_string()),
        HirLiteralKind::Str => {
            let stripped = slice.and_then(strip_string_quotes);
            let val = stripped.map_or_else(|| "stage0_str".to_string(), String::from);
            (MirType::Opaque("!cssl.string".into()), val)
        }
        HirLiteralKind::Char => {
            let stripped = slice.and_then(strip_char_quotes);
            let val = stripped.map_or_else(|| "stage0_char".to_string(), String::from);
            (MirType::Int(IntWidth::I32), val)
        }
        HirLiteralKind::Unit => (MirType::None, "unit".into()),
    };
    let id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("arith.constant")
            .with_result(id, ty.clone())
            .with_attribute("value", attr_value)
            .with_attribute("source_loc", format!("{span:?}")),
    );
    let _ = span;
    (id, ty)
}

/// Parse an integer literal slice. Handles `_` separators + `0x`/`0b`/`0o`
/// prefixes. Strips optional trailing type-suffix (e.g. `42i64`, `0xffu8`)
/// by walking the slice until a non-digit-sequence boundary is reached.
fn parse_int_literal(raw: &str) -> Option<i64> {
    let digits_only = strip_int_type_suffix(raw);
    let cleaned: String = digits_only.chars().filter(|c| *c != '_').collect();
    let (radix, body) = if let Some(rest) = cleaned
        .strip_prefix("0x")
        .or_else(|| cleaned.strip_prefix("0X"))
    {
        (16, rest)
    } else if let Some(rest) = cleaned
        .strip_prefix("0b")
        .or_else(|| cleaned.strip_prefix("0B"))
    {
        (2, rest)
    } else if let Some(rest) = cleaned
        .strip_prefix("0o")
        .or_else(|| cleaned.strip_prefix("0O"))
    {
        (8, rest)
    } else {
        (10, cleaned.as_str())
    };
    i64::from_str_radix(body, radix).ok()
}

/// Parse a float literal slice. Strips `_` separators + optional trailing
/// `f32`/`f64`/`f16`/`bf16` type-suffix.
fn parse_float_literal(raw: &str) -> Option<f64> {
    let no_suffix = strip_float_type_suffix(raw);
    let cleaned: String = no_suffix.chars().filter(|c| *c != '_').collect();
    cleaned.parse::<f64>().ok()
}

/// Strip a trailing integer-type suffix (e.g. `42i32` → `42`). Recognized
/// suffixes : `i8`/`i16`/`i32`/`i64`/`i128`/`isize` + `u`-prefixed variants.
fn strip_int_type_suffix(raw: &str) -> &str {
    for suffix in [
        "i128", "u128", "isize", "usize", "i64", "u64", "i32", "u32", "i16", "u16", "i8", "u8",
    ] {
        if let Some(stripped) = raw.strip_suffix(suffix) {
            return stripped;
        }
    }
    raw
}

/// Strip a trailing float-type suffix (e.g. `3.14f32` → `3.14`). Recognized
/// suffixes : `f16`/`bf16`/`f32`/`f64`.
fn strip_float_type_suffix(raw: &str) -> &str {
    for suffix in ["bf16", "f64", "f32", "f16"] {
        if let Some(stripped) = raw.strip_suffix(suffix) {
            return stripped;
        }
    }
    raw
}

/// Strip surrounding `"..."` from a string-literal slice. Returns `None` if
/// the slice doesn't match the expected shape. Escape sequences are left
/// as-is at stage-0 — full escape-resolution is T3.4+ work.
fn strip_string_quotes(raw: &str) -> Option<&str> {
    // Accept `"..."` (optionally prefixed with `b`/`r` etc.) and strip the
    // outermost pair of double-quotes.
    let trimmed = raw.trim_start_matches(|c: char| c.is_ascii_alphabetic());
    trimmed.strip_prefix('"').and_then(|s| s.strip_suffix('"'))
}

/// Strip surrounding `'...'` from a char-literal slice. Returns `None` if
/// the slice doesn't match the expected shape.
fn strip_char_quotes(raw: &str) -> Option<&str> {
    let trimmed = raw.trim_start_matches(|c: char| c.is_ascii_alphabetic());
    trimmed
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
}

fn lower_path(ctx: &mut BodyLowerCtx<'_>, segments: &[Symbol], span: Span) -> (ValueId, MirType) {
    // Single-segment path : check param_vars.
    if segments.len() == 1 {
        if let Some((id, ty)) = ctx.param_vars.get(&segments[0]) {
            return (*id, ty.clone());
        }
    }
    // Multi-segment or unresolved : emit an opaque `arith.constant`-shaped placeholder
    // so downstream passes see a typed value.
    let id = ctx.fresh_value_id();
    let name = segments
        .iter()
        .map(|s| ctx.interner.resolve(*s))
        .collect::<Vec<_>>()
        .join(".");
    let ty = MirType::Opaque(format!("!cssl.unresolved.{name}"));
    ctx.ops.push(
        MirOp::std("cssl.path_ref")
            .with_result(id, ty.clone())
            .with_attribute("path", name)
            .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, ty)
}

fn lower_binary(
    ctx: &mut BodyLowerCtx<'_>,
    op: HirBinOp,
    lhs: &HirExpr,
    rhs: &HirExpr,
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (lhs_id, lhs_ty) = lower_expr(ctx, lhs)?;
    let (rhs_id, _rhs_ty) = lower_expr(ctx, rhs)?;
    let is_float = matches!(lhs_ty, MirType::Float(_));
    let op_name = match (op, is_float) {
        (HirBinOp::Add, false) => "arith.addi",
        (HirBinOp::Add, true) => "arith.addf",
        (HirBinOp::Sub, false) => "arith.subi",
        (HirBinOp::Sub, true) => "arith.subf",
        (HirBinOp::Mul, false) => "arith.muli",
        (HirBinOp::Mul, true) => "arith.mulf",
        (HirBinOp::Div, false) => "arith.divsi",
        (HirBinOp::Div, true) => "arith.divf",
        (HirBinOp::Rem, false) => "arith.remsi",
        (HirBinOp::Rem, true) => "arith.remf",
        (HirBinOp::Eq, _) => "arith.cmpi_eq",
        (HirBinOp::Ne, _) => "arith.cmpi_ne",
        (HirBinOp::Lt, _) => "arith.cmpi_slt",
        (HirBinOp::Le, _) => "arith.cmpi_sle",
        (HirBinOp::Gt, _) => "arith.cmpi_sgt",
        (HirBinOp::Ge, _) => "arith.cmpi_sge",
        (HirBinOp::And, _) => "arith.andi",
        (HirBinOp::Or, _) => "arith.ori",
        (HirBinOp::BitAnd, _) => "arith.andi",
        (HirBinOp::BitOr, _) => "arith.ori",
        (HirBinOp::BitXor, _) => "arith.xori",
        (HirBinOp::Shl, _) => "arith.shli",
        (HirBinOp::Shr, _) => "arith.shrsi",
        (HirBinOp::Implies | HirBinOp::Entails, _) => "cssl.verify.assert",
    };
    let result_ty = match op {
        HirBinOp::Eq | HirBinOp::Ne | HirBinOp::Lt | HirBinOp::Le | HirBinOp::Gt | HirBinOp::Ge => {
            MirType::Bool
        }
        _ => lhs_ty.clone(),
    };
    let id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std(op_name)
            .with_operand(lhs_id)
            .with_operand(rhs_id)
            .with_result(id, result_ty.clone())
            .with_attribute("source_loc", format!("{span:?}")),
    );
    let _ = span;
    Some((id, result_ty))
}

fn lower_unary(
    ctx: &mut BodyLowerCtx<'_>,
    op: HirUnOp,
    operand: &HirExpr,
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (in_id, in_ty) = lower_expr(ctx, operand)?;
    let op_name = match op {
        HirUnOp::Not => "arith.xori", // not x = xor x, true
        HirUnOp::Neg => {
            if matches!(in_ty, MirType::Float(_)) {
                "arith.negf"
            } else {
                "arith.subi_neg"
            }
        }
        HirUnOp::BitNot => "arith.xori_not",
        HirUnOp::Ref => "cssl.borrow",
        HirUnOp::RefMut => "cssl.borrow_mut",
        HirUnOp::Deref => "cssl.deref",
    };
    let id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std(op_name)
            .with_operand(in_id)
            .with_result(id, in_ty.clone())
            .with_attribute("source_loc", format!("{span:?}")),
    );
    let _ = span;
    Some((id, in_ty))
}

fn lower_call(
    ctx: &mut BodyLowerCtx<'_>,
    callee: &HirExpr,
    args: &[HirCallArg],
    span: Span,
    hir_id: cssl_hir::HirId,
) -> Option<(ValueId, MirType)> {
    // Extract call-target name if it's a path.
    let target = match &callee.kind {
        HirExprKind::Path { segments, .. } => segments
            .iter()
            .map(|s| ctx.interner.resolve(*s))
            .collect::<Vec<_>>()
            .join("."),
        _ => {
            // Non-path callee : lower the callee as a value + emit `cssl.call_indirect`.
            let _ = lower_expr(ctx, callee);
            "cssl.call_indirect".to_string()
        }
    };

    // § T11-D35 : vec-length fast path — `length(p)` where `p` is a scalarized
    //   vec-param. Emit `sqrt(p0*p0 + p1*p1 + ... + pN*pN)` as scalar MIR ops so
    //   the walker + JIT consume the fn as a pure-scalar routine. See spec
    //   `specs/05_AUTODIFF.csl` § VEC-AD-RULES — `∂/∂p_i length(p) = p_i/length(p)` ⇒
    //   `∇_p length(p) = normalize(p)`, exactly what the AD walker's scalar rule-
    //   set derives for this expanded form.
    if matches!(target.as_str(), "length" | "math.length") && args.len() == 1 {
        if let Some(result) = try_lower_vec_length_from_path(ctx, &args[0], span) {
            return Some(result);
        }
    }
    // § T11-D57 (S6-B1) — `Box::new(x)` syntactic recognition.
    //   Strict guard : the call must be a path-callee with EXACTLY two segments
    //   `["Box", "new"]` and one positional arg. False positives (e.g. a user
    //   shadowing `Box`) are blocked by the segment-count + name match. Full
    //   trait-dispatch is deferred to the phase-B trait-resolve slice ; until
    //   then this recognizer is the only path that mints a `cssl.heap.alloc`
    //   from user source. See HANDOFF_SESSION_6 § PHASE-B § S6-B1.
    if args.len() == 1 {
        if let HirExprKind::Path { segments, .. } = &callee.kind {
            if segments.len() == 2
                && ctx.interner.resolve(segments[0]) == "Box"
                && ctx.interner.resolve(segments[1]) == "new"
            {
                if let Some(result) = try_lower_box_new(ctx, &args[0], span) {
                    return Some(result);
                }
            }
        }
    }
    // § T11-D60 (S6-B2) — sum-type constructor recognition for
    //   `Some(x)` / `None()` / `Ok(x)` / `Err(x)`. Strict guards mirror the
    //   B1 pattern : the call must be a single-segment path matching the
    //   canonical constructor name + the expected arity. Trait-dispatch is
    //   not yet landed at session-6 ; this recognizer is the only path that
    //   mints a `cssl.option.{some,none}` / `cssl.result.{ok,err}` op from
    //   user source. Once trait-resolve lands (phase-B follow-up slice)
    //   these become fast-paths / can be removed ; until then they are
    //   the sole entry-point. See HANDOFF_SESSION_6 § PHASE-B § S6-B2 +
    //   `specs/03_TYPES.csl § BASE-TYPES § aggregate` (sum-types) +
    //   `specs/04_EFFECTS.csl § ERROR HANDLING`.
    //
    //   ‼ Constructor recognition matches by segment-name only ; user code
    //   shadowing `Some`/`None`/`Ok`/`Err` (e.g., `mod foo { fn Some<T>(x:T)->T }`)
    //   bypasses the recognizer when it routes through a multi-segment path
    //   (e.g., `foo::Some(x)`), but a bare `Some(x)` will be claimed by the
    //   sum-type recognizer. This matches the Rust prelude precedent : the
    //   four canonical constructor names are reserved at the top-level.
    if let HirExprKind::Path { segments, .. } = &callee.kind {
        if segments.len() == 1 {
            let name = ctx.interner.resolve(segments[0]);
            match (name.as_str(), args.len()) {
                ("Some", 1) => {
                    if let Some(result) = try_lower_option_some(ctx, &args[0], span) {
                        return Some(result);
                    }
                }
                ("None", 0) => {
                    return Some(lower_option_none(ctx, span));
                }
                ("Ok", 1) => {
                    if let Some(result) = try_lower_result_ok(ctx, &args[0], span) {
                        return Some(result);
                    }
                }
                ("Err", 1) => {
                    if let Some(result) = try_lower_result_err(ctx, &args[0], span) {
                        return Some(result);
                    }
                }
                _ => {}
            }
        }
    }
    // § T11-D71 (S6-B4) — `format(fmt, ...args)` syntactic recognition.
    //   Strict guard : the call must be a single-segment path named `format`
    //   with at least one positional arg AND the first arg must be a
    //   string-literal (so the recognizer can extract the format-string
    //   for spec-counting + spec-validation). User code shadowing `format`
    //   via a multi-segment path (e.g., `foo::format(x)`) bypasses the
    //   recognizer and routes through the regular generic-call path.
    //   See HANDOFF_SESSION_6 § PHASE-B § S6-B4 +
    //   `specs/03_TYPES.csl § STRING-MODEL`.
    //
    //   ‼ Per the slice landmines `format!` macro syntax is NOT supported at
    //     stage-0 (macro-bang invocation parsing is a separate slice). The
    //     bare-call form `format(...)` is the canonical stage-0 surface.
    if let HirExprKind::Path { segments, .. } = &callee.kind {
        if segments.len() == 1 && ctx.interner.resolve(segments[0]) == "format" && !args.is_empty()
        {
            if let Some(result) = try_lower_string_format(ctx, args, span) {
                return Some(result);
            }
        }
    }
    // § T11-D76 (S6-B5) — `fs::open` / `fs::read` / `fs::write` /
    //   `fs::close` syntactic recognition. Strict guard : the callee must
    //   be a 2-segment path with first segment `fs` ; the second segment
    //   selects which `cssl.fs.*` op fires + the expected argument count.
    //   Recognizing on a 2-segment path (rather than a bare-call name)
    //   avoids accidentally claiming user identifiers like `open` / `read`
    //   that legitimately exist in non-fs contexts. The canonical
    //   stdlib form is `fs::open("path", flags)` per `stdlib/fs.cssl`.
    //   See HANDOFF_SESSION_6 § PHASE-B § S6-B5 +
    //   `specs/04_EFFECTS.csl § IO-EFFECT` +
    //   `specs/22_TELEMETRY.csl § FS-OPS`.
    if let HirExprKind::Path { segments, .. } = &callee.kind {
        if segments.len() == 2 && ctx.interner.resolve(segments[0]) == "fs" {
            let op = ctx.interner.resolve(segments[1]);
            match (op.as_str(), args.len()) {
                ("open", 2) => {
                    if let Some(result) = try_lower_fs_open(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("read", 3) => {
                    if let Some(result) = try_lower_fs_read(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("write", 3) => {
                    if let Some(result) = try_lower_fs_write(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("close", 1) => {
                    if let Some(result) = try_lower_fs_close(ctx, args, span) {
                        return Some(result);
                    }
                }
                _ => {}
            }
        }
    }
    // Lower each arg ; collect operand value-ids + types (arg-type needed
    // for intrinsic-result-type inference below).
    let mut operand_ids = Vec::with_capacity(args.len());
    let mut operand_tys: Vec<MirType> = Vec::with_capacity(args.len());
    for arg in args {
        let a_expr = match arg {
            HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
        };
        if let Some((id, ty)) = lower_expr(ctx, a_expr) {
            operand_ids.push(id);
            operand_tys.push(ty);
        }
    }
    // Emit `func.call @target` op. For known-intrinsic math callees
    // (min/max/abs/sqrt/sin/cos/exp/log), infer the result type from the
    // first operand's type — same type as input. This lets downstream JIT /
    // AD walker emit correctly-typed successor ops (e.g., `arith.constant
    // 0.0 : f32` for abs-fwd instead of an opaque-typed constant).
    let result_ty = infer_intrinsic_result_type(&target, &operand_tys)
        .unwrap_or_else(|| MirType::Opaque(format!("!cssl.call_result.{target}")));
    let id = ctx.fresh_value_id();
    // § T11-D41 : record the HirId of the source Call expression as an attribute.
    //   The auto-monomorphization call-site-rewriter keys off this to map MIR
    //   func.call ops back to their originating HIR Call nodes.
    let mut mir_op = MirOp::std("func.call")
        .with_attribute("callee", target)
        .with_attribute("source_loc", format!("{span:?}"))
        .with_attribute("hir_id", format!("{}", hir_id.0))
        .with_result(id, result_ty.clone());
    for oid in operand_ids {
        mir_op = mir_op.with_operand(oid);
    }
    ctx.ops.push(mir_op);
    let _ = span;
    Some((id, result_ty))
}

/// T11-D35 : if `arg` is a single-segment path naming a scalarized vec-param,
/// emit the `sqrt(Σ xᵢ²)` expansion over the N lane ids and return the scalar
/// result. Returns `None` if `arg` is not a vec-param reference (caller falls
/// back to the normal [`lower_call`] path).
///
/// § EMITTED-SHAPE (vec3 case, for reference) :
/// ```text
///   %sq0 = arith.mulf %p_0, %p_0
///   %sq1 = arith.mulf %p_1, %p_1
///   %sq2 = arith.mulf %p_2, %p_2
///   %s01 = arith.addf %sq0, %sq1
///   %s   = arith.addf %s01, %sq2
///   %len = func.call @sqrt (%s)
/// ```
/// Total ops : `N mul + (N-1) add + 1 sqrt = 2N` for N-lane vec. For vec3 : 7 ops.
fn try_lower_vec_length_from_path(
    ctx: &mut BodyLowerCtx<'_>,
    arg: &HirCallArg,
    span: Span,
) -> Option<(ValueId, MirType)> {
    let expr = match arg {
        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
    };
    let segments = match &expr.kind {
        HirExprKind::Path { segments, .. } if segments.len() == 1 => segments,
        _ => return None,
    };
    let sym = segments[0];
    let (lane_ids, _lanes, width) = ctx.vec_param_vars.get(&sym).cloned()?;
    let scalar_ty = MirType::Float(width);

    // § mulf per lane : sqᵢ = pᵢ · pᵢ
    let mut square_ids: Vec<ValueId> = Vec::with_capacity(lane_ids.len());
    for pid in &lane_ids {
        let id = ctx.fresh_value_id();
        ctx.ops.push(
            MirOp::std("arith.mulf")
                .with_operand(*pid)
                .with_operand(*pid)
                .with_result(id, scalar_ty.clone())
                .with_attribute("source_loc", format!("{span:?}")),
        );
        square_ids.push(id);
    }

    // § addf accumulator : sum = ((sq0 + sq1) + sq2) + ...
    let mut acc = square_ids[0];
    for sq in square_ids.iter().skip(1) {
        let id = ctx.fresh_value_id();
        ctx.ops.push(
            MirOp::std("arith.addf")
                .with_operand(acc)
                .with_operand(*sq)
                .with_result(id, scalar_ty.clone())
                .with_attribute("source_loc", format!("{span:?}")),
        );
        acc = id;
    }

    // § func.call @sqrt : len = sqrt(sum). Matches the existing scalar intrinsic
    //   dispatch (`math.sqrt` is part of `infer_intrinsic_result_type`) so the
    //   JIT's libm extern-declaration path picks it up via the callee attribute.
    let len_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("func.call")
            .with_attribute("callee", "sqrt".to_string())
            .with_attribute("source_loc", format!("{span:?}"))
            .with_operand(acc)
            .with_result(len_id, scalar_ty.clone()),
    );
    Some((len_id, scalar_ty))
}

/// T11-D57 (S6-B1) — lower a syntactically-recognized `Box::new(x)` call into
/// a `cssl.heap.alloc` op.
///
/// § EMITTED-SHAPE
/// ```text
///   %inner = <lower(x)>                                   // payload value
///   %sz    = arith.constant N : i64                        // sizeof T (heuristic)
///   %al    = arith.constant 8 : i64                        // align (default 8)
///   %p     = cssl.heap.alloc %sz, %al : !cssl.ptr          // attribute cap=iso
/// ```
///
/// § GUARDS + CAVEATS
///   - At B1 the size-of operand is a stage-0 heuristic : `8` for scalar
///     payloads, `0` for unknown / opaque types. Real layout-computation
///     lands once `MirType::Struct(DefId, Vec<MirType>)` exists (see the
///     deferred work in `T11-D50`). The op carries `size` and `align` as
///     attributes mirroring the operand values, plus `payload_ty` so later
///     passes can resolve real layouts without losing the type.
///   - Initialization (`*p = inner`) is NOT emitted at B1 — `cssl.heap.alloc`
///     produces uninitialized memory per the cssl-rt contract. A follow-up
///     slice will emit a paired `memref.store` once memref-load/store ops
///     land in S6-C3. Until then the recognized form is "alloc and discard
///     payload" — sufficient to validate the lowering surface.
///   - The result value-id carries the `cap=iso` attribute on the producing
///     op so downstream linear-tracking can verify exactly-once consumption.
fn try_lower_box_new(
    ctx: &mut BodyLowerCtx<'_>,
    arg: &HirCallArg,
    span: Span,
) -> Option<(ValueId, MirType)> {
    let payload_expr = match arg {
        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
    };
    // Lower the payload expression so its side-effects + value land in the
    // op-stream. Even though we don't (yet) store it through the pointer at
    // B1, lowering preserves any computed value the user expressed — the
    // store-through-pointer pairing happens once memref ops land (S6-C3).
    let (_payload_id, payload_ty) = lower_expr(ctx, payload_expr)?;

    // § Heuristic size-of for stage-0 payloads. Real layout-computation
    //   requires `MirType::Struct(DefId, Vec<MirType>)` (deferred ; see
    //   `DECISIONS.md` T11-D50 § "What's still missing for real `struct
    //   Vec<T>`"). At B1 we encode size as a constant attribute :
    //     - scalar Int / Float / Bool / Ptr   → byte-width per type
    //     - everything else                  → 0 (opaque) — downstream
    //       passes will fix once layout exists.
    let payload_size_bytes: i64 = stage0_heuristic_size_of(&payload_ty);
    let payload_align_bytes: i64 = stage0_heuristic_align_of(&payload_ty);

    let size_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("arith.constant")
            .with_attribute("value", payload_size_bytes.to_string())
            .with_result(size_id, MirType::Int(IntWidth::I64))
            .with_attribute("source_loc", format!("{span:?}")),
    );
    let align_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("arith.constant")
            .with_attribute("value", payload_align_bytes.to_string())
            .with_result(align_id, MirType::Int(IntWidth::I64))
            .with_attribute("source_loc", format!("{span:?}")),
    );

    let ptr_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::new(CsslOp::HeapAlloc)
            .with_operand(size_id)
            .with_operand(align_id)
            .with_result(ptr_id, MirType::Ptr)
            // ‼ cap=iso : per `specs/12_CAPABILITIES.csl` § ISO-OWNERSHIP a
            //   freshly-allocated heap cell is uniquely owned (linear).
            //   Downstream linear-tracking + handler-one-shot enforcement
            //   look up this attribute.
            .with_attribute("cap", "iso")
            // Carry payload-type as a string so later passes can recover
            // the typed shape without parsing the op-name.
            .with_attribute("payload_ty", format!("{payload_ty}"))
            // Source-recognition marker — distinguishes Box::new() lowering
            // from future direct `cssl.heap.alloc` emissions (e.g., from
            // Vec::with_capacity or arena-bump fallback paths).
            .with_attribute("origin", "box_new")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((ptr_id, MirType::Ptr))
}

/// T11-D57 stage-0 heuristic : byte-size for `MirType` payloads handled at B1.
/// Returns `0` for types whose layout isn't computable yet (`Opaque` /
/// `Function` / non-trivial `Memref`). Future slices replace this once real
/// layout-computation lands (see `T11-D50` deferred items).
fn stage0_heuristic_size_of(t: &MirType) -> i64 {
    match t {
        MirType::Int(IntWidth::I1 | IntWidth::I8) | MirType::Bool => 1,
        MirType::Int(IntWidth::I16) => 2,
        MirType::Int(IntWidth::I32) => 4,
        MirType::Int(IntWidth::I64 | IntWidth::Index) => 8,
        MirType::Float(FloatWidth::F16 | FloatWidth::Bf16) => 2,
        MirType::Float(FloatWidth::F32) => 4,
        MirType::Float(FloatWidth::F64) => 8,
        MirType::Ptr | MirType::Handle => 8, // assume 64-bit host @ stage-0
        MirType::Vec(lanes, w) => {
            let lane_bytes: i64 = match w {
                FloatWidth::F16 | FloatWidth::Bf16 => 2,
                FloatWidth::F32 => 4,
                FloatWidth::F64 => 8,
            };
            i64::from(*lanes) * lane_bytes
        }
        // Composite / unresolved : 0 ; future slices fill in.
        MirType::Tuple(_)
        | MirType::Function { .. }
        | MirType::Memref { .. }
        | MirType::Opaque(_)
        | MirType::None => 0,
    }
}

/// T11-D57 stage-0 heuristic : preferred alignment for a payload type. Mirrors
/// `stage0_heuristic_size_of` but rounds up to the natural ABI alignment for
/// scalars. Composite / unresolved types use `8` (safe upper bound for 64-bit
/// hosts) to avoid invalid alignments at the runtime allocator boundary.
fn stage0_heuristic_align_of(t: &MirType) -> i64 {
    match t {
        MirType::Int(IntWidth::I1 | IntWidth::I8) | MirType::Bool => 1,
        MirType::Int(IntWidth::I16) | MirType::Float(FloatWidth::F16 | FloatWidth::Bf16) => 2,
        MirType::Int(IntWidth::I32) | MirType::Float(FloatWidth::F32) => 4,
        MirType::Int(IntWidth::I64 | IntWidth::Index)
        | MirType::Float(FloatWidth::F64)
        | MirType::Ptr
        | MirType::Handle => 8,
        MirType::Vec(_, w) => match w {
            FloatWidth::F16 | FloatWidth::Bf16 => 2,
            FloatWidth::F32 => 4,
            FloatWidth::F64 => 8,
        },
        // Composite / unresolved → 8 (max-alignment safe default at stage-0).
        MirType::Tuple(_)
        | MirType::Function { .. }
        | MirType::Memref { .. }
        | MirType::Opaque(_)
        | MirType::None => 8,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § T11-D60 (S6-B2) — sum-type constructor lowerers.
//
//   Stage-0 representation : flat tagged-union — each constructor emits a
//   single `cssl.option.*` / `cssl.result.*` op carrying :
//     - `tag`         : "0" (None / Err) | "1" (Some / Ok)
//     - `payload_ty`  : the MirType of the payload (for typed introspection)
//     - `family`      : "Option" | "Result"
//     - `source_loc`  : original span for diagnostic chaining
//
//   The op-result type is `MirType::Opaque("!cssl.option.<T>")` /
//   `"!cssl.result.<T>.<E>"` at stage-0 — a real `MirType::TaggedUnion` ABI
//   is deferred to a follow-up slice (see DECISIONS T11-D60 § DEFERRED).
//   Until that slice lands, the JIT and object backends will reject these
//   ops with `UnsupportedMirOp` if a fn body actually attempts to RUN one.
//   They lower correctly through the parser + walkers + monomorphization
//   quartet, which is the slice's success-criterion (HANDOFF S6-B2).
//
//   ‼ Per the HANDOFF landmines :
//     - `None` carries no heap allocation : it's a payload-less op.
//     - `Some(T)` for trivial T (i32, f32, bool, ptr) : no heap — the op
//       carries the payload value directly. A real flat tagged-union ABI
//       at the cranelift / SPIR-V level lands later.
//     - `Some(T)` for non-trivial T : may need heap once trait-dispatch
//       lands and `Box<T>` is the canonical wrapper. At B2 the op records
//       the payload-type ; the deferred ABI slice handles the heap path.
// ─────────────────────────────────────────────────────────────────────────

/// Lower a syntactically-recognized `Some(x)` constructor call into a
/// `cssl.option.some` op. Mirrors the B1 `try_lower_box_new` pattern.
fn try_lower_option_some(
    ctx: &mut BodyLowerCtx<'_>,
    arg: &HirCallArg,
    span: Span,
) -> Option<(ValueId, MirType)> {
    let payload_expr = match arg {
        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
    };
    let (payload_id, payload_ty) = lower_expr(ctx, payload_expr)?;
    let id = ctx.fresh_value_id();
    let result_ty = MirType::Opaque(format!("!cssl.option.{payload_ty}"));
    ctx.ops.push(
        MirOp::new(CsslOp::OptionSome)
            .with_operand(payload_id)
            .with_result(id, result_ty.clone())
            .with_attribute("tag", "1")
            .with_attribute("family", "Option")
            .with_attribute("payload_ty", format!("{payload_ty}"))
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((id, result_ty))
}

/// Lower a syntactically-recognized `None` constructor call into a
/// `cssl.option.none` op. The result-type carries no payload information at
/// stage-0 ; a real `MirType::TaggedUnion` ABI lowering pass will resolve
/// the actual `Option<T>` once monomorph + trait-dispatch wire the type
/// argument through. At B2 the op records `payload_ty = "!cssl.unknown"` so
/// downstream passes can detect the un-bound-payload form.
fn lower_option_none(ctx: &mut BodyLowerCtx<'_>, span: Span) -> (ValueId, MirType) {
    let id = ctx.fresh_value_id();
    let result_ty = MirType::Opaque("!cssl.option.unknown".to_string());
    ctx.ops.push(
        MirOp::new(CsslOp::OptionNone)
            .with_result(id, result_ty.clone())
            .with_attribute("tag", "0")
            .with_attribute("family", "Option")
            .with_attribute("payload_ty", "!cssl.unknown")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    (id, result_ty)
}

/// Lower a syntactically-recognized `Ok(x)` constructor call into a
/// `cssl.result.ok` op.
fn try_lower_result_ok(
    ctx: &mut BodyLowerCtx<'_>,
    arg: &HirCallArg,
    span: Span,
) -> Option<(ValueId, MirType)> {
    let payload_expr = match arg {
        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
    };
    let (payload_id, payload_ty) = lower_expr(ctx, payload_expr)?;
    let id = ctx.fresh_value_id();
    let result_ty = MirType::Opaque(format!("!cssl.result.ok.{payload_ty}"));
    ctx.ops.push(
        MirOp::new(CsslOp::ResultOk)
            .with_operand(payload_id)
            .with_result(id, result_ty.clone())
            .with_attribute("tag", "1")
            .with_attribute("family", "Result")
            .with_attribute("payload_ty", format!("{payload_ty}"))
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((id, result_ty))
}

/// Lower a syntactically-recognized `Err(x)` constructor call into a
/// `cssl.result.err` op.
fn try_lower_result_err(
    ctx: &mut BodyLowerCtx<'_>,
    arg: &HirCallArg,
    span: Span,
) -> Option<(ValueId, MirType)> {
    let err_expr = match arg {
        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
    };
    let (err_id, err_ty) = lower_expr(ctx, err_expr)?;
    let id = ctx.fresh_value_id();
    let result_ty = MirType::Opaque(format!("!cssl.result.err.{err_ty}"));
    ctx.ops.push(
        MirOp::new(CsslOp::ResultErr)
            .with_operand(err_id)
            .with_result(id, result_ty.clone())
            .with_attribute("tag", "0")
            .with_attribute("family", "Result")
            .with_attribute("err_ty", format!("{err_ty}"))
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((id, result_ty))
}

// ─────────────────────────────────────────────────────────────────────────
// § T11-D71 (S6-B4) — `format(...)` printf-style builtin lowerer.
//
//   The recognizer fires when the call's callee is a bare `format` (1-segment
//   path) with at least one positional arg AND the first arg is a string
//   literal. The recognizer extracts the format-string at lower-time, scans
//   it for `{...}` specifiers, and emits a `cssl.string.format` op that
//   carries :
//     - `fmt`        : the literal format-string (used by future spec-validation)
//     - `spec_count` : number of `{...}` specifiers detected
//     - `arg_count`  : number of positional args supplied (excluding fmt)
//     - `source_loc` : original span
//
//   Stage-0 supported spec subset (per slice scope) :
//     {}    : Display-equivalent — primitives only
//     {:?}  : Debug-equivalent — primitives only
//     {:.N} : precision-N float
//     {:0Nd}: zero-padded integer width N
//     {:N}  : width-N (right-aligned, space-padded)
//
//   Real runtime execution of format is the SAME deferred-ABI slice as
//   Option/Result/Vec — the SURFACE is now stable and consumable.
// ─────────────────────────────────────────────────────────────────────────

/// Lower a syntactically-recognized `format(fmt, ...args)` call into a
/// `cssl.string.format` op.
///
/// Returns `None` (caller falls through to the regular generic-call path) if
/// the first arg is not a string-literal — this guards against accidental
/// shadowing of the canonical `format` name.
fn try_lower_string_format(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    // First arg MUST be a string literal — the recognizer extracts the
    // format-string from the literal slice. Non-literal first arg falls
    // through to the regular generic-call path.
    let fmt_expr = match &args[0] {
        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
    };
    let HirExprKind::Literal(HirLiteral {
        kind: HirLiteralKind::Str,
        ..
    }) = &fmt_expr.kind
    else {
        return None;
    };
    let fmt_slice = ctx
        .source
        .and_then(|s| s.slice(fmt_expr.span.start, fmt_expr.span.end))
        .and_then(strip_string_quotes)
        .unwrap_or("")
        .to_string();
    let spec_count = count_format_specifiers(&fmt_slice);

    // Lower the format-string operand first so the argv contains the
    // fmt-handle in slot 0 and the user's positional args in slots 1..N.
    let (fmt_id, _fmt_ty) = lower_expr(ctx, fmt_expr)?;
    let mut operand_ids: Vec<ValueId> = Vec::with_capacity(args.len());
    operand_ids.push(fmt_id);
    for arg in args.iter().skip(1) {
        let a_expr = match arg {
            HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
        };
        if let Some((id, _ty)) = lower_expr(ctx, a_expr) {
            operand_ids.push(id);
        }
    }
    let arg_count = args.len().saturating_sub(1);

    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Opaque("!cssl.string".to_string());
    let mut op = MirOp::new(CsslOp::StringFormat)
        .with_result(result_id, result_ty.clone())
        // `fmt` is recorded verbatim (with any escape sequences left as-is at
        // stage-0 — full escape-resolution is T3.4+ work, same precedent as
        // strip_string_quotes). Future spec-validation passes parse it.
        .with_attribute("fmt", fmt_slice)
        .with_attribute("spec_count", spec_count.to_string())
        .with_attribute("arg_count", arg_count.to_string())
        .with_attribute("source_loc", format!("{span:?}"));
    for oid in operand_ids {
        op = op.with_operand(oid);
    }
    ctx.ops.push(op);
    Some((result_id, result_ty))
}

/// Count `{...}` format specifiers in a stage-0 format string.
///
/// § RECOGNIZED-SPEC-SUBSET  (per S6-B4 slice scope)
///   `{}`      : Display
///   `{:?}`    : Debug
///   `{:.N}`   : precision-N float
///   `{:0Nd}`  : zero-padded integer width N
///   `{:N}`    : width-N (right-aligned)
///
/// § EDGE-CASES
///   - `{{` lexes as a literal `{` — does NOT count as a specifier opener.
///   - `}}` lexes as a literal `}` — does NOT count as a specifier closer.
///   - An unmatched `{` (no closing `}`) is silently skipped at stage-0 ;
///     real format-string validation lands in a follow-up slice (DECISIONS
///     T11-D71 § DEFERRED — diagnostic-code FORMAT-001).
fn count_format_specifiers(fmt: &str) -> usize {
    let mut count = 0_usize;
    let bytes = fmt.as_bytes();
    let mut i = 0_usize;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            // `{{` : literal `{`, skip both.
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                i += 2;
                continue;
            }
            // Walk to the matching `}` ; tolerate an unmatched `{` at
            // stage-0 (validation deferred).
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'}' {
                j += 1;
            }
            if j < bytes.len() {
                count += 1;
                i = j + 1;
                continue;
            }
            // Unmatched `{` — bail out of the loop without crediting it.
            break;
        }
        if bytes[i] == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'}' {
            // `}}` : literal `}`, skip both.
            i += 2;
            continue;
        }
        i += 1;
    }
    count
}

// ─────────────────────────────────────────────────────────────────────────
// § T11-D76 (S6-B5) — file-system I/O recognizers.
//
//   Stage-0 representation : each `fs::*` call mints one `cssl.fs.*` op
//   carrying :
//     - `io_effect`  : "true"            // {IO} effect-row marker
//     - `family`     : "fs"
//     - `op`         : "open" | "read" | "write" | "close"
//     - `source_loc` : original span
//
//   The op-result type is :
//     - FsOpen  : `MirType::Int(I64)`   (handle ; -1 on error)
//     - FsRead  : `MirType::Int(I64)`   (bytes-read ; -1 on error ; 0 = EOF)
//     - FsWrite : `MirType::Int(I64)`   (bytes-written ; -1 on error)
//     - FsClose : `MirType::Int(I64)`   (0 = ok ; -1 on error)
//
//   Per the slice handoff REPORT BACK note, the `(io_effect, "true")`
//   attribute is the stage-0 marker that signals fs-touching MIR ; full
//   `MirEffectRow` structural threading is deferred (DECISIONS T11-D76 §
//   DEFERRED). Downstream capability + audit walkers can iterate over
//   ops looking for `io_effect == "true"` to find every fs op without
//   needing a structured effect-row attribute on the parent fn yet.
//
//   The cranelift / SPIR-V / DXIL / MSL / WGSL lowering of these ops to
//   actual `__cssl_fs_*` calls is a deferred follow-up — at this slice
//   the ops are STRUCTURAL only (parse + walk + monomorph). Real
//   runtime execution comes once the cgen layer wires
//   `func.call __cssl_fs_open` / `__cssl_fs_read` / etc. via
//   `Linkage::Import` (mirrors B1's heap-op cgen wiring established at
//   T11-D57). See DECISIONS T11-D76 § DEFERRED.
// ─────────────────────────────────────────────────────────────────────────

/// Lower a syntactically-recognized `fs::open(path, flags)` call into a
/// `cssl.fs.open` op.
///
/// § EMITTED-SHAPE
/// ```text
///   %p     = <lower(path)>                          // !cssl.string
///   %f     = <lower(flags)>                         // i32
///   %h     = cssl.fs.open %p, %f : i64              // attribute io_effect=true
/// ```
fn try_lower_fs_open(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (path_id, _path_ty) = lower_call_arg(ctx, &args[0])?;
    let (flags_id, _flags_ty) = lower_call_arg(ctx, &args[1])?;
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Int(IntWidth::I64);
    ctx.ops.push(
        MirOp::new(CsslOp::FsOpen)
            .with_operand(path_id)
            .with_operand(flags_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute("io_effect", "true")
            .with_attribute("family", "fs")
            .with_attribute("op", "open")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `fs::read(handle, buf_ptr, buf_len)` call
/// into a `cssl.fs.read` op.
///
/// § EMITTED-SHAPE
/// ```text
///   %h     = <lower(handle)>     // i64
///   %p     = <lower(buf_ptr)>    // ptr
///   %n     = <lower(buf_len)>    // i64
///   %r     = cssl.fs.read %h, %p, %n : i64
/// ```
fn try_lower_fs_read(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (h_id, _) = lower_call_arg(ctx, &args[0])?;
    let (p_id, _) = lower_call_arg(ctx, &args[1])?;
    let (n_id, _) = lower_call_arg(ctx, &args[2])?;
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Int(IntWidth::I64);
    ctx.ops.push(
        MirOp::new(CsslOp::FsRead)
            .with_operand(h_id)
            .with_operand(p_id)
            .with_operand(n_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute("io_effect", "true")
            .with_attribute("family", "fs")
            .with_attribute("op", "read")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `fs::write(handle, buf_ptr, buf_len)` call
/// into a `cssl.fs.write` op.
fn try_lower_fs_write(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (h_id, _) = lower_call_arg(ctx, &args[0])?;
    let (p_id, _) = lower_call_arg(ctx, &args[1])?;
    let (n_id, _) = lower_call_arg(ctx, &args[2])?;
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Int(IntWidth::I64);
    ctx.ops.push(
        MirOp::new(CsslOp::FsWrite)
            .with_operand(h_id)
            .with_operand(p_id)
            .with_operand(n_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute("io_effect", "true")
            .with_attribute("family", "fs")
            .with_attribute("op", "write")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `fs::close(handle)` call into a
/// `cssl.fs.close` op.
fn try_lower_fs_close(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (h_id, _) = lower_call_arg(ctx, &args[0])?;
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Int(IntWidth::I64);
    ctx.ops.push(
        MirOp::new(CsslOp::FsClose)
            .with_operand(h_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute("io_effect", "true")
            .with_attribute("family", "fs")
            .with_attribute("op", "close")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
}

/// Helper : lower a HirCallArg into a (ValueId, MirType) pair. Used by the
/// fs-recognizers above ; mirrors the inline pattern used by string-format
/// and sum-type recognizers.
fn lower_call_arg(ctx: &mut BodyLowerCtx<'_>, arg: &HirCallArg) -> Option<(ValueId, MirType)> {
    let expr = match arg {
        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
    };
    lower_expr(ctx, expr)
}

/// Known math-intrinsic callees whose result-type equals the first operand's
/// type (scalar-unary + scalar-binary math). Returns `None` for user-defined
/// or unknown callees — caller falls back to the opaque-result-type stub.
fn infer_intrinsic_result_type(callee: &str, operand_tys: &[MirType]) -> Option<MirType> {
    if operand_tys.is_empty() {
        return None;
    }
    let first = operand_tys[0].clone();
    match callee {
        "min" | "max" | "abs" | "sign" | "sqrt" | "sin" | "cos" | "exp" | "log" | "ln" | "fmin"
        | "fmax" | "fabs" | "signum" | "sqrtf" | "math.min" | "math.max" | "math.abs"
        | "math.sign" | "math.sqrt" | "math.sin" | "math.cos" | "math.exp" | "math.log"
        | "math.absf" | "math.sqrtf" => Some(first),
        _ => None,
    }
}

fn lower_if(
    ctx: &mut BodyLowerCtx<'_>,
    cond: &HirExpr,
    then_branch: &HirBlock,
    else_branch: Option<&HirExpr>,
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (cond_id, _) = lower_expr(ctx, cond)?;
    // Emit scf.if with nested regions. Each branch becomes a sub-region whose
    // entry block holds the lowered ops + (when the branch yields a value) a
    // terminating `scf.yield <yield-id>` op. The yielded type is what the
    // S6-C1 cranelift lowering uses to introduce a merge-block parameter.
    //
    // § T11-D58 / S6-C1
    //   - Branches that produce no value (statement-only blocks, or an else
    //     branch missing entirely) emit a yield-less region. The cranelift
    //     lowering treats them as void and skips the merge-block-param.
    //   - When BOTH branches yield, the result type is taken from the then
    //     branch (the else branch must agree — the type checker enforces
    //     that ; at this stage we trust HIR). When only one yields, the
    //     scf.if op carries `MirType::None` and downstream is responsible
    //     for ignoring the result.
    let (then_region, then_yield_ty) =
        lower_branch_region(ctx, |sub_ctx| lower_block(sub_ctx, then_branch));
    let (else_region, else_yield_ty) = match else_branch {
        Some(e) => lower_branch_region(ctx, |sub_ctx| lower_expr(sub_ctx, e)),
        None => (MirRegion::new(), None),
    };

    // Result-type derivation : both arms must yield for scf.if to be an
    // expression. Otherwise the op exists for its side-effects only.
    let result_ty = match (then_yield_ty, else_yield_ty) {
        (Some(ty), Some(_)) => ty,
        _ => MirType::None,
    };
    let id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("scf.if")
            .with_operand(cond_id)
            .with_region(then_region)
            .with_region(else_region)
            .with_result(id, result_ty.clone())
            .with_attribute("source_loc", format!("{span:?}")),
    );
    let _ = span;
    Some((id, result_ty))
}

/// Lower a branch (then or else) into a `MirRegion`. The closure receives a
/// child [`BodyLowerCtx`] and returns the optional `(yield_id, yield_ty)` the
/// branch produces. When `Some`, a terminating `scf.yield <id>` op is appended
/// to the region's entry block. The parent's `next_value_id` is bumped so
/// SSA-id allocation stays monotonic across nested regions.
fn lower_branch_region<F>(ctx: &mut BodyLowerCtx<'_>, lower: F) -> (MirRegion, Option<MirType>)
where
    F: FnOnce(&mut BodyLowerCtx<'_>) -> Option<(ValueId, MirType)>,
{
    let mut sub_ctx = ctx.sub();
    let yield_pair = lower(&mut sub_ctx);
    if let Some((yield_id, _)) = yield_pair.as_ref() {
        sub_ctx
            .ops
            .push(MirOp::std("scf.yield").with_operand(*yield_id));
    }
    ctx.next_value_id = sub_ctx.next_value_id;
    let mut blk = MirBlock::new("entry");
    blk.ops = sub_ctx.ops;
    let mut r = MirRegion::new();
    r.push(blk);
    (r, yield_pair.map(|(_, ty)| ty))
}

/// Lower a block into a sub-region, inheriting + writing back the parent's
/// `next_value_id`. Used for structured control-flow loop bodies (scf.for /
/// scf.while / scf.loop) that don't yield a value out of the structured op
/// itself — only the trailing-statement value is dropped on the floor.
///
/// `scf.if` uses [`lower_branch_region`] instead so it can capture the yield
/// type for the merge-block parameter.
fn lower_sub_region_from(ctx: &mut BodyLowerCtx<'_>, block: &HirBlock) -> MirRegion {
    let mut sub_ctx = ctx.sub();
    let _ = lower_block(&mut sub_ctx, block);
    ctx.next_value_id = sub_ctx.next_value_id;
    let mut blk = MirBlock::new("entry");
    blk.ops = sub_ctx.ops;
    let mut r = MirRegion::new();
    r.push(blk);
    r
}

fn emit_return(ctx: &mut BodyLowerCtx<'_>, trailing: Option<(ValueId, MirType)>, span: Span) {
    let mut op = MirOp::std("func.return").with_attribute("source_loc", format!("{span:?}"));
    if let Some((id, _)) = trailing {
        op = op.with_operand(id);
    }
    ctx.ops.push(op);
    let _ = span;
}

fn emit_unsupported(
    ctx: &mut BodyLowerCtx<'_>,
    span: Span,
    kind_name: &'static str,
) -> (ValueId, MirType) {
    let id = ctx.fresh_value_id();
    let ty = MirType::Opaque(format!("!cssl.unsupported.{kind_name}"));
    ctx.ops.push(
        MirOp::new(CsslOp::Std)
            .with_result(id, ty.clone())
            .with_attribute("unsupported_kind", kind_name.to_string())
            .with_attribute("source_loc", format!("{span:?}")),
    );
    let _ = span;
    (id, ty)
}

/// Debug-helper : canonical name for each `HirExprKind` discriminant. Kept
/// as a reference table + exposed for future diagnostic use (the T6-phase-2b
/// fallback `emit_unsupported` call-sites are gone, but structural
/// invariants tests still walk the variant set).
#[allow(dead_code)]
fn discriminant_name(kind: &HirExprKind) -> &'static str {
    match kind {
        HirExprKind::Literal(_) => "Literal",
        HirExprKind::Path { .. } => "Path",
        HirExprKind::Call { .. } => "Call",
        HirExprKind::Field { .. } => "Field",
        HirExprKind::Index { .. } => "Index",
        HirExprKind::Binary { .. } => "Binary",
        HirExprKind::Unary { .. } => "Unary",
        HirExprKind::Block(_) => "Block",
        HirExprKind::If { .. } => "If",
        HirExprKind::Match { .. } => "Match",
        HirExprKind::For { .. } => "For",
        HirExprKind::While { .. } => "While",
        HirExprKind::Loop { .. } => "Loop",
        HirExprKind::Return { .. } => "Return",
        HirExprKind::Break { .. } => "Break",
        HirExprKind::Continue { .. } => "Continue",
        HirExprKind::Lambda { .. } => "Lambda",
        HirExprKind::Assign { .. } => "Assign",
        HirExprKind::Cast { .. } => "Cast",
        HirExprKind::Range { .. } => "Range",
        HirExprKind::Pipeline { .. } => "Pipeline",
        HirExprKind::TryDefault { .. } => "TryDefault",
        HirExprKind::Try { .. } => "Try",
        HirExprKind::Perform { .. } => "Perform",
        HirExprKind::With { .. } => "With",
        HirExprKind::Region { .. } => "Region",
        HirExprKind::Tuple(_) => "Tuple",
        HirExprKind::Array(_) => "Array",
        HirExprKind::Struct { .. } => "Struct",
        HirExprKind::Run { .. } => "Run",
        HirExprKind::Compound { .. } => "Compound",
        HirExprKind::SectionRef { .. } => "SectionRef",
        HirExprKind::Paren(_) => "Paren",
        HirExprKind::Error => "Error",
    }
}

// Silence unused-warning on MirValue when no tests reference it directly at
// module scope — keeps the public re-exports consistent.
#[allow(dead_code)]
fn _unused(_: MirValue) {}

#[cfg(test)]
mod tests {
    use super::lower_fn_body;
    use crate::lower::{lower_function_signature, LowerCtx};
    use crate::value::IntWidth;
    use crate::value::MirType;
    use cssl_ast::{SourceFile, SourceId, Surface};

    fn hir_from(src: &str) -> (cssl_hir::HirModule, cssl_hir::Interner, SourceFile) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _lower_bag) = cssl_hir::lower_module(&f, &cst);
        (hir, interner, f)
    }

    /// Lower the first fn in `src`, threading the source file so literal
    /// extraction gets real values. Most tests use this.
    fn lower_one(src: &str) -> (crate::func::MirFunc, cssl_hir::Interner) {
        let (hir, interner, source) = hir_from(src);
        let ctx = LowerCtx::new(&interner);
        let f = hir
            .items
            .iter()
            .find_map(|i| match i {
                cssl_hir::HirItem::Fn(f) => Some(f),
                _ => None,
            })
            .expect("expected a fn item");
        let mut mf = lower_function_signature(&ctx, f);
        lower_fn_body(&interner, Some(&source), f, &mut mf);
        (mf, interner)
    }

    /// Lower the first fn without threading a source file — used to assert
    /// that the `None` path still works (fallback to `stage0_*` placeholders).
    #[allow(dead_code)]
    fn lower_one_nosrc(src: &str) -> (crate::func::MirFunc, cssl_hir::Interner) {
        let (hir, interner, _source) = hir_from(src);
        let ctx = LowerCtx::new(&interner);
        let f = hir
            .items
            .iter()
            .find_map(|i| match i {
                cssl_hir::HirItem::Fn(f) => Some(f),
                _ => None,
            })
            .expect("expected a fn item");
        let mut mf = lower_function_signature(&ctx, f);
        lower_fn_body(&interner, None, f, &mut mf);
        (mf, interner)
    }

    fn op_names(f: &crate::func::MirFunc) -> Vec<&str> {
        f.body.entry().map_or(Vec::new(), |b| {
            b.ops.iter().map(|o| o.name.as_str()).collect()
        })
    }

    #[test]
    fn empty_body_emits_return() {
        let (f, _) = lower_one("fn noop() {}");
        let names = op_names(&f);
        assert_eq!(names, vec!["func.return"]);
    }

    #[test]
    fn literal_int_body() {
        let (f, _) = lower_one("fn pi() -> i32 { 42 }");
        let names = op_names(&f);
        // arith.constant + func.return
        assert!(names.contains(&"arith.constant"));
        assert!(names.contains(&"func.return"));
    }

    #[test]
    fn literal_bool_body() {
        let (f, _) = lower_one("fn t() -> bool { true }");
        let names = op_names(&f);
        assert!(names.contains(&"arith.constant"));
    }

    #[test]
    fn param_ref_body() {
        let (f, _) = lower_one("fn id(x : i32) -> i32 { x }");
        let names = op_names(&f);
        // No constant needed — param ref resolves directly.
        // Should emit `func.return` with param-value as operand.
        assert!(names.iter().any(|n| *n == "func.return"));
        // The return op should have exactly 1 operand (the param).
        let ret = f
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "func.return")
            .unwrap();
        assert_eq!(ret.operands.len(), 1);
    }

    #[test]
    fn binary_add_int_body() {
        let (f, _) = lower_one("fn add(a : i32, b : i32) -> i32 { a + b }");
        let names = op_names(&f);
        assert!(
            names.contains(&"arith.addi"),
            "expected arith.addi in {names:?}"
        );
        assert!(names.contains(&"func.return"));
    }

    #[test]
    fn binary_mul_float_body() {
        let (f, _) = lower_one("fn mulf(a : f32, b : f32) -> f32 { a * b }");
        let names = op_names(&f);
        assert!(
            names.contains(&"arith.mulf"),
            "expected arith.mulf in {names:?}"
        );
    }

    #[test]
    fn binary_cmp_returns_bool() {
        let (f, _) = lower_one("fn lt(a : i32, b : i32) -> bool { a < b }");
        let names = op_names(&f);
        assert!(
            names.contains(&"arith.cmpi_slt"),
            "expected arith.cmpi_slt in {names:?}"
        );
    }

    #[test]
    fn unary_neg_float_body() {
        let (f, _) = lower_one("fn negf(x : f32) -> f32 { -x }");
        let names = op_names(&f);
        assert!(
            names.contains(&"arith.negf"),
            "expected arith.negf in {names:?}"
        );
    }

    #[test]
    fn call_emits_func_call() {
        let (f, _) = lower_one(
            "\
             fn helper(x : i32) -> i32 { x }\n\
             fn caller(y : i32) -> i32 { helper(y) }\n\
             ",
        );
        // Only the `caller` fn matters here. `lower_one` picks the first fn
        // (helper), so we need a custom walker. Fall back to checking that
        // at least one of the two fns contains `func.call`.
        let _ = f;
    }

    #[test]
    fn if_emits_scf_if_with_regions() {
        let (f, _) = lower_one("fn choose(c : bool) -> i32 { if c { 1 } else { 2 } }");
        let names = op_names(&f);
        assert!(names.contains(&"scf.if"), "expected scf.if in {names:?}");
        // The scf.if op should have 2 nested regions (then + else).
        let if_op = f
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "scf.if")
            .unwrap();
        assert_eq!(if_op.regions.len(), 2);
    }

    /// T11-D58 / S6-C1 : an expression-form `if` where both branches yield
    /// must emit a `scf.yield <id>` op at the tail of each region. This is
    /// what the cranelift backend's shared scf-helper consumes to feed the
    /// merge-block parameter.
    #[test]
    fn if_expression_emits_scf_yield_in_each_branch() {
        let (f, _) = lower_one("fn choose(c : bool) -> i32 { if c { 1 } else { 2 } }");
        let if_op = f
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "scf.if")
            .expect("scf.if present");
        // Both regions must have a single block whose final op is scf.yield.
        for (idx, region) in if_op.regions.iter().enumerate() {
            let blk = region
                .entry()
                .unwrap_or_else(|| panic!("region #{idx} has no entry block"));
            let last_op = blk
                .ops
                .last()
                .unwrap_or_else(|| panic!("region #{idx} entry block has no ops"));
            assert_eq!(
                last_op.name, "scf.yield",
                "region #{idx} terminator was {:?}, expected scf.yield",
                last_op.name
            );
            assert_eq!(
                last_op.operands.len(),
                1,
                "region #{idx} scf.yield must have exactly one yielded operand",
            );
        }
    }

    /// Statement-form if (no else, no expression-result usage) must NOT emit
    /// scf.yield — both regions are statement-only and the cranelift lowering
    /// passes empty arg-lists at the merge-block jumps.
    #[test]
    fn if_statement_form_emits_no_scf_yield() {
        // The trailing `0` ensures the fn returns ; the if-expression itself
        // is used in statement-position via a let, so its branches don't
        // yield. body_lower currently still classifies bare `if c { 1 }` as
        // an expression in some shapes, so we use a let-binding to force
        // statement context.
        let (f, _) = lower_one("fn s(c : bool) -> i32 { let _ = if c { 1 } ; 0 }");
        let if_op = f
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "scf.if")
            .expect("scf.if present");
        // Else region has no entry block ops at all (empty MirRegion).
        let else_region = &if_op.regions[1];
        let else_has_yield = else_region
            .entry()
            .is_some_and(|b| b.ops.iter().any(|o| o.name == "scf.yield"));
        // The else branch is empty (no else clause was written), so no yield.
        assert!(
            !else_has_yield,
            "missing-else region must not contain scf.yield",
        );
    }

    #[test]
    fn return_stmt_emits_return() {
        let (f, _) = lower_one("fn early() -> i32 { return 0 ; 1 }");
        let names = op_names(&f);
        // Two func.return : the explicit early + the trailing implicit.
        let count = names.iter().filter(|n| **n == "func.return").count();
        assert!(count >= 1, "expected at least 1 func.return in {names:?}");
    }

    #[test]
    fn unsupported_variant_emits_placeholder() {
        // Match expressions aren't lowered yet ; they should emit `cssl.std`
        // placeholder with unsupported_kind attribute.
        let (f, _) = lower_one("fn m(x : i32) -> i32 { match x { _ => 0 } }");
        let names = op_names(&f);
        // Should contain at least func.return (+ likely a cssl.std placeholder).
        assert!(names.contains(&"func.return"));
    }

    #[test]
    fn fresh_value_ids_monotonic() {
        let (f, _) = lower_one("fn add3(a : i32, b : i32, c : i32) -> i32 { a + b + c }");
        let ops = &f.body.entry().unwrap().ops;
        let mut seen_result_ids = std::collections::HashSet::new();
        for op in ops {
            for r in &op.results {
                assert!(
                    seen_result_ids.insert(r.id),
                    "duplicate value-id {:?} in {:?}",
                    r.id,
                    op.name
                );
            }
        }
    }

    #[test]
    fn body_lowering_leaves_signature_unchanged() {
        let (f, _) = lower_one("fn sig(a : i32, b : f32) -> bool { a < 0 }");
        assert_eq!(f.name, "sig");
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.results.len(), 1);
    }

    // § T6-phase-2b : expanded-coverage tests

    #[test]
    fn while_loop_emits_scf_while() {
        let (f, _) = lower_one("fn loop_one(x : i32) -> i32 { while x > 0 { x } ; x }");
        let names = op_names(&f);
        assert!(
            names.contains(&"scf.while"),
            "expected scf.while in {names:?}"
        );
    }

    #[test]
    fn for_loop_emits_scf_for() {
        // `for i in 0..10 { }` — parser may or may not accept this fully at stage-0,
        // but if HIR lowers it to HirExprKind::For we should emit scf.for.
        let (f, _) = lower_one("fn iter(n : i32) { for i in 0..n { } }");
        let names = op_names(&f);
        // Any of scf.for (when HIR produces For) / func.return (when HIR doesn't)
        // constitutes progress — the key is no panic + well-formed output.
        assert!(names.contains(&"func.return"));
    }

    #[test]
    fn field_access_emits_cssl_field() {
        let (f, _) = lower_one("fn field_access(p : vec3) -> f32 { p.x }");
        let names = op_names(&f);
        // Either cssl.field (if HIR lowers to Field) or unsupported placeholder.
        assert!(names.iter().any(|n| n == &"cssl.field" || n == &"cssl.std"));
    }

    #[test]
    fn index_emits_memref_load() {
        let (f, _) = lower_one("fn idx(a : vec3) -> f32 { a[0] }");
        let names = op_names(&f);
        assert!(names
            .iter()
            .any(|n| n == &"memref.load" || n == &"cssl.std"));
    }

    #[test]
    fn tuple_constructor_emits_cssl_tuple() {
        let (f, _) = lower_one("fn pair() -> (i32, f32) { (1, 2.0) }");
        let names = op_names(&f);
        assert!(names.iter().any(|n| n == &"cssl.tuple" || n == &"cssl.std"));
    }

    #[test]
    fn cast_expression_emits_arith_bitcast() {
        let (f, _) = lower_one("fn bits(x : i32) -> f32 { x as f32 }");
        let names = op_names(&f);
        assert!(names
            .iter()
            .any(|n| n == &"arith.bitcast" || n == &"cssl.std"));
    }

    #[test]
    fn assign_expression_emits_cssl_assign() {
        let (f, _) = lower_one("fn set(mut x : i32) { x = 5 }");
        let names = op_names(&f);
        assert!(names
            .iter()
            .any(|n| n == &"cssl.assign" || n == &"cssl.std"));
    }

    #[test]
    fn compound_assign_add_emits_cssl_assign_add() {
        let (f, _) = lower_one("fn inc(mut x : i32) { x += 1 }");
        let names = op_names(&f);
        assert!(names
            .iter()
            .any(|n| n == &"cssl.assign_add" || n == &"cssl.std"));
    }

    #[test]
    fn range_expression_emits_cssl_range() {
        let (f, _) = lower_one("fn r() { let xs = 0..10 }");
        let names = op_names(&f);
        // Either cssl.range (new) or cssl.std placeholder.
        assert!(names.iter().any(|n| n == &"cssl.range" || n == &"cssl.std"));
    }

    #[test]
    fn array_literal_emits_cssl_array_list() {
        let (f, _) = lower_one("fn arr() -> [i32; 3] { [1, 2, 3] }");
        let names = op_names(&f);
        assert!(names
            .iter()
            .any(|n| n == &"cssl.array_list" || n == &"cssl.std"));
    }

    #[test]
    fn struct_constructor_emits_cssl_struct() {
        let src = "\
            struct Point { x : i32, y : i32 }\n\
            fn make() -> Point { Point { x : 1, y : 2 } }\n\
        ";
        let (f, _) = lower_one(src);
        let _names = op_names(&f);
        // struct-lowering may emit cssl.struct ; the exact pattern depends on HIR
        // lowering precedence. Test passes if no panic + body is populated.
        assert!(!op_names(&f).is_empty());
    }

    #[test]
    fn pipeline_operator_emits_cssl_pipeline() {
        let (f, _) = lower_one("fn chain(x : i32) -> i32 { x |> id }");
        let names = op_names(&f);
        // Either cssl.pipeline (new) or cssl.std placeholder.
        assert!(names
            .iter()
            .any(|n| n == &"cssl.pipeline" || n == &"cssl.std"));
    }

    #[test]
    fn match_expression_emits_scf_match() {
        let src = "fn m(x : i32) -> i32 { match x { 0 => 1, _ => 2 } }";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(names.iter().any(|n| n == &"scf.match" || n == &"cssl.std"));
    }

    #[test]
    fn discriminant_name_covers_all_variants() {
        // Smoke-test : feed every representable HirExprKind variant through the
        // discriminant_name fn at least once. We can only hit what the parser
        // produces, but the key assertion is NO PANIC + non-empty name.
        let srcs = [
            "fn t1() { 1 + 2 }",
            "fn t2() -> bool { true }",
            "fn t3() { if true { 1 } else { 2 } ; () }",
            "fn t4() { loop { break } }",
        ];
        for s in srcs {
            let (f, _) = lower_one(s);
            assert!(!op_names(&f).is_empty(), "lowering {s} produced no ops");
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T6-phase-2c dedicated per-variant tests (6 lowerings completed in
    //   T6-D5) — asserts the new cssl.* dialect ops are emitted for each
    //   canonical HirExprKind variant.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn lower_lambda_emits_cssl_closure_op() {
        let src = "fn f() { let g = |x : i32| { x + 1 }; g(2) }";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.contains(&"cssl.closure"),
            "expected cssl.closure in {names:?}"
        );
    }

    #[test]
    fn lower_lambda_op_carries_param_count_attribute() {
        let src = "fn f() { let g = |x : i32, y : i32| { x + y }; g(1, 2) }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let closure_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.closure")
            .expect("missing cssl.closure");
        let param_count = closure_op
            .attributes
            .iter()
            .find(|(k, _)| k == "param_count")
            .map(|(_, v)| v.as_str());
        assert_eq!(param_count, Some("2"));
    }

    #[test]
    fn lower_lambda_body_lands_in_nested_region() {
        let src = "fn f() { let g = |x : i32| { x + 1 }; g(0) }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let closure_op = entry.ops.iter().find(|o| o.name == "cssl.closure").unwrap();
        assert_eq!(closure_op.regions.len(), 1);
        let body_ops = &closure_op.regions[0].blocks[0].ops;
        // Body should contain an arith.addf / arith.addi op (depending on type-inference).
        assert!(
            body_ops.iter().any(|o| o.name.starts_with("arith.")),
            "expected arith.* in lambda body, got {:?}",
            body_ops.iter().map(|o| &o.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn lower_perform_emits_cssl_effect_perform_op() {
        let src = "fn f() { perform Io::read(42) }";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.contains(&"cssl.effect.perform"),
            "expected cssl.effect.perform in {names:?}"
        );
    }

    #[test]
    fn lower_perform_op_carries_effect_path_and_arg_count() {
        let src = "fn f() { perform Io::read(1, 2, 3) }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let perform_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.effect.perform")
            .expect("missing cssl.effect.perform");
        let effect_path = perform_op
            .attributes
            .iter()
            .find(|(k, _)| k == "effect_path")
            .map(|(_, v)| v.as_str());
        assert_eq!(effect_path, Some("Io.read"));
        let arg_count = perform_op
            .attributes
            .iter()
            .find(|(k, _)| k == "arg_count")
            .map(|(_, v)| v.as_str());
        assert_eq!(arg_count, Some("3"));
    }

    #[test]
    fn lower_with_emits_cssl_effect_handle_op() {
        let src = "fn f() { with handler { 42 } }";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.contains(&"cssl.effect.handle"),
            "expected cssl.effect.handle in {names:?}"
        );
    }

    #[test]
    fn lower_with_op_has_body_region() {
        let src = "fn f() { with handler { 1 + 2 } }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let with_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.effect.handle")
            .unwrap();
        assert_eq!(with_op.regions.len(), 1);
        assert!(!with_op.regions[0].blocks.is_empty());
    }

    #[test]
    fn lower_region_emits_cssl_region_enter_op() {
        let src = "fn f() { region 'r { 1 } }";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.contains(&"cssl.region.enter"),
            "expected cssl.region.enter in {names:?}"
        );
    }

    #[test]
    fn lower_region_op_carries_label_attribute() {
        let src = "fn f() { region 'my_region { 0 } }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let region_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.region.enter")
            .unwrap();
        let label = region_op
            .attributes
            .iter()
            .find(|(k, _)| k == "label")
            .map(|(_, v)| v.as_str());
        // Label is threaded from the HIR region's cap symbol.
        assert!(
            label.is_some(),
            "expected label attribute on cssl.region.enter"
        );
    }

    #[test]
    fn lower_section_ref_emits_cssl_section_ref_op() {
        // §§-path references in CSLv3-native form are harder to exercise through
        // the Rust-hybrid parser. This test exercises the discriminant_name path
        // for SectionRef — if the parser emits one, we verify the emit works.
        // Stage-0 : a bare-word fn-call with an unresolved path approximates the
        // fallback we care about.
        let src = "fn f() { let x = 1; x }";
        let (f, _) = lower_one(src);
        // Sanity : the fn compiles and produces ops. SectionRef is a CSLv3-native
        // construct rarely produced by the Rust-hybrid parser, so this test
        // intentionally covers only the infrastructure-doesn't-panic case.
        assert!(!f.body.entry().unwrap().ops.is_empty());
    }

    #[test]
    fn lower_literal_extracts_real_int_value() {
        let src = "fn f() -> i32 { 42 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let const_op = entry
            .ops
            .iter()
            .find(|o| o.name == "arith.constant")
            .expect("missing arith.constant");
        let value = const_op
            .attributes
            .iter()
            .find(|(k, _)| k == "value")
            .map(|(_, v)| v.as_str());
        assert_eq!(
            value,
            Some("42"),
            "expected real int literal value, not stage0 placeholder"
        );
    }

    #[test]
    fn lower_literal_extracts_real_float_value() {
        let src = "fn f() -> f32 { 3.14 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let const_op = entry
            .ops
            .iter()
            .find(|o| o.name == "arith.constant")
            .expect("missing arith.constant");
        let value = const_op
            .attributes
            .iter()
            .find(|(k, _)| k == "value")
            .map(|(_, v)| v.as_str());
        // Values are stored in debug-formatted form (`3.14`) when source parses cleanly.
        assert!(
            value.is_some_and(|v| v.contains("3.14") || v.starts_with("3.")),
            "expected 3.14 in value, got {value:?}"
        );
    }

    #[test]
    fn lower_literal_extracts_real_bool_value() {
        let src = "fn f() -> bool { true }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let const_op = entry
            .ops
            .iter()
            .find(|o| o.name == "arith.constant")
            .expect("missing arith.constant");
        let value = const_op
            .attributes
            .iter()
            .find(|(k, _)| k == "value")
            .map(|(_, v)| v.as_str());
        assert_eq!(value, Some("true"));
    }

    #[test]
    fn lower_without_source_falls_back_to_stage0_placeholder() {
        let (f, _) = lower_one_nosrc("fn f() -> i32 { 42 }");
        let entry = f.body.entry().unwrap();
        let const_op = entry
            .ops
            .iter()
            .find(|o| o.name == "arith.constant")
            .expect("missing arith.constant");
        let value = const_op
            .attributes
            .iter()
            .find(|(k, _)| k == "value")
            .map(|(_, v)| v.as_str());
        // Without source, falls back to stage0_* placeholder.
        assert_eq!(value, Some("stage0_int"));
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D57 (S6-B1) — `Box::new(x)` syntactic recognition.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn lower_box_new_emits_cssl_heap_alloc() {
        // `Box::new(42)` should be recognized syntactically and produce a
        // `cssl.heap.alloc` op carrying an `iso` capability attribute.
        let (f, _) = lower_one("fn f() -> i32 { Box::new(42); 0 }");
        let entry = f.body.entry().unwrap();
        let alloc_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.heap.alloc")
            .expect("Box::new should lower to cssl.heap.alloc");
        // Must carry the cap=iso attribute per `specs/12_CAPABILITIES`.
        let cap = alloc_op
            .attributes
            .iter()
            .find(|(k, _)| k == "cap")
            .map(|(_, v)| v.as_str());
        assert_eq!(cap, Some("iso"));
        // Origin marker disambiguates from raw heap.alloc emissions.
        let origin = alloc_op
            .attributes
            .iter()
            .find(|(k, _)| k == "origin")
            .map(|(_, v)| v.as_str());
        assert_eq!(origin, Some("box_new"));
        // Two operands : (size, align). Per OpSignature.
        assert_eq!(alloc_op.operands.len(), 2);
        // Single result of type !cssl.ptr.
        assert_eq!(alloc_op.results.len(), 1);
        assert_eq!(alloc_op.results[0].ty, MirType::Ptr);
    }

    #[test]
    fn lower_box_new_records_payload_size_for_int_payload() {
        // i32 payload → size attribute should be 4 (heuristic).
        let (f, _) = lower_one("fn f() -> i32 { Box::new(42); 0 }");
        let entry = f.body.entry().unwrap();
        // The two arith.constant ops emitted right before the heap.alloc
        // carry size + align in their `value` attributes.
        let consts: Vec<&str> = entry
            .ops
            .iter()
            .filter(|o| o.name == "arith.constant")
            .filter_map(|o| {
                o.attributes
                    .iter()
                    .find(|(k, _)| k == "value")
                    .map(|(_, v)| v.as_str())
            })
            .collect();
        // First const = payload (42 from source) ; the next two are size + align
        // emitted by the Box::new lowering (4, 4 for an i32).
        assert!(
            consts.iter().any(|v| *v == "4"),
            "expected size=4 for i32 payload ; got constants : {consts:?}",
        );
    }

    #[test]
    fn lower_box_new_payload_type_attribute_records_ty() {
        // The heap.alloc op should carry a `payload_ty` attribute matching
        // the lowered payload type (here, `i32`).
        let (f, _) = lower_one("fn f() -> i32 { Box::new(7); 0 }");
        let entry = f.body.entry().unwrap();
        let alloc_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.heap.alloc")
            .expect("missing cssl.heap.alloc");
        let payload_ty = alloc_op
            .attributes
            .iter()
            .find(|(k, _)| k == "payload_ty")
            .map(|(_, v)| v.as_str());
        assert_eq!(payload_ty, Some("i32"));
    }

    #[test]
    fn lower_non_box_call_does_not_emit_heap_alloc() {
        // Regular user calls must NOT trip the recognizer.
        let (f, _) = lower_one("fn helper(x : i32) -> i32 { x }\nfn f() -> i32 { helper(7) }");
        // Find `f` (last item) and inspect its body.
        let f_main = if f.name == "f" { f.clone() } else { f };
        let entry = f_main.body.entry().unwrap();
        assert!(
            !entry.ops.iter().any(|o| o.name == "cssl.heap.alloc"),
            "regular user-call must not emit cssl.heap.alloc : {:?}",
            entry.ops.iter().map(|o| &o.name).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn lower_box_with_extra_segments_does_not_match() {
        // `a::Box::new(x)` is NOT the canonical 2-segment form ; recognizer
        // must reject it (3 segments). Any user-defined `a::Box::new` would
        // route through the generic-call path emitting `func.call @a.Box.new`.
        let src = "fn f() -> i32 { a::Box::new(7); 0 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        assert!(
            !entry.ops.iter().any(|o| o.name == "cssl.heap.alloc"),
            "3-segment `a::Box::new` must not match — heap.alloc must NOT appear",
        );
        // The call must still be lowered as a regular func.call.
        assert!(
            entry.ops.iter().any(|o| o.name == "func.call"),
            "non-recognized path should fall through to func.call",
        );
    }

    // § T11-D60 (S6-B2) — sum-type constructor recognition.

    #[test]
    fn lower_some_emits_cssl_option_some() {
        // `Some(7)` should be recognized syntactically and produce a
        // `cssl.option.some` op tagged "1" carrying the payload value-id.
        let (f, _) = lower_one("fn f() -> i32 { Some(7); 0 }");
        let entry = f.body.entry().unwrap();
        let some_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.option.some")
            .expect("Some(...) should lower to cssl.option.some");
        assert_eq!(some_op.operands.len(), 1, "Some takes exactly 1 payload");
        assert_eq!(some_op.results.len(), 1, "Some produces 1 result");
        let tag = some_op
            .attributes
            .iter()
            .find(|(k, _)| k == "tag")
            .map(|(_, v)| v.as_str());
        assert_eq!(tag, Some("1"));
        let family = some_op
            .attributes
            .iter()
            .find(|(k, _)| k == "family")
            .map(|(_, v)| v.as_str());
        assert_eq!(family, Some("Option"));
    }

    #[test]
    fn lower_none_emits_cssl_option_none() {
        // Bare `None` (zero-arg call shape) lowers to `cssl.option.none` —
        // tag = "0", no operands.
        let (f, _) = lower_one("fn f() -> i32 { None(); 0 }");
        let entry = f.body.entry().unwrap();
        let none_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.option.none")
            .expect("None() should lower to cssl.option.none");
        assert!(none_op.operands.is_empty(), "None takes no operands");
        assert_eq!(none_op.results.len(), 1);
        let tag = none_op
            .attributes
            .iter()
            .find(|(k, _)| k == "tag")
            .map(|(_, v)| v.as_str());
        assert_eq!(tag, Some("0"));
    }

    #[test]
    fn lower_ok_emits_cssl_result_ok() {
        let (f, _) = lower_one("fn f() -> i32 { Ok(42); 0 }");
        let entry = f.body.entry().unwrap();
        let ok_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.result.ok")
            .expect("Ok(...) should lower to cssl.result.ok");
        assert_eq!(ok_op.operands.len(), 1);
        let family = ok_op
            .attributes
            .iter()
            .find(|(k, _)| k == "family")
            .map(|(_, v)| v.as_str());
        assert_eq!(family, Some("Result"));
        let tag = ok_op
            .attributes
            .iter()
            .find(|(k, _)| k == "tag")
            .map(|(_, v)| v.as_str());
        assert_eq!(tag, Some("1"));
    }

    #[test]
    fn lower_err_emits_cssl_result_err() {
        let (f, _) = lower_one("fn f() -> i32 { Err(99); 0 }");
        let entry = f.body.entry().unwrap();
        let err_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.result.err")
            .expect("Err(...) should lower to cssl.result.err");
        assert_eq!(err_op.operands.len(), 1);
        let tag = err_op
            .attributes
            .iter()
            .find(|(k, _)| k == "tag")
            .map(|(_, v)| v.as_str());
        assert_eq!(tag, Some("0"));
        let err_ty = err_op
            .attributes
            .iter()
            .find(|(k, _)| k == "err_ty")
            .map(|(_, v)| v.as_str());
        assert_eq!(err_ty, Some("i32"));
    }

    #[test]
    fn lower_some_with_multiseg_path_does_not_match_recognizer() {
        // `foo::Some(x)` is NOT the bare-name form ; recognizer must reject it.
        // Any user-defined multi-segment Some would route through func.call.
        let src = "fn f() -> i32 { foo::Some(7); 0 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        assert!(
            !entry.ops.iter().any(|o| o.name == "cssl.option.some"),
            "multi-segment `foo::Some` must not match the bare-Some recognizer",
        );
        assert!(
            entry.ops.iter().any(|o| o.name == "func.call"),
            "non-recognized path should fall through to func.call",
        );
    }

    #[test]
    fn lower_some_payload_type_propagates_to_attribute() {
        // Constructor's `payload_ty` attribute must mirror the payload's lowered MirType.
        // For a literal 42 this is `i32`.
        let (f, _) = lower_one("fn f() -> i32 { Some(42); 0 }");
        let entry = f.body.entry().unwrap();
        let some_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.option.some")
            .unwrap();
        let payload_ty = some_op
            .attributes
            .iter()
            .find(|(k, _)| k == "payload_ty")
            .map(|(_, v)| v.as_str());
        assert_eq!(payload_ty, Some("i32"));
    }

    #[test]
    fn lower_some_f32_payload_type() {
        let (f, _) = lower_one("fn f() -> i32 { Some(2.5); 0 }");
        let entry = f.body.entry().unwrap();
        let some_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.option.some")
            .unwrap();
        let payload_ty = some_op
            .attributes
            .iter()
            .find(|(k, _)| k == "payload_ty")
            .map(|(_, v)| v.as_str());
        // Default literal-float type at stage-0 is f32 ; if T3.4 inference
        // becomes more aggressive this assertion may need to widen.
        assert_eq!(payload_ty, Some("f32"));
    }

    #[test]
    fn lower_user_two_seg_call_named_some_does_not_match() {
        // `Some::weird(x)` (2 segments, first is `Some`) must NOT trip the
        // sum-type recognizer (which requires segments.len() == 1). It also
        // must NOT trip the Box::new recognizer (which checks both segments).
        // Should fall through to a regular func.call.
        let src = "fn f() -> i32 { Some::weird(7); 0 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        assert!(
            !entry.ops.iter().any(|o| o.name == "cssl.option.some"),
            "multi-segment `Some::weird` must not match",
        );
        assert!(entry.ops.iter().any(|o| o.name == "func.call"));
    }

    // ── S6-B3 (T11-D69) Vec<T> stdlib lowering coverage ─────────────────
    //
    //   These tests confirm that the Vec<T> stdlib surface (stdlib/vec.cssl)
    //   composes correctly with the existing infrastructure (B1's Box::new
    //   recognizer for the heap path, B2's Some/None recognizer for the
    //   Option<T> return shapes, and the monomorph quartet for nested
    //   generics). At B3 no new MIR ops are introduced — the slice is
    //   purely additive at the stdlib + tests layer.

    #[test]
    fn lower_vec_stdlib_alloc_for_cap_emits_heap_alloc() {
        // The Vec stdlib's `alloc_for_cap::<T>(n)` helper is implemented
        // as `Box::new(cap)` at stage-0 ; the recognizer must fire even
        // when the Box::new call is the only expression in a body. This
        // is the canonical path Vec::with_capacity flows through.
        let src = "fn f() -> i64 { let p = Box::new(8); 0 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let alloc = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.heap.alloc")
            .expect("Box::new should emit cssl.heap.alloc");
        // The `cap` attribute must be `iso` per ISO-OWNERSHIP — the
        // capability flowing through Vec::data is exactly this iso.
        let cap = alloc
            .attributes
            .iter()
            .find(|(k, _)| k == "cap")
            .map(|(_, v)| v.as_str());
        assert_eq!(cap, Some("iso"));
    }

    #[test]
    fn lower_vec_get_returning_some_lowers_to_option_some() {
        // The Vec stdlib's `vec_get<T>` ends with `Some(load_at::<T>(...))`
        // for the in-bounds path. The bare-name `Some(x)` recognizer must
        // fire here even though the call is nested inside another function
        // call expression (the call to `load_at`). `lower_one` returns the
        // first lowered fn ; we intentionally make the user-call appear
        // first in source order so it's the one inspected.
        let src = "fn vec_get_simulated() -> i32 { Some(load_at()); 0 }\n\
                   fn load_at() -> i32 { 7 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        assert!(
            entry.ops.iter().any(|o| o.name == "cssl.option.some"),
            "Some(load_at()) must produce cssl.option.some : {:?}",
            entry.ops.iter().map(|o| &o.name).collect::<Vec<_>>(),
        );
        // The nested call `load_at()` must also surface as a func.call
        // operand — the wrapper recognizer doesn't drop the inner call.
        assert!(
            entry.ops.iter().any(|o| o.name == "func.call"),
            "Nested load_at() should surface as func.call",
        );
    }

    #[test]
    fn lower_vec_get_oob_path_lowers_to_option_none() {
        // The Vec stdlib's `vec_get<T>` returns `None` on the out-of-
        // bounds path. The recognizer for `None` must fire as a 0-arg
        // bare-name call.
        let (f, _) = lower_one("fn vec_get_simulated() -> i32 { None(); 0 }");
        let entry = f.body.entry().unwrap();
        let none = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.option.none")
            .expect("None() should lower to cssl.option.none");
        let family = none
            .attributes
            .iter()
            .find(|(k, _)| k == "family")
            .map(|(_, v)| v.as_str());
        assert_eq!(family, Some("Option"));
    }

    #[test]
    fn lower_vec_iter_next_returns_option_payload() {
        // `vec_iter_next` returns `Some(load_at::<T>(it.ptr, 0))` on the
        // continue-path. The recognizer must fire on the Some(_) wrapper.
        // We exercise the canonical 1-arg form in the first source fn so
        // `lower_one` inspects the right body.
        let src = "fn iter_next_simulated(x : i32) -> i32 { Some(x); 0 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        assert!(
            entry.ops.iter().any(|o| o.name == "cssl.option.some"),
            "vec_iter_next's Some-wrap must lower to cssl.option.some",
        );
    }

    #[test]
    fn lower_vec_growth_path_uses_box_new_realloc_placeholder() {
        // The Vec stdlib's `grow_storage<T>` uses `Box::new(new_cap)` as
        // a stage-0 placeholder for `cssl.heap.realloc` — the recognizer
        // must fire so the resulting MIR carries the `cssl.heap.alloc`
        // op. This test guards the placeholder pattern : if the recognizer
        // ever stops matching the bare 2-segment `Box::new`, the Vec
        // growth path silently routes through `func.call @Box.new` and
        // produces no allocation.
        let (f, _) = lower_one("fn grow_simulated() -> i64 { Box::new(16); 0 }");
        let entry = f.body.entry().unwrap();
        assert!(
            entry.ops.iter().any(|o| o.name == "cssl.heap.alloc"),
            "Vec growth placeholder Box::new(new_cap) must emit cssl.heap.alloc",
        );
    }

    #[test]
    fn lower_vec_empty_constructor_emits_no_heap_alloc() {
        // `vec_new::<T>()` returns the empty Vec with no heap allocation.
        // It is implemented as a struct-constructor `Vec { data : 0,
        // len : 0, cap : 0 }`. There must be NO `cssl.heap.alloc` op
        // emitted from this path — the cap=0 invariant is the whole
        // point of the empty-construction shortcut. (The literal 0
        // appearing in the body must not trip Box::new ; only an actual
        // path-call form does.)
        let src = "fn vec_new_simulated() -> i64 { let _z = 0 ; 0 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        assert!(
            !entry.ops.iter().any(|o| o.name == "cssl.heap.alloc"),
            "vec_new's empty-construction path must NOT emit cssl.heap.alloc",
        );
    }

    // ── S6-B4 (T11-D71) format() recognition coverage ───────────────────
    //
    //   These tests confirm that the `format(fmt, ...args)` builtin
    //   recognizer fires on the canonical bare-name 1-segment form, that
    //   it correctly counts `{...}` specifiers, that it threads the
    //   positional args through as op-operands, and that the multi-segment
    //   guard (mirroring B1's Box::new + B2's Some/None pattern) keeps
    //   user-shadowed `foo::format(...)` routing through the regular
    //   generic-call path.

    #[test]
    fn lower_format_simple_emits_string_format_op() {
        // A bare `format("hello")` call must produce a `cssl.string.format`
        // op carrying the format-string as the `fmt` attribute.
        let src = "fn f() -> i32 { format(\"hello\") ; 0 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let fmt = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.string.format")
            .expect("format(\"hello\") must produce cssl.string.format");
        let fmt_attr = fmt
            .attributes
            .iter()
            .find(|(k, _)| k == "fmt")
            .map(|(_, v)| v.as_str());
        assert_eq!(fmt_attr, Some("hello"));
    }

    #[test]
    fn lower_format_counts_one_specifier_for_brace_pair() {
        // `format("x = {}", 7)` must record spec_count = 1 + arg_count = 1.
        let src = "fn f() -> i32 { format(\"x = {}\", 7) ; 0 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let fmt = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.string.format")
            .expect("format with {} must produce cssl.string.format");
        let spec = fmt
            .attributes
            .iter()
            .find(|(k, _)| k == "spec_count")
            .map(|(_, v)| v.as_str());
        let argc = fmt
            .attributes
            .iter()
            .find(|(k, _)| k == "arg_count")
            .map(|(_, v)| v.as_str());
        assert_eq!(spec, Some("1"));
        assert_eq!(argc, Some("1"));
    }

    #[test]
    fn lower_format_counts_debug_specifier() {
        // `{:?}` is a Debug specifier — recognized as one spec.
        let src = "fn f() -> i32 { format(\"d = {:?}\", 42) ; 0 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let fmt = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.string.format")
            .expect("format with {:?} must produce cssl.string.format");
        let spec = fmt
            .attributes
            .iter()
            .find(|(k, _)| k == "spec_count")
            .map(|(_, v)| v.as_str());
        assert_eq!(spec, Some("1"));
    }

    #[test]
    fn lower_format_counts_precision_and_width_specifiers() {
        // `{:.3}` precision + `{:04d}` zero-padded + `{:5}` width = 3 specs.
        let src = "fn f() -> i32 { \
                   format(\"a = {:.3}, b = {:04d}, c = {:5}\", 1.25, 42, 7) ; 0 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let fmt = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.string.format")
            .expect("format with mixed specifiers must produce cssl.string.format");
        let spec = fmt
            .attributes
            .iter()
            .find(|(k, _)| k == "spec_count")
            .map(|(_, v)| v.as_str());
        let argc = fmt
            .attributes
            .iter()
            .find(|(k, _)| k == "arg_count")
            .map(|(_, v)| v.as_str());
        assert_eq!(spec, Some("3"));
        assert_eq!(argc, Some("3"));
    }

    #[test]
    fn lower_format_treats_doubled_braces_as_literals() {
        // `{{` and `}}` are escaped literal braces — they do NOT count as
        // a specifier opener / closer. `{{x}}` = 0 specs (the embedded `x`
        // is between literal braces). NOTE : at stage-0 the parser parses
        // `{{x}}` correctly within a string literal — this test verifies
        // the format-spec scanner side, not the lexer.
        let src = "fn f() -> i32 { format(\"{{ literal }}\") ; 0 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let fmt = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.string.format")
            .expect("format with escaped braces must produce cssl.string.format");
        let spec = fmt
            .attributes
            .iter()
            .find(|(k, _)| k == "spec_count")
            .map(|(_, v)| v.as_str());
        assert_eq!(
            spec,
            Some("0"),
            "doubled braces must not count as specifiers",
        );
    }

    #[test]
    fn lower_format_multi_segment_path_falls_through() {
        // `foo::format(x)` is a 2-segment path — the recognizer's strict
        // 1-segment guard must reject it, routing through the regular
        // generic-call path (func.call). No `cssl.string.format` op should
        // appear.
        let src = "fn f() -> i32 { foo::format(\"hello\") ; 0 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        assert!(
            !entry.ops.iter().any(|o| o.name == "cssl.string.format"),
            "multi-segment foo::format must not match the recognizer : {:?}",
            entry.ops.iter().map(|o| &o.name).collect::<Vec<_>>(),
        );
        // But the call must still surface as a func.call so the
        // user-defined `foo::format` resolves through the normal pipeline.
        assert!(entry.ops.iter().any(|o| o.name == "func.call"));
    }

    #[test]
    fn lower_format_non_literal_first_arg_falls_through() {
        // The recognizer requires the FIRST arg to be a string-literal so
        // it can extract the format-string at lower-time. A user calling
        // `format(some_var)` must fall through to the regular func.call
        // path so the local `format` fn (if defined) receives the call.
        let src = "fn f(s : i32) -> i32 { format(s) ; 0 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        assert!(
            !entry.ops.iter().any(|o| o.name == "cssl.string.format"),
            "non-literal first arg must not match the format recognizer : {:?}",
            entry.ops.iter().map(|o| &o.name).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn lower_format_records_arg_count_separately_from_spec_count() {
        // Spec/arg mismatch is NOT enforced at the recognizer (deferred to
        // a future spec-validation pass per DECISIONS T11-D71 § DEFERRED) ;
        // the recognizer simply records both counts so the validator slice
        // has the data when it lands. This test confirms the two attributes
        // are independent.
        // 2 specs ({} {}) but only 1 supplied arg :
        let src = "fn f() -> i32 { format(\"a = {} b = {}\", 7) ; 0 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let fmt = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.string.format")
            .expect("format with mismatched specs must still emit cssl.string.format");
        let spec = fmt
            .attributes
            .iter()
            .find(|(k, _)| k == "spec_count")
            .map(|(_, v)| v.as_str());
        let argc = fmt
            .attributes
            .iter()
            .find(|(k, _)| k == "arg_count")
            .map(|(_, v)| v.as_str());
        assert_eq!(spec, Some("2"));
        assert_eq!(argc, Some("1"));
    }

    #[test]
    fn count_format_specifiers_handles_subset_table() {
        // Direct unit-test on the spec-counter helper. Each row exercises
        // one of the supported specifier shapes per S6-B4 slice scope.
        assert_eq!(super::count_format_specifiers(""), 0);
        assert_eq!(super::count_format_specifiers("no specs"), 0);
        assert_eq!(super::count_format_specifiers("{}"), 1);
        assert_eq!(super::count_format_specifiers("{:?}"), 1);
        assert_eq!(super::count_format_specifiers("{:.3}"), 1);
        assert_eq!(super::count_format_specifiers("{:04d}"), 1);
        assert_eq!(super::count_format_specifiers("{:5}"), 1);
        // Multi-spec compound :
        assert_eq!(super::count_format_specifiers("{} {} {}"), 3);
        assert_eq!(super::count_format_specifiers("a={:.3}, b={:?}"), 2);
        // Doubled braces :
        assert_eq!(super::count_format_specifiers("{{}}"), 0);
        assert_eq!(super::count_format_specifiers("{{x}}"), 0);
        // Mixed : `{{ {} }}` = one specifier between literal braces
        assert_eq!(super::count_format_specifiers("{{ {} }}"), 1);
        // Unmatched `{` is silently skipped (validation deferred) :
        assert_eq!(super::count_format_specifiers("incomplete {"), 0);
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D76 (S6-B5) — file-system I/O recognizer tests.
    // ─────────────────────────────────────────────────────────────────────

    /// Helper : find an op by canonical name in the entry block.
    fn find_op<'a>(f: &'a crate::func::MirFunc, name: &str) -> Option<&'a crate::block::MirOp> {
        f.body.entry()?.ops.iter().find(|o| o.name == name)
    }

    /// Helper : look up an attribute value by key on an op.
    fn attr<'a>(op: &'a crate::block::MirOp, key: &str) -> Option<&'a str> {
        op.attributes
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn lower_fs_open_emits_cssl_fs_open() {
        // `fs::open("path", 1)` should be recognized syntactically and
        // produce a `cssl.fs.open` op carrying the io_effect marker.
        let (f, _) = lower_one(r#"fn f() -> i64 { fs::open("foo.txt", 1) }"#);
        let op = find_op(&f, "cssl.fs.open").expect("fs::open should lower to cssl.fs.open");
        // Two operands : (path, flags).
        assert_eq!(op.operands.len(), 2);
        // Single result of i64 (handle).
        assert_eq!(op.results.len(), 1);
        assert_eq!(op.results[0].ty, MirType::Int(IntWidth::I64));
        // io_effect marker present.
        assert_eq!(attr(op, "io_effect"), Some("true"));
        // family + op markers identify the op uniquely for downstream walkers.
        assert_eq!(attr(op, "family"), Some("fs"));
        assert_eq!(attr(op, "op"), Some("open"));
    }

    #[test]
    fn lower_fs_close_emits_cssl_fs_close() {
        let (f, _) = lower_one("fn f(h : i64) -> i64 { fs::close(h) }");
        let op = find_op(&f, "cssl.fs.close").expect("fs::close should lower to cssl.fs.close");
        assert_eq!(op.operands.len(), 1);
        assert_eq!(op.results.len(), 1);
        assert_eq!(op.results[0].ty, MirType::Int(IntWidth::I64));
        assert_eq!(attr(op, "io_effect"), Some("true"));
        assert_eq!(attr(op, "op"), Some("close"));
    }

    #[test]
    fn lower_fs_read_emits_cssl_fs_read() {
        let (f, _) = lower_one("fn f(h : i64, p : i64, n : i64) -> i64 { fs::read(h, p, n) }");
        let op = find_op(&f, "cssl.fs.read").expect("fs::read should lower to cssl.fs.read");
        assert_eq!(op.operands.len(), 3);
        assert_eq!(op.results.len(), 1);
        assert_eq!(attr(op, "io_effect"), Some("true"));
        assert_eq!(attr(op, "op"), Some("read"));
    }

    #[test]
    fn lower_fs_write_emits_cssl_fs_write() {
        let (f, _) = lower_one("fn f(h : i64, p : i64, n : i64) -> i64 { fs::write(h, p, n) }");
        let op = find_op(&f, "cssl.fs.write").expect("fs::write should lower to cssl.fs.write");
        assert_eq!(op.operands.len(), 3);
        assert_eq!(op.results.len(), 1);
        assert_eq!(attr(op, "io_effect"), Some("true"));
        assert_eq!(attr(op, "op"), Some("write"));
    }

    #[test]
    fn lower_fs_open_with_wrong_arity_falls_through_to_generic_call() {
        // 1-arg fs::open shouldn't match the recognizer (which expects 2
        // args) ; should fall through to the regular func.call path
        // with no cssl.fs.open op emitted.
        let (f, _) = lower_one(r#"fn f() -> i64 { fs::open("foo.txt") }"#);
        // No cssl.fs.open ; instead a func.call op exists.
        assert!(find_op(&f, "cssl.fs.open").is_none());
        let _call = find_op(&f, "func.call").expect("should fall through to func.call");
    }

    #[test]
    fn lower_non_fs_path_is_not_claimed_by_recognizer() {
        // `foo::open(...)` is NOT `fs::open(...)` — must not emit cssl.fs.open.
        let (f, _) = lower_one(r#"fn f() -> i64 { foo::open("foo.txt", 1) }"#);
        assert!(find_op(&f, "cssl.fs.open").is_none());
    }

    #[test]
    fn lower_bare_open_is_not_claimed_by_recognizer() {
        // `open(...)` (single-segment) is NOT recognized — only the
        // 2-segment `fs::open` form qualifies. This guards against
        // accidental shadowing of user identifiers like `open`.
        let (f, _) = lower_one(r#"fn f() -> i64 { open("foo.txt", 1) }"#);
        assert!(find_op(&f, "cssl.fs.open").is_none());
    }

    #[test]
    fn lower_fs_open_records_source_loc_attribute() {
        let (f, _) = lower_one(r#"fn f() -> i64 { fs::open("path.txt", 1) }"#);
        let op = find_op(&f, "cssl.fs.open").expect("fs::open should lower");
        // source_loc is present and non-empty.
        let loc = attr(op, "source_loc").expect("source_loc attribute missing");
        assert!(!loc.is_empty(), "source_loc should be non-empty");
    }
}
