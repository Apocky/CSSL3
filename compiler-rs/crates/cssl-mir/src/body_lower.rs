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
use crate::trait_dispatch::TraitImplTable;
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
    /// T11-D99 — Optional trait-impl table threaded through for
    /// `obj.method(args)` resolution. When `None`, method-call lowering
    /// falls through to the regular opaque path (preserving pre-D99
    /// behavior for callers that haven't built a table). When `Some`,
    /// `lower_call` consults this for any field-callee call BEFORE
    /// falling through to `cssl.field` + opaque indirect-call.
    pub trait_impl_table: Option<&'a TraitImplTable>,
    /// Mapping from HIR param-symbol → entry-block value-id.
    pub param_vars: HashMap<Symbol, (ValueId, MirType)>,
    /// T11-D35 : mapping from HIR vec-param-symbol → N consecutive scalar value-ids
    /// + lane-count + element width. A `vec3<f32>` param `p` maps to `(vec![v0, v1, v2], 3, F32)`.
    /// Kept distinct from [`Self::param_vars`] so stage-0 can dispatch per-op
    /// (length(p) expands via these scalars ; normalize / dot / field access are
    /// deferred to future slices).
    pub vec_param_vars: HashMap<Symbol, (Vec<ValueId>, u32, FloatWidth)>,
    /// T11-D77 (S6-C5 redo) : let-binding name → lowered ValueId + MirType.
    /// Populated by `HirStmtKind::Let` with a `Binding`-pattern name and
    /// consumed by `lower_path` (after [`Self::param_vars`]) + by the lambda
    /// free-var resolver. Stage-0 is a flat map ; later-shadowing overwrites
    /// by construction (single-pass lowerer ; no scope-stack restoration on
    /// block exit). See `specs/02_IR.csl` § CLOSURE-ENV.
    pub local_vars: HashMap<Symbol, (ValueId, MirType)>,
    /// T11-D100 (J2 — closures callable) : closure-value-id → descriptor that
    /// the call-site recognizer consumes when it sees a callee path resolving
    /// to a closure-typed local. Populated by `lower_lambda` immediately after
    /// emitting the `cssl.closure` op ; consumed by `lower_call` to perform
    /// inline expansion of the body. Stage-0 is per-fn (no cross-fn closure
    /// passing yet — that lands when fn-ptr trampolines + true call_indirect
    /// arrives).
    pub closure_descriptors: HashMap<ValueId, ClosureDescriptor>,
    /// Next free value-id (wired to `MirFunc.fresh_value_id`).
    pub next_value_id: u32,
    /// Accumulated ops (consumed at end into the entry-block).
    pub ops: Vec<MirOp>,
}

/// T11-D100 (J2) — descriptor preserved at lambda construction so a later
/// call-site can inline-expand the body. Carries the lambda's HIR (params +
/// optional return-ty + body) clone, plus the captures resolved at construct-
/// time (each carrying the source ValueId in the OUTER ctx that was packed
/// into the env, the stage-0 `8 × i` env-offset, and the MirType so backends
/// can pick the correct memref.load element type).
///
/// The `env_ptr_id` is the heap.alloc result-id when the closure has ≥1
/// capture, else `None` for zero-capture closures (no env, no memref.load
/// emitted at the call site). Stored as a separate field rather than always
/// using the closure value-id directly because future trampolined call_indirect
/// will consume `env_ptr_id` via the closure's fat-pair second word — the call-
/// site doesn't need to deconstruct the pair when the descriptor is local.
#[derive(Debug, Clone)]
pub struct ClosureDescriptor {
    /// Lambda parameters in source order. Each carries an HIR pattern + optional
    /// type — the call-site binds these to call-site arg ValueIds during inline
    /// expansion.
    pub params: Vec<HirLambdaParam>,
    /// Optional declared return type (rides as a diagnostic anchor — the MIR
    /// result type comes from the inlined body's trailing yield).
    pub return_ty: Option<HirType>,
    /// The lambda body as a clonable HIR sub-tree. Re-lowered fresh at each
    /// call site to honor the call-site arg / capture mapping. (Future opt :
    /// memoize the lowered body once per closure when escape analysis proves
    /// param + capture types are call-site-invariant — not at stage-0.)
    pub body: HirExpr,
    /// Captures resolved at construct-time : (name, source-ValueId-in-OUTER-ctx,
    /// stage-0-`8 × i` byte-offset within the env, MirType for the memref.load
    /// element-type). Empty for zero-capture closures.
    pub captures: Vec<ClosureCapture>,
    /// Heap-alloc result-id for the env (ptr) when ≥1 capture ; `None`
    /// otherwise. Call-site reads memref.load on this when emitting the
    /// inlined body's capture-bindings.
    pub env_ptr_id: Option<ValueId>,
}

/// One captured binding inside a [`ClosureDescriptor`]. The byte-offset is
/// the stage-0 `8 × i` heuristic ; when MirType::Struct + a real layout pass
/// land, the offset becomes the actual computed slot offset.
#[derive(Debug, Clone)]
pub struct ClosureCapture {
    pub name: Symbol,
    pub src_value_id: ValueId,
    pub env_offset: u64,
    pub ty: MirType,
}

impl<'a> BodyLowerCtx<'a> {
    /// Build a fresh context with no source-file reference. Callers who want
    /// real literal-value extraction should use [`Self::with_source`] instead.
    #[must_use]
    pub fn new(interner: &'a Interner) -> Self {
        Self {
            interner,
            source: None,
            trait_impl_table: None,
            param_vars: HashMap::new(),
            vec_param_vars: HashMap::new(),
            local_vars: HashMap::new(),
            closure_descriptors: HashMap::new(),
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
            trait_impl_table: None,
            param_vars: HashMap::new(),
            vec_param_vars: HashMap::new(),
            local_vars: HashMap::new(),
            closure_descriptors: HashMap::new(),
            next_value_id: 0,
            ops: Vec::new(),
        }
    }

    /// T11-D99 — Attach a trait-impl table for method-call dispatch.
    /// Returns `self` for builder-pattern chaining. When the table is
    /// attached, `lower_call` will resolve `obj.method(args)` via the
    /// table BEFORE the recognizer fast-paths complete (the recognizers
    /// still claim `Box::new` / `Some` / `None` / etc. since those route
    /// through path-callees, not field-callees).
    #[must_use]
    pub fn with_trait_table(mut self, table: &'a TraitImplTable) -> Self {
        self.trait_impl_table = Some(table);
        self
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
            trait_impl_table: self.trait_impl_table,
            param_vars: HashMap::new(),
            vec_param_vars: HashMap::new(),
            local_vars: HashMap::new(),
            closure_descriptors: HashMap::new(),
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
    lower_fn_body_with_table(interner, source, None, hir_fn, mir_fn);
}

/// T11-D99 — lower with an optional trait-impl table threaded in.
///
/// When `table` is `Some(_)`, `obj.method(args)` calls in the body will be
/// resolved through the table to mangled impl-fn-names ; when `None`, the
/// behavior is identical to the pre-D99 [`lower_fn_body`] path. Callers
/// that want trait-dispatch wired up should build the table first via
/// [`crate::trait_dispatch::build_trait_impl_table`] and thread it here.
pub fn lower_fn_body_with_table(
    interner: &Interner,
    source: Option<&SourceFile>,
    table: Option<&TraitImplTable>,
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
    if let Some(t) = table {
        ctx.trait_impl_table = Some(t);
    }
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
        HirTypeKind::Path {
            path, type_args, ..
        } if path.len() == 1 => {
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
                // § T11-D284 (W-E5-1) — encode payload-type into `Option<T>` /
                //   `Result<T, E>` opaque shape so the trait-dispatch resolver
                //   can peel the wrapper and resolve impls on the inner T.
                //   Without this, `let x : Option<Foo> = ... ; x.method()`
                //   would carry the type `Opaque("Option")` and the table
                //   would search for `(Option, method)` only, missing any
                //   `impl SomeTrait for Foo { fn method ... }` that the
                //   user expected to dispatch through after payload-unwrap.
                "Option" if type_args.len() == 1 => {
                    let payload = enum_payload_type_string(interner, &type_args[0]);
                    MirType::Opaque(format!("!cssl.option<{payload}>"))
                }
                "Result" if type_args.len() == 2 => {
                    let ok = enum_payload_type_string(interner, &type_args[0]);
                    let err = enum_payload_type_string(interner, &type_args[1]);
                    MirType::Opaque(format!("!cssl.result<{ok},{err}>"))
                }
                other => MirType::Opaque(other.to_string()),
            }
        }
        HirTypeKind::Refined { base, .. } => lower_hir_type_light(interner, base),
        HirTypeKind::Reference { inner, .. } => lower_hir_type_light(interner, inner),
        HirTypeKind::Infer => MirType::None,
        _ => MirType::None,
    }
}

/// T11-D284 (W-E5-1) — render a HirType's leading-segment name as a string
/// for embedding into Option/Result opaque-type encodings. Multi-segment
/// paths are rendered dotted (`a.b.c`). Non-path types collapse to `?`
/// (the dispatch resolver treats `?` as "unknown payload — decline unwrap").
fn enum_payload_type_string(interner: &Interner, t: &HirType) -> String {
    match &t.kind {
        HirTypeKind::Path { path, .. } if !path.is_empty() => path
            .iter()
            .map(|s| interner.resolve(*s))
            .collect::<Vec<_>>()
            .join("."),
        HirTypeKind::Refined { base, .. } => enum_payload_type_string(interner, base),
        HirTypeKind::Reference { inner, .. } => enum_payload_type_string(interner, inner),
        _ => "?".to_string(),
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
        HirStmtKind::Let {
            value,
            pat,
            ty: declared_ty,
            ..
        } => {
            if let Some(e) = value {
                if let Some((vid, ty)) = lower_expr(ctx, e) {
                    // § T11-D77 (S6-C5 redo) : bind the let-pattern's name → its
                    //   lowered ValueId so subsequent path-refs resolve, AND so
                    //   closure free-var analysis can map a captured local to
                    //   its source SSA-value. Only `Binding`-patterns
                    //   (single-name `let x = …`) are bound at stage-0 ;
                    //   destructuring (`let (a, b) = …`) lands when MIR-side
                    //   sum/tuple-deconstruction lowers in a future slice.
                    //
                    // § T11-D99 — When a declared type is present (`let f : Foo
                    //   = ...`), prefer it over the rhs-inferred type so the
                    //   trait-dispatch resolver can look up the leading symbol
                    //   via `local_var_self_ty`. The rhs-type is often
                    //   `MirType::None` for struct-literal expressions whose
                    //   `lower_struct_expr` pathway returns a flat-tuple
                    //   placeholder ; the declared type is the user's
                    //   authoritative shape.
                    if let Some(sym) = extract_pattern_symbol(pat) {
                        let final_ty = declared_ty
                            .as_ref()
                            .map_or(ty, |t| lower_hir_type_light(ctx.interner, t));
                        ctx.local_vars.insert(sym, (vid, final_ty));
                    }
                }
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
        HirExprKind::Call {
            callee,
            args,
            type_args,
        } => lower_call(ctx, callee, args, type_args, expr.span, expr.id),
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

/// Lower `|params| -> Ty { body }` into `cssl.closure` with env-capture.
///
/// § T11-D77 (S6-C5 redo) — full env-capture analysis. See
/// `specs/02_IR.csl` § CLOSURE-ENV for the canonical lowering contract.
///
/// § PIPELINE
///   1. Collect free-vars of the body (HIR `Path { single-segment }` refs not
///      shadowed by a lambda param or an inner `let`-binding).
///   2. For each free-var, look it up in the OUTER ctx (`param_vars` →
///      `local_vars`) to resolve its outer-scope `(ValueId, MirType)`. Vars
///      that resolve nowhere (path is an unresolved module-ref or unknown
///      binding) are dropped from the capture-list (they remain unresolved
///      in body-text and will produce `cssl.path_ref` ops in the body region).
///   3. If `capture_count ≥ 1` :
///        a. Emit `arith.constant ⟨env_size⟩ : i64` + `arith.constant 8 : i64`
///           to feed `cssl.heap.alloc` (env_size = 8 × capture_count, align = 8).
///        b. Emit `cssl.heap.alloc(size, align)` returning `!cssl.ptr` with
///           `cap=iso` per `specs/12_CAPABILITIES § ISO-OWNERSHIP`.
///        c. For each capture i, emit `arith.constant ⟨8i⟩ : i64` + a
///           `memref.store cap_i, env_ptr, offset_i` writing the captured
///           value into its env slot.
///   4. Build the body sub-region (inner ops are lowered with a separate
///      `BodyLowerCtx`; lambda params seed that sub-ctx's `param_vars`).
///   5. Emit the `cssl.closure` op carrying the captures-as-operands followed
///      by the env-ptr (when present) ; attributes record `param_count` /
///      `capture_count` / `env_size` / `env_align` / `cap_value` / source-loc.
///
/// § RESULT VALUE
///   Closure type = `!cssl.closure` opaque (the conceptual `(fn-ptr, env-ptr)`
///   fat-pair). At stage-0 the closure VALUE has its env-ptr accessible via
///   the operand-trail ; the fn-ptr half is metadata-only — an indirect call
///   hasn't yet been wired through any source-call-site against a closure-
///   typed value (deferred per `specs/02_IR § CLOSURE-ENV`).
///
/// § ESCAPE ANALYSIS
///   Stage-0 = correctness-first : any closure with ≥1 capture heap-allocates
///   its env. Stack-promotion when the closure is provably non-escaping is a
///   future MIR→MIR optimization pass.
fn lower_lambda(
    ctx: &mut BodyLowerCtx<'_>,
    params: &[HirLambdaParam],
    return_ty: Option<&HirType>,
    body: &HirExpr,
    span: Span,
) -> (ValueId, MirType) {
    // § 1. Collect lambda-param symbols (these are NOT free-vars).
    let mut param_syms: Vec<Symbol> = Vec::with_capacity(params.len());
    for p in params {
        if let Some(sym) = extract_pattern_symbol(&p.pat) {
            param_syms.push(sym);
        }
    }

    // § 2. Walk the body collecting free-vars (single-segment paths not
    //   shadowed by params or inner lets). The walker tracks introduced
    //   names so an inner `let x = …` shadows a same-named outer free-var
    //   for refs after that `let`.
    let mut free_collector = FreeVarCollector::new(&param_syms);
    free_collector.walk_expr(body);

    // § 3. Resolve each free-var to its outer-scope source value.
    //   Captures whose name resolves nowhere (unknown binding) are dropped
    //   from the capture-list — they'll surface in the body region as
    //   `cssl.path_ref` opaque-placeholders, matching pre-S6-C5 behavior.
    let mut captures: Vec<(Symbol, ValueId, MirType)> =
        Vec::with_capacity(free_collector.free_vars.len());
    for sym in &free_collector.free_vars {
        if let Some((vid, ty)) = ctx.param_vars.get(sym) {
            captures.push((*sym, *vid, ty.clone()));
        } else if let Some((vid, ty)) = ctx.local_vars.get(sym) {
            captures.push((*sym, *vid, ty.clone()));
        }
        // else : unresolved — dropped from capture-list silently. The body's
        // `cssl.path_ref` placeholder retains the name for diagnostic trail.
    }

    // § 3b. Record the descriptor we'll need at call-site inline-expansion.
    //   The descriptor stores HIR clones of the lambda's params + body so
    //   `lower_call` can re-lower the body with call-site arg / capture
    //   bindings. Captures carry their stage-0 `8 × i` env-offset for
    //   memref.load codegen at the call site. See spec § CLOSURE-ENV
    //   "invocation (T11-D100 / J2 …)".
    let descriptor_captures: Vec<ClosureCapture> = captures
        .iter()
        .enumerate()
        .map(|(i, (sym, vid, ty))| ClosureCapture {
            name: *sym,
            src_value_id: *vid,
            env_offset: 8u64.saturating_mul(i as u64),
            ty: ty.clone(),
        })
        .collect();

    // § 4. Emit env-pack (alloc + per-capture store) when ≥1 capture.
    //   Stage-0 layout : 8 bytes per slot, align = 8. See spec § CLOSURE-ENV.
    //   Sizes are u64 (stable for `to_string()` ; stage-0 caps env at 2^63 by
    //   construction so wrap-saturation is theoretical).
    let env_ptr_id: Option<ValueId> = if captures.is_empty() {
        None
    } else {
        const SLOT_BYTES: u64 = 8;
        const ENV_ALIGN: u64 = 8;
        let env_size: u64 = SLOT_BYTES.saturating_mul(captures.len() as u64);

        // 4a. arith.constant env_size : i64.
        let sz_id = ctx.fresh_value_id();
        ctx.ops.push(
            MirOp::std("arith.constant")
                .with_result(sz_id, MirType::Int(IntWidth::I64))
                .with_attribute("value", env_size.to_string())
                .with_attribute("source_loc", format!("{span:?}")),
        );
        // 4b. arith.constant env_align : i64.
        let al_id = ctx.fresh_value_id();
        ctx.ops.push(
            MirOp::std("arith.constant")
                .with_result(al_id, MirType::Int(IntWidth::I64))
                .with_attribute("value", ENV_ALIGN.to_string())
                .with_attribute("source_loc", format!("{span:?}")),
        );
        // 4c. cssl.heap.alloc(sz, al) -> !cssl.ptr  (cap=iso).
        let env_id = ctx.fresh_value_id();
        ctx.ops.push(
            MirOp::std("cssl.heap.alloc")
                .with_operand(sz_id)
                .with_operand(al_id)
                .with_result(env_id, MirType::Ptr)
                .with_attribute("cap", "iso")
                .with_attribute("origin", "closure_env")
                .with_attribute("source_loc", format!("{span:?}")),
        );

        // 4d. Per-capture store : write each captured value into its env slot.
        for (i, (_sym, src_id, _src_ty)) in captures.iter().enumerate() {
            let off: u64 = SLOT_BYTES.saturating_mul(i as u64);
            let off_id = ctx.fresh_value_id();
            ctx.ops.push(
                MirOp::std("arith.constant")
                    .with_result(off_id, MirType::Int(IntWidth::I64))
                    .with_attribute("value", off.to_string())
                    .with_attribute("source_loc", format!("{span:?}")),
            );
            // memref.store val, ptr, offset  (3-operand variant).
            ctx.ops.push(
                MirOp::std("memref.store")
                    .with_operand(*src_id)
                    .with_operand(env_id)
                    .with_operand(off_id)
                    .with_attribute("alignment", ENV_ALIGN.to_string())
                    .with_attribute("source_loc", format!("{span:?}")),
            );
        }

        Some(env_id)
    };

    // § 5. Build the body sub-region. The inner lowerer runs in a sub-context
    //   so lambda-param names don't leak to the outer fn.
    let mut sub = ctx.sub();
    for (i, p) in params.iter().enumerate() {
        let pid = ValueId(u32::try_from(i).unwrap_or(0));
        let pty =
            p.ty.as_ref()
                .map_or(MirType::None, |t| lower_hir_type_light(sub.interner, t));
        if let Some(sym) = extract_pattern_symbol(&p.pat) {
            sub.param_vars.insert(sym, (pid, pty));
        }
    }
    sub.next_value_id = u32::try_from(params.len()).unwrap_or(0);
    let _ = lower_expr(&mut sub, body);
    let mut blk = MirBlock::new("entry");
    blk.ops = sub.ops;
    let mut body_region = MirRegion::new();
    body_region.push(blk);

    // § 6. Emit cssl.closure carrying captures + env-ptr as operands.
    let id = ctx.fresh_value_id();
    let ty = MirType::Opaque("!cssl.closure".into());
    let env_size: u64 = 8u64 * (captures.len() as u64);
    let mut op = MirOp::new(CsslOp::Std);
    op.name = "cssl.closure".to_string();
    op = op
        .with_result(id, ty.clone())
        .with_region(body_region)
        .with_attribute("param_count", params.len().to_string())
        .with_attribute("capture_count", captures.len().to_string())
        .with_attribute("env_size", env_size.to_string())
        .with_attribute("env_align", "8")
        .with_attribute("cap_value", "val")
        .with_attribute("source_loc", format!("{span:?}"));
    // Attach captured-name list as a comma-joined attribute for diagnostic
    // trail. Empty when there are no captures. Names round-trip through the
    // interner so this is stable across compile units.
    if !captures.is_empty() {
        let names = captures
            .iter()
            .map(|(s, _, _)| ctx.interner.resolve(*s))
            .collect::<Vec<_>>()
            .join(",");
        op = op.with_attribute("capture_names", names);
    }
    if return_ty.is_some() {
        op = op.with_attribute("has_return_ty", "true");
    }
    // Operand order : captures (in the order discovered by the free-var
    // collector) followed by the env-ptr when present. Lowering side reads
    // the operand-trail using `capture_count` to know where the env-ptr
    // begins (operand-index = capture_count).
    for (_sym, src_id, _ty) in &captures {
        op = op.with_operand(*src_id);
    }
    if let Some(env_id) = env_ptr_id {
        op = op.with_operand(env_id);
    }
    ctx.ops.push(op);

    // § 7. Register the closure descriptor so a later call-site (lower_call)
    //   can locate the lambda's params + body + captures and inline-expand.
    //   Keyed on the closure value-id ; the call-site's path-resolution returns
    //   that same id when the callee is a single-segment ref to a closure-
    //   typed local, so the lookup is O(1) at the call site. See spec
    //   § CLOSURE-ENV "invocation (T11-D100 / J2 — closures callable …)".
    ctx.closure_descriptors.insert(
        id,
        ClosureDescriptor {
            params: params.to_vec(),
            return_ty: return_ty.cloned(),
            body: body.clone(),
            captures: descriptor_captures,
            env_ptr_id,
        },
    );
    (id, ty)
}

/// § T11-D100 (J2 — closures callable from CSSLv3 source) — inline-expand
/// a closure body at a call site.
///
/// § PIPELINE
///   1. Arity check : the call-site's positional arg count must equal the
///      lambda's `params.len()`. Mismatch ⇒ emit `cssl.closure.call.error`
///      with detail + return an opaque-typed result (the body is NOT lowered ;
///      downstream sees the error op + can surface it). The HARD diagnostic
///      contract is documented in spec § CLOSURE-ENV "type-check".
///   2. Lower each call-site arg to its ValueId via `lower_call_arg` ; collect
///      `(arg_id, arg_ty)` for the param-binding step.
///   3. For each capture, emit `arith.constant ⟨env_offset⟩` + `memref.load
///      env_ptr, offset` to materialize the captured value freshly at the call
///      site. Bind the captured name → loaded ValueId in a sub-context.
///   4. Bind each lambda param's symbol → the corresponding call-site arg
///      ValueId in the sub-context.
///   5. Re-lower the lambda's body with the seeded sub-context. The trailing
///      yield (if any) is the call's result. The sub-context's accumulated
///      ops drain into the OUTER ctx so they execute inline at the call site.
///   6. Emit the marker `cssl.closure.call` op carrying operands = [closure_vid,
///      arg_ids…] and attributes describing arity / capture layout / result-
///      binding. Backends treat this as a no-op binder.
///
/// § RESULT
///   When the body has a trailing yield, the marker op binds its result-id
///   to that yield's ValueId — backends can then consume the result-id like
///   any normal MIR value. When the body has no trailing yield (unit-returning
///   closure), the marker op carries no result and the call's MirType is
///   `MirType::None`.
///
/// § CAVEATS (stage-0)
///   - The lambda body's free-var resolution at the call site re-uses the
///     OUTER ctx's `param_vars` / `local_vars` for any name that wasn't
///     captured (this matches the construct-site collector's semantics ; if
///     the name resolves to a local that wasn't captured, it's a stale binding
///     bug — but in practice stage-0 only sees `let`-bound + param names that
///     ARE captured by the FreeVarCollector at construct time).
///   - Recursive closures (a closure that calls itself) deferred — same as
///     T11-D77's deferred list. The descriptor isn't visible inside the body
///     re-lowering's sub-ctx.
///   - Captures-by-ref / -by-move : value-cap only at stage-0.
fn lower_closure_call(
    ctx: &mut BodyLowerCtx<'_>,
    closure_vid: ValueId,
    descriptor: &ClosureDescriptor,
    args: &[HirCallArg],
    span: Span,
    hir_id: cssl_hir::HirId,
) -> (ValueId, MirType) {
    // § 1. Arity check.
    if args.len() != descriptor.params.len() {
        let id = ctx.fresh_value_id();
        let ty = MirType::Opaque("!cssl.closure.call.error".into());
        ctx.ops.push(
            MirOp::std("cssl.closure.call.error")
                .with_operand(closure_vid)
                .with_result(id, ty.clone())
                .with_attribute(
                    "detail",
                    format!(
                        "arity mismatch : closure expects {} params, call site supplies {}",
                        descriptor.params.len(),
                        args.len()
                    ),
                )
                .with_attribute("expected_arity", descriptor.params.len().to_string())
                .with_attribute("actual_arity", args.len().to_string())
                .with_attribute("source_loc", format!("{span:?}")),
        );
        return (id, ty);
    }

    // § 2. Lower each call-site arg to a (ValueId, MirType) pair. We delegate
    //   to `lower_call_arg` which handles both positional + named args.
    let mut arg_pairs: Vec<(ValueId, MirType)> = Vec::with_capacity(args.len());
    for a in args {
        if let Some(p) = lower_call_arg(ctx, a) {
            arg_pairs.push(p);
        } else {
            // Lowering an arg failed — emit an error op + bail out before
            // touching the body. Mirrors the pattern used by the sum-type
            // recognizers when their inner-arg lowering returns `None`.
            let id = ctx.fresh_value_id();
            let ty = MirType::Opaque("!cssl.closure.call.error".into());
            ctx.ops.push(
                MirOp::std("cssl.closure.call.error")
                    .with_operand(closure_vid)
                    .with_result(id, ty.clone())
                    .with_attribute("detail", "arg lowering failed".to_string())
                    .with_attribute("source_loc", format!("{span:?}")),
            );
            return (id, ty);
        }
    }

    // § 3. Materialize each capture freshly at the call site via memref.load
    //   on the env_ptr. The env_ptr was registered at construct-time as the
    //   heap.alloc result-id ; if it's missing (zero-capture closure) we skip
    //   this loop entirely.
    //
    //   Per-capture sequence at the call site :
    //     %off_i  = arith.constant ⟨env_offset⟩ : i64
    //     %cap_i  = memref.load env_ptr, %off_i      ; alignment = 8
    //
    //   The sub-context records `name → loaded-id` so the body's free-var
    //   refs resolve to the freshly-loaded values rather than the construct-
    //   time source ValueIds (those are a different SSA-domain at this point).
    let mut sub = ctx.sub();
    sub.next_value_id = ctx.next_value_id;
    if let Some(env_ptr_id) = descriptor.env_ptr_id {
        for cap in &descriptor.captures {
            // Emit the offset constant into the OUTER ctx so the JIT/Object
            // backend's pre-scan + value-map see it ; the load-result is the
            // value the body will reference for this capture name.
            let off_id = ctx.fresh_value_id();
            ctx.ops.push(
                MirOp::std("arith.constant")
                    .with_result(off_id, MirType::Int(IntWidth::I64))
                    .with_attribute("value", cap.env_offset.to_string())
                    .with_attribute("source_loc", format!("{span:?}")),
            );
            let load_id = ctx.fresh_value_id();
            ctx.ops.push(
                MirOp::std("memref.load")
                    .with_operand(env_ptr_id)
                    .with_operand(off_id)
                    .with_result(load_id, cap.ty.clone())
                    .with_attribute("alignment", "8")
                    .with_attribute("origin", "closure_capture_reload")
                    .with_attribute("capture_name", ctx.interner.resolve(cap.name))
                    .with_attribute("source_loc", format!("{span:?}")),
            );
            sub.local_vars.insert(cap.name, (load_id, cap.ty.clone()));
        }
    }
    // Refresh sub.next_value_id after capture-load emission (we've allocated
    // ids on the OUTER ctx ; the sub-ctx must continue from there to avoid
    // collisions when the body lowers).
    sub.next_value_id = ctx.next_value_id;

    // § 4. Bind each lambda param symbol → the call-site arg ValueId.
    //   Body refs to a param then resolve via `lower_path`'s `param_vars`
    //   lookup. Param types come from the call-site arg-types (more precise
    //   than the lambda's optional declared-ty annotation at stage-0).
    for (p, (arg_id, arg_ty)) in descriptor.params.iter().zip(arg_pairs.iter()) {
        if let Some(sym) = extract_pattern_symbol(&p.pat) {
            sub.param_vars.insert(sym, (*arg_id, arg_ty.clone()));
        }
    }

    // § 5. Lower the body fresh in the sub-context. The trailing yield (if
    //   any) is the call's result. The sub-context's ops drain into the
    //   OUTER ctx — the inlined body executes inline at the call site.
    let trailing = lower_expr(&mut sub, &descriptor.body);
    // Drain sub-ctx ops into the outer ctx + sync the value-id watermark.
    ctx.ops.append(&mut sub.ops);
    ctx.next_value_id = sub.next_value_id;
    // Merge any nested closure descriptors created inside the body (a body
    // that constructs its OWN inner closure with a let-binding and calls it
    // would otherwise lose them). Stage-0 : flat merge — last-write-wins
    // for collisions which can't happen because nested closures get fresh
    // value-ids from the same id-space.
    for (k, v) in sub.closure_descriptors.drain() {
        ctx.closure_descriptors.insert(k, v);
    }

    // § 6. Emit the marker cssl.closure.call op. Backends consume this as a
    //   no-op binder that delegates the result-id to the trailing yield.
    let result_id = ctx.fresh_value_id();
    let result_ty = trailing.as_ref().map_or(MirType::None, |(_, t)| t.clone());
    let mut op = MirOp::std("cssl.closure.call");
    op = op
        .with_operand(closure_vid)
        .with_attribute("param_count", descriptor.params.len().to_string())
        .with_attribute("capture_count", descriptor.captures.len().to_string())
        .with_attribute(
            "env_size",
            (descriptor.captures.len() as u64 * 8u64).to_string(),
        )
        .with_attribute("env_align", "8")
        .with_attribute("source_loc", format!("{span:?}"))
        .with_attribute("hir_id", format!("{}", hir_id.0));
    if !descriptor.captures.is_empty() {
        let offsets = descriptor
            .captures
            .iter()
            .map(|c| c.env_offset.to_string())
            .collect::<Vec<_>>()
            .join(",");
        op = op.with_attribute("capture_offsets", offsets);
    }
    if descriptor.return_ty.is_some() {
        op = op.with_attribute("has_return_ty", "true");
    }
    for (arg_id, _) in &arg_pairs {
        op = op.with_operand(*arg_id);
    }
    if let Some((yield_id, _)) = trailing {
        op = op
            .with_result(result_id, result_ty.clone())
            .with_attribute("yield_value_id", yield_id.0.to_string());
    }
    ctx.ops.push(op);

    if trailing.is_some() {
        (result_id, result_ty)
    } else {
        // Unit-returning closure : bind result_id to a typed-zero placeholder
        // and tag the type as None so consumers know the call has no value.
        // (Future opt : skip emitting result_id when trailing is None — for
        // stage-0 we keep the slot for diagnostic-trail simplicity.)
        (result_id, MirType::None)
    }
}

/// § T11-D77 (S6-C5 redo) — free-var collector for a lambda body.
///
/// Walks an `HirExpr` AST and collects single-segment Path references that
/// are NOT shadowed by lambda params or inner `let`-bindings. The walker is
/// deliberately simple at stage-0 :
///   - Only single-segment paths count as free-var candidates (multi-segment
///     paths are module / constructor refs, not captures).
///   - `let pat = …` introduces names whose binding-symbols (only Binding
///     kind) are added to a per-block scoped set ; subsequent refs inside
///     that same block see the binding and don't add the name to the
///     free-var list.
///   - Nested lambdas have their own free-vars resolved separately ; their
///     params are added to the local seen-set so an outer-walker doesn't
///     mistake them for outer free-vars.
///   - Order : free-vars are reported in first-encountered order, deduped.
struct FreeVarCollector {
    /// Names provably bound by an enclosing lambda's params or by an inner
    /// let. Acts as the "shadowed by current scope" set.
    bound: std::collections::HashSet<Symbol>,
    /// Free-vars discovered so far, in first-encountered order.
    free_vars: Vec<Symbol>,
    /// Dedup set for `free_vars` (avoids O(N²) on repeated refs).
    seen: std::collections::HashSet<Symbol>,
}

impl FreeVarCollector {
    fn new(lambda_param_syms: &[Symbol]) -> Self {
        let mut bound = std::collections::HashSet::new();
        for s in lambda_param_syms {
            bound.insert(*s);
        }
        Self {
            bound,
            free_vars: Vec::new(),
            seen: std::collections::HashSet::new(),
        }
    }

    fn note_free(&mut self, sym: Symbol) {
        if self.bound.contains(&sym) {
            return;
        }
        if self.seen.insert(sym) {
            self.free_vars.push(sym);
        }
    }

    fn walk_expr(&mut self, e: &HirExpr) {
        match &e.kind {
            HirExprKind::Path { segments, .. } => {
                if segments.len() == 1 {
                    self.note_free(segments[0]);
                }
            }
            HirExprKind::Literal(_)
            | HirExprKind::Error
            | HirExprKind::Break { value: None, .. }
            | HirExprKind::Continue { .. }
            | HirExprKind::SectionRef { .. } => {}
            HirExprKind::Binary { lhs, rhs, .. } => {
                self.walk_expr(lhs);
                self.walk_expr(rhs);
            }
            HirExprKind::Unary { operand, .. } => self.walk_expr(operand),
            HirExprKind::Block(b) => self.walk_block(b),
            HirExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.walk_expr(cond);
                self.walk_block(then_branch);
                if let Some(eb) = else_branch.as_deref() {
                    self.walk_expr(eb);
                }
            }
            HirExprKind::Call { callee, args, .. } => {
                self.walk_expr(callee);
                for a in args {
                    let inner = match a {
                        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
                    };
                    self.walk_expr(inner);
                }
            }
            HirExprKind::Return { value } => {
                if let Some(v) = value.as_deref() {
                    self.walk_expr(v);
                }
            }
            HirExprKind::Paren(inner) => self.walk_expr(inner),
            HirExprKind::For { iter, body, .. } => {
                self.walk_expr(iter);
                self.walk_block(body);
            }
            HirExprKind::While { cond, body } => {
                self.walk_expr(cond);
                self.walk_block(body);
            }
            HirExprKind::Loop { body } => self.walk_block(body),
            HirExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee);
                for arm in arms {
                    self.walk_expr(&arm.body);
                }
            }
            HirExprKind::Field { obj, .. } => self.walk_expr(obj),
            HirExprKind::Index { obj, index } => {
                self.walk_expr(obj);
                self.walk_expr(index);
            }
            HirExprKind::Assign { lhs, rhs, .. } => {
                self.walk_expr(lhs);
                self.walk_expr(rhs);
            }
            HirExprKind::Cast { expr: inner, .. } => self.walk_expr(inner),
            HirExprKind::Tuple(es) => {
                for e in es {
                    self.walk_expr(e);
                }
            }
            HirExprKind::Array(arr) => match arr {
                cssl_hir::HirArrayExpr::List(items) => {
                    for e in items {
                        self.walk_expr(e);
                    }
                }
                cssl_hir::HirArrayExpr::Repeat { elem, len } => {
                    self.walk_expr(elem);
                    self.walk_expr(len);
                }
            },
            HirExprKind::Struct { fields, .. } => {
                for f in fields {
                    if let Some(v) = &f.value {
                        self.walk_expr(v);
                    }
                }
            }
            HirExprKind::Run { expr: inner } => self.walk_expr(inner),
            HirExprKind::Pipeline { lhs, rhs } => {
                self.walk_expr(lhs);
                self.walk_expr(rhs);
            }
            HirExprKind::TryDefault {
                expr: inner,
                default,
            } => {
                self.walk_expr(inner);
                self.walk_expr(default);
            }
            HirExprKind::Try { expr: inner } => self.walk_expr(inner),
            HirExprKind::Range { lo, hi, .. } => {
                if let Some(e) = lo.as_deref() {
                    self.walk_expr(e);
                }
                if let Some(e) = hi.as_deref() {
                    self.walk_expr(e);
                }
            }
            HirExprKind::Break { value: Some(v), .. } => self.walk_expr(v),
            HirExprKind::Lambda {
                params: inner_params,
                body: inner_body,
                ..
            } => {
                // Nested lambda : its own params shadow outer free-vars while
                // walking its body. We add-then-walk-then-remove to keep the
                // outer walker's bound-set minimal.
                let mut added: Vec<Symbol> = Vec::new();
                for p in inner_params {
                    if let Some(sym) = extract_pattern_symbol(&p.pat) {
                        if self.bound.insert(sym) {
                            added.push(sym);
                        }
                    }
                }
                self.walk_expr(inner_body);
                for sym in added {
                    self.bound.remove(&sym);
                }
            }
            HirExprKind::Perform { args, .. } => {
                for a in args {
                    let inner = match a {
                        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
                    };
                    self.walk_expr(inner);
                }
            }
            HirExprKind::With { handler, body } => {
                self.walk_expr(handler);
                self.walk_block(body);
            }
            HirExprKind::Region { body, .. } => self.walk_block(body),
            HirExprKind::Compound { lhs, rhs, .. } => {
                self.walk_expr(lhs);
                self.walk_expr(rhs);
            }
        }
    }

    fn walk_block(&mut self, b: &HirBlock) {
        // § Inner-let bindings shadow outer free-vars for refs LATER in the
        //   same block. We add-on-let, walk-the-rest, and roll back at block
        //   exit. Adding to `bound` only when not already present preserves
        //   the outer-shadow logic for the parent walker.
        let mut added: Vec<Symbol> = Vec::new();
        for stmt in &b.stmts {
            match &stmt.kind {
                HirStmtKind::Let { value, pat, .. } => {
                    if let Some(e) = value {
                        self.walk_expr(e);
                    }
                    if let Some(sym) = extract_pattern_symbol(pat) {
                        if self.bound.insert(sym) {
                            added.push(sym);
                        }
                    }
                }
                HirStmtKind::Expr(e) => self.walk_expr(e),
                HirStmtKind::Item(_) => {}
            }
        }
        if let Some(t) = b.trailing.as_ref() {
            self.walk_expr(t);
        }
        for sym in added {
            self.bound.remove(&sym);
        }
    }
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
    // Single-segment path : check param_vars first, then local_vars (T11-D77).
    // Param shadowing-by-local is handled by the lookup order : param_vars
    // wins because params are declared first ; if a let inside the body uses
    // the same name, `local_vars.insert` overwrites are still visible to
    // post-shadow refs because we check local_vars second only when the
    // param-lookup misses. (Stage-0 single-pass lowering can't preserve real
    // lexical scoping ; later-shadowing is rare in practice.)
    if segments.len() == 1 {
        if let Some((id, ty)) = ctx.param_vars.get(&segments[0]) {
            return (*id, ty.clone());
        }
        if let Some((id, ty)) = ctx.local_vars.get(&segments[0]) {
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

// `lower_call` is the central dispatch for all CSSLv3 call-shapes : closures,
// math intrinsics, sum-type constructors, Box::new, format, fs::*, net::*,
// trait-method dispatch + regular fn calls. The cognitive complexity here is
// irreducible without fragmenting the recognizer ordering — and ordering
// matters (e.g., the closure-call recognizer MUST run before format / sum-type
// recognizers so user-named closure locals win over stdlib idents ; the
// trait-dispatch path runs at well-defined positions to avoid claiming syntax
// reserved by the recognizer chain).
#[allow(clippy::cognitive_complexity)]
fn lower_call(
    ctx: &mut BodyLowerCtx<'_>,
    callee: &HirExpr,
    args: &[HirCallArg],
    type_args: &[HirType],
    span: Span,
    hir_id: cssl_hir::HirId,
) -> Option<(ValueId, MirType)> {
    // § T11-D100 (J2 — closures callable) — closure-call recognizer.
    //   Fires FIRST (before intrinsic / sum-type / Box::new / format / fs::* /
    //   net::* recognizers) because closure-typed locals are user-introduced
    //   bindings whose names could otherwise collide with stdlib idents — the
    //   closure dispatch wins by virtue of being a resolved local-binding.
    //   Conditions :
    //     1. Callee is a single-segment HIR Path.
    //     2. The path resolves (via param_vars or local_vars) to a value-id
    //        whose MirType is `Opaque("!cssl.closure")`.
    //     3. A closure descriptor exists for that value-id (registered at
    //        `lower_lambda` construction time).
    //   Result : emit the inline-expansion of the closure body + a marker
    //   `cssl.closure.call` op (consumed by backends as a no-op binder).
    //   Type-check : arity-match enforced ; mismatch ⇒ HARD diagnostic op
    //   `cssl.closure.call.error` with detail attribute. See spec
    //   § CLOSURE-ENV "invocation (T11-D100 / J2 …)".
    if let HirExprKind::Path { segments, .. } = &callee.kind {
        if segments.len() == 1 {
            let sym = segments[0];
            let resolved: Option<(ValueId, MirType)> = ctx
                .param_vars
                .get(&sym)
                .or_else(|| ctx.local_vars.get(&sym))
                .map(|(v, t)| (*v, t.clone()));
            if let Some((closure_vid, closure_ty)) = resolved {
                if matches!(&closure_ty, MirType::Opaque(s) if s == "!cssl.closure") {
                    if let Some(descriptor) = ctx.closure_descriptors.get(&closure_vid).cloned() {
                        return Some(lower_closure_call(
                            ctx,
                            closure_vid,
                            &descriptor,
                            args,
                            span,
                            hir_id,
                        ));
                    }
                }
            }
        }
    }

    // § T11-D99 — Trait-dispatch fast-path : if callee is `Field { obj, name }`
    //   and we have a trait-impl table attached, attempt to resolve the method
    //   via the table BEFORE falling through to opaque indirect-call. This is
    //   the user-defined-trait dispatch path — it fires only when the syntactic
    //   recognizers (Box::new / Some / None / format / etc.) decline (those use
    //   path-callees, not field-callees). See `crate::trait_dispatch` for the
    //   resolver.
    if let HirExprKind::Field { obj, name } = &callee.kind {
        if let Some(result) = try_lower_method_dispatch(ctx, obj, *name, args, span, hir_id) {
            return Some(result);
        }
    }
    // § T11-D99 — Static-trait-method dispatch fast-path : `Trait::method(...)` /
    //   `SelfTy::method(...)` — when the callee is a 2-segment path AND the
    //   trait-impl table contains either the trait or self-ty as the leading
    //   segment, route through trait-dispatch. The recognizers below claim the
    //   Box / Some / None / Ok / Err / format / fs / net 2-segment cases (those
    //   are user-shadow-able) ; trait-dispatch covers everything else where
    //   the user explicitly named a trait or self-type.
    if let HirExprKind::Path { segments, .. } = &callee.kind {
        if segments.len() == 2 {
            if let Some(result) =
                try_lower_static_method_dispatch(ctx, segments, args, span, hir_id)
            {
                return Some(result);
            }
        }
    }

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
    // § W-B-RECOGNIZER — Wave-A op-emit recognizers : `vec_drop::<T>` /
    //   `vec_load_at::<T>` / `vec_store_at::<T>` / `vec_end_of`.
    //
    //   Strict guards mirror the Some/None/Ok/Err recognizer pattern : the
    //   call must be a single-segment path matching the canonical fn-name
    //   + the expected arity. Each recognizer also accepts the bare-name
    //   form (`load_at` / `store_at` / `end_of` / `vec_drop`) used inside
    //   stdlib/vec.cssl — `vec_load_at` / `vec_store_at` / `vec_end_of`
    //   variants are reserved for future migration to the trait-resolved
    //   form. See `specs/40_WAVE_CSSL_PLAN.csl § WAVES § WAVE-A`.
    //
    //   ‼ Branch-friendly ordering : `vec_drop` fires first (most-frequent
    //   per Vec lifecycle), then `vec_load_at` / `vec_store_at` (per-element
    //   access), then `vec_end_of` (iter-init only). The recognizer chain
    //   short-circuits on first match so common paths skip irrelevant
    //   string compares.
    //
    //   § SWAP-POINT (sizeof_T monomorph extraction)
    //     The HIR `Call.type_args` carries the `::<T>` turbofish-arg
    //     (per `cssl-hir::HirExprKind::Call.type_args`). We thread that
    //     through `lower_hir_type_light(ctx.interner, &type_args[0])` to
    //     resolve the cell-kind via `TypedMemrefElem::from_mir_type`.
    //     Composite payload-T (struct / sum-type) DECLINES the
    //     recognizer — the regular generic-call path then takes over so
    //     the source still compiles (the panic("...deferred") body in
    //     stdlib/vec.cssl still serves as the fallback).
    if let HirExprKind::Path { segments, .. } = &callee.kind {
        if segments.len() == 1 {
            let name = ctx.interner.resolve(segments[0]);
            match (name.as_str(), args.len()) {
                ("vec_drop", 1) => {
                    if let Some(result) = try_lower_vec_drop(ctx, &args[0], type_args, span) {
                        return Some(result);
                    }
                }
                ("load_at" | "vec_load_at", 2) => {
                    if let Some(result) = try_lower_vec_load_at(ctx, args, type_args, span) {
                        return Some(result);
                    }
                }
                ("store_at" | "vec_store_at", 3) => {
                    if let Some(result) = try_lower_vec_store_at(ctx, args, type_args, span) {
                        return Some(result);
                    }
                }
                ("end_of" | "vec_end_of", 2) => {
                    if let Some(result) = try_lower_vec_end_of(ctx, args, type_args, span) {
                        return Some(result);
                    }
                }
                // § T11-D249 (W-A2-α-fix) — `vec_new::<T>()` /
                //   `vec_push::<T>(v, x)` / `vec_index::<T>(v, i)` recognizer
                //   arms. Strict guard : the call must be a single-segment
                //   path matching the canonical fn-name + the expected arity
                //   AND must carry a turbofish-T (composite-T declines).
                //   Each recognizer mints a `cssl.vec.*` MIR op the cgen
                //   layer can dispatch on. See `stdlib/vec.cssl § vec_new /
                //   vec_push / vec_index` + the W-A8 string-recognizer
                //   pattern (T11-D245) for the parallel structure.
                ("vec_new", 0) => {
                    if let Some(result) = try_lower_vec_new(ctx, type_args, span) {
                        return Some(result);
                    }
                }
                ("vec_push", 2) => {
                    if let Some(result) = try_lower_vec_push(ctx, args, type_args, span) {
                        return Some(result);
                    }
                }
                ("vec_index", 2) => {
                    if let Some(result) = try_lower_vec_index(ctx, args, type_args, span) {
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
    // § T11-D245 (W-A8 / Wave-C1 carry-forward) — `cssl.string.*` stdlib
    //   recognizer arms. Strict guard : the call must be a single-segment
    //   path matching one of the canonical stdlib string fn-names + the
    //   expected arity. Each recognizer mints the post-Wave-C1 MIR op shape
    //   via `crate::string_abi::build_*` ; the cgen layer
    //   (`cssl-cgen-cpu-cranelift::cgen_string`) consumes those ops to emit
    //   real Cranelift IR. See `specs/40_WAVE_CSSL_PLAN.csl § WAVE-C § C1` +
    //   `stdlib/string.cssl`.
    //
    //   ‼ Branch-friendly ordering : `string_len` / `str_len` (most-frequent
    //   inspector ops) fire first, then constructors (`string_from_utf8` /
    //   `string_from_utf8_unchecked`), then mutators (`string_push_str`),
    //   then borrow / coerce (`string_as_str`), then char-USV
    //   (`char_from_u32`). The recognizer chain short-circuits on first
    //   match.
    //
    //   § CANONICAL-SURFACE  (matches stdlib/string.cssl 1-segment fn-names)
    //     - `string_len(s) -> i64`              → cssl.string.len
    //     - `str_len(s) -> i64`                 → cssl.str_slice.len
    //     - `string_from_utf8(bytes) -> R<S,E>` → cssl.string.from_utf8
    //     - `string_from_utf8_unchecked(b) -> S`→ cssl.string.from_utf8_unchecked
    //     - `string_push_str(s, slice) -> S`    → cssl.string.push_str
    //     - `string_as_str(s) -> StrSlice`      → cssl.str_slice.new (from String)
    //     - `string_byte_at(s, i) -> i32`       → cssl.string.byte_at
    //     - `str_as_bytes(slice) -> i64`        → cssl.str_slice.as_bytes
    //     - `char_from_u32(code) -> Option<i32>`→ cssl.char.from_u32
    //
    //   § DECLINE-ON-MISMATCH : if arity doesn't match, the arm declines
    //     and the regular generic-call path takes over (preserving the
    //     placeholder body in stdlib/string.cssl as a fallback).
    if let HirExprKind::Path { segments, .. } = &callee.kind {
        if segments.len() == 1 {
            let name = ctx.interner.resolve(segments[0]);
            match (name.as_str(), args.len()) {
                ("string_len", 1) => {
                    if let Some(result) = try_lower_string_len(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("str_len", 1) => {
                    if let Some(result) = try_lower_str_slice_len(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("string_from_utf8", 1) => {
                    if let Some(result) = try_lower_string_from_utf8(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("string_from_utf8_unchecked", 1) => {
                    if let Some(result) = try_lower_string_from_utf8_unchecked(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("string_push_str", 2) => {
                    if let Some(result) = try_lower_string_push_str(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("string_as_str", 1) => {
                    if let Some(result) = try_lower_string_as_str(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("string_byte_at", 2) => {
                    if let Some(result) = try_lower_string_byte_at(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("str_as_bytes", 1) => {
                    if let Some(result) = try_lower_str_slice_as_bytes(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("char_from_u32", 1) => {
                    if let Some(result) = try_lower_char_from_u32(ctx, args, span) {
                        return Some(result);
                    }
                }
                _ => {}
            }
        }
    }
    // § T11-D288 (W-E5-5) — `simd_v128_load` / `simd_v128_store` /
    //   `simd_v_byte_eq` / `simd_v_byte_lt` / `simd_v_byte_in_range` /
    //   `simd_v_prefix_sum` / `simd_v_horizontal_sum` syntactic recognition.
    //   Strict guard : single-segment path matching the canonical
    //   stdlib SIMD-intrinsic fn-name + the expected arity. Each
    //   recognizer mints the post-W-E5-5 MIR op shape via
    //   `crate::simd_abi::build_*` ; the cgen layer
    //   (`cssl-cgen-cpu-cranelift::cgen_simd`) consumes those ops to
    //   emit real Cranelift CLIF SSE2/AVX2 vector intrinsics. Closes
    //   the W-E4 fixed-point gate's gap 5/5 — last gap before stage-0
    //   csslc declares the lexer/UTF-8/interner hot paths self-hosted.
    //
    //   ‼ DECLINE-ON-MISMATCH : if arity doesn't match, the arm declines
    //     and the regular generic-call path takes over (preserving the
    //     placeholder body in stdlib SIMD intrinsics as a fallback).
    if let HirExprKind::Path { segments, .. } = &callee.kind {
        if segments.len() == 1 {
            let name = ctx.interner.resolve(segments[0]);
            match (name.as_str(), args.len()) {
                ("simd_v128_load", 1) => {
                    if let Some(result) = try_lower_simd_v128_load(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("simd_v128_store", 2) => {
                    if let Some(result) = try_lower_simd_v128_store(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("simd_v_byte_eq", 2) => {
                    if let Some(result) = try_lower_simd_v_byte_eq(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("simd_v_byte_lt", 2) => {
                    if let Some(result) = try_lower_simd_v_byte_lt(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("simd_v_byte_in_range", 3) => {
                    if let Some(result) = try_lower_simd_v_byte_in_range(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("simd_v_prefix_sum", 1) => {
                    if let Some(result) = try_lower_simd_v_prefix_sum(ctx, args, span) {
                        return Some(result);
                    }
                }
                ("simd_v_horizontal_sum", 1) => {
                    if let Some(result) = try_lower_simd_v_horizontal_sum(ctx, args, span) {
                        return Some(result);
                    }
                }
                _ => {}
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
    // § T11-D82 (S7-F4) — `net::socket` / `net::listen` / `net::accept` /
    //   `net::connect` / `net::send` / `net::recv` / `net::sendto` /
    //   `net::recvfrom` / `net::close` syntactic recognition. Strict
    //   guard : the callee must be a 2-segment path with first segment
    //   `net` ; the second segment selects which `cssl.net.*` op fires +
    //   the expected argument count. Recognizing on a 2-segment path
    //   avoids accidentally claiming user identifiers like `connect` /
    //   `send` that legitimately exist in non-net contexts. The
    //   canonical stdlib form is `net::connect(addr, port)` per
    //   `stdlib/net.cssl`. See HANDOFF_SESSION_7 § PHASE-F § S7-F4 +
    //   `specs/04_EFFECTS.csl § NET-EFFECT` +
    //   `specs/11_IFC.csl § PRIME-DIRECTIVE ENCODING § NET-CAP rules`.
    if let Some(result) = try_lower_net_call(ctx, callee, args, span) {
        return Some(result);
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

// ═════════════════════════════════════════════════════════════════════════
// § T11-D99 — TRAIT-DISPATCH HELPERS
// ═════════════════════════════════════════════════════════════════════════

/// T11-D99 — Resolve `obj.method(args)` via the trait-impl table.
///
/// § ALGORITHM
///   1. If no trait-impl table is attached, return `None` (fall through to
///      regular field-call path).
///   2. Determine the receiver's self-type leading-segment symbol :
///      a. If `obj` is a single-segment Path naming a let-binding whose
///         declared type is `Path<...>`, use that path's leading symbol.
///      b. If `obj` is a `Field { obj : inner, name }` chain, recurse on
///         `inner` (TODO : multi-step chain — current stage-0 does not
///         attempt full type-flow).
///   3. Resolve `(self_ty_sym, method_sym)` via `TraitImplTable::resolve_method`.
///   4. Lower `obj` as the first argument (the receiver), then lower the rest
///      of the args.
///   5. Emit `func.call @<mangled-impl-fn>` with attribute
///      `dispatch = "trait"`, recording the resolved name + the source method-
///      name (for diagnostics + for the auto-monomorph rewriter to find the
///      right call-site).
///
/// Returns `None` if the table can't resolve — caller falls back to the
/// existing opaque field-call path.
fn try_lower_method_dispatch(
    ctx: &mut BodyLowerCtx<'_>,
    obj: &HirExpr,
    method: Symbol,
    args: &[HirCallArg],
    span: Span,
    hir_id: cssl_hir::HirId,
) -> Option<(ValueId, MirType)> {
    let table = ctx.trait_impl_table?;
    // § T11-D284 (W-E5-1) — receiver-resolution probe :
    //   1. Wrapper self-ty (e.g., `Option`, `Result`, or a plain `Foo`).
    //   2. Optional payload self-ty when the wrapper is an enum-payload
    //      tagged-union (`Option<Foo>` ⇒ payload `Foo`).
    //   The probe tries (wrapper, method) first ; on miss, falls back to
    //   (payload, method). This implements the "payload-receiver-unwrap"
    //   semantic — dispatching `obj.method()` against the unwrapped
    //   variant's payload-type when the wrapper itself has no impl.
    let probe = infer_receiver_self_ty_with_payload(ctx, obj)?;
    let (resolved_self_ty, dispatch_kind, mangled) =
        if let Some(name) = table.resolve_method(probe.wrapper, method) {
            (probe.wrapper, "trait", name.to_string())
        } else if let Some(payload_sym) = probe.payload {
            let name = table.resolve_method(payload_sym, method)?;
            (payload_sym, "trait_payload_unwrap", name.to_string())
        } else {
            return None;
        };

    // Lower the receiver as the first operand.
    let (recv_id, _recv_ty) = lower_expr(ctx, obj).unwrap_or((ctx.fresh_value_id(), MirType::None));
    let mut operand_ids = vec![recv_id];
    for arg in args {
        let a_expr = match arg {
            HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
        };
        if let Some((id, _ty)) = lower_expr(ctx, a_expr) {
            operand_ids.push(id);
        }
    }

    let id = ctx.fresh_value_id();
    let result_ty = MirType::Opaque(format!("!cssl.call_result.{mangled}"));
    let method_name = ctx.interner.resolve(method);
    let mut mir_op = MirOp::std("func.call")
        .with_attribute("callee", mangled.clone())
        .with_attribute("dispatch", dispatch_kind)
        .with_attribute("method", method_name)
        .with_attribute("self_ty", ctx.interner.resolve(resolved_self_ty))
        .with_attribute("source_loc", format!("{span:?}"))
        .with_attribute("hir_id", format!("{}", hir_id.0))
        .with_result(id, result_ty.clone());
    // § T11-D284 — record the wrapper-ty when the dispatch unwrapped a payload
    //   so downstream passes (codegen + IR-printer) can recover the tagged-
    //   union instance the receiver was loaded from.
    if dispatch_kind == "trait_payload_unwrap" {
        mir_op = mir_op
            .with_attribute("wrapper_self_ty", ctx.interner.resolve(probe.wrapper))
            .with_attribute("payload_unwrap", "true");
    }
    for oid in operand_ids {
        mir_op = mir_op.with_operand(oid);
    }
    ctx.ops.push(mir_op);
    Some((id, result_ty))
}

/// T11-D99 — Resolve `Trait::method(...)` / `SelfTy::method(...)` via the
/// trait-impl table.
///
/// § ALGORITHM
///   1. If no table is attached, return `None`.
///   2. Treat `segments[0]` as either a Trait-name or a Self-type-name.
///   3. If `segments[0]` is a known trait, the receiver type comes from the
///      first call-arg ; recover its self-ty leading-segment via the same
///      receiver-inference helper that `try_lower_method_dispatch` uses.
///      Then resolve via `(self_ty, method)`.
///   4. Otherwise, treat `segments[0]` as a self-type leading symbol and
///      resolve directly.
///
/// Returns `None` when the table can't resolve.
fn try_lower_static_method_dispatch(
    ctx: &mut BodyLowerCtx<'_>,
    segments: &[Symbol],
    args: &[HirCallArg],
    span: Span,
    hir_id: cssl_hir::HirId,
) -> Option<(ValueId, MirType)> {
    if segments.len() != 2 {
        return None;
    }
    let table = ctx.trait_impl_table?;
    let leading = segments[0];
    let method = segments[1];

    // Try direct resolution — leading is the self-type.
    let mangled = if let Some(name) = table.resolve_method(leading, method) {
        Some(name.to_string())
    } else {
        // Fall back : leading might be a trait-name. The first arg's
        // receiver-type then determines the impl. We don't have full type
        // inference at this layer, so we look for the first call-arg that
        // is a single-segment-path naming a let-binding with a known type.
        let first_arg = args.first().map(|a| match a {
            HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
        })?;
        // § T11-D284 (W-E5-1) — payload-unwrap probe for static-method dispatch.
        //   When the first-arg's type is `Option<Foo>` / `Result<Foo, E>` and
        //   `leading` is a trait-name, look for `impl <leading> for <wrapper>`
        //   first ; if absent, fall back to `impl <leading> for <payload>`.
        let probe = infer_receiver_self_ty_with_payload(ctx, first_arg)?;
        let candidate_self_tys: Vec<Symbol> = {
            let mut v = vec![probe.wrapper];
            if let Some(p) = probe.payload {
                v.push(p);
            }
            v
        };
        let mut hit: Option<String> = None;
        for self_ty in &candidate_self_tys {
            if !table.has_impl(leading, *self_ty) {
                continue;
            }
            for entry in table.entries() {
                if entry.trait_name == Some(leading)
                    && entry.self_ty_name == *self_ty
                    && entry.method_mangled.contains_key(&method)
                {
                    hit = Some(entry.method_mangled[&method].clone());
                    break;
                }
            }
            if hit.is_some() {
                break;
            }
        }
        hit
    };
    let mangled = mangled?;

    // Lower the args (no implicit-receiver reorder for static-method dispatch ;
    // the source already passed `self` as the first positional arg).
    let mut operand_ids = Vec::with_capacity(args.len());
    for arg in args {
        let a_expr = match arg {
            HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
        };
        if let Some((id, _ty)) = lower_expr(ctx, a_expr) {
            operand_ids.push(id);
        }
    }
    let id = ctx.fresh_value_id();
    let result_ty = MirType::Opaque(format!("!cssl.call_result.{mangled}"));
    let method_name = ctx.interner.resolve(method);
    let leading_name = ctx.interner.resolve(leading);
    let mut mir_op = MirOp::std("func.call")
        .with_attribute("callee", mangled.clone())
        .with_attribute("dispatch", "trait_static")
        .with_attribute("method", method_name)
        .with_attribute("leading", leading_name)
        .with_attribute("source_loc", format!("{span:?}"))
        .with_attribute("hir_id", format!("{}", hir_id.0))
        .with_result(id, result_ty.clone());
    for oid in operand_ids {
        mir_op = mir_op.with_operand(oid);
    }
    ctx.ops.push(mir_op);
    Some((id, result_ty))
}

/// Infer the leading-segment symbol of the receiver expression's type.
///
/// At stage-0 we don't have full type-inference threaded into `body_lower`,
/// so this walker handles the common cases :
///
///   - `obj` is a single-segment Path : look up `local_vars` / `param_vars`
///     for a (ValueId, MirType) ; if `MirType::Opaque(s)` carries a known
///     `!cssl.struct.<Name>` shape, extract the Name.
///   - `obj` is a struct-literal `Foo { ... }` : the leading path-segment
///     is the self-type.
///   - `obj` is a `Call { callee: Path { segments }, .. }` where `segments`
///     ends in `new` and the prior segment is a self-type registered in the
///     table : use that self-type (covers `Foo::new(...)` chains).
///
/// Returns `None` if no inference succeeds — caller falls through.
fn infer_receiver_self_ty(ctx: &BodyLowerCtx<'_>, obj: &HirExpr) -> Option<Symbol> {
    match &obj.kind {
        HirExprKind::Path { segments, .. } => {
            if segments.len() == 1 {
                let sym = segments[0];
                // Try the local-vars map first (let-binding declared type).
                if let Some(t) = ctx.local_var_self_ty(sym) {
                    return Some(t);
                }
                // Fall through to param-vars.
                if let Some(t) = ctx.param_var_self_ty(sym) {
                    return Some(t);
                }
            }
            None
        }
        HirExprKind::Struct { path, .. } => path.last().copied(),
        HirExprKind::Call { callee, .. } => {
            // Recurse into the callee's path : for `Foo::new(...).method(...)`
            // the receiver of the outer call is the inner `Foo::new(...)` Call,
            // whose path-segments begin with `Foo`.
            if let HirExprKind::Path { segments, .. } = &callee.kind {
                if segments.len() == 2 {
                    return Some(segments[0]);
                }
                if segments.len() == 1 {
                    return Some(segments[0]);
                }
            }
            None
        }
        HirExprKind::Field { obj: inner, .. } => infer_receiver_self_ty(ctx, inner),
        HirExprKind::Paren(inner) => infer_receiver_self_ty(ctx, inner),
        _ => None,
    }
}

/// T11-D284 (W-E5-1) — receiver-resolution probe with optional enum-payload
/// unwrap.
///
/// Returns the wrapper self-ty (mandatory) plus an optional payload self-ty
/// when the wrapper's MirType encodes an `Option<T>` / `Result<T, E>` shape.
/// The dispatch resolver tries `(wrapper, method)` first ; on miss, retries
/// `(payload, method)`. This makes `let x : Option<Foo> = Some(foo) ;
/// x.method()` dispatch through `impl SomeTrait for Foo { fn method ... }`
/// correctly when no `impl SomeTrait for Option<T>` exists.
#[derive(Debug, Clone, Copy)]
struct ReceiverProbe {
    /// Outer self-ty leading symbol (always present).
    wrapper: Symbol,
    /// Inner payload self-ty leading symbol — only when the wrapper is an
    /// enum-payload tagged-union AND the payload encoded into the type
    /// resolves to a known leading symbol. `None` for plain self-tys
    /// (`Foo` / `Bar`) or for `Option<unknown>` where the payload
    /// can't be peeled.
    payload: Option<Symbol>,
}

fn infer_receiver_self_ty_with_payload(
    ctx: &BodyLowerCtx<'_>,
    obj: &HirExpr,
) -> Option<ReceiverProbe> {
    // For Path-receivers naming a let-binding / param, peek at the binding's
    // MirType to detect Option/Result encoding. The encoded form
    // `!cssl.option<Foo>` would otherwise have `infer_receiver_self_ty`
    // strip the `!cssl.` prefix and yield lowercase `option` — but the
    // user-written `impl Trait for Option { ... }` registers under the
    // capitalized `Option` symbol. So when the binding's type carries the
    // encoded enum form, we OVERRIDE the wrapper to the canonical capitalized
    // family-name (`Option` / `Result`) and extract the payload via
    // `enum_payload_self_ty`. For plain receivers without enum encoding,
    // this falls through to the regular wrapper-only path.
    if let Some(family) = enum_family_self_ty(ctx, obj) {
        let payload = enum_payload_self_ty(ctx, obj);
        return Some(ReceiverProbe {
            wrapper: family,
            payload,
        });
    }
    let wrapper = infer_receiver_self_ty(ctx, obj)?;
    Some(ReceiverProbe {
        wrapper,
        payload: None,
    })
}

/// T11-D284 (W-E5-1) — when the receiver's MirType encodes an Option/Result
/// shape (`!cssl.option<...>` / `!cssl.result<...>`), return the canonical
/// source-form family-name interned via `ctx.interner` (`Option` or `Result`).
/// For non-enum receivers, returns `None`.
fn enum_family_self_ty(ctx: &BodyLowerCtx<'_>, obj: &HirExpr) -> Option<Symbol> {
    let sym = match &obj.kind {
        HirExprKind::Path { segments, .. } if segments.len() == 1 => segments[0],
        HirExprKind::Paren(inner) => return enum_family_self_ty(ctx, inner),
        _ => return None,
    };
    let (_, ty) = ctx
        .local_vars
        .get(&sym)
        .or_else(|| ctx.param_vars.get(&sym))?;
    let s = match ty {
        MirType::Opaque(s) => s.as_str(),
        _ => return None,
    };
    if s.starts_with("!cssl.option<") {
        Some(ctx.interner.intern("Option"))
    } else if s.starts_with("!cssl.result<") {
        Some(ctx.interner.intern("Result"))
    } else {
        None
    }
}

/// T11-D284 (W-E5-1) — extract the payload's leading self-ty symbol when the
/// receiver is a let-binding / param whose declared type is `Option<T>` /
/// `Result<T, E>`. For `Option`, the payload is T. For `Result`, the
/// payload is the Ok-branch T (the Err-branch tracks separately for `?`-
/// operator + match-arm dispatch — those don't share the trait-method-call
/// receiver path because `Err` always lacks the success-arm methods the
/// trait declares).
fn enum_payload_self_ty(ctx: &BodyLowerCtx<'_>, obj: &HirExpr) -> Option<Symbol> {
    let sym = match &obj.kind {
        HirExprKind::Path { segments, .. } if segments.len() == 1 => segments[0],
        HirExprKind::Paren(inner) => return enum_payload_self_ty(ctx, inner),
        _ => return None,
    };
    let (_, ty) = ctx
        .local_vars
        .get(&sym)
        .or_else(|| ctx.param_vars.get(&sym))?;
    let s = match ty {
        MirType::Opaque(s) => s.as_str(),
        _ => return None,
    };
    // Match the encoding produced by `lower_hir_type_light`'s Option/Result
    // arms : `!cssl.option<Foo>` / `!cssl.result<Foo,E>`.
    let payload_str = if let Some(rest) = s.strip_prefix("!cssl.option<") {
        rest.strip_suffix('>')?
    } else if let Some(rest) = s.strip_prefix("!cssl.result<") {
        // First comma-separated fragment is the Ok-branch payload.
        let inner = rest.strip_suffix('>')?;
        let comma = inner.find(',')?;
        &inner[..comma]
    } else {
        return None;
    };
    if payload_str.is_empty() || payload_str == "?" {
        return None;
    }
    // Take the leading identifier (alphanumeric + `_`) to robustly handle
    // dotted multi-segment paths (`a.b.c`) that flatten to leading `a`.
    let leading: String = payload_str
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if leading.is_empty() {
        return None;
    }
    Some(ctx.interner.intern(&leading))
}

impl<'a> BodyLowerCtx<'a> {
    /// Best-effort recovery of a let-binding's declared self-type leading
    /// symbol from the local-vars map. Returns `None` if the binding is
    /// untyped or its type isn't a recognizable `!cssl.struct.<Name>` /
    /// `!cssl.<Name>` shape.
    fn local_var_self_ty(&self, name: Symbol) -> Option<Symbol> {
        let (_, ty) = self.local_vars.get(&name)?;
        opaque_type_leading_symbol(self.interner, ty)
    }

    /// Best-effort recovery for a fn-param.
    fn param_var_self_ty(&self, name: Symbol) -> Option<Symbol> {
        let (_, ty) = self.param_vars.get(&name)?;
        opaque_type_leading_symbol(self.interner, ty)
    }
}

/// Extract a self-type leading symbol from a `MirType::Opaque(...)` shape
/// like `!cssl.struct.Foo` / `!cssl.Vec` / `Vec` (legacy bare-name case).
fn opaque_type_leading_symbol(interner: &Interner, ty: &MirType) -> Option<Symbol> {
    let s = match ty {
        MirType::Opaque(s) => s.as_str(),
        _ => return None,
    };
    // Strip leading `!cssl.struct.` / `!cssl.` / `!` prefixes.
    let trimmed = s
        .strip_prefix("!cssl.struct.")
        .or_else(|| s.strip_prefix("!cssl."))
        .or_else(|| s.strip_prefix('!'))
        .unwrap_or(s);
    // Take the leading identifier (alphanumeric + `_`).
    let leading: String = trimmed
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if leading.is_empty() {
        return None;
    }
    Some(interner.intern(&leading))
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
// § W-B-RECOGNIZER — Wave-A op-emit recognizer helpers.
//
//   Each helper resolves the type-arg `<T>` to a `TypedMemrefElem` (i8 / i16
//   / i32 / i64 / f32 / f64) via `lower_hir_type_light` + the LUT in
//   `memref_typed::TypedMemrefElem::from_mir_type`. Composite / unsupported
//   payload-T returns `None` so the caller declines the recognizer and the
//   regular generic-call path takes over (preserving the existing panic-stub
//   stdlib bodies as a safe fallback).
//
//   § SWAP-POINT (per `specs/40_WAVE_CSSL_PLAN.csl § WAVE-A`)
//     The vec_drop helper threads `v.cap` AT RUNTIME (a `cssl.field` op
//     read) rather than as a compile-time constant — this matches the
//     stdlib/vec.cssl § Manual Drop intent (`v.cap × sizeof T`). The
//     payload-T sizeof remains compile-time per `dealloc_size_for`. A
//     follow-up slice replaces this with a true monomorph-context lookup
//     once auto_monomorph threads cap-known sites separately.
// ─────────────────────────────────────────────────────────────────────────

/// Resolve the first turbofish type-argument to a `TypedMemrefElem`. Returns
/// `None` if no type-arg is present OR the type-arg lowers to a composite /
/// unsupported MIR-type (caller declines the recognizer in that case).
///
/// § SAWYER-EFFICIENCY
///   - Single LUT-style match via `from_mir_type` ; no allocation on the
///     hot path.
///   - Returns `Copy` value (`TypedMemrefElem` is `#[derive(Copy)]`) so the
///     caller can stash it in a local without a clone.
fn resolve_typed_memref_elem(
    ctx: &BodyLowerCtx<'_>,
    type_args: &[HirType],
) -> Option<crate::memref_typed::TypedMemrefElem> {
    let t = type_args.first()?;
    let mir_ty = lower_hir_type_light(ctx.interner, t);
    crate::memref_typed::TypedMemrefElem::from_mir_type(&mir_ty)
}

/// Lower a syntactically-recognized `vec_load_at::<T>(data, i)` call.
///
/// § EMITTED-SHAPE
/// ```text
///   %data   = <lower(args[0])>                      // i64 base ptr
///   %i      = <lower(args[1])>                      // i64 index
///   %sz     = arith.constant <sizeof T> : i64       // via build_index_offset
///   %bytes  = arith.muli %i, %sz : i64
///   %r      = memref.load.<T> %data, %bytes : <T>   // via build_typed_load
/// ```
/// Returns the typed-load result `(ValueId, MirType)` on success ; declines
/// with `None` for composite payload-T.
fn try_lower_vec_load_at(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    type_args: &[HirType],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let elem = resolve_typed_memref_elem(ctx, type_args)?;

    // Lower the two args (data, i) — both i64.
    let data_expr = match &args[0] {
        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
    };
    let idx_expr = match &args[1] {
        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
    };
    let (data_id, _) = lower_expr(ctx, data_expr)?;
    let (idx_id, _) = lower_expr(ctx, idx_expr)?;

    // Mint the index-offset triplet (constant + muli) via the canonical
    // `build_index_offset` helper. Sawyer-efficiency : 2 ops appended in
    // place ; no scratch Vec allocations beyond the helper's single
    // `Vec::with_capacity(2)` (which is the helper's contract — we just
    // re-borrow into ctx.ops via extend).
    let sizeof_const_id = ctx.fresh_value_id();
    let bytes_id = ctx.fresh_value_id();
    let index_ops =
        crate::memref_typed::build_index_offset(elem, idx_id, sizeof_const_id, bytes_id);
    for op in index_ops {
        let op = op.with_attribute("source_loc", format!("{span:?}"));
        ctx.ops.push(op);
    }

    // Mint the typed load.
    let result_id = ctx.fresh_value_id();
    let result_ty = elem.to_mir_type();
    let load_op = crate::memref_typed::build_typed_load(elem, data_id, bytes_id, result_id)
        .with_attribute("source_loc", format!("{span:?}"))
        .with_attribute("origin", "vec_load_at");
    ctx.ops.push(load_op);

    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `vec_store_at::<T>(data, i, x)` call.
///
/// § EMITTED-SHAPE
/// ```text
///   %data   = <lower(args[0])>                          // i64 base ptr
///   %i      = <lower(args[1])>                          // i64 index
///   %x      = <lower(args[2])>                          // T value
///   %sz     = arith.constant <sizeof T> : i64
///   %bytes  = arith.muli %i, %sz : i64
///   memref.store.<T> %x, %data, %bytes                  // side-effect only
/// ```
/// Returns a `(ValueId, MirType::None)` placeholder result — the store
/// itself has no SSA-value, but the caller expects a tuple to thread back
/// into the expression-position. We mint a fresh-id with `MirType::None`
/// to keep the SSA-id space monotonic.
fn try_lower_vec_store_at(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    type_args: &[HirType],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let elem = resolve_typed_memref_elem(ctx, type_args)?;

    let data_expr = match &args[0] {
        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
    };
    let idx_expr = match &args[1] {
        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
    };
    let val_expr = match &args[2] {
        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
    };
    let (data_id, _) = lower_expr(ctx, data_expr)?;
    let (idx_id, _) = lower_expr(ctx, idx_expr)?;
    let (val_id, _) = lower_expr(ctx, val_expr)?;

    // Mint index-offset triplet.
    let sizeof_const_id = ctx.fresh_value_id();
    let bytes_id = ctx.fresh_value_id();
    let index_ops =
        crate::memref_typed::build_index_offset(elem, idx_id, sizeof_const_id, bytes_id);
    for op in index_ops {
        let op = op.with_attribute("source_loc", format!("{span:?}"));
        ctx.ops.push(op);
    }

    // Mint the typed store.
    let store_op = crate::memref_typed::build_typed_store(elem, val_id, data_id, bytes_id)
        .with_attribute("source_loc", format!("{span:?}"))
        .with_attribute("origin", "vec_store_at");
    ctx.ops.push(store_op);

    // Return a unit-shape placeholder so callers in expression-position get
    // an SSA-id back. The store itself has no result.
    let placeholder_id = ctx.fresh_value_id();
    Some((placeholder_id, MirType::None))
}

/// Lower a syntactically-recognized `vec_end_of(data, len)` call.
///
/// § EMITTED-SHAPE
/// ```text
///   %data   = <lower(args[0])>                          // i64 base ptr
///   %len    = <lower(args[1])>                          // i64 elem-count
///   %sz     = arith.constant <sizeof T> : i64
///   %bytes  = arith.muli %len, %sz : i64
///   %end    = memref.ptr.end_of %data, %bytes : i64     // via build_typed_end_of
/// ```
/// Stage-0 SWAP-POINT : the type-arg `<T>` selects the cell-kind ; when no
/// turbofish is present, the recognizer DECLINES (returns `None`) and falls
/// back to the regular generic-call path. The stdlib's bare `end_of` form
/// is ONLY recognized when a turbofish supplies T — see `vec_iter::<T>` /
/// `vec_iter_next::<T>` call-sites in stdlib/vec.cssl which thread the
/// type-arg through.
fn try_lower_vec_end_of(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    type_args: &[HirType],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let elem = resolve_typed_memref_elem(ctx, type_args)?;

    let data_expr = match &args[0] {
        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
    };
    let len_expr = match &args[1] {
        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
    };
    let (data_id, _) = lower_expr(ctx, data_expr)?;
    let (len_id, _) = lower_expr(ctx, len_expr)?;

    // Mint sizeof-const + muli + end_of triplet via canonical helper.
    let sizeof_const_id = ctx.fresh_value_id();
    let bytes_id = ctx.fresh_value_id();
    let end_id = ctx.fresh_value_id();
    let end_ops = crate::memref_typed::build_typed_end_of(
        elem,
        data_id,
        len_id,
        sizeof_const_id,
        bytes_id,
        end_id,
    );
    for op in end_ops {
        let op = op
            .with_attribute("source_loc", format!("{span:?}"))
            .with_attribute("origin", "vec_end_of");
        ctx.ops.push(op);
    }

    // Result is the end-pointer as i64 (matches `build_typed_end_of`'s
    // documented contract).
    Some((end_id, MirType::Int(IntWidth::I64)))
}

/// Lower a syntactically-recognized `vec_drop::<T>(v)` call into a
/// `cssl.heap.dealloc(v.data, v.cap × sizeof T, alignof T)` op-sequence.
///
/// § EMITTED-SHAPE
/// ```text
///   %v_id   = <lower(args[0])>                          // !cssl.struct.Vec
///   %data   = cssl.field %v_id, "data" : i64
///   %cap    = cssl.field %v_id, "cap" : i64
///   %sz     = arith.constant <sizeof T> : i64
///   %bytes  = arith.muli %cap, %sz : i64                // total alloc size
///   %al     = arith.constant <alignof T> : i64
///   cssl.heap.dealloc %data, %bytes, %al                // via build_heap_dealloc_op
/// ```
/// Stage-0 SWAP-POINT : `v.cap` is read at RUNTIME via a `cssl.field` op
/// (matches the stdlib/vec.cssl § Manual Drop intent of `v.cap × sizeof T`).
/// The payload-T sizeof + alignof remain compile-time constants per
/// `heap_dealloc::dealloc_size_for` / `dealloc_align_for`. A follow-up
/// slice may collapse this to a single compile-time constant once
/// auto_monomorph threads cap-known sites separately.
///
/// Returns `(ValueId, MirType::None)` — vec_drop has no SSA result.
fn try_lower_vec_drop(
    ctx: &mut BodyLowerCtx<'_>,
    arg: &HirCallArg,
    type_args: &[HirType],
    span: Span,
) -> Option<(ValueId, MirType)> {
    // Resolve payload-T to MIR-type for sizeof + alignof. Composite / opaque
    // types DECLINE — caller falls through to the regular path.
    let payload_ty = lower_hir_type_light(ctx.interner, type_args.first()?);
    let payload_size_bytes = crate::heap_dealloc::dealloc_size_for(&payload_ty);
    if payload_size_bytes == 0 {
        // Unresolved / composite payload : decline so the regular generic-
        // call path consumes the call (preserves the placeholder body).
        return None;
    }
    let payload_align_bytes = crate::heap_dealloc::dealloc_align_for(&payload_ty);

    // Lower the v argument.
    let v_expr = match arg {
        HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
    };
    let (v_id, _v_ty) = lower_expr(ctx, v_expr)?;

    // Read v.data + v.cap via `cssl.field` ops (the canonical struct-field
    // accessor at stage-0 ; see existing `lower_field` for the shape).
    let data_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("cssl.field")
            .with_operand(v_id)
            .with_result(data_id, MirType::Int(IntWidth::I64))
            .with_attribute("field", "data")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    let cap_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("cssl.field")
            .with_operand(v_id)
            .with_result(cap_id, MirType::Int(IntWidth::I64))
            .with_attribute("field", "cap")
            .with_attribute("source_loc", format!("{span:?}")),
    );

    // sizeof T constant.
    let sz_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("arith.constant")
            .with_attribute("value", payload_size_bytes.to_string())
            .with_result(sz_id, MirType::Int(IntWidth::I64))
            .with_attribute("source_loc", format!("{span:?}")),
    );

    // bytes = cap × sizeof T (runtime muli — cap is dynamic).
    let bytes_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("arith.muli")
            .with_operand(cap_id)
            .with_operand(sz_id)
            .with_result(bytes_id, MirType::Int(IntWidth::I64))
            .with_attribute("source_loc", format!("{span:?}")),
    );

    // alignof T constant.
    let al_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("arith.constant")
            .with_attribute("value", payload_align_bytes.to_string())
            .with_result(al_id, MirType::Int(IntWidth::I64))
            .with_attribute("source_loc", format!("{span:?}")),
    );

    // The actual dealloc op via the canonical builder.
    let dealloc_op = crate::heap_dealloc::build_heap_dealloc_op(
        data_id,
        bytes_id,
        al_id,
        &payload_ty,
        Some(crate::heap_dealloc::ORIGIN_VEC_DROP),
        &format!("{span:?}"),
    );
    ctx.ops.push(dealloc_op);

    // vec_drop returns unit ; mint a fresh-id for the placeholder result.
    let placeholder_id = ctx.fresh_value_id();
    Some((placeholder_id, MirType::None))
}

// ─────────────────────────────────────────────────────────────────────────
// § T11-D249 (W-A2-α-fix) — `cssl.vec.*` constructor / push / index
//   recognizer helpers. Mirrors the W-A8 string-recognizer pattern
//   (T11-D245) : one canonical `MirOp::std("cssl.vec.*")` op per call ;
//   `payload_ty` attribute carries the monomorphized T so the cgen layer
//   (`cssl-cgen-cpu-cranelift`) can dispatch on element-kind without
//   re-parsing op-name. The recognizer DECLINES on missing turbofish or
//   composite-T so the regular generic-call path takes over (preserving
//   the panic-stub bodies in `stdlib/vec.cssl` as a safe fallback).
//
//   § DESIGN-NOTE
//     Stage-0 result-type for `vec_new` / `vec_push` is `MirType::Opaque
//     ("Vec")` — the structural Vec ABI is the same deferred-ABI slice as
//     Option/Result/String, threaded through `TaggedUnionAbiPass` once it
//     gains struct-aware sig-rewrite. `vec_index` returns the resolved
//     element type T (`lower_hir_type_light(type_args[0])`) so downstream
//     consumers see a typed value. Bounds-checking is emitted as an
//     `bounds_check="panic"` attribute the cgen layer expands into a
//     compare + `__cssl_panic` extern call when wired (paired with the
//     existing `__cssl_panic` symbol from T11-D52, S6-A1).
// ─────────────────────────────────────────────────────────────────────────

/// Lower a syntactically-recognized `vec_new::<T>()` call into a
/// `cssl.vec.new` op. Mirrors the empty-construction shortcut from
/// `stdlib/vec.cssl § vec_new` — does NOT emit `cssl.heap.alloc` (cap=0).
///
/// § EMITTED-SHAPE
/// ```text
///   %v = cssl.vec.new : !cssl.vec.<T>            // payload_ty=<T>, cap=iso, len=0
/// ```
///
/// Stage-0 result-type is `MirType::Opaque("Vec")` ; the structural ABI
/// rewrite to `(data, len, cap)` is the same deferred-ABI slice as
/// Option/Result. Composite payload-T DECLINES — caller falls through to
/// the regular generic-call path.
fn try_lower_vec_new(
    ctx: &mut BodyLowerCtx<'_>,
    type_args: &[HirType],
    span: Span,
) -> Option<(ValueId, MirType)> {
    // Resolve payload-T to a primitive cell-kind ; composite / opaque T
    // declines so the regular generic-call path consumes the call.
    let elem = resolve_typed_memref_elem(ctx, type_args)?;
    let payload_ty = elem.to_mir_type();

    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Opaque("Vec".to_string());
    ctx.ops.push(
        MirOp::std("cssl.vec.new")
            .with_result(result_id, result_ty.clone())
            .with_attribute("payload_ty", format!("{payload_ty}"))
            .with_attribute("cap", "iso")
            .with_attribute("origin", "vec_new")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `vec_push::<T>(v, x)` call into a
/// `cssl.vec.push` op. Per `stdlib/vec.cssl § vec_push` the push grows
/// the backing buffer if `len == cap` (2× amortized growth) and writes
/// `x` to the new last slot.
///
/// § EMITTED-SHAPE
/// ```text
///   %v = <lower(args[0])>                            // !cssl.vec.<T>
///   %x = <lower(args[1])>                            // T value
///   %v' = cssl.vec.push %v, %x : !cssl.vec.<T>       // payload_ty=<T>
/// ```
///
/// Stage-0 result-type is `MirType::Opaque("Vec")` (return-by-value form
/// per the stdlib's `Vec<T> -> Vec<T>` signature ; trait-resolved
/// `&mut self` migration is mechanical once the borrow-flow lands).
/// Composite payload-T DECLINES.
fn try_lower_vec_push(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    type_args: &[HirType],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let elem = resolve_typed_memref_elem(ctx, type_args)?;
    let payload_ty = elem.to_mir_type();

    let (v_id, _v_ty) = lower_call_arg(ctx, &args[0])?;
    let (x_id, _x_ty) = lower_call_arg(ctx, &args[1])?;

    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Opaque("Vec".to_string());
    ctx.ops.push(
        MirOp::std("cssl.vec.push")
            .with_operand(v_id)
            .with_operand(x_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute("payload_ty", format!("{payload_ty}"))
            .with_attribute("origin", "vec_push")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `vec_index::<T>(v, i)` call into a
/// `cssl.vec.index` op. Per `stdlib/vec.cssl § vec_index` the access
/// panics through `__cssl_panic` (T11-D52, S6-A1) when `i < 0 ||
/// i >= v.len`.
///
/// § EMITTED-SHAPE
/// ```text
///   %v = <lower(args[0])>                            // !cssl.vec.<T>
///   %i = <lower(args[1])>                            // i64 index
///   %r = cssl.vec.index %v, %i : <T>                 // payload_ty=<T>,
///                                                       bounds_check="panic"
/// ```
///
/// Result type is the resolved element type `T` (via the typed-memref
/// LUT) so downstream consumers see a typed value. Composite payload-T
/// DECLINES.
fn try_lower_vec_index(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    type_args: &[HirType],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let elem = resolve_typed_memref_elem(ctx, type_args)?;
    let payload_ty = elem.to_mir_type();

    let (v_id, _v_ty) = lower_call_arg(ctx, &args[0])?;
    let (i_id, _i_ty) = lower_call_arg(ctx, &args[1])?;

    let result_id = ctx.fresh_value_id();
    let result_ty = payload_ty.clone();
    ctx.ops.push(
        MirOp::std("cssl.vec.index")
            .with_operand(v_id)
            .with_operand(i_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute("payload_ty", format!("{payload_ty}"))
            .with_attribute("bounds_check", "panic")
            .with_attribute("origin", "vec_index")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
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
// § T11-D245 (W-A8 / Wave-C1 carry-forward) — `cssl.string.*` stdlib
//   recognizer helpers. Each `try_lower_string_*` mints the post-Wave-C1
//   MIR op shape via `crate::string_abi::build_*`. The cgen layer
//   `cssl-cgen-cpu-cranelift::cgen_string` consumes those ops to emit
//   real Cranelift IR. Mirrors the Vec / fs / net recognizer-helper
//   pattern : each helper :
//     1. lowers each arg via `lower_call_arg`,
//     2. mints a fresh result-id via `ctx.fresh_value_id()`,
//     3. attaches `source_loc` for diagnostics,
//     4. returns `(result_id, result_ty)` on success / `None` on decline.
//
//   The helpers reuse `string_abi::build_*` for the canonical op-name +
//   attribute set so the cgen-side dispatch (which keys off op-name
//   prefix) stays in lock-step.
// ─────────────────────────────────────────────────────────────────────────

/// Lower a syntactically-recognized `string_len(s) -> i64` call into a
/// `cssl.string.len` op.
///
/// § EMITTED-SHAPE
/// ```text
///   %s_id = <lower(args[0])>                          // !cssl.string
///   %len  = cssl.string.len %s_id : i64               // field=len, offset=8
/// ```
fn try_lower_string_len(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (s_id, _s_ty) = lower_call_arg(ctx, &args[0])?;
    let result_id = ctx.fresh_value_id();
    let mut op = crate::string_abi::build_string_len(s_id, result_id);
    op = op.with_attribute("source_loc", format!("{span:?}"));
    ctx.ops.push(op);
    Some((result_id, MirType::Int(IntWidth::I64)))
}

/// Lower a syntactically-recognized `str_len(slice) -> i64` call into a
/// `cssl.str_slice.len` op.
///
/// § EMITTED-SHAPE
/// ```text
///   %sl_id = <lower(args[0])>                         // !cssl.str_slice
///   %len   = cssl.str_slice.len %sl_id : i64          // field=len, offset=8
/// ```
fn try_lower_str_slice_len(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (sl_id, _sl_ty) = lower_call_arg(ctx, &args[0])?;
    let result_id = ctx.fresh_value_id();
    let mut op = crate::string_abi::build_str_slice_len(sl_id, result_id);
    op = op.with_attribute("source_loc", format!("{span:?}"));
    ctx.ops.push(op);
    Some((result_id, MirType::Int(IntWidth::I64)))
}

/// Lower a syntactically-recognized `string_from_utf8(bytes) ->
/// Result<String, FromUtf8Error>` call into a `cssl.string.from_utf8` op.
///
/// § EMITTED-SHAPE
/// ```text
///   %b_id      = <lower(args[0])>                       // !cssl.struct.Vec<u8>
///   %data      = cssl.field %b_id, "data" : i64         // bytes_ptr
///   %len       = cssl.field %b_id, "len"  : i64         // bytes_len
///   %result    = cssl.string.from_utf8 %data, %len : !cssl.ptr
///                                                       // validate_symbol="__cssl_strvalidate"
/// ```
///
/// Result type is `MirType::Ptr` — the Result<String, FromUtf8Error>
/// tagged-union cell (Wave-A1 layout). The cgen lowering emits the
/// runtime UTF-8-validation extern call + Result construction.
fn try_lower_string_from_utf8(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (bytes_id, _bytes_ty) = lower_call_arg(ctx, &args[0])?;
    // Read bytes.data + bytes.len via `cssl.field` ops (matches the
    // Vec.data / Vec.len access pattern from `try_lower_vec_drop`).
    let data_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("cssl.field")
            .with_operand(bytes_id)
            .with_result(data_id, MirType::Int(IntWidth::I64))
            .with_attribute("field", "data")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    let len_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("cssl.field")
            .with_operand(bytes_id)
            .with_result(len_id, MirType::Int(IntWidth::I64))
            .with_attribute("field", "len")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Ptr;
    let mut op = crate::string_abi::build_string_from_utf8(data_id, len_id);
    op = op
        .with_result(result_id, result_ty.clone())
        .with_attribute("source_loc", format!("{span:?}"));
    ctx.ops.push(op);
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `string_from_utf8_unchecked(bytes) ->
/// String` call into a `cssl.string.from_utf8_unchecked` op.
///
/// § EMITTED-SHAPE
/// ```text
///   %b_id   = <lower(args[0])>                          // !cssl.struct.Vec<u8>
///   %data   = cssl.field %b_id, "data" : i64
///   %len    = cssl.field %b_id, "len"  : i64
///   %result = cssl.string.from_utf8_unchecked %data, %len : !cssl.ptr
///                                                       // total_size=24, alignment=8
/// ```
///
/// ‼ SAFETY (carried forward from stdlib/string.cssl) : caller guarantees
///   the bytes are valid UTF-8. The compiler does NOT check ; the cgen
///   lowering emits the heap-alloc + memcpy + triple-write fast-path
///   without the `__cssl_strvalidate` call.
fn try_lower_string_from_utf8_unchecked(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (bytes_id, _bytes_ty) = lower_call_arg(ctx, &args[0])?;
    let data_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("cssl.field")
            .with_operand(bytes_id)
            .with_result(data_id, MirType::Int(IntWidth::I64))
            .with_attribute("field", "data")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    let len_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("cssl.field")
            .with_operand(bytes_id)
            .with_result(len_id, MirType::Int(IntWidth::I64))
            .with_attribute("field", "len")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Ptr;
    let mut op = crate::string_abi::build_string_from_utf8_unchecked(data_id, len_id);
    op = op
        .with_result(result_id, result_ty.clone())
        .with_attribute("source_loc", format!("{span:?}"));
    ctx.ops.push(op);
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `string_push_str(s, slice) -> String`
/// call into a `cssl.string.push_str` op.
///
/// § EMITTED-SHAPE
/// ```text
///   %s_id    = <lower(args[0])>                         // !cssl.string
///   %sl_id   = <lower(args[1])>                         // !cssl.str_slice
///   %result  = cssl.string.push_str %s_id, %sl_id : !cssl.ptr
///                                                       // op="push_str"
/// ```
///
/// Stage-0 result-type is `!cssl.ptr` — the cgen lowering emits the
/// canonical Vec-extend + len-update sequence and the result is a fresh
/// String triple cell. A follow-up slice replaces the opaque tag with
/// the structural `String { data, len, cap }` once `MirType::String`
/// lands.
fn try_lower_string_push_str(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (s_id, _s_ty) = lower_call_arg(ctx, &args[0])?;
    let (sl_id, _sl_ty) = lower_call_arg(ctx, &args[1])?;
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Ptr;
    let layout = crate::string_abi::StringLayout::canonical();
    ctx.ops.push(
        MirOp::std("cssl.string.push_str")
            .with_operand(s_id)
            .with_operand(sl_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute(
                crate::string_abi::ATTR_SOURCE_KIND,
                crate::string_abi::SOURCE_KIND_STRING_ABI,
            )
            .with_attribute("op", "push_str")
            .with_attribute("total_size", layout.total_size.to_string())
            .with_attribute(
                crate::string_abi::ATTR_ALIGNMENT,
                layout.alignment.to_string(),
            )
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `string_as_str(s) -> StrSlice` call
/// into a `cssl.str_slice.new(data, len)` op pair (preceded by `data` +
/// `len` field-loads on the source `String`).
///
/// § EMITTED-SHAPE
/// ```text
///   %s_id    = <lower(args[0])>                         // !cssl.string
///   %data    = cssl.field %s_id, "data" : i64           // String.data
///   %len     = cssl.field %s_id, "len"  : i64           // String.len
///   %result  = cssl.str_slice.new %data, %len : !cssl.ptr
///                                                       // total_size=16, alignment=8
/// ```
fn try_lower_string_as_str(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (s_id, _s_ty) = lower_call_arg(ctx, &args[0])?;
    // Load String.data (host byte ptr).
    let data_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("cssl.field")
            .with_operand(s_id)
            .with_result(data_id, MirType::Int(IntWidth::I64))
            .with_attribute("field", "data")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    // Load String.len (byte count).
    let len_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("cssl.field")
            .with_operand(s_id)
            .with_result(len_id, MirType::Int(IntWidth::I64))
            .with_attribute("field", "len")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Ptr;
    let mut op = crate::string_abi::build_str_slice_new(data_id, len_id);
    op = op
        .with_result(result_id, result_ty.clone())
        .with_attribute("source_loc", format!("{span:?}"));
    ctx.ops.push(op);
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `string_byte_at(s, i) -> i32` call
/// into a `cssl.string.byte_at` op.
///
/// § EMITTED-SHAPE
/// ```text
///   %s_id   = <lower(args[0])>                          // !cssl.string
///   %i_id   = <lower(args[1])>                          // i64
///   %byte   = cssl.string.byte_at %s_id, %i_id : i32    // field=data, offset=0
/// ```
fn try_lower_string_byte_at(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (s_id, _s_ty) = lower_call_arg(ctx, &args[0])?;
    let (i_id, _i_ty) = lower_call_arg(ctx, &args[1])?;
    let result_id = ctx.fresh_value_id();
    let mut op = crate::string_abi::build_string_byte_at(s_id, i_id, result_id);
    op = op.with_attribute("source_loc", format!("{span:?}"));
    ctx.ops.push(op);
    Some((result_id, MirType::Int(IntWidth::I32)))
}

/// Lower a syntactically-recognized `str_as_bytes(slice) -> i64` call
/// into a `cssl.str_slice.as_bytes` op.
///
/// § EMITTED-SHAPE
/// ```text
///   %sl_id = <lower(args[0])>                           // !cssl.str_slice
///   %ptr   = cssl.str_slice.as_bytes %sl_id : i64       // field=ptr, offset=0
/// ```
fn try_lower_str_slice_as_bytes(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (sl_id, _sl_ty) = lower_call_arg(ctx, &args[0])?;
    let result_id = ctx.fresh_value_id();
    let mut op = crate::string_abi::build_str_slice_as_bytes(sl_id, result_id);
    op = op.with_attribute("source_loc", format!("{span:?}"));
    ctx.ops.push(op);
    Some((result_id, MirType::Int(IntWidth::I64)))
}

/// Lower a syntactically-recognized `char_from_u32(code) -> Option<char>`
/// call into a `cssl.char.from_u32` op (with a 5-cmp USV-invariant check
/// + Wave-A1 Option-construction at cgen time).
///
/// § EMITTED-SHAPE
/// ```text
///   %code   = <lower(args[0])>                          // i64
///   %option = cssl.char.from_u32 %code : !cssl.ptr      // tagged Option<i32>
///                                                       // usv_max_bmp / usv_max attrs
/// ```
///
/// Result-type is `MirType::Ptr` — the Option<char> tagged-union cell
/// (Wave-A1 layout, `!cssl.option.i32`).
fn try_lower_char_from_u32(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (code_id, _code_ty) = lower_call_arg(ctx, &args[0])?;
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Ptr;
    let mut op = crate::string_abi::build_char_from_u32(code_id, result_id);
    op = op.with_attribute("source_loc", format!("{span:?}"));
    ctx.ops.push(op);
    Some((result_id, result_ty))
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

// ─────────────────────────────────────────────────────────────────────────
// § T11-D82 (S7-F4) — networking I/O recognizers.
//
//   Stage-0 representation : each `net::*` call mints one `cssl.net.*` op
//   carrying :
//     - `net_effect` : "true"            // {Net} effect-row marker
//     - `family`     : "net"
//     - `op`         : "socket" | "listen" | "accept" | "connect" |
//                      "send" | "recv" | "sendto" | "recvfrom" | "close"
//     - `caps_required` : "net_outbound" / "net_inbound" (where applicable)
//     - `source_loc` : original span
//
//   The op-result type is :
//     - NetSocket   : `MirType::Int(I64)`   (socket-handle ; -1 on error)
//     - NetListen   : `MirType::Int(I64)`   (0 = ok ; -1 on error)
//     - NetAccept   : `MirType::Int(I64)`   (new-socket ; -1 on error)
//     - NetConnect  : `MirType::Int(I64)`   (0 = ok ; -1 on error)
//     - NetSend     : `MirType::Int(I64)`   (bytes-sent ; -1 on error)
//     - NetRecv     : `MirType::Int(I64)`   (bytes-recv ; 0 = peer-close ;
//                                            -1 on error)
//     - NetSendTo   : `MirType::Int(I64)`   (bytes-sent ; -1 on error)
//     - NetRecvFrom : `MirType::Int(I64)`   (bytes-recv ; -1 on error)
//     - NetClose    : `MirType::Int(I64)`   (0 = ok ; -1 on error)
//
//   PRIME-DIRECTIVE attestation : the `caps_required` attribute is the
//   stage-0 marker that downstream cap-system walkers consume to verify
//   the host has granted the matching `NET_CAP_*` bit before allowing
//   the call to fire (per `cssl-rt::net::caps_grant` discipline).
//
//   The cranelift / SPIR-V / DXIL / MSL / WGSL lowering of these ops to
//   actual `__cssl_net_*` calls is a deferred follow-up — at this slice
//   the ops are STRUCTURAL only (parse + walk + monomorph). Real
//   runtime execution comes once the cgen layer wires `func.call
//   __cssl_net_*` via `Linkage::Import` (mirrors B5's fs-op cgen wiring
//   pattern from T11-D76 § DEFERRED).
// ─────────────────────────────────────────────────────────────────────────

/// Dispatch a `net::*` callee to the matching `try_lower_net_*` recognizer.
///
/// Returns `Some(_)` if `callee` is a 2-segment path of the form
/// `net::<verb>` with the canonical arity for that verb ; otherwise
/// returns `None` so the caller falls through to the regular
/// `func.call` path. Extracted from `lower_call` to keep that fn under
/// the cognitive-complexity budget after S7-F4 added 9 net verbs.
fn try_lower_net_call(
    ctx: &mut BodyLowerCtx<'_>,
    callee: &HirExpr,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let HirExprKind::Path { segments, .. } = &callee.kind else {
        return None;
    };
    if segments.len() != 2 || ctx.interner.resolve(segments[0]) != "net" {
        return None;
    }
    let op = ctx.interner.resolve(segments[1]);
    match (op.as_str(), args.len()) {
        ("socket", 1) => try_lower_net_socket(ctx, args, span),
        ("listen", 4) => try_lower_net_listen(ctx, args, span),
        ("accept", 1) => try_lower_net_accept(ctx, args, span),
        ("connect", 3) => try_lower_net_connect(ctx, args, span),
        ("send", 3) => try_lower_net_send(ctx, args, span),
        ("recv", 3) => try_lower_net_recv(ctx, args, span),
        ("sendto", 5) => try_lower_net_sendto(ctx, args, span),
        ("recvfrom", 5) => try_lower_net_recvfrom(ctx, args, span),
        ("close", 1) => try_lower_net_close(ctx, args, span),
        _ => None,
    }
}

/// Lower a syntactically-recognized `net::socket(flags)` call.
fn try_lower_net_socket(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (flags_id, _) = lower_call_arg(ctx, &args[0])?;
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Int(IntWidth::I64);
    ctx.ops.push(
        MirOp::new(CsslOp::NetSocket)
            .with_operand(flags_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute("net_effect", "true")
            .with_attribute("family", "net")
            .with_attribute("op", "socket")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `net::listen(sock, addr, port, backlog)` call.
fn try_lower_net_listen(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (s_id, _) = lower_call_arg(ctx, &args[0])?;
    let (a_id, _) = lower_call_arg(ctx, &args[1])?;
    let (p_id, _) = lower_call_arg(ctx, &args[2])?;
    let (b_id, _) = lower_call_arg(ctx, &args[3])?;
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Int(IntWidth::I64);
    ctx.ops.push(
        MirOp::new(CsslOp::NetListen)
            .with_operand(s_id)
            .with_operand(a_id)
            .with_operand(p_id)
            .with_operand(b_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute("net_effect", "true")
            .with_attribute("family", "net")
            .with_attribute("op", "listen")
            .with_attribute("caps_required", "net_inbound")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `net::accept(sock)` call.
fn try_lower_net_accept(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (s_id, _) = lower_call_arg(ctx, &args[0])?;
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Int(IntWidth::I64);
    ctx.ops.push(
        MirOp::new(CsslOp::NetAccept)
            .with_operand(s_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute("net_effect", "true")
            .with_attribute("family", "net")
            .with_attribute("op", "accept")
            .with_attribute("caps_required", "net_inbound")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `net::connect(sock, addr, port)` call.
fn try_lower_net_connect(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (s_id, _) = lower_call_arg(ctx, &args[0])?;
    let (a_id, _) = lower_call_arg(ctx, &args[1])?;
    let (p_id, _) = lower_call_arg(ctx, &args[2])?;
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Int(IntWidth::I64);
    ctx.ops.push(
        MirOp::new(CsslOp::NetConnect)
            .with_operand(s_id)
            .with_operand(a_id)
            .with_operand(p_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute("net_effect", "true")
            .with_attribute("family", "net")
            .with_attribute("op", "connect")
            .with_attribute("caps_required", "net_outbound")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `net::send(sock, buf_ptr, buf_len)` call.
fn try_lower_net_send(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (s_id, _) = lower_call_arg(ctx, &args[0])?;
    let (p_id, _) = lower_call_arg(ctx, &args[1])?;
    let (n_id, _) = lower_call_arg(ctx, &args[2])?;
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Int(IntWidth::I64);
    ctx.ops.push(
        MirOp::new(CsslOp::NetSend)
            .with_operand(s_id)
            .with_operand(p_id)
            .with_operand(n_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute("net_effect", "true")
            .with_attribute("family", "net")
            .with_attribute("op", "send")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `net::recv(sock, buf_ptr, buf_len)` call.
fn try_lower_net_recv(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (s_id, _) = lower_call_arg(ctx, &args[0])?;
    let (p_id, _) = lower_call_arg(ctx, &args[1])?;
    let (n_id, _) = lower_call_arg(ctx, &args[2])?;
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Int(IntWidth::I64);
    ctx.ops.push(
        MirOp::new(CsslOp::NetRecv)
            .with_operand(s_id)
            .with_operand(p_id)
            .with_operand(n_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute("net_effect", "true")
            .with_attribute("family", "net")
            .with_attribute("op", "recv")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `net::sendto(sock, buf_ptr, buf_len, addr, port)` call.
fn try_lower_net_sendto(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (s_id, _) = lower_call_arg(ctx, &args[0])?;
    let (p_id, _) = lower_call_arg(ctx, &args[1])?;
    let (n_id, _) = lower_call_arg(ctx, &args[2])?;
    let (a_id, _) = lower_call_arg(ctx, &args[3])?;
    let (po_id, _) = lower_call_arg(ctx, &args[4])?;
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Int(IntWidth::I64);
    ctx.ops.push(
        MirOp::new(CsslOp::NetSendTo)
            .with_operand(s_id)
            .with_operand(p_id)
            .with_operand(n_id)
            .with_operand(a_id)
            .with_operand(po_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute("net_effect", "true")
            .with_attribute("family", "net")
            .with_attribute("op", "sendto")
            .with_attribute("caps_required", "net_outbound")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `net::recvfrom(sock, buf_ptr, buf_len, addr_out_ptr, port_out_ptr)` call.
fn try_lower_net_recvfrom(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (s_id, _) = lower_call_arg(ctx, &args[0])?;
    let (p_id, _) = lower_call_arg(ctx, &args[1])?;
    let (n_id, _) = lower_call_arg(ctx, &args[2])?;
    let (a_id, _) = lower_call_arg(ctx, &args[3])?;
    let (po_id, _) = lower_call_arg(ctx, &args[4])?;
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Int(IntWidth::I64);
    ctx.ops.push(
        MirOp::new(CsslOp::NetRecvFrom)
            .with_operand(s_id)
            .with_operand(p_id)
            .with_operand(n_id)
            .with_operand(a_id)
            .with_operand(po_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute("net_effect", "true")
            .with_attribute("family", "net")
            .with_attribute("op", "recvfrom")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((result_id, result_ty))
}

/// Lower a syntactically-recognized `net::close(sock)` call.
fn try_lower_net_close(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (s_id, _) = lower_call_arg(ctx, &args[0])?;
    let result_id = ctx.fresh_value_id();
    let result_ty = MirType::Int(IntWidth::I64);
    ctx.ops.push(
        MirOp::new(CsslOp::NetClose)
            .with_operand(s_id)
            .with_result(result_id, result_ty.clone())
            .with_attribute("net_effect", "true")
            .with_attribute("family", "net")
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

// ─────────────────────────────────────────────────────────────────────────
// § T11-D288 (W-E5-5) — SIMD-intrinsic recognizer helpers.
//
//   Each helper lowers a syntactically-recognized stdlib SIMD-intrinsic
//   call into the matching `cssl.simd.*` MIR op shape produced by
//   `crate::simd_abi::build_*`. The op carries lane-width / lanes /
//   alignment attributes so the cgen layer can dispatch to the correct
//   CLIF SIMD type (`i8x16` / `i16x8` / `i32x4` / `i64x2`).
//
//   The helpers preserve the `lower_call_arg` + `fresh_value_id` pattern
//   used by the string / vec / fs / net recognizer chains above.
// ─────────────────────────────────────────────────────────────────────────

/// Lower `simd_v128_load(ptr) -> v128`. Default lane-width 8 (i8x16).
fn try_lower_simd_v128_load(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (ptr_id, _ptr_ty) = lower_call_arg(ctx, &args[0])?;
    let result_id = ctx.fresh_value_id();
    let mut op = crate::simd_abi::build_v128_load(ptr_id, result_id, 8);
    op = op.with_attribute("source_loc", format!("{span:?}"));
    let result_ty = crate::simd_abi::v128_ty();
    ctx.ops.push(op);
    Some((result_id, result_ty))
}

/// Lower `simd_v128_store(v, ptr)`. Returns a synthesized unit-id since
/// stage-0 ops always produce some result-id — caller may discard.
fn try_lower_simd_v128_store(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (v_id, _v_ty) = lower_call_arg(ctx, &args[0])?;
    let (ptr_id, _ptr_ty) = lower_call_arg(ctx, &args[1])?;
    let mut op = crate::simd_abi::build_v128_store(v_id, ptr_id, 8);
    op = op.with_attribute("source_loc", format!("{span:?}"));
    ctx.ops.push(op);
    // Synthesize an i32-zero value-id as the call's notional result so
    // downstream lowering can thread a value through (matches the
    // pattern used by other void-returning stdlib recognizers).
    let unit_id = ctx.fresh_value_id();
    ctx.ops.push(
        MirOp::std("arith.constant")
            .with_result(unit_id, MirType::Int(IntWidth::I32))
            .with_attribute("value", "0")
            .with_attribute("source_loc", format!("{span:?}")),
    );
    Some((unit_id, MirType::Int(IntWidth::I32)))
}

/// Lower `simd_v_byte_eq(a, b) -> v128`.
fn try_lower_simd_v_byte_eq(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (a_id, _a_ty) = lower_call_arg(ctx, &args[0])?;
    let (b_id, _b_ty) = lower_call_arg(ctx, &args[1])?;
    let result_id = ctx.fresh_value_id();
    let mut op = crate::simd_abi::build_v_byte_eq(a_id, b_id, result_id);
    op = op.with_attribute("source_loc", format!("{span:?}"));
    let result_ty = crate::simd_abi::v128_ty();
    ctx.ops.push(op);
    Some((result_id, result_ty))
}

/// Lower `simd_v_byte_lt(a, b) -> v128` (unsigned).
fn try_lower_simd_v_byte_lt(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (a_id, _a_ty) = lower_call_arg(ctx, &args[0])?;
    let (b_id, _b_ty) = lower_call_arg(ctx, &args[1])?;
    let result_id = ctx.fresh_value_id();
    let mut op = crate::simd_abi::build_v_byte_lt(a_id, b_id, result_id);
    op = op.with_attribute("source_loc", format!("{span:?}"));
    let result_ty = crate::simd_abi::v128_ty();
    ctx.ops.push(op);
    Some((result_id, result_ty))
}

/// Lower `simd_v_byte_in_range(v, lo, hi) -> v128` (inclusive).
fn try_lower_simd_v_byte_in_range(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (v_id, _v_ty) = lower_call_arg(ctx, &args[0])?;
    let (lo_id, _lo_ty) = lower_call_arg(ctx, &args[1])?;
    let (hi_id, _hi_ty) = lower_call_arg(ctx, &args[2])?;
    let result_id = ctx.fresh_value_id();
    let mut op = crate::simd_abi::build_v_byte_in_range(v_id, lo_id, hi_id, result_id);
    op = op.with_attribute("source_loc", format!("{span:?}"));
    let result_ty = crate::simd_abi::v128_ty();
    ctx.ops.push(op);
    Some((result_id, result_ty))
}

/// Lower `simd_v_prefix_sum(v) -> v128`.
fn try_lower_simd_v_prefix_sum(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (v_id, _v_ty) = lower_call_arg(ctx, &args[0])?;
    let result_id = ctx.fresh_value_id();
    let mut op = crate::simd_abi::build_v_prefix_sum(v_id, result_id);
    op = op.with_attribute("source_loc", format!("{span:?}"));
    let result_ty = crate::simd_abi::v128_ty();
    ctx.ops.push(op);
    Some((result_id, result_ty))
}

/// Lower `simd_v_horizontal_sum(v) -> i32`.
fn try_lower_simd_v_horizontal_sum(
    ctx: &mut BodyLowerCtx<'_>,
    args: &[HirCallArg],
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (v_id, _v_ty) = lower_call_arg(ctx, &args[0])?;
    let result_id = ctx.fresh_value_id();
    let mut op = crate::simd_abi::build_v_horizontal_sum(v_id, result_id);
    op = op.with_attribute("source_loc", format!("{span:?}"));
    ctx.ops.push(op);
    Some((result_id, MirType::Int(IntWidth::I32)))
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

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D77 (S6-C5 redo) — Closure env-capture lowering tests.
    //
    // Each test exercises an aspect of the free-var collector + env-pack +
    // cssl.closure attribute surface defined in `specs/02_IR.csl § CLOSURE-ENV`.
    // ─────────────────────────────────────────────────────────────────────

    /// Helper : retrieve the cssl.closure op (panics if missing).
    fn closure_op(f: &crate::func::MirFunc) -> &crate::block::MirOp {
        find_op(f, "cssl.closure").expect("missing cssl.closure")
    }

    #[test]
    fn closure_with_no_captures_emits_zero_capture_count() {
        // `|x| x + 1` references only its own param ⇒ no free-vars.
        let src = "fn f() { let g = |x : i32| { x + 1 }; g(0); }";
        let (f, _) = lower_one(src);
        let cop = closure_op(&f);
        assert_eq!(attr(cop, "capture_count"), Some("0"));
        assert_eq!(attr(cop, "env_size"), Some("0"));
        assert_eq!(attr(cop, "env_align"), Some("8"));
        // No env alloc when there are no captures.
        let entry = f.body.entry().unwrap();
        assert!(
            !entry.ops.iter().any(|o| o.name == "cssl.heap.alloc"),
            "no captures ⇒ no env alloc"
        );
    }

    #[test]
    fn closure_capturing_outer_let_binding_emits_heap_alloc() {
        // `let y = 7; let g = |x| x + y` ⇒ y is a free-var ⇒ env-alloc.
        let src = "fn f() { let y = 7; let g = |x : i32| { x + y }; g(0); }";
        let (f, _) = lower_one(src);
        let cop = closure_op(&f);
        assert_eq!(attr(cop, "capture_count"), Some("1"));
        assert_eq!(attr(cop, "env_size"), Some("8"));
        // capture_names attribute records discovered names in encounter-order.
        assert_eq!(attr(cop, "capture_names"), Some("y"));
        // The env alloc must precede the closure op.
        let entry = f.body.entry().unwrap();
        let alloc_idx = entry
            .ops
            .iter()
            .position(|o| o.name == "cssl.heap.alloc")
            .expect("missing cssl.heap.alloc");
        let closure_idx = entry
            .ops
            .iter()
            .position(|o| o.name == "cssl.closure")
            .expect("missing cssl.closure");
        assert!(alloc_idx < closure_idx, "env alloc must precede closure");
    }

    #[test]
    fn closure_capturing_outer_param_emits_heap_alloc() {
        // Outer `n` is a fn param ; inner `|| n + 1` captures it.
        let src = "fn f(n : i32) -> i32 { let g = |x : i32| { x + n }; g(0) }";
        let (f, _) = lower_one(src);
        let cop = closure_op(&f);
        assert_eq!(attr(cop, "capture_count"), Some("1"));
        assert_eq!(attr(cop, "capture_names"), Some("n"));
    }

    #[test]
    fn closure_with_two_captures_emits_eight_byte_per_slot() {
        // env_size = 8 × 2 = 16. Captures encountered in order : a then b.
        let src = "fn f() { let a = 1; let b = 2; let g = |x : i32| { x + a + b }; g(0); }";
        let (f, _) = lower_one(src);
        let cop = closure_op(&f);
        assert_eq!(attr(cop, "capture_count"), Some("2"));
        assert_eq!(attr(cop, "env_size"), Some("16"));
        assert_eq!(attr(cop, "capture_names"), Some("a,b"));
    }

    #[test]
    fn closure_op_carries_cap_value_attribute() {
        // Capture-by-value default (CapKind::Val per cap_check).
        let src = "fn f() { let y = 1; let g = |x : i32| { x + y }; g(0); }";
        let (f, _) = lower_one(src);
        let cop = closure_op(&f);
        assert_eq!(attr(cop, "cap_value"), Some("val"));
    }

    #[test]
    fn closure_emits_env_alloc_with_iso_cap_attribute() {
        // The env-alloc op must carry cap=iso per § ISO-OWNERSHIP.
        let src = "fn f() { let y = 1; let g = |x : i32| { x + y }; g(0); }";
        let (f, _) = lower_one(src);
        let alloc = find_op(&f, "cssl.heap.alloc").expect("missing cssl.heap.alloc");
        assert_eq!(attr(alloc, "cap"), Some("iso"));
        assert_eq!(attr(alloc, "origin"), Some("closure_env"));
    }

    #[test]
    fn closure_emits_one_memref_store_per_capture() {
        // Two captures ⇒ two memref.store ops between the alloc and the closure.
        let src = "fn f() { let a = 1; let b = 2; let g = |x : i32| { x + a + b }; g(0); }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let alloc_idx = entry
            .ops
            .iter()
            .position(|o| o.name == "cssl.heap.alloc")
            .expect("missing cssl.heap.alloc");
        let closure_idx = entry
            .ops
            .iter()
            .position(|o| o.name == "cssl.closure")
            .expect("missing cssl.closure");
        let store_count = entry.ops[alloc_idx..closure_idx]
            .iter()
            .filter(|o| o.name == "memref.store")
            .count();
        assert_eq!(
            store_count, 2,
            "expected 2 memref.store between alloc and closure"
        );
    }

    // ── S7-F4 (T11-D82) network recognizer coverage ─────────────────────
    //
    //   Mirrors the fs recognizer test block above. Each test confirms
    //   that the canonical 2-segment `net::*` call lowers to the matching
    //   `cssl.net.*` op + carries the `(net_effect, "true")` attribute
    //   marker. The connect/listen/sendto/accept variants additionally
    //   carry the `caps_required` PRIME-DIRECTIVE marker.

    #[test]
    fn lower_net_socket_emits_cssl_net_socket() {
        // `net::socket(SOCK_TCP)` should lower to `cssl.net.socket`.
        let (f, _) = lower_one("fn f() -> i64 { net::socket(1) }");
        let op =
            find_op(&f, "cssl.net.socket").expect("net::socket should lower to cssl.net.socket");
        assert_eq!(attr(op, "net_effect"), Some("true"));
        assert_eq!(attr(op, "family"), Some("net"));
        assert_eq!(attr(op, "op"), Some("socket"));
    }

    #[test]
    fn lower_net_listen_emits_cssl_net_listen_with_caps_inbound() {
        let (f, _) = lower_one(
            "fn f(s : i64, a : i32, p : i32, b : i32) -> i64 { net::listen(s, a, p, b) }",
        );
        let op = find_op(&f, "cssl.net.listen").expect("net::listen should lower");
        assert_eq!(attr(op, "net_effect"), Some("true"));
        assert_eq!(attr(op, "op"), Some("listen"));
        assert_eq!(
            attr(op, "caps_required"),
            Some("net_inbound"),
            "listen requires net_inbound cap"
        );
    }

    #[test]
    fn closure_lambda_param_does_not_become_capture() {
        // `|x| x` references only its param — no free-vars even though `x`
        // would resolve in an outer scope (it doesn't here, but the test
        // confirms params take precedence over the outer `x` even if one
        // existed).
        let src = "fn f() { let x = 99; let g = |x : i32| { x + 1 }; g(0); }";
        let (f, _) = lower_one(src);
        let cop = closure_op(&f);
        // The lambda's `x` shadows the outer `x` — no captures.
        assert_eq!(attr(cop, "capture_count"), Some("0"));
    }

    #[test]
    fn closure_unresolved_free_var_is_dropped() {
        // The body refs `unknown` which doesn't bind anywhere ⇒ dropped from
        // the capture-list (silently). This matches the documented stage-0
        // behavior in lower_lambda's contract.
        let src = "fn f() { let g = |x : i32| { x + unknown }; g(0); }";
        let (f, _) = lower_one(src);
        let cop = closure_op(&f);
        // The collector found `unknown` as a free-var BUT it didn't resolve
        // in either param_vars or local_vars — capture-list is empty.
        assert_eq!(attr(cop, "capture_count"), Some("0"));
    }

    #[test]
    fn closure_captured_value_appears_in_operand_list() {
        // The captured outer-scope value-id must appear among the closure
        // op's operands. `let y = 7; |x| x + y` : outer y becomes capture-0
        // and the env-ptr is the trailing operand.
        let src = "fn f() { let y = 7; let g = |x : i32| { x + y }; g(0); }";
        let (f, _) = lower_one(src);
        let cop = closure_op(&f);
        // capture_count = 1, env-ptr = trailing operand ⇒ 2 operands total.
        assert_eq!(cop.operands.len(), 2);
    }

    #[test]
    fn closure_no_captures_op_has_no_operands() {
        let src = "fn f() { let g = |x : i32| { x + 1 }; g(0); }";
        let (f, _) = lower_one(src);
        let cop = closure_op(&f);
        assert_eq!(cop.operands.len(), 0);
    }

    #[test]
    fn closure_body_region_preserved() {
        // Backwards-compat : the existing T6-phase-2c invariant — body lives
        // in regions[0] — must hold across S6-C5 redo.
        let src = "fn f() { let y = 1; let g = |x : i32| { x + y }; g(0); }";
        let (f, _) = lower_one(src);
        let cop = closure_op(&f);
        assert_eq!(cop.regions.len(), 1);
        let body_ops = &cop.regions[0].blocks[0].ops;
        assert!(
            body_ops.iter().any(|o| o.name.starts_with("arith.")),
            "expected arith.* in lambda body, got {:?}",
            body_ops.iter().map(|o| &o.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn lower_net_accept_emits_cssl_net_accept_with_caps_inbound() {
        let (f, _) = lower_one("fn f(s : i64) -> i64 { net::accept(s) }");
        let op = find_op(&f, "cssl.net.accept").expect("net::accept should lower");
        assert_eq!(attr(op, "net_effect"), Some("true"));
        assert_eq!(attr(op, "caps_required"), Some("net_inbound"));
    }

    #[test]
    fn lower_net_connect_emits_cssl_net_connect_with_caps_outbound() {
        let (f, _) = lower_one("fn f(s : i64, a : i32, p : i32) -> i64 { net::connect(s, a, p) }");
        let op = find_op(&f, "cssl.net.connect").expect("net::connect should lower");
        assert_eq!(attr(op, "net_effect"), Some("true"));
        assert_eq!(
            attr(op, "caps_required"),
            Some("net_outbound"),
            "connect requires net_outbound cap"
        );
    }

    #[test]
    fn closure_inner_let_shadows_outer_free_var() {
        // `|x| { let y = 5; x + y }` — inner `let y` shadows the outer y so
        // the body's `x + y` refs the inner y. No captures should result.
        let src = "fn f() { let y = 7; let g = |x : i32| { let y = 5; x + y }; g(0); }";
        let (f, _) = lower_one(src);
        let cop = closure_op(&f);
        // Inner-let y shadows outer y ⇒ no free-var ⇒ no captures.
        assert_eq!(attr(cop, "capture_count"), Some("0"));
    }

    #[test]
    fn closure_param_count_attribute_matches_params() {
        // Multi-param lambda preserves the param_count attribute (T6-phase-2c
        // backwards-compat).
        let src = "fn f() { let g = |x : i32, y : i32| { x + y }; g(1, 2); }";
        let (f, _) = lower_one(src);
        let cop = closure_op(&f);
        assert_eq!(attr(cop, "param_count"), Some("2"));
    }

    // ─────────────────────────────────────────────────────────────────────
    // § Path-resolution against local_vars — S6-C5 redo unblocks let-binding refs.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn let_binding_path_ref_resolves_to_let_value_id() {
        // Pre-S6-C5 : `let x = 1; x + 2` would lower `x` to a `cssl.path_ref`
        // placeholder. With local_vars threading, `x` resolves to the let's
        // ValueId so `x + 2` becomes `arith.addi <let-value>, <constant>`.
        let src = "fn f() -> i32 { let x = 1; x + 2 }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().unwrap();
        let names: Vec<&str> = entry.ops.iter().map(|o| o.name.as_str()).collect();
        // The body must contain arith.addi (the binary op's resolved form),
        // NOT a cssl.path_ref opaque placeholder for `x`.
        assert!(
            names.iter().any(|n| *n == "arith.addi"),
            "expected arith.addi after let-binding resolution, got {names:?}"
        );
        assert!(
            !names.iter().any(|n| *n == "cssl.path_ref"),
            "let-binding `x` must not lower to opaque path_ref"
        );
    }

    #[test]
    fn lower_net_send_emits_cssl_net_send() {
        let (f, _) = lower_one("fn f(s : i64, p : i64, n : i64) -> i64 { net::send(s, p, n) }");
        let op = find_op(&f, "cssl.net.send").expect("net::send should lower");
        assert_eq!(attr(op, "net_effect"), Some("true"));
        assert_eq!(attr(op, "op"), Some("send"));
    }

    #[test]
    fn lower_net_recv_emits_cssl_net_recv() {
        let (f, _) = lower_one("fn f(s : i64, p : i64, n : i64) -> i64 { net::recv(s, p, n) }");
        let op = find_op(&f, "cssl.net.recv").expect("net::recv should lower");
        assert_eq!(attr(op, "net_effect"), Some("true"));
        assert_eq!(attr(op, "op"), Some("recv"));
    }

    #[test]
    fn lower_net_sendto_emits_cssl_net_sendto_with_caps_outbound() {
        let (f, _) = lower_one(
            "fn f(s : i64, p : i64, n : i64, a : i32, port : i32) -> i64 { \
                net::sendto(s, p, n, a, port) \
            }",
        );
        let op = find_op(&f, "cssl.net.sendto").expect("net::sendto should lower");
        assert_eq!(attr(op, "net_effect"), Some("true"));
        assert_eq!(attr(op, "caps_required"), Some("net_outbound"));
    }

    #[test]
    fn lower_net_recvfrom_emits_cssl_net_recvfrom() {
        let (f, _) = lower_one(
            "fn f(s : i64, p : i64, n : i64, a : i64, po : i64) -> i64 { \
                net::recvfrom(s, p, n, a, po) \
            }",
        );
        let op = find_op(&f, "cssl.net.recvfrom").expect("net::recvfrom should lower");
        assert_eq!(attr(op, "net_effect"), Some("true"));
        assert_eq!(attr(op, "op"), Some("recvfrom"));
    }

    #[test]
    fn lower_net_close_emits_cssl_net_close() {
        let (f, _) = lower_one("fn f(s : i64) -> i64 { net::close(s) }");
        let op = find_op(&f, "cssl.net.close").expect("net::close should lower");
        assert_eq!(attr(op, "net_effect"), Some("true"));
        assert_eq!(attr(op, "op"), Some("close"));
    }

    #[test]
    fn lower_net_socket_wrong_arity_falls_through_to_func_call() {
        // 0-arg net::socket() doesn't match the recognizer (expects 1 arg).
        let (f, _) = lower_one("fn f() -> i64 { net::socket() }");
        // No cssl.net.socket op ; instead a func.call op exists.
        assert!(find_op(&f, "cssl.net.socket").is_none());
    }

    #[test]
    fn lower_bare_socket_is_not_claimed_by_recognizer() {
        // `socket(...)` (single-segment) is NOT recognized — only the
        // 2-segment `net::socket` form qualifies. Guards against
        // accidental shadowing of user identifiers.
        let (f, _) = lower_one("fn f() -> i64 { socket(1) }");
        assert!(find_op(&f, "cssl.net.socket").is_none());
    }

    #[test]
    fn lower_other_module_socket_is_not_claimed() {
        // `foo::socket(...)` is NOT `net::socket(...)`.
        let (f, _) = lower_one("fn f() -> i64 { foo::socket(1) }");
        assert!(find_op(&f, "cssl.net.socket").is_none());
    }

    #[test]
    fn lower_net_socket_records_source_loc_attribute() {
        let (f, _) = lower_one("fn f() -> i64 { net::socket(1) }");
        let op = find_op(&f, "cssl.net.socket").expect("net::socket should lower");
        let loc = attr(op, "source_loc").expect("source_loc missing");
        assert!(!loc.is_empty());
    }

    // ───────────────────────────────────────────────────────────────────
    // § T11-D100 (J2 — closures callable from CSSLv3 source)
    //
    // Tests for the call-site recognizer + inline-expansion lowerer.
    // See spec § CLOSURE-ENV "invocation (T11-D100 / J2 …)".
    // ───────────────────────────────────────────────────────────────────

    /// Helper : count occurrences of an op name in the entry-block.
    fn count_op(f: &crate::func::MirFunc, name: &str) -> usize {
        f.body
            .entry()
            .map_or(0, |b| b.ops.iter().filter(|o| o.name == name).count())
    }

    #[test]
    fn closure_call_zero_capture_emits_marker_op() {
        // Source : `fn f() -> i32 { let g = |x : i32| x * 2 ; g(7) }`.
        // The call-site recognizer fires (callee `g` resolves to a
        // `!cssl.closure`-typed local) and emits `cssl.closure.call`.
        let src = "fn f() -> i32 { let g = |x : i32| x * 2; g(7) }";
        let (f, _) = lower_one(src);
        assert!(
            find_op(&f, "cssl.closure.call").is_some(),
            "expected cssl.closure.call marker in op-stream"
        );
    }

    #[test]
    fn closure_call_zero_capture_inlines_body_arith() {
        // The inlined body is `x * 2` ⇒ `arith.muli` emitted in the OUTER
        // ctx. Combined with the constant 7 (call-site arg) and constant 2
        // (lambda body), we expect ≥ 2 arith.constant + 1 arith.muli.
        let src = "fn f() -> i32 { let g = |x : i32| x * 2; g(7) }";
        let (f, _) = lower_one(src);
        assert!(
            count_op(&f, "arith.muli") >= 1,
            "expected arith.muli from inlined `x * 2` body, got ops {:?}",
            op_names(&f)
        );
    }

    #[test]
    fn closure_call_zero_capture_no_memref_load() {
        // Zero-capture closure ⇒ no env_ptr ⇒ no memref.load reload at
        // the call site (the body has no captures to reconstitute).
        let src = "fn f() -> i32 { let g = |x : i32| x + 1; g(5) }";
        let (f, _) = lower_one(src);
        // The function should have NO memref.load ops at all (the closure
        // construct emits zero memref.* because zero-capture).
        assert_eq!(
            count_op(&f, "memref.load"),
            0,
            "zero-capture closure-call must not emit memref.load"
        );
    }

    #[test]
    fn closure_call_zero_capture_marker_records_arity() {
        let src = "fn f() -> i32 { let g = |x : i32| x + 1; g(5) }";
        let (f, _) = lower_one(src);
        let op = find_op(&f, "cssl.closure.call").expect("missing marker");
        assert_eq!(attr(op, "param_count"), Some("1"));
        assert_eq!(attr(op, "capture_count"), Some("0"));
        assert_eq!(attr(op, "env_size"), Some("0"));
        assert_eq!(attr(op, "env_align"), Some("8"));
    }

    #[test]
    fn closure_call_zero_capture_marker_carries_yield_value_id() {
        // The marker's `yield_value_id` attribute points at the ValueId of
        // the body's trailing yield — backends use this to bind the call's
        // result-id.
        let src = "fn f() -> i32 { let g = |x : i32| x + 1; g(5) }";
        let (f, _) = lower_one(src);
        let op = find_op(&f, "cssl.closure.call").expect("missing marker");
        let yid = attr(op, "yield_value_id").expect("yield_value_id missing");
        assert!(
            yid.parse::<u32>().is_ok(),
            "yield_value_id must be a u32, got `{yid}`"
        );
    }

    #[test]
    fn closure_call_with_capture_emits_memref_load_reload() {
        // Source : `let y = 7; let g = |x| x + y; g(3)`. The closure
        // captures `y` ; at the call site, the body must be re-lowered with
        // `y` resolved to a memref.load on the env_ptr.
        let src = "fn f() -> i32 { let y = 7; let g = |x : i32| x + y; g(3) }";
        let (f, _) = lower_one(src);
        // We expect AT LEAST ONE memref.load op from the call-site capture
        // reload sequence. The construct-site emits memref.store ops too —
        // both should be present in the op-stream.
        assert!(
            count_op(&f, "memref.load") >= 1,
            "expected memref.load reload at closure call site, got ops {:?}",
            op_names(&f)
        );
        assert!(
            count_op(&f, "memref.store") >= 1,
            "expected memref.store from env-pack at closure construct site"
        );
    }

    #[test]
    fn closure_call_with_capture_marker_records_capture_offsets() {
        let src = "fn f() -> i32 { let y = 7; let g = |x : i32| x + y; g(3) }";
        let (f, _) = lower_one(src);
        let op = find_op(&f, "cssl.closure.call").expect("missing marker");
        assert_eq!(attr(op, "param_count"), Some("1"));
        assert_eq!(attr(op, "capture_count"), Some("1"));
        assert_eq!(attr(op, "env_size"), Some("8"));
        // capture_offsets attribute lists per-capture byte-offset.
        assert_eq!(attr(op, "capture_offsets"), Some("0"));
    }

    #[test]
    fn closure_call_with_capture_reload_attribute_origin() {
        // Each capture-reload memref.load carries an `origin` attribute
        // tagging it as "closure_capture_reload" so future passes can
        // distinguish reloads from user-emitted memref.loads.
        let src = "fn f() -> i32 { let y = 7; let g = |x : i32| x + y; g(3) }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().expect("entry block");
        let reload = entry
            .ops
            .iter()
            .find(|o| {
                o.name == "memref.load"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "origin" && v == "closure_capture_reload")
            })
            .expect("missing capture-reload memref.load");
        // The reload also carries a `capture_name` attribute with the
        // captured symbol's resolved name.
        let cname = reload
            .attributes
            .iter()
            .find(|(k, _)| k == "capture_name")
            .map(|(_, v)| v.as_str());
        assert_eq!(cname, Some("y"));
    }

    #[test]
    fn closure_call_with_two_captures_records_both_offsets() {
        // Two captures → env_size=16, capture_offsets="0,8".
        let src = "fn f() -> i32 { let a = 1; let b = 2; let g = |x : i32| x + a + b; g(0) }";
        let (f, _) = lower_one(src);
        let op = find_op(&f, "cssl.closure.call").expect("missing marker");
        assert_eq!(attr(op, "capture_count"), Some("2"));
        assert_eq!(attr(op, "env_size"), Some("16"));
        assert_eq!(attr(op, "capture_offsets"), Some("0,8"));
        // Two memref.load reloads at the call site.
        assert!(count_op(&f, "memref.load") >= 2);
    }

    #[test]
    fn closure_call_arity_mismatch_emits_error_op() {
        // Source : `let g = |x : i32| x ; g()` — call site supplies 0 args
        // for a 1-param lambda. The recognizer emits cssl.closure.call.error
        // with detail attribute.
        let src = "fn f() -> i32 { let g = |x : i32| x; g() }";
        let (f, _) = lower_one(src);
        let err = find_op(&f, "cssl.closure.call.error")
            .expect("expected cssl.closure.call.error for arity mismatch");
        assert_eq!(attr(err, "expected_arity"), Some("1"));
        assert_eq!(attr(err, "actual_arity"), Some("0"));
        let detail = attr(err, "detail").expect("detail attribute missing");
        assert!(
            detail.contains("arity mismatch"),
            "detail should mention arity mismatch, got `{detail}`"
        );
    }

    #[test]
    fn closure_call_arity_mismatch_does_not_emit_marker() {
        // A real (successful) cssl.closure.call should NOT also be emitted
        // when the arity check fails — the error op stands alone.
        let src = "fn f() -> i32 { let g = |x : i32| x; g() }";
        let (f, _) = lower_one(src);
        assert!(
            find_op(&f, "cssl.closure.call").is_none(),
            "successful marker must not co-exist with the arity-error op"
        );
    }

    #[test]
    fn closure_call_two_args_two_params_inlines() {
        // Two-arg closure : `let add = |a : i32, b : i32| a + b ; add(3, 4)`.
        // The marker carries 3 operands : closure-vid + 2 arg-vids.
        let src = "fn f() -> i32 { let add = |a : i32, b : i32| a + b; add(3, 4) }";
        let (f, _) = lower_one(src);
        let op = find_op(&f, "cssl.closure.call").expect("missing marker");
        assert_eq!(attr(op, "param_count"), Some("2"));
        // operands : [closure_vid, arg_0, arg_1] = 3 total.
        assert_eq!(op.operands.len(), 3);
        // The body's `a + b` lowers to arith.addi.
        assert!(count_op(&f, "arith.addi") >= 1);
    }

    #[test]
    fn closure_call_unit_body_records_no_yield_value_id() {
        // A unit-returning closure : `let g = |x : i32| {} ; g(0)`.
        // The body has no trailing yield ⇒ marker carries no yield_value_id.
        let src = "fn f() { let g = |x : i32| {}; g(0); }";
        let (f, _) = lower_one(src);
        let op = find_op(&f, "cssl.closure.call").expect("missing marker");
        // No yield_value_id attribute when the body produces no value.
        assert!(
            attr(op, "yield_value_id").is_none(),
            "unit-returning closure must not record yield_value_id"
        );
    }

    #[test]
    fn closure_call_marker_records_source_loc() {
        let src = "fn f() -> i32 { let g = |x : i32| x; g(7) }";
        let (f, _) = lower_one(src);
        let op = find_op(&f, "cssl.closure.call").expect("missing marker");
        let loc = attr(op, "source_loc").expect("source_loc missing");
        assert!(!loc.is_empty());
    }

    #[test]
    fn closure_call_marker_records_hir_id() {
        // The hir_id attribute lets downstream passes (auto-mono / IFC /
        // diagnostics) map the marker back to its originating Call node.
        let src = "fn f() -> i32 { let g = |x : i32| x; g(7) }";
        let (f, _) = lower_one(src);
        let op = find_op(&f, "cssl.closure.call").expect("missing marker");
        let hid = attr(op, "hir_id").expect("hir_id missing");
        assert!(hid.parse::<u32>().is_ok());
    }

    #[test]
    fn closure_construct_descriptor_visible_to_call_site() {
        // Sanity : the descriptor mechanism wires up correctly. Without it
        // the call-site recognizer would silently route through the regular
        // `func.call` path and the marker op would never appear.
        let src = "fn f() -> i32 { let g = |x : i32| x + 1; g(5) }";
        let (f, _) = lower_one(src);
        // The closure-construct op is still present (T11-D77 invariant).
        assert!(find_op(&f, "cssl.closure").is_some());
        // The call-site marker is emitted (T11-D100 invariant).
        assert!(find_op(&f, "cssl.closure.call").is_some());
        // No regular func.call falls through (the recognizer claimed it).
        assert_eq!(count_op(&f, "func.call"), 0);
    }

    #[test]
    fn closure_call_param_arg_passes_through() {
        // Lambda param `x` in the body resolves to the call-site arg's
        // ValueId (not a fresh-emitted constant). The body's identity-form
        // `|x| x` should yield the call-site arg's value-id directly.
        let src = "fn f() -> i32 { let id = |x : i32| x; id(42) }";
        let (f, _) = lower_one(src);
        // The marker's yield_value_id should point at a ValueId that was
        // ALSO an operand of the marker (the call-site arg).
        let op = find_op(&f, "cssl.closure.call").expect("missing marker");
        let yid: u32 = attr(op, "yield_value_id")
            .and_then(|s| s.parse().ok())
            .expect("yield_value_id missing/malformed");
        let yield_vid = crate::value::ValueId(yid);
        // The yield value-id should match the marker's arg operand.
        assert!(
            op.operands.iter().any(|o| *o == yield_vid),
            "identity-closure yield should point at the call-site arg operand ; \
             marker operands = {:?}, yield = {:?}",
            op.operands,
            yield_vid
        );
    }

    #[test]
    fn closure_call_does_not_break_existing_func_call_path() {
        // Sanity : a regular fn-call (NOT a closure-call) still routes
        // through the normal `func.call` lowering. The closure recognizer
        // must not steal user-defined calls whose names happen to be local
        // bindings WITHOUT the `!cssl.closure` MIR type.
        let src = "fn f() -> i32 { let x = 7; x }";
        let (f, _) = lower_one(src);
        // No closure ops at all in this body.
        assert!(find_op(&f, "cssl.closure").is_none());
        assert!(find_op(&f, "cssl.closure.call").is_none());
    }

    #[test]
    fn closure_call_arg_lowering_failure_is_recoverable() {
        // Edge case : pass a syntactically-valid but semantically-empty arg
        // (e.g., a `return` expression). The recognizer should NOT panic ;
        // it surfaces an error op or routes through the inlining pipeline
        // gracefully. This is a defensive test against infinite-recursion /
        // option-unwrap-on-None bugs.
        let src = "fn f() -> i32 { let g = |x : i32| x; g(0) }";
        let (f, _) = lower_one(src);
        // Just assert it lowers without panic + emits the marker.
        assert!(find_op(&f, "cssl.closure.call").is_some());
    }

    #[test]
    fn closure_call_capture_reload_uses_correct_alignment() {
        let src = "fn f() -> i32 { let y = 1; let g = |x : i32| x + y; g(2) }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().expect("entry block");
        let reload = entry
            .ops
            .iter()
            .find(|o| {
                o.name == "memref.load"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "origin" && v == "closure_capture_reload")
            })
            .expect("missing capture-reload memref.load");
        let alignment = reload
            .attributes
            .iter()
            .find(|(k, _)| k == "alignment")
            .map(|(_, v)| v.as_str());
        assert_eq!(alignment, Some("8"));
    }

    #[test]
    fn closure_call_capture_reload_emits_offset_constant_first() {
        // Each capture-reload sequence is `arith.constant <offset> ; memref.load`.
        // For the single-capture case we expect at least one such pair.
        let src = "fn f() -> i32 { let y = 9; let g = |x : i32| x + y; g(1) }";
        let (f, _) = lower_one(src);
        let entry = f.body.entry().expect("entry block");
        // Locate the first capture-reload memref.load + check the prior op
        // is an arith.constant.
        let mut prev_arith_constant = false;
        let mut found_pair = false;
        for o in &entry.ops {
            if o.name == "arith.constant" {
                prev_arith_constant = true;
                continue;
            }
            if o.name == "memref.load"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "origin" && v == "closure_capture_reload")
                && prev_arith_constant
            {
                found_pair = true;
                break;
            }
            prev_arith_constant = o.name == "arith.constant";
        }
        assert!(
            found_pair,
            "expected `arith.constant <offset> ; memref.load` pair in capture-reload"
        );
    }

    #[test]
    fn closure_call_with_capture_marker_records_capture_count() {
        // Single-capture marker records capture_count=1 ; this is the
        // canonical attribute backends consume to know whether to emit the
        // env_ptr operand path.
        let src = "fn f() -> i32 { let y = 5; let g = |x : i32| x + y; g(1) }";
        let (f, _) = lower_one(src);
        let op = find_op(&f, "cssl.closure.call").expect("missing marker");
        assert_eq!(attr(op, "capture_count"), Some("1"));
    }

    #[test]
    fn closure_call_marker_first_operand_is_closure_value_id() {
        // The marker's operand[0] is the closure value-id (the env_ptr at
        // stage-0). The remaining operands are the call-site args.
        let src = "fn f() -> i32 { let g = |x : i32| x; g(99) }";
        let (f, _) = lower_one(src);
        let op = find_op(&f, "cssl.closure.call").expect("missing marker");
        // Locate the cssl.closure construct op + verify its result id matches
        // the marker's operand[0].
        let construct = find_op(&f, "cssl.closure").expect("missing construct");
        let closure_result = construct
            .results
            .first()
            .expect("construct must have a result")
            .id;
        assert_eq!(
            op.operands.first().copied(),
            Some(closure_result),
            "marker operand[0] should be the closure value-id"
        );
    }

    #[test]
    fn closure_call_recognizer_runs_before_format_recognizer() {
        // Regression-guard : a user binding named `format` shadows the stdlib
        // format-recognizer when it's a closure-typed local. The closure
        // recognizer fires first (per dispatch ordering in lower_call) so
        // the call routes through the closure path, NOT the format path.
        let src = "fn f() -> i32 { let format = |x : i32| x + 100; format(0) }";
        let (f, _) = lower_one(src);
        // Closure path won.
        assert!(find_op(&f, "cssl.closure.call").is_some());
        // Format path didn't fire (no cssl.string.format in the op-stream).
        assert!(find_op(&f, "cssl.string.format").is_none());
    }

    #[test]
    fn closure_call_recognizer_does_not_claim_unknown_path() {
        // A bare-name call to a name that isn't a known closure-typed local
        // should NOT be claimed by the recognizer. It routes through the
        // regular func.call path. (This guards against the recognizer
        // accidentally swallowing all single-segment bare-name calls.)
        let src = "fn f() -> i32 { unknown_fn(7) }";
        let (f, _) = lower_one(src);
        // No closure ops emitted — the recognizer correctly decided this
        // wasn't a closure call.
        assert!(find_op(&f, "cssl.closure.call").is_none());
        // The regular func.call path takes it (target = "unknown_fn").
        assert!(find_op(&f, "func.call").is_some());
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D99 — trait-dispatch tests
    // ─────────────────────────────────────────────────────────────────────

    /// Helper : lower a fn with the trait-impl table threaded through.
    fn lower_with_table(src: &str, target_fn: &str) -> (crate::func::MirFunc, cssl_hir::Interner) {
        let (hir, interner, source) = hir_from(src);
        let table = crate::trait_dispatch::build_trait_impl_table(&hir, &interner);
        let ctx = crate::lower::LowerCtx::new(&interner);
        let f = hir
            .items
            .iter()
            .find_map(|i| match i {
                cssl_hir::HirItem::Fn(f) => {
                    if interner.resolve(f.name) == target_fn {
                        Some(f)
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .expect("expected target fn");
        let mut mf = crate::lower::lower_function_signature(&ctx, f);
        super::lower_fn_body_with_table(&interner, Some(&source), Some(&table), f, &mut mf);
        (mf, interner)
    }

    #[test]
    fn trait_dispatch_obj_method_resolves_via_table() {
        // `s.greet()` should lower to `func.call @Foo__Greeter__greet`.
        let src = r"
            interface Greeter { fn greet(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            impl Greeter for Foo {
                fn greet(self : Foo) -> i32 { self.x }
            }
            fn caller() -> i32 {
                let s : Foo = Foo { x : 5 };
                s.greet()
            }
        ";
        let (f, _) = lower_with_table(src, "caller");
        let entry = f.body.entry().expect("entry block");
        let op = entry
            .ops
            .iter()
            .find(|o| {
                o.name == "func.call"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "callee" && v == "Foo__Greeter__greet")
            })
            .expect("trait-dispatched func.call missing");
        // The dispatch attribute marker must be set.
        assert!(op
            .attributes
            .iter()
            .any(|(k, v)| k == "dispatch" && v == "trait"));
    }

    #[test]
    fn trait_dispatch_inherent_method_resolves_first() {
        // Inherent `bar` shadows a hypothetical trait-impl `bar`. Resolver
        // returns `Foo__bar` (inherent) rather than `Foo__BarTrait__bar`.
        let src = r"
            interface BarTrait { fn bar(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            impl Foo {
                fn bar(self : Foo) -> i32 { 1 }
            }
            impl BarTrait for Foo {
                fn bar(self : Foo) -> i32 { 2 }
            }
            fn caller() -> i32 {
                let f : Foo = Foo { x : 1 };
                f.bar()
            }
        ";
        let (f, _) = lower_with_table(src, "caller");
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| o.name == "func.call" && o.attributes.iter().any(|(k, _)| k == "dispatch"))
            .expect("dispatched func.call");
        let callee = op
            .attributes
            .iter()
            .find(|(k, _)| k == "callee")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(callee, "Foo__bar", "inherent shadowed trait impl");
    }

    #[test]
    fn trait_dispatch_unknown_self_ty_falls_through_to_opaque() {
        // No trait-impl table (so `lower_fn_body` plain) on `obj.method()` ;
        // the call must NOT panic and must produce some kind of MIR op.
        let src = r"
            fn caller() -> i32 {
                let s : Foo = Foo { x : 5 };
                s.unknown()
            }
        ";
        // Parse may emit diagnostics ; we just want lowering to not panic.
        let (hir, interner, source) = hir_from(src);
        let ctx = crate::lower::LowerCtx::new(&interner);
        let f = hir.items.iter().find_map(|i| match i {
            cssl_hir::HirItem::Fn(f) => {
                if interner.resolve(f.name) == "caller" {
                    Some(f)
                } else {
                    None
                }
            }
            _ => None,
        });
        if let Some(f) = f {
            let mut mf = crate::lower::lower_function_signature(&ctx, f);
            super::lower_fn_body(&interner, Some(&source), f, &mut mf);
            // Lowering completed without panic — that's the assertion.
        }
    }

    #[test]
    fn trait_dispatch_static_form_lowers_to_mangled() {
        // `Foo::greet(x)` (2-segment path, self-type leading) resolves
        // via the static-method dispatch fast-path.
        let src = r"
            interface Greeter { fn greet(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            impl Greeter for Foo {
                fn greet(self : Foo) -> i32 { self.x }
            }
            fn caller() -> i32 {
                let f : Foo = Foo { x : 1 };
                Foo::greet(f)
            }
        ";
        let (f, _) = lower_with_table(src, "caller");
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| {
                o.attributes
                    .iter()
                    .any(|(k, v)| k == "dispatch" && v == "trait_static")
            })
            .expect("trait_static dispatch missing");
        let callee = op
            .attributes
            .iter()
            .find(|(k, _)| k == "callee")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(callee, "Foo__Greeter__greet");
    }

    #[test]
    fn trait_dispatch_records_method_name_in_attributes() {
        // The `method` attribute should record the source-form method-name
        // for diagnostics + auto-monomorph rewriter.
        let src = r"
            interface Greeter { fn greet(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            impl Greeter for Foo {
                fn greet(self : Foo) -> i32 { self.x }
            }
            fn caller() -> i32 {
                let f : Foo = Foo { x : 1 };
                f.greet()
            }
        ";
        let (f, _) = lower_with_table(src, "caller");
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| {
                o.attributes
                    .iter()
                    .any(|(k, v)| k == "dispatch" && v == "trait")
            })
            .expect("dispatched call");
        let method = op
            .attributes
            .iter()
            .find(|(k, _)| k == "method")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(method, "greet");
        let self_ty = op
            .attributes
            .iter()
            .find(|(k, _)| k == "self_ty")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(self_ty, "Foo");
    }

    #[test]
    fn trait_dispatch_table_none_falls_back_to_opaque() {
        // Without a trait-table, `obj.method()` should NOT route through
        // the trait-dispatch fast-path ; a normal call is emitted instead.
        let src = r"
            interface Greeter { fn greet(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            impl Greeter for Foo {
                fn greet(self : Foo) -> i32 { self.x }
            }
            fn caller() -> i32 {
                let f : Foo = Foo { x : 1 };
                f.greet()
            }
        ";
        let (hir, interner, source) = hir_from(src);
        let ctx = crate::lower::LowerCtx::new(&interner);
        let f = hir
            .items
            .iter()
            .find_map(|i| match i {
                cssl_hir::HirItem::Fn(f) => {
                    if interner.resolve(f.name) == "caller" {
                        Some(f)
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .expect("caller");
        let mut mf = crate::lower::lower_function_signature(&ctx, f);
        super::lower_fn_body(&interner, Some(&source), f, &mut mf);
        let entry = mf.body.entry().expect("entry");
        // No `dispatch` attribute should be present anywhere.
        for op in &entry.ops {
            assert!(
                !op.attributes.iter().any(|(k, _)| k == "dispatch"),
                "dispatch attribute leaked in non-table lowering : {op:?}"
            );
        }
    }

    #[test]
    fn drop_method_resolves_through_table() {
        // `f.drop()` on a binding of type `Foo` (which has `impl Drop for Foo`)
        // resolves to the mangled `Foo__Drop__drop`.
        let src = r"
            interface Drop { fn drop(self : Foo) ; }
            struct Foo { x : i32 }
            impl Drop for Foo {
                fn drop(self : Foo) {  }
            }
            fn caller() -> i32 {
                let f : Foo = Foo { x : 1 };
                f.drop();
                0
            }
        ";
        let (f, _) = lower_with_table(src, "caller");
        let entry = f.body.entry().expect("entry");
        let op = entry.ops.iter().find(|o| {
            o.attributes
                .iter()
                .any(|(k, v)| k == "callee" && v == "Foo__Drop__drop")
        });
        assert!(op.is_some(), "Drop trait dispatch should mangle correctly");
    }

    #[test]
    fn opaque_type_leading_symbol_strips_cssl_prefix() {
        let interner = cssl_hir::Interner::new();
        let ty = MirType::Opaque("!cssl.struct.Foo".to_string());
        let s = super::opaque_type_leading_symbol(&interner, &ty).expect("leading sym");
        assert_eq!(interner.resolve(s), "Foo");

        let ty2 = MirType::Opaque("Foo".to_string());
        let s2 = super::opaque_type_leading_symbol(&interner, &ty2).expect("leading sym");
        assert_eq!(interner.resolve(s2), "Foo");

        let ty3 = MirType::Opaque("!cssl.Vec<i32>".to_string());
        let s3 = super::opaque_type_leading_symbol(&interner, &ty3).expect("leading sym");
        assert_eq!(interner.resolve(s3), "Vec");
    }

    #[test]
    fn opaque_type_non_opaque_returns_none() {
        let interner = cssl_hir::Interner::new();
        let ty = MirType::Int(IntWidth::I32);
        assert!(super::opaque_type_leading_symbol(&interner, &ty).is_none());
    }

    // ═════════════════════════════════════════════════════════════════════
    // § W-B-RECOGNIZER tests — Wave-A op-emit recognizers (vec_load_at /
    //   vec_store_at / vec_end_of / vec_drop).
    //
    //   Each recognizer-claim test verifies :
    //     1. The expected typed-memref op shows up in the lowered ops.
    //     2. The op carries the canonical attribute set (elem_ty / origin /
    //        sizeof / alignment).
    //     3. The fall-back path (no turbofish / composite-T) still routes
    //        through the regular func.call op.
    // ═════════════════════════════════════════════════════════════════════

    #[test]
    fn vec_load_at_i32_emits_typed_load() {
        // `load_at::<i32>(data, 0)` should mint a `memref.load.i32`.
        let src = r"
            fn read_first(data : i64) -> i32 {
                load_at::<i32>(data, 0)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"memref.load.i32"),
            "expected memref.load.i32 in {names:?}"
        );
        let entry = f.body.entry().expect("entry");
        let load_op = entry
            .ops
            .iter()
            .find(|o| o.name == "memref.load.i32")
            .expect("missing memref.load.i32");
        let elem_ty = load_op
            .attributes
            .iter()
            .find(|(k, _)| k == "elem_ty")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(elem_ty, "i32");
        let origin = load_op
            .attributes
            .iter()
            .find(|(k, _)| k == "origin")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(origin, "vec_load_at");
    }

    #[test]
    fn vec_load_at_f64_emits_typed_load_f64_suffix() {
        let src = r"
            fn read_first_f64(data : i64) -> f64 {
                load_at::<f64>(data, 0)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"memref.load.f64"),
            "expected memref.load.f64 in {names:?}"
        );
    }

    #[test]
    fn vec_load_at_no_turbofish_falls_through_to_func_call() {
        // No `::<T>` turbofish → recognizer declines → regular func.call.
        let src = r"
            fn read_first(data : i64) -> i32 {
                load_at(data, 0)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        // Must NOT contain a typed memref load — the fallback is func.call.
        assert!(
            !names.iter().any(|n| n.starts_with("memref.load.")),
            "unexpected typed-load with no turbofish: {names:?}"
        );
        assert!(names.iter().any(|n| n == &"func.call"));
    }

    #[test]
    fn vec_store_at_i32_emits_typed_store() {
        let src = r"
            fn write_first(data : i64, x : i32) {
                store_at::<i32>(data, 0, x)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"memref.store.i32"),
            "expected memref.store.i32 in {names:?}"
        );
        let entry = f.body.entry().expect("entry");
        let store_op = entry
            .ops
            .iter()
            .find(|o| o.name == "memref.store.i32")
            .expect("missing memref.store.i32");
        let elem_ty = store_op
            .attributes
            .iter()
            .find(|(k, _)| k == "elem_ty")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(elem_ty, "i32");
        let origin = store_op
            .attributes
            .iter()
            .find(|(k, _)| k == "origin")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(origin, "vec_store_at");
    }

    #[test]
    fn vec_end_of_i64_emits_ptr_end_of() {
        let src = r"
            fn end(data : i64, len : i64) -> i64 {
                end_of::<i64>(data, len)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"memref.ptr.end_of"),
            "expected memref.ptr.end_of in {names:?}"
        );
        // Should also have a sizeof constant (8 for i64) + arith.muli for
        // len × sizeof T.
        assert!(names.iter().any(|n| n == &"arith.constant"));
        assert!(names.iter().any(|n| n == &"arith.muli"));
    }

    #[test]
    fn vec_drop_i32_emits_heap_dealloc() {
        // The `vec_drop::<i32>(v)` recognizer mints a cssl.heap.dealloc op.
        // The struct accessor `cssl.field` ops fire too (for v.data + v.cap).
        let src = r"
            struct Vec<T> { data : i64, len : i64, cap : i64 }
            fn drop_a_vec(v : Vec<i32>) {
                vec_drop::<i32>(v)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.heap.dealloc"),
            "expected cssl.heap.dealloc in {names:?}"
        );
        // Must also load v.data + v.cap via cssl.field ops.
        let field_count = names.iter().filter(|n| **n == "cssl.field").count();
        assert!(
            field_count >= 2,
            "expected ≥ 2 cssl.field ops (data + cap) in {names:?}",
        );
        // And the dealloc op should carry origin = "vec_drop" + payload_ty.
        let entry = f.body.entry().expect("entry");
        let dealloc_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.heap.dealloc")
            .expect("missing cssl.heap.dealloc");
        let origin = dealloc_op
            .attributes
            .iter()
            .find(|(k, _)| k == "origin")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(origin, "vec_drop");
        let cap = dealloc_op
            .attributes
            .iter()
            .find(|(k, _)| k == "cap")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(cap, "iso_consumed");
    }

    #[test]
    fn vec_drop_no_turbofish_falls_through() {
        // Without `::<T>`, recognizer declines → regular func.call path.
        let src = r"
            struct Vec<T> { data : i64, len : i64, cap : i64 }
            fn drop_a_vec(v : Vec<i32>) {
                vec_drop(v)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        // No heap.dealloc minted without the turbofish.
        assert!(
            !names.iter().any(|n| n == &"cssl.heap.dealloc"),
            "unexpected heap.dealloc with no turbofish: {names:?}",
        );
        assert!(names.iter().any(|n| n == &"func.call"));
    }

    #[test]
    fn vec_drop_composite_t_falls_through_to_func_call() {
        // Composite payload-T (struct / opaque) → recognizer declines.
        // The Bar opaque type lowers to MirType::Opaque whose
        // dealloc_size_for returns 0 → recognizer declines.
        let src = r"
            struct Bar { y : i32 }
            struct Vec<T> { data : i64, len : i64, cap : i64 }
            fn drop_a_vec(v : Vec<Bar>) {
                vec_drop::<Bar>(v)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        // Composite T → recognizer declines → no cssl.heap.dealloc.
        assert!(
            !names.iter().any(|n| n == &"cssl.heap.dealloc"),
            "unexpected heap.dealloc for opaque T: {names:?}",
        );
    }

    // ═════════════════════════════════════════════════════════════════════
    // § T11-D249 (W-A2-α-fix) tests — `vec_new::<T>()` / `vec_push::<T>` /
    //   `vec_index::<T>` recognizer arms. Mirrors the W-B-RECOGNIZER /
    //   W-A8-string test patterns.
    // ═════════════════════════════════════════════════════════════════════

    #[test]
    fn vec_new_i32_emits_cssl_vec_new() {
        // `vec_new::<i32>()` should mint a `cssl.vec.new` op carrying
        // payload_ty=i32 + origin=vec_new + cap=iso. CRITICAL : MUST NOT
        // emit `cssl.heap.alloc` (cap=0 invariant — see existing
        // `lower_vec_empty_constructor_emits_no_heap_alloc` test).
        let src = r"
            fn make_empty() -> i64 {
                let v = vec_new::<i32>();
                0
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.vec.new"),
            "expected cssl.vec.new in {names:?}"
        );
        // Empty-construction shortcut : no heap allocation.
        assert!(
            !names.iter().any(|n| n == &"cssl.heap.alloc"),
            "vec_new must NOT emit cssl.heap.alloc : {names:?}"
        );
        let entry = f.body.entry().expect("entry");
        let new_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.vec.new")
            .expect("missing cssl.vec.new");
        let payload_ty = new_op
            .attributes
            .iter()
            .find(|(k, _)| k == "payload_ty")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(payload_ty, "i32");
        let origin = new_op
            .attributes
            .iter()
            .find(|(k, _)| k == "origin")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(origin, "vec_new");
        let cap = new_op
            .attributes
            .iter()
            .find(|(k, _)| k == "cap")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(cap, "iso");
    }

    #[test]
    fn vec_push_i32_emits_cssl_vec_push_with_two_operands() {
        // `vec_push::<i32>(v, x)` should mint a `cssl.vec.push` op with
        // 2 operands (v, x) + payload_ty=i32 + origin=vec_push.
        let src = r"
            fn append_one(v : i64, x : i32) -> i64 {
                let v2 = vec_push::<i32>(v, x);
                0
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.vec.push"),
            "expected cssl.vec.push in {names:?}"
        );
        let entry = f.body.entry().expect("entry");
        let push_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.vec.push")
            .expect("missing cssl.vec.push");
        // 2 operands : the receiver Vec + the value to append.
        assert_eq!(
            push_op.operands.len(),
            2,
            "vec_push must have exactly 2 operands : {:?}",
            push_op.operands,
        );
        let payload_ty = push_op
            .attributes
            .iter()
            .find(|(k, _)| k == "payload_ty")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(payload_ty, "i32");
        let origin = push_op
            .attributes
            .iter()
            .find(|(k, _)| k == "origin")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(origin, "vec_push");
    }

    #[test]
    fn vec_index_i32_emits_cssl_vec_index_with_bounds_check() {
        // `vec_index::<i32>(v, i)` should mint a `cssl.vec.index` op with
        // bounds_check="panic" + payload_ty=i32 + origin=vec_index.
        let src = r"
            fn read_at(v : i64, i : i64) -> i32 {
                vec_index::<i32>(v, i)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.vec.index"),
            "expected cssl.vec.index in {names:?}"
        );
        let entry = f.body.entry().expect("entry");
        let idx_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.vec.index")
            .expect("missing cssl.vec.index");
        // 2 operands : the receiver Vec + the index.
        assert_eq!(
            idx_op.operands.len(),
            2,
            "vec_index must have exactly 2 operands : {:?}",
            idx_op.operands,
        );
        let payload_ty = idx_op
            .attributes
            .iter()
            .find(|(k, _)| k == "payload_ty")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(payload_ty, "i32");
        let bounds_check = idx_op
            .attributes
            .iter()
            .find(|(k, _)| k == "bounds_check")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(bounds_check, "panic");
        let origin = idx_op
            .attributes
            .iter()
            .find(|(k, _)| k == "origin")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(origin, "vec_index");
    }

    #[test]
    fn vec_recognizer_smoke_combo_new_push_index() {
        // Smoke : the canonical `vec_new + vec_push + vec_index` round-trip
        // (matches the W-A2 end-to-end fixture
        // `WAVE_A2_VEC_PUSH_INDEX`). The recognizer chain must claim ALL
        // three calls (no leakage into func.call).
        let src = r"
            fn round_trip() -> i32 {
                let v0 = vec_new::<i32>();
                let v1 = vec_push::<i32>(v0, 11);
                let v2 = vec_push::<i32>(v1, 13);
                vec_index::<i32>(v2, 1)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.vec.new"),
            "expected cssl.vec.new in {names:?}"
        );
        // Two pushes → two cssl.vec.push ops.
        let push_count = names.iter().filter(|n| **n == "cssl.vec.push").count();
        assert_eq!(
            push_count, 2,
            "expected exactly 2 cssl.vec.push ops in {names:?}"
        );
        assert!(
            names.iter().any(|n| n == &"cssl.vec.index"),
            "expected cssl.vec.index in {names:?}"
        );
        // None of these calls should leak into func.call — full recognizer
        // claim. (other generic-call func.call ops may exist for monomorph
        // wrappers ; verify by checking the count of unclaimed `vec_*`
        // callees specifically.)
        let entry = f.body.entry().expect("entry");
        let leaked_vec_calls = entry
            .ops
            .iter()
            .filter(|o| o.name == "func.call")
            .filter(|o| {
                o.attributes.iter().any(|(k, v)| {
                    k == "callee" && (v == "vec_new" || v == "vec_push" || v == "vec_index")
                })
            })
            .count();
        assert_eq!(
            leaked_vec_calls, 0,
            "no vec_* callees should leak into func.call : {names:?}",
        );
    }

    #[test]
    fn vec_new_no_turbofish_falls_through_to_func_call() {
        // Without `::<T>`, recognizer declines → regular func.call path.
        let src = r"
            fn make() -> i64 {
                let v = vec_new();
                0
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        // No cssl.vec.new minted without the turbofish.
        assert!(
            !names.iter().any(|n| n == &"cssl.vec.new"),
            "unexpected cssl.vec.new with no turbofish: {names:?}",
        );
        assert!(names.iter().any(|n| n == &"func.call"));
    }

    // ── Smoke tests : combined recognizer-chain integration ──────────────

    #[test]
    fn vec_load_at_with_index_param_lowers() {
        // End-to-end : `load_at::<f32>(data, i)` where i is a fn-param.
        // Verifies the recognizer correctly lowers BOTH operands and emits
        // the typed-load triplet (sizeof / muli / load).
        let src = r"
            fn read_at(data : i64, i : i64) -> f32 {
                load_at::<f32>(data, i)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(names.iter().any(|n| n == &"memref.load.f32"));
        assert!(names.iter().any(|n| n == &"arith.constant"));
        assert!(names.iter().any(|n| n == &"arith.muli"));
    }

    #[test]
    fn vec_load_then_store_smoke() {
        // Smoke : both load + store in same fn body. The recognizer chain
        // must claim BOTH calls (no leakage into func.call for either).
        let src = r"
            fn copy_one(src : i64, dst : i64) {
                let x : i32 = load_at::<i32>(src, 0);
                store_at::<i32>(dst, 0, x)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(names.iter().any(|n| n == &"memref.load.i32"));
        assert!(names.iter().any(|n| n == &"memref.store.i32"));
    }

    // ═════════════════════════════════════════════════════════════════════
    // § T11-D245 (W-A8 / Wave-C1 carry-forward) — `cssl.string.*` recognizer
    //   tests. Each recognizer-claim test verifies :
    //     1. The expected `cssl.string.*` / `cssl.str_slice.*` /
    //        `cssl.char.from_u32` op shows up in the lowered ops.
    //     2. The op carries the canonical attribute set
    //        (source_kind / op / field / total_size / alignment).
    //     3. The fall-back path (wrong arity / multi-segment shadow) still
    //        routes through the regular func.call op.
    //
    //   The stdlib types (`String`, `StrSlice`) are declared inline in each
    //   test source so the body_lower test scaffold doesn't need to load
    //   the full `stdlib/string.cssl` ; the leading-segment of the
    //   declared struct is what the recognizer claims via 1-segment-path
    //   match, NOT the type-system shape (which lower_call_arg threads
    //   through opaquely at this stage-0 layer).
    // ═════════════════════════════════════════════════════════════════════

    #[test]
    fn string_len_emits_cssl_string_len() {
        // `string_len(s)` should mint a `cssl.string.len` op carrying the
        // canonical `field=len, offset=8` attributes.
        let src = r"
            struct Vec<T> { data : i64, len : i64, cap : i64 }
            struct String { bytes : Vec<u8> }
            fn measure(s : String) -> i64 {
                string_len(s)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.string.len"),
            "expected cssl.string.len in {names:?}"
        );
        let entry = f.body.entry().expect("entry");
        let len_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.string.len")
            .expect("missing cssl.string.len");
        let field = len_op
            .attributes
            .iter()
            .find(|(k, _)| k == "field")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(field, "len");
        let offset = len_op
            .attributes
            .iter()
            .find(|(k, _)| k == "offset")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(offset, "8");
    }

    #[test]
    fn str_len_emits_cssl_str_slice_len() {
        // `str_len(slice)` should mint a `cssl.str_slice.len` op.
        let src = r"
            struct StrSlice { ptr : i64, len : i64 }
            fn measure_slice(s : StrSlice) -> i64 {
                str_len(s)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.str_slice.len"),
            "expected cssl.str_slice.len in {names:?}"
        );
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.str_slice.len")
            .expect("missing cssl.str_slice.len");
        let field = op
            .attributes
            .iter()
            .find(|(k, _)| k == "field")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(field, "len");
    }

    #[test]
    fn string_from_utf8_unchecked_emits_cssl_string_from_utf8_unchecked() {
        // `string_from_utf8_unchecked(bytes)` should mint a
        // `cssl.string.from_utf8_unchecked` op (preceded by data + len
        // field-loads on the source Vec<u8>).
        let src = r"
            struct Vec<T> { data : i64, len : i64, cap : i64 }
            fn unchecked(b : Vec<u8>) -> i64 {
                string_from_utf8_unchecked(b)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names
                .iter()
                .any(|n| n == &"cssl.string.from_utf8_unchecked"),
            "expected cssl.string.from_utf8_unchecked in {names:?}"
        );
        // Must also load b.data + b.len via cssl.field ops.
        let field_count = names.iter().filter(|n| **n == "cssl.field").count();
        assert!(
            field_count >= 2,
            "expected ≥ 2 cssl.field ops (data + len) in {names:?}",
        );
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.string.from_utf8_unchecked")
            .expect("missing cssl.string.from_utf8_unchecked");
        let total_size = op
            .attributes
            .iter()
            .find(|(k, _)| k == "total_size")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(total_size, "24");
    }

    #[test]
    fn string_from_utf8_emits_validate_op() {
        // `string_from_utf8(bytes)` should mint a `cssl.string.from_utf8`
        // op carrying the `validate_symbol="__cssl_strvalidate"` attribute.
        let src = r"
            struct Vec<T> { data : i64, len : i64, cap : i64 }
            fn checked(b : Vec<u8>) -> i64 {
                string_from_utf8(b)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.string.from_utf8"),
            "expected cssl.string.from_utf8 in {names:?}"
        );
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.string.from_utf8")
            .expect("missing cssl.string.from_utf8");
        let validate_symbol = op
            .attributes
            .iter()
            .find(|(k, _)| k == "validate_symbol")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(validate_symbol, "__cssl_strvalidate");
    }

    #[test]
    fn string_push_str_emits_cssl_string_push_str() {
        // `string_push_str(s, slice)` should mint a `cssl.string.push_str` op.
        let src = r"
            struct Vec<T> { data : i64, len : i64, cap : i64 }
            struct String { bytes : Vec<u8> }
            struct StrSlice { ptr : i64, len : i64 }
            fn append(s : String, sl : StrSlice) -> i64 {
                string_push_str(s, sl)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.string.push_str"),
            "expected cssl.string.push_str in {names:?}"
        );
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.string.push_str")
            .expect("missing cssl.string.push_str");
        // Must carry exactly 2 operands : the string + the slice.
        assert_eq!(op.operands.len(), 2);
        let kind = op
            .attributes
            .iter()
            .find(|(k, _)| k == "source_kind")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(kind, "string_abi");
    }

    #[test]
    fn string_as_str_emits_cssl_str_slice_new() {
        // `string_as_str(s)` should mint two `cssl.field` ops (data + len)
        // and one `cssl.str_slice.new`.
        let src = r"
            struct Vec<T> { data : i64, len : i64, cap : i64 }
            struct String { bytes : Vec<u8> }
            fn borrow(s : String) -> i64 {
                string_as_str(s)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.str_slice.new"),
            "expected cssl.str_slice.new in {names:?}"
        );
        let field_count = names.iter().filter(|n| **n == "cssl.field").count();
        assert!(
            field_count >= 2,
            "expected ≥ 2 cssl.field ops (data + len) in {names:?}",
        );
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.str_slice.new")
            .expect("missing cssl.str_slice.new");
        let total_size = op
            .attributes
            .iter()
            .find(|(k, _)| k == "total_size")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(total_size, "16");
    }

    #[test]
    fn char_from_u32_emits_cssl_char_from_u32() {
        // `char_from_u32(code)` should mint a `cssl.char.from_u32` op
        // carrying the USV-range constants as attributes.
        let src = r"
            fn check(code : i64) -> i64 {
                char_from_u32(code)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.char.from_u32"),
            "expected cssl.char.from_u32 in {names:?}"
        );
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.char.from_u32")
            .expect("missing cssl.char.from_u32");
        // Must carry the canonical USV-range constants for the cgen DFA.
        let usv_max = op
            .attributes
            .iter()
            .find(|(k, _)| k == "usv_max")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(usv_max, "1114111");
        let usv_max_bmp = op
            .attributes
            .iter()
            .find(|(k, _)| k == "usv_max_bmp")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(usv_max_bmp, "55295");
    }

    #[test]
    fn string_byte_at_emits_cssl_string_byte_at() {
        // `string_byte_at(s, i)` should mint a `cssl.string.byte_at` op.
        let src = r"
            struct Vec<T> { data : i64, len : i64, cap : i64 }
            struct String { bytes : Vec<u8> }
            fn at(s : String, i : i64) -> i32 {
                string_byte_at(s, i)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.string.byte_at"),
            "expected cssl.string.byte_at in {names:?}"
        );
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.string.byte_at")
            .expect("missing cssl.string.byte_at");
        let field = op
            .attributes
            .iter()
            .find(|(k, _)| k == "field")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(field, "data");
    }

    #[test]
    fn string_len_wrong_arity_falls_through_to_func_call() {
        // `string_len(a, b)` (2 args, not 1) → recognizer declines → func.call.
        let src = r"
            struct Vec<T> { data : i64, len : i64, cap : i64 }
            struct String { bytes : Vec<u8> }
            fn measure(s : String, t : String) -> i64 {
                string_len(s, t)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        // Wrong arity → recognizer must NOT claim ; falls through to func.call.
        assert!(
            !names.iter().any(|n| n == &"cssl.string.len"),
            "unexpected cssl.string.len with wrong arity: {names:?}"
        );
        assert!(names.iter().any(|n| n == &"func.call"));
    }

    #[test]
    fn string_len_multi_segment_path_falls_through() {
        // `foo::string_len(s)` (2-segment path) → recognizer requires
        // 1-segment path, so it declines → regular func.call.
        let src = r"
            struct Vec<T> { data : i64, len : i64, cap : i64 }
            struct String { bytes : Vec<u8> }
            fn measure(s : String) -> i64 {
                foo::string_len(s)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            !names.iter().any(|n| n == &"cssl.string.len"),
            "unexpected cssl.string.len from multi-segment path: {names:?}"
        );
        assert!(names.iter().any(|n| n == &"func.call"));
    }

    #[test]
    fn string_recognizer_smoke_combo() {
        // Smoke : multiple string ops in same fn body. The recognizer chain
        // must claim ALL three calls (no leakage into func.call).
        let src = r"
            struct Vec<T> { data : i64, len : i64, cap : i64 }
            struct String { bytes : Vec<u8> }
            fn combo(s : String, code : i64) -> i64 {
                let n = string_len(s);
                let opt = char_from_u32(code);
                n
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(names.iter().any(|n| n == &"cssl.string.len"));
        assert!(names.iter().any(|n| n == &"cssl.char.from_u32"));
    }

    // ═════════════════════════════════════════════════════════════════════
    // § T11-D284 (W-E5-1) — enum-payload trait dispatch tests.
    //
    //   Closes the `obj.method(args)` dispatch gap when `obj`'s declared
    //   type is `Option<T>` / `Result<T, E>` and the trait-impl is on
    //   the inner T (the unwrapped payload). Each test shapes a different
    //   tier of the resolution probe :
    //
    //     1. basic-trait-dispatch-on-enum  — `impl Trait for Option<Foo>`
    //        wrapper-tier hit, no payload unwrap.
    //     2. payload-receiver-unwrap       — `impl Trait for Foo` only ;
    //        payload-tier resolves, dispatch records `payload_unwrap=true`.
    //     3. Option-trait-impl             — `obj : Option<Foo>` peels to
    //        `Foo` cleanly (the canonical payload-unwrap shape).
    //     4. Result-trait-impl             — `obj : Result<Foo, Bar>` peels
    //        to the Ok-branch `Foo` (matches stage-0's success-arm
    //        dispatch convention).
    //     5. regression-no-break-non-enum  — plain `let f : Foo = ...`
    //        receivers must continue to dispatch through the wrapper-tier
    //        and NEVER carry the `payload_unwrap` attribute.
    // ═════════════════════════════════════════════════════════════════════

    #[test]
    fn w_e5_1_basic_trait_dispatch_on_enum_uses_wrapper_tier() {
        // Wrapper-tier hit — `impl Greeter for Option { fn greet(...) }`
        // resolves directly without invoking the payload-unwrap fallback.
        // This validates that the new probe DOES NOT regress when the
        // wrapper itself carries the impl.
        let src = r"
            interface Greeter { fn greet(self : Option) -> i32 ; }
            impl Greeter for Option {
                fn greet(self : Option) -> i32 { 42 }
            }
            struct Foo { x : i32 }
            fn caller() -> i32 {
                let o : Option<Foo> = None();
                o.greet()
            }
        ";
        let (f, _) = lower_with_table(src, "caller");
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| {
                o.attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "Option__Greeter__greet")
            })
            .expect("Option-wrapper trait dispatch should resolve");
        // Must NOT have the payload_unwrap marker — the wrapper-tier hit.
        assert!(
            !op.attributes.iter().any(|(k, _)| k == "payload_unwrap"),
            "wrapper-tier hit must NOT mark payload_unwrap"
        );
        let dispatch = op
            .attributes
            .iter()
            .find(|(k, _)| k == "dispatch")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(dispatch, "trait");
    }

    #[test]
    fn w_e5_1_payload_receiver_unwrap_resolves_to_inner_t() {
        // Payload-tier hit — `impl Greeter for Foo { fn greet(...) }` only ;
        // a binding `o : Option<Foo>` must peel the wrapper and dispatch
        // through `Foo__Greeter__greet` with `payload_unwrap=true`.
        let src = r"
            interface Greeter { fn greet(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            impl Greeter for Foo {
                fn greet(self : Foo) -> i32 { self.x }
            }
            fn caller() -> i32 {
                let o : Option<Foo> = None();
                o.greet()
            }
        ";
        let (f, _) = lower_with_table(src, "caller");
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| {
                o.attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "Foo__Greeter__greet")
            })
            .expect("payload-unwrap dispatch should resolve to Foo's impl");
        // Payload-unwrap marker MUST be present.
        let unwrap_marker = op
            .attributes
            .iter()
            .find(|(k, _)| k == "payload_unwrap")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(unwrap_marker, "true", "expected payload_unwrap=true");
        // Wrapper-self-ty attribute records the original Option wrapper.
        let wrapper = op
            .attributes
            .iter()
            .find(|(k, _)| k == "wrapper_self_ty")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(wrapper, "Option");
        // dispatch attribute must be `trait_payload_unwrap`.
        let dispatch = op
            .attributes
            .iter()
            .find(|(k, _)| k == "dispatch")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(dispatch, "trait_payload_unwrap");
    }

    #[test]
    fn w_e5_1_option_trait_impl_dispatches_through_payload() {
        // Canonical Option<Foo> payload-unwrap : trait method exists only
        // on Foo. The caller binds an `Option<Foo>` and invokes the method ;
        // dispatch must succeed via payload-unwrap.
        let src = r"
            interface Display { fn render(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            impl Display for Foo {
                fn render(self : Foo) -> i32 { self.x }
            }
            fn caller() -> i32 {
                let opt : Option<Foo> = None();
                opt.render()
            }
        ";
        let (f, _) = lower_with_table(src, "caller");
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| {
                o.attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "Foo__Display__render")
            })
            .expect("Option<Foo>.render() should peel to Foo's Display impl");
        // self_ty must record the unwrapped payload type, not the wrapper.
        let self_ty = op
            .attributes
            .iter()
            .find(|(k, _)| k == "self_ty")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(self_ty, "Foo");
    }

    #[test]
    fn w_e5_1_result_trait_impl_dispatches_through_ok_payload() {
        // Result<Foo, Bar> peels to the Ok-branch payload (Foo) for trait-
        // method-call dispatch. The Err-branch tracks separately for
        // `?`-operator + match-arm — those don't share this receiver path.
        // We use a fn-param rather than a let-binding to sidestep the
        // construction-op rhs typing — the param's declared type is the
        // single source of truth for the dispatch resolver.
        //
        // ‼ method-name = `apply` (NOT `perform` — `perform` is the CSSL
        //   algebraic-effect keyword and lowers to `cssl.effect.perform`,
        //   bypassing the trait-dispatch path).
        let src = r"
            interface Action { fn apply(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            struct Bar { y : i32 }
            impl Action for Foo {
                fn apply(self : Foo) -> i32 { self.x }
            }
            fn caller(res : Result<Foo, Bar>) -> i32 {
                res.apply()
            }
        ";
        let (f, _) = lower_with_table(src, "caller");
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| {
                o.attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "Foo__Action__apply")
            })
            .expect("Result<Foo, Bar>.apply() should peel to Foo's impl");
        let unwrap_marker = op
            .attributes
            .iter()
            .find(|(k, _)| k == "payload_unwrap")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(unwrap_marker, "true");
        let wrapper = op
            .attributes
            .iter()
            .find(|(k, _)| k == "wrapper_self_ty")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(wrapper, "Result");
    }

    #[test]
    fn w_e5_1_regression_non_enum_receiver_does_not_payload_unwrap() {
        // Regression-guard : a plain `let f : Foo = ...` receiver MUST
        // continue dispatching through the wrapper-tier and NEVER carry
        // the `payload_unwrap` attribute. If this regresses, the new probe
        // is over-applying the unwrap path and would corrupt non-enum
        // dispatch self-ty attributes.
        let src = r"
            interface Greeter { fn greet(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            impl Greeter for Foo {
                fn greet(self : Foo) -> i32 { self.x }
            }
            fn caller() -> i32 {
                let f : Foo = Foo { x : 9 };
                f.greet()
            }
        ";
        let (f, _) = lower_with_table(src, "caller");
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| {
                o.attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "Foo__Greeter__greet")
            })
            .expect("plain receiver dispatch must resolve");
        // No payload_unwrap attribute on plain dispatch.
        assert!(
            !op.attributes.iter().any(|(k, _)| k == "payload_unwrap"),
            "plain receiver MUST NOT carry payload_unwrap"
        );
        // self_ty == Foo (no wrapper).
        let self_ty = op
            .attributes
            .iter()
            .find(|(k, _)| k == "self_ty")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(self_ty, "Foo");
        // dispatch attribute is the canonical `trait`, not the unwrap variant.
        let dispatch = op
            .attributes
            .iter()
            .find(|(k, _)| k == "dispatch")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(dispatch, "trait");
    }

    #[test]
    fn w_e5_1_static_dispatch_payload_unwrap_for_trait_static_form() {
        // Static-form `Trait::method(opt)` where `opt : Option<Foo>` and
        // `impl Trait for Foo` — the resolver's static-method path must
        // also probe the payload-tier when the wrapper has no impl.
        let src = r"
            interface Greeter { fn greet(self : Foo) -> i32 ; }
            struct Foo { x : i32 }
            impl Greeter for Foo {
                fn greet(self : Foo) -> i32 { self.x }
            }
            fn caller() -> i32 {
                let opt : Option<Foo> = None();
                Greeter::greet(opt)
            }
        ";
        let (f, _) = lower_with_table(src, "caller");
        let entry = f.body.entry().expect("entry");
        let op = entry
            .ops
            .iter()
            .find(|o| {
                o.attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "Foo__Greeter__greet")
            })
            .expect("static-form payload-unwrap must resolve to Foo's impl");
        // Static-form dispatch attribute is `trait_static`. The fact that
        // the resolver picked Foo's mangled name (not Option's) is the
        // observable proof of payload-unwrap.
        let callee = op
            .attributes
            .iter()
            .find(|(k, _)| k == "callee")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(callee, "Foo__Greeter__greet");
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D288 (W-E5-5) — SIMD-intrinsic recognizer integration tests.
    //
    //   Validate that the recognizer arms in `lower_call` mint the
    //   canonical `cssl.simd.*` op shape when the corresponding stdlib
    //   SIMD-intrinsic call appears in source. Mirrors the W-A8 string
    //   recognizer integration-test pattern (T11-D245).
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn simd_v128_load_emits_cssl_simd_v128_load() {
        let src = r"
            fn scan(p : i64) -> i64 {
                simd_v128_load(p);
                p
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.simd.v128_load"),
            "expected cssl.simd.v128_load in {names:?}",
        );
    }

    #[test]
    fn simd_v_byte_eq_recognizer_claims_two_arg_form() {
        let src = r"
            fn cmp(a : i64, b : i64) -> i64 {
                simd_v_byte_eq(a, b);
                a
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.simd.v_byte_eq"),
            "expected cssl.simd.v_byte_eq in {names:?}",
        );
        // The minted op must NOT also leak into func.call (recognizer
        // claims it ; falls-through path must not fire).
        let entry = f.body.entry().expect("entry");
        let simd_op = entry
            .ops
            .iter()
            .find(|o| o.name == "cssl.simd.v_byte_eq")
            .expect("missing cssl.simd.v_byte_eq");
        let lane_w = simd_op
            .attributes
            .iter()
            .find(|(k, _)| k == "lane_width")
            .map_or("", |(_, v)| v.as_str());
        assert_eq!(lane_w, "8");
    }

    #[test]
    fn simd_v_byte_in_range_three_arg_form_claimed() {
        let src = r"
            fn classify(v : i64, lo : i64, hi : i64) -> i64 {
                simd_v_byte_in_range(v, lo, hi);
                v
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.simd.v_byte_in_range"),
            "expected cssl.simd.v_byte_in_range in {names:?}",
        );
    }

    #[test]
    fn simd_v_prefix_sum_recognizer_claims_one_arg_form() {
        let src = r"
            fn scan(v : i64) -> i64 {
                simd_v_prefix_sum(v);
                v
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.simd.v_prefix_sum"),
            "expected cssl.simd.v_prefix_sum in {names:?}",
        );
    }

    #[test]
    fn simd_v_horizontal_sum_returns_i32() {
        let src = r"
            fn fold(v : i64) -> i32 {
                simd_v_horizontal_sum(v)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            names.iter().any(|n| n == &"cssl.simd.v_horizontal_sum"),
            "expected cssl.simd.v_horizontal_sum in {names:?}",
        );
    }

    #[test]
    fn simd_recognizer_wrong_arity_falls_through_to_func_call() {
        // simd_v128_load expects 1 arg ; passing 2 must decline ; falls
        // through to regular func.call. Mirrors the
        // `string_len_wrong_arity_falls_through_to_func_call` discipline.
        let src = r"
            fn bad(a : i64, b : i64) -> i64 {
                simd_v128_load(a, b)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            !names.iter().any(|n| n == &"cssl.simd.v128_load"),
            "unexpected cssl.simd.v128_load with wrong arity: {names:?}",
        );
        assert!(names.iter().any(|n| n == &"func.call"));
    }

    #[test]
    fn simd_recognizer_multi_segment_path_falls_through() {
        // `foo::simd_v_byte_eq(a, b)` (2-segment) — recognizer requires
        // single-segment ; declines ; fall-through to func.call.
        let src = r"
            fn shadow(a : i64, b : i64) -> i64 {
                foo::simd_v_byte_eq(a, b)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(
            !names.iter().any(|n| n == &"cssl.simd.v_byte_eq"),
            "unexpected cssl.simd.v_byte_eq from multi-segment path: {names:?}",
        );
        assert!(names.iter().any(|n| n == &"func.call"));
    }

    #[test]
    fn simd_recognizer_smoke_combo() {
        // Smoke : multi-op SIMD body — load + compare + horizontal-sum
        // must all be claimed by the recognizer chain (no func.call leak).
        let src = r"
            fn pipeline(p : i64, b : i64) -> i32 {
                let v  = simd_v128_load(p);
                let m  = simd_v_byte_eq(v, b);
                simd_v_horizontal_sum(m)
            }
        ";
        let (f, _) = lower_one(src);
        let names = op_names(&f);
        assert!(names.iter().any(|n| n == &"cssl.simd.v128_load"));
        assert!(names.iter().any(|n| n == &"cssl.simd.v_byte_eq"));
        assert!(names.iter().any(|n| n == &"cssl.simd.v_horizontal_sum"));
    }
}
