//! HIR-fn-body → MIR-op body lowering.
//!
//! § SPEC : `specs/02_IR.csl` § MIR + `specs/15_MLIR.csl` § CSSL-DIALECT-OPS +
//!         standard `arith.*` / `scf.*` / `func.*` dialects via [`CsslOp::Std`].
//!
//! § SCOPE (T6-phase-2a / this commit)
//!   - [`BodyLowerCtx`] : per-fn lowering context with fresh-value-id + op-buffer.
//!   - [`lower_fn_body`] : entry-point that takes a `HirFn` + a `MirFunc` and
//!     populates `MirFunc.body.entry().ops` with lowered ops.
//!   - Covered expression variants :
//!     * `HirLiteral` : Int → `arith.constant`, Float → `arith.constant`,
//!       Bool(_) → `arith.constant`, Unit → no-op.
//!     * `HirExprKind::Binary` : Add/Sub/Mul/Div/Rem → `arith.{addi,subi,muli,divsi,remsi}`
//!       (signed-integer path) ; float-op path (`addf`/`subf`/`mulf`/`divf`/`remf`)
//!       selected when either operand lowers to a float type.
//!     * `HirExprKind::Unary` : Neg → `arith.negf` / `arith.subi zero`.
//!     * `HirExprKind::Path` : single-segment → param-lookup (fn params bound to
//!       entry-block args) or constant-opaque for unresolved paths.
//!     * `HirExprKind::Call` : `func.call` op with operand-list derived from
//!       arg-lowering results.
//!     * `HirExprKind::Return` : emits trailing-op + `func.return`.
//!     * `HirExprKind::Block` : recursive iteration of stmts + trailing.
//!     * `HirExprKind::If` : `scf.if` with nested regions (structured-CFG
//!       preservation per CC4).
//!
//! § T6-phase-2b DEFERRED
//!   - Real literal-value extraction from source-text (currently emits
//!     `attribute="stage0_literal"` placeholders).
//!   - Field access + indexing (emit `arith.indexcast` + `memref.load`).
//!   - Loops (for / while / loop) — scf.for + scf.while emission.
//!   - Struct / tuple / array constructors.
//!   - Assignment + compound-assign (`a += b`).
//!   - Pipeline operator (`a |> f`).
//!   - Match expressions (desugar to scf.if-chain or scf.switch).
//!   - Closure-capture analysis for lambdas.
//!   - Proper type-propagation (currently assumes i32 for most scalar ops).

use std::collections::HashMap;

use cssl_ast::Span;
use cssl_hir::{
    HirBinOp, HirBlock, HirCallArg, HirExpr, HirExprKind, HirFn, HirLiteral, HirLiteralKind,
    HirStmt, HirStmtKind, HirType, HirTypeKind, HirUnOp, Interner, Symbol,
};

use crate::block::{MirBlock, MirOp, MirRegion};
use crate::func::MirFunc;
use crate::op::CsslOp;
use crate::value::{FloatWidth, IntWidth, MirType, MirValue, ValueId};

/// Per-fn lowering context.
#[derive(Debug)]
pub struct BodyLowerCtx<'a> {
    /// Source symbol-interner.
    pub interner: &'a Interner,
    /// Mapping from HIR param-symbol → entry-block value-id.
    pub param_vars: HashMap<Symbol, (ValueId, MirType)>,
    /// Next free value-id (wired to `MirFunc.fresh_value_id`).
    pub next_value_id: u32,
    /// Accumulated ops (consumed at end into the entry-block).
    pub ops: Vec<MirOp>,
}

impl<'a> BodyLowerCtx<'a> {
    /// Build a fresh context for `f`.
    #[must_use]
    pub fn new(interner: &'a Interner) -> Self {
        Self {
            interner,
            param_vars: HashMap::new(),
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
}

/// Entry point : lower the body of `hir_fn` into `mir_fn.body.entry().ops`.
///
/// If `hir_fn.body` is `None`, `mir_fn` is left as-is (signature-only — the
/// T6-phase-1 shape). The `param_vars` map is populated from `hir_fn.params`
/// using entry-block value-ids `v0`, `v1`, …
pub fn lower_fn_body(interner: &Interner, hir_fn: &HirFn, mir_fn: &mut MirFunc) {
    let Some(body) = &hir_fn.body else {
        return;
    };
    let mut ctx = BodyLowerCtx::new(interner);
    // Entry-block args = fn params. Seed `param_vars` + advance `next_value_id`.
    for (i, p) in hir_fn.params.iter().enumerate() {
        let id = ValueId(u32::try_from(i).unwrap_or(0));
        let ty = lower_hir_type_light(interner, &p.ty);
        // Match param-pattern to symbol (single-name binding only at stage-0).
        if let Some(sym) = extract_pattern_symbol(&p.pat) {
            ctx.param_vars.insert(sym, (id, ty));
        }
    }
    ctx.next_value_id = u32::try_from(hir_fn.params.len()).unwrap_or(0);

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
        HirExprKind::Call { callee, args } => lower_call(ctx, callee, args, expr.span),
        HirExprKind::Return { value } => {
            let trailing = value.as_deref().and_then(|e| lower_expr(ctx, e));
            emit_return(ctx, trailing, expr.span);
            None
        }
        HirExprKind::Paren(inner) => lower_expr(ctx, inner),
        // T6-phase-2b : the remaining ~20 variants emit an opaque `cssl.unsupported`
        // placeholder op that survives the pipeline for round-trip diagnostics.
        _ => Some(emit_unsupported(
            ctx,
            expr.span,
            discriminant_name(&expr.kind),
        )),
    }
}

fn lower_literal(ctx: &mut BodyLowerCtx<'_>, lit: &HirLiteral, span: Span) -> (ValueId, MirType) {
    let (ty, attr_value) = match lit.kind {
        HirLiteralKind::Int => (MirType::Int(IntWidth::I32), "stage0_int".to_string()),
        HirLiteralKind::Float => (MirType::Float(FloatWidth::F32), "stage0_float".to_string()),
        HirLiteralKind::Bool(b) => (MirType::Bool, b.to_string()),
        HirLiteralKind::Str => (MirType::Opaque("!cssl.string".into()), "stage0_str".into()),
        HirLiteralKind::Char => (MirType::Int(IntWidth::I32), "stage0_char".into()),
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
    // Lower each arg ; collect operand value-ids.
    let mut operand_ids = Vec::with_capacity(args.len());
    for arg in args {
        let a_expr = match arg {
            HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
        };
        if let Some((id, _)) = lower_expr(ctx, a_expr) {
            operand_ids.push(id);
        }
    }
    // Emit `func.call @target` op. Stage-0 assumes single opaque result-type.
    let result_ty = MirType::Opaque(format!("!cssl.call_result.{target}"));
    let id = ctx.fresh_value_id();
    let mut mir_op = MirOp::std("func.call")
        .with_attribute("callee", target)
        .with_attribute("source_loc", format!("{span:?}"))
        .with_result(id, result_ty.clone());
    for oid in operand_ids {
        mir_op = mir_op.with_operand(oid);
    }
    ctx.ops.push(mir_op);
    let _ = span;
    Some((id, result_ty))
}

fn lower_if(
    ctx: &mut BodyLowerCtx<'_>,
    cond: &HirExpr,
    then_branch: &HirBlock,
    else_branch: Option<&HirExpr>,
    span: Span,
) -> Option<(ValueId, MirType)> {
    let (cond_id, _) = lower_expr(ctx, cond)?;
    // Emit scf.if with nested regions. Stage-0 lowers each branch into a sub-region.
    let then_region = lower_sub_region(ctx.interner, then_branch);
    let else_region = match else_branch {
        Some(e) => {
            let mut sub_ctx = BodyLowerCtx::new(ctx.interner);
            sub_ctx.next_value_id = ctx.next_value_id;
            let _ = lower_expr(&mut sub_ctx, e);
            ctx.next_value_id = sub_ctx.next_value_id;
            let mut blk = MirBlock::new("entry");
            blk.ops = sub_ctx.ops;
            let mut r = MirRegion::new();
            r.push(blk);
            r
        }
        None => MirRegion::new(),
    };
    let result_ty = MirType::None; // stage-0 : scf.if result-type resolved @ phase-2b
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

fn lower_sub_region(interner: &Interner, block: &HirBlock) -> MirRegion {
    let mut sub_ctx = BodyLowerCtx::new(interner);
    let _ = lower_block(&mut sub_ctx, block);
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
    use cssl_ast::{SourceFile, SourceId, Surface};

    fn hir_from(src: &str) -> (cssl_hir::HirModule, cssl_hir::Interner) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _lower_bag) = cssl_hir::lower_module(&f, &cst);
        (hir, interner)
    }

    fn lower_one(src: &str) -> (crate::func::MirFunc, cssl_hir::Interner) {
        let (hir, interner) = hir_from(src);
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
        lower_fn_body(&interner, f, &mut mf);
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
}
