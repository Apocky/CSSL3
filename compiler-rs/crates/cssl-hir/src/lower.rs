//! CST → HIR structural lowering pass.
//!
//! § SCOPE (T3.3)
//!   Pure structural transform : every CST node gets a matching HIR node with identifiers
//!   interned and a fresh `HirId` / `DefId` assigned. No type inference, no capability
//!   inference, no IFC-label propagation — those are T3.4 concerns. Path references are
//!   left with `def: None` ; a subsequent `resolve_module` pass walks the HIR and fills
//!   in module-level resolutions.
//!
//! § ENTRY
//!   [`lower_module`] — consumes a `(cst::Module, SourceFile)` pair and returns
//!   `(HirModule, Interner, DiagnosticBag)`. The interner is separated from `HirModule`
//!   so downstream passes (inference, elaboration, IFC) can access both.
//!
//! § DECISIONS (`DECISIONS.md`)
//!   - T3-D2 : interning lives here ; CST carries spans only.
//!   - T3-D3 : morpheme chains arrive as `cst::Expr::Compound` — lowered 1:1 to
//!     `HirExprKind::Compound` with the operator-class preserved.

use cssl_ast::cst;
use cssl_ast::{DiagnosticBag, Ident, Module as CstModule, SourceFile, Span};

use crate::arena::{AttributionKey, DefId, DefKind, HirArena, HirId};
use crate::attr::{HirAttr, HirAttrArg, HirAttrKind};
use crate::expr::{
    HirArrayExpr, HirBinOp, HirBlock, HirCallArg, HirCompoundOp, HirExpr, HirExprKind,
    HirLambdaParam, HirLiteral, HirLiteralKind, HirMatchArm, HirStructFieldInit, HirUnOp,
};
use crate::item::{
    HirAssocTypeDecl, HirAssocTypeDef, HirConst, HirEffect, HirEnum, HirEnumVariant, HirFieldDecl,
    HirFn, HirFnParam, HirGenericParam, HirGenericParamKind, HirGenerics, HirHandler, HirImpl,
    HirInterface, HirItem, HirModule, HirNestedModule, HirStruct, HirStructBody, HirTypeAlias,
    HirUse, HirUseBinding, HirVisibility, HirWhereClause,
};
use crate::pat::{HirPattern, HirPatternField, HirPatternKind};
use crate::resolve::ScopeMap;
use crate::stmt::{HirStmt, HirStmtKind};
use crate::symbol::{Interner, Symbol};
use crate::ty::{
    HirCapKind, HirEffectAnnotation, HirEffectArg, HirEffectRow, HirRefinementKind, HirType,
    HirTypeKind,
};

/// Lowering context — shared state threaded through each `lower_*` method.
#[derive(Debug)]
pub struct LowerCtx<'a> {
    pub interner: Interner,
    pub arena: HirArena,
    pub source: &'a SourceFile,
    pub diagnostics: DiagnosticBag,
}

impl<'a> LowerCtx<'a> {
    /// Build a fresh context tied to the given source file.
    #[must_use]
    pub fn new(source: &'a SourceFile) -> Self {
        Self {
            interner: Interner::new(),
            arena: HirArena::new(),
            source,
            diagnostics: DiagnosticBag::new(),
        }
    }

    // ─ ID allocation ────────────────────────────────────────────────────────

    fn hir_id(&mut self) -> HirId {
        self.arena.fresh_hir_id()
    }

    fn def_id(&mut self) -> DefId {
        self.arena.fresh_def_id()
    }

    /// Allocate a fresh `DefId` AND record its content-stable attribution-key.
    /// Lowering call-sites for definition-bearing items must call this so the
    /// fixed-point gate can fingerprint the module without depending on `Spur`
    /// numerics — see `arena::AttributionKey` doc-comment.
    fn def_id_for(&mut self, kind: DefKind, span: Span, name: Ident) -> DefId {
        self.arena.fresh_def_id_with(AttributionKey::new(
            kind,
            span.start,
            span.end,
            name.span.start,
            name.span.end,
        ))
    }

    // ─ Identifier / path interning ──────────────────────────────────────────

    fn intern_ident(&mut self, ident: Ident) -> Symbol {
        let text = self
            .source
            .slice(ident.span.start, ident.span.end)
            .unwrap_or("");
        self.interner.intern(text)
    }

    fn intern_path(&mut self, path: &cst::ModulePath) -> Vec<Symbol> {
        path.segments
            .iter()
            .map(|s| self.intern_ident(*s))
            .collect()
    }

    // ─ Visibility ───────────────────────────────────────────────────────────

    fn lower_visibility(v: cst::Visibility) -> HirVisibility {
        match v.kind {
            cst::VisibilityKind::Public => HirVisibility::Public,
            cst::VisibilityKind::Private => HirVisibility::Private,
        }
    }

    // ─ Attributes ───────────────────────────────────────────────────────────

    fn lower_attr(&mut self, a: &cst::Attr) -> HirAttr {
        let kind = match a.kind {
            cst::AttrKind::Outer => HirAttrKind::Outer,
            cst::AttrKind::Inner => HirAttrKind::Inner,
        };
        let path = self.intern_path(&a.path);
        let args = a.args.iter().map(|arg| self.lower_attr_arg(arg)).collect();
        HirAttr {
            span: a.span,
            kind,
            path,
            args,
        }
    }

    fn lower_attr_arg(&mut self, a: &cst::AttrArg) -> HirAttrArg {
        match a {
            cst::AttrArg::Positional(e) => HirAttrArg::Positional(self.lower_expr(e)),
            cst::AttrArg::Named { name, value } => HirAttrArg::Named {
                name: self.intern_ident(*name),
                value: self.lower_expr(value),
            },
        }
    }

    fn lower_attrs(&mut self, attrs: &[cst::Attr]) -> Vec<HirAttr> {
        attrs.iter().map(|a| self.lower_attr(a)).collect()
    }

    // ─ Generics / where clauses ─────────────────────────────────────────────

    fn lower_generics(&mut self, g: &cst::Generics) -> HirGenerics {
        let params = g
            .params
            .iter()
            .map(|p| self.lower_generic_param(p))
            .collect();
        HirGenerics { params }
    }

    fn lower_generic_param(&mut self, p: &cst::GenericParam) -> HirGenericParam {
        let kind = match p.kind {
            cst::GenericParamKind::Type => HirGenericParamKind::Type,
            cst::GenericParamKind::Region => HirGenericParamKind::Region,
            cst::GenericParamKind::Const => HirGenericParamKind::Const,
        };
        HirGenericParam {
            span: p.span,
            id: self.hir_id(),
            name: self.intern_ident(p.name),
            kind,
            bounds: p.bounds.iter().map(|t| self.lower_type(t)).collect(),
            default: p.default.as_ref().map(|t| self.lower_type(t)),
        }
    }

    fn lower_where_clauses(&mut self, ws: &[cst::WhereClause]) -> Vec<HirWhereClause> {
        ws.iter()
            .map(|w| HirWhereClause {
                span: w.span,
                ty: self.lower_type(&w.ty),
                bounds: w.bounds.iter().map(|b| self.lower_type(b)).collect(),
            })
            .collect()
    }

    // ─ Types ────────────────────────────────────────────────────────────────

    fn lower_type(&mut self, t: &cst::Type) -> HirType {
        let kind = self.lower_type_kind(&t.kind);
        HirType {
            span: t.span,
            id: self.hir_id(),
            kind,
        }
    }

    fn lower_type_kind(&mut self, k: &cst::TypeKind) -> HirTypeKind {
        match k {
            cst::TypeKind::Path { path, type_args } => HirTypeKind::Path {
                path: self.intern_path(path),
                def: None,
                type_args: type_args.iter().map(|t| self.lower_type(t)).collect(),
            },
            cst::TypeKind::Array { elem, len } => HirTypeKind::Array {
                elem: Box::new(self.lower_type(elem)),
                len: Box::new(self.lower_expr(len)),
            },
            cst::TypeKind::Slice { elem } => HirTypeKind::Slice {
                elem: Box::new(self.lower_type(elem)),
            },
            cst::TypeKind::Tuple { elems } => HirTypeKind::Tuple {
                elems: elems.iter().map(|t| self.lower_type(t)).collect(),
            },
            cst::TypeKind::Reference { mutable, inner } => HirTypeKind::Reference {
                mutable: *mutable,
                inner: Box::new(self.lower_type(inner)),
            },
            cst::TypeKind::Capability { cap, inner } => HirTypeKind::Capability {
                cap: Self::lower_cap(*cap),
                inner: Box::new(self.lower_type(inner)),
            },
            cst::TypeKind::Function {
                params,
                return_ty,
                effect_row,
            } => HirTypeKind::Function {
                params: params.iter().map(|t| self.lower_type(t)).collect(),
                return_ty: Box::new(self.lower_type(return_ty)),
                effect_row: effect_row.as_ref().map(|r| self.lower_effect_row(r)),
            },
            cst::TypeKind::Refined { base, kind } => HirTypeKind::Refined {
                base: Box::new(self.lower_type(base)),
                kind: self.lower_refinement(kind),
            },
            cst::TypeKind::Infer => HirTypeKind::Infer,
        }
    }

    const fn lower_cap(c: cst::CapKind) -> HirCapKind {
        match c {
            cst::CapKind::Iso => HirCapKind::Iso,
            cst::CapKind::Trn => HirCapKind::Trn,
            cst::CapKind::Ref => HirCapKind::Ref,
            cst::CapKind::Val => HirCapKind::Val,
            cst::CapKind::Box => HirCapKind::Box,
            cst::CapKind::Tag => HirCapKind::Tag,
        }
    }

    fn lower_refinement(&mut self, r: &cst::RefinementKind) -> HirRefinementKind {
        match r {
            cst::RefinementKind::Tag { name } => HirRefinementKind::Tag {
                name: self.intern_ident(*name),
            },
            cst::RefinementKind::Predicate { binder, predicate } => HirRefinementKind::Predicate {
                binder: self.intern_ident(*binder),
                predicate: Box::new(self.lower_expr(predicate)),
            },
            cst::RefinementKind::Lipschitz { bound } => HirRefinementKind::Lipschitz {
                bound: Box::new(self.lower_expr(bound)),
            },
        }
    }

    fn lower_effect_row(&mut self, r: &cst::EffectRow) -> HirEffectRow {
        HirEffectRow {
            span: r.span,
            effects: r.effects.iter().map(|e| self.lower_effect_ann(e)).collect(),
            tail: r.tail.map(|t| self.intern_ident(t)),
        }
    }

    fn lower_effect_ann(&mut self, e: &cst::EffectAnnotation) -> HirEffectAnnotation {
        HirEffectAnnotation {
            span: e.span,
            name: self.intern_path(&e.name),
            args: e.args.iter().map(|a| self.lower_effect_arg(a)).collect(),
        }
    }

    fn lower_effect_arg(&mut self, a: &cst::EffectArg) -> HirEffectArg {
        match a {
            cst::EffectArg::Type(t) => HirEffectArg::Type(self.lower_type(t)),
            cst::EffectArg::Expr(e) => HirEffectArg::Expr(self.lower_expr(e)),
        }
    }

    // ─ Patterns ─────────────────────────────────────────────────────────────

    fn lower_pattern(&mut self, p: &cst::Pattern) -> HirPattern {
        let kind = self.lower_pattern_kind(&p.kind);
        HirPattern {
            span: p.span,
            id: self.hir_id(),
            kind,
        }
    }

    fn lower_pattern_kind(&mut self, k: &cst::PatternKind) -> HirPatternKind {
        match k {
            cst::PatternKind::Wildcard => HirPatternKind::Wildcard,
            cst::PatternKind::Literal(l) => HirPatternKind::Literal(Self::lower_literal(l)),
            cst::PatternKind::Binding { mutable, name } => HirPatternKind::Binding {
                mutable: *mutable,
                name: self.intern_ident(*name),
            },
            cst::PatternKind::Tuple(elems) => {
                HirPatternKind::Tuple(elems.iter().map(|p| self.lower_pattern(p)).collect())
            }
            cst::PatternKind::Struct { path, fields, rest } => HirPatternKind::Struct {
                path: self.intern_path(path),
                def: None,
                fields: fields.iter().map(|f| self.lower_pattern_field(f)).collect(),
                rest: *rest,
            },
            cst::PatternKind::Variant { path, args } => HirPatternKind::Variant {
                path: self.intern_path(path),
                def: None,
                args: args.iter().map(|a| self.lower_pattern(a)).collect(),
            },
            cst::PatternKind::Or(alts) => {
                HirPatternKind::Or(alts.iter().map(|a| self.lower_pattern(a)).collect())
            }
            cst::PatternKind::Range {
                start,
                end,
                inclusive,
            } => HirPatternKind::Range {
                start: start.as_ref().map(|p| Box::new(self.lower_pattern(p))),
                end: end.as_ref().map(|p| Box::new(self.lower_pattern(p))),
                inclusive: *inclusive,
            },
            cst::PatternKind::Ref { mutable, inner } => HirPatternKind::Ref {
                mutable: *mutable,
                inner: Box::new(self.lower_pattern(inner)),
            },
        }
    }

    fn lower_pattern_field(&mut self, f: &cst::PatternField) -> HirPatternField {
        HirPatternField {
            span: f.span,
            name: self.intern_ident(f.name),
            pat: f.pat.as_ref().map(|p| self.lower_pattern(p)),
        }
    }

    // ─ Expressions ──────────────────────────────────────────────────────────

    /// Lower a CST expression into a HIR expression.
    pub fn lower_expr(&mut self, e: &cst::Expr) -> HirExpr {
        let attrs = self.lower_attrs(&e.attrs);
        let kind = self.lower_expr_kind(&e.kind);
        HirExpr {
            span: e.span,
            id: self.hir_id(),
            attrs,
            kind,
        }
    }

    fn lower_expr_kind(&mut self, k: &cst::ExprKind) -> HirExprKind {
        match k {
            cst::ExprKind::Literal(l) => HirExprKind::Literal(Self::lower_literal(l)),
            cst::ExprKind::Path(path) => HirExprKind::Path {
                segments: self.intern_path(path),
                def: None,
            },
            cst::ExprKind::Call {
                callee,
                args,
                type_args,
            } => HirExprKind::Call {
                callee: Box::new(self.lower_expr(callee)),
                args: args.iter().map(|a| self.lower_call_arg(a)).collect(),
                type_args: type_args.iter().map(|t| self.lower_type(t)).collect(),
            },
            cst::ExprKind::Field { obj, name } => HirExprKind::Field {
                obj: Box::new(self.lower_expr(obj)),
                name: self.intern_ident(*name),
            },
            cst::ExprKind::Index { obj, index } => HirExprKind::Index {
                obj: Box::new(self.lower_expr(obj)),
                index: Box::new(self.lower_expr(index)),
            },
            cst::ExprKind::Binary { op, lhs, rhs } => HirExprKind::Binary {
                op: Self::lower_binop(*op),
                lhs: Box::new(self.lower_expr(lhs)),
                rhs: Box::new(self.lower_expr(rhs)),
            },
            cst::ExprKind::Unary { op, operand } => HirExprKind::Unary {
                op: Self::lower_unop(*op),
                operand: Box::new(self.lower_expr(operand)),
            },
            cst::ExprKind::Block(b) => HirExprKind::Block(self.lower_block(b)),
            cst::ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => HirExprKind::If {
                cond: Box::new(self.lower_expr(cond)),
                then_branch: self.lower_block(then_branch),
                else_branch: else_branch.as_ref().map(|e| Box::new(self.lower_expr(e))),
            },
            cst::ExprKind::Match { scrutinee, arms } => HirExprKind::Match {
                scrutinee: Box::new(self.lower_expr(scrutinee)),
                arms: arms.iter().map(|a| self.lower_match_arm(a)).collect(),
            },
            cst::ExprKind::For { pat, iter, body } => HirExprKind::For {
                pat: self.lower_pattern(pat),
                iter: Box::new(self.lower_expr(iter)),
                body: self.lower_block(body),
            },
            cst::ExprKind::While { cond, body } => HirExprKind::While {
                cond: Box::new(self.lower_expr(cond)),
                body: self.lower_block(body),
            },
            cst::ExprKind::Loop { body } => HirExprKind::Loop {
                body: self.lower_block(body),
            },
            cst::ExprKind::Return { value } => HirExprKind::Return {
                value: value.as_ref().map(|e| Box::new(self.lower_expr(e))),
            },
            cst::ExprKind::Break { label, value } => HirExprKind::Break {
                label: label.map(|l| self.intern_ident(l)),
                value: value.as_ref().map(|e| Box::new(self.lower_expr(e))),
            },
            cst::ExprKind::Continue { label } => HirExprKind::Continue {
                label: label.map(|l| self.intern_ident(l)),
            },
            cst::ExprKind::Lambda {
                params,
                return_ty,
                body,
            } => HirExprKind::Lambda {
                params: params.iter().map(|p| self.lower_lambda_param(p)).collect(),
                return_ty: return_ty.as_ref().map(|t| self.lower_type(t)),
                body: Box::new(self.lower_expr(body)),
            },
            cst::ExprKind::Assign { op, lhs, rhs } => HirExprKind::Assign {
                op: op.map(Self::lower_binop),
                lhs: Box::new(self.lower_expr(lhs)),
                rhs: Box::new(self.lower_expr(rhs)),
            },
            cst::ExprKind::Cast { expr, ty } => HirExprKind::Cast {
                expr: Box::new(self.lower_expr(expr)),
                ty: self.lower_type(ty),
            },
            cst::ExprKind::Range { lo, hi, inclusive } => HirExprKind::Range {
                lo: lo.as_ref().map(|e| Box::new(self.lower_expr(e))),
                hi: hi.as_ref().map(|e| Box::new(self.lower_expr(e))),
                inclusive: *inclusive,
            },
            cst::ExprKind::Pipeline { lhs, rhs } => HirExprKind::Pipeline {
                lhs: Box::new(self.lower_expr(lhs)),
                rhs: Box::new(self.lower_expr(rhs)),
            },
            cst::ExprKind::TryDefault { expr, default } => HirExprKind::TryDefault {
                expr: Box::new(self.lower_expr(expr)),
                default: Box::new(self.lower_expr(default)),
            },
            cst::ExprKind::Try { expr } => HirExprKind::Try {
                expr: Box::new(self.lower_expr(expr)),
            },
            cst::ExprKind::Perform { path, args } => HirExprKind::Perform {
                path: self.intern_path(path),
                def: None,
                args: args.iter().map(|a| self.lower_call_arg(a)).collect(),
            },
            cst::ExprKind::With { handler, body } => HirExprKind::With {
                handler: Box::new(self.lower_expr(handler)),
                body: self.lower_block(body),
            },
            cst::ExprKind::Region { label, body } => HirExprKind::Region {
                label: label.map(|l| self.intern_ident(l)),
                body: self.lower_block(body),
            },
            cst::ExprKind::Tuple(elems) => {
                HirExprKind::Tuple(elems.iter().map(|e| self.lower_expr(e)).collect())
            }
            cst::ExprKind::Array(arr) => HirExprKind::Array(match arr {
                cst::ArrayExpr::List(items) => {
                    HirArrayExpr::List(items.iter().map(|e| self.lower_expr(e)).collect())
                }
                cst::ArrayExpr::Repeat { elem, len } => HirArrayExpr::Repeat {
                    elem: Box::new(self.lower_expr(elem)),
                    len: Box::new(self.lower_expr(len)),
                },
            }),
            cst::ExprKind::Struct {
                path,
                fields,
                spread,
            } => HirExprKind::Struct {
                path: self.intern_path(path),
                def: None,
                fields: fields.iter().map(|f| self.lower_struct_field(f)).collect(),
                spread: spread.as_ref().map(|e| Box::new(self.lower_expr(e))),
            },
            cst::ExprKind::Run { expr } => HirExprKind::Run {
                expr: Box::new(self.lower_expr(expr)),
            },
            cst::ExprKind::Compound { op, lhs, rhs } => HirExprKind::Compound {
                op: Self::lower_compound_op(*op),
                lhs: Box::new(self.lower_expr(lhs)),
                rhs: Box::new(self.lower_expr(rhs)),
            },
            cst::ExprKind::SectionRef { path } => HirExprKind::SectionRef {
                path: self.intern_path(path),
            },
            cst::ExprKind::Paren(e) => HirExprKind::Paren(Box::new(self.lower_expr(e))),
            cst::ExprKind::Error => HirExprKind::Error,
        }
    }

    fn lower_call_arg(&mut self, a: &cst::CallArg) -> HirCallArg {
        match a {
            cst::CallArg::Positional(e) => HirCallArg::Positional(self.lower_expr(e)),
            cst::CallArg::Named { name, value } => HirCallArg::Named {
                name: self.intern_ident(*name),
                value: self.lower_expr(value),
            },
        }
    }

    fn lower_match_arm(&mut self, a: &cst::MatchArm) -> HirMatchArm {
        HirMatchArm {
            span: a.span,
            attrs: self.lower_attrs(&a.attrs),
            pat: self.lower_pattern(&a.pat),
            guard: a.guard.as_ref().map(|g| self.lower_expr(g)),
            body: self.lower_expr(&a.body),
        }
    }

    fn lower_lambda_param(&mut self, p: &cst::Param) -> HirLambdaParam {
        HirLambdaParam {
            span: p.span,
            pat: self.lower_pattern(&p.pat),
            ty: match &p.ty.kind {
                cst::TypeKind::Infer => None,
                _ => Some(self.lower_type(&p.ty)),
            },
        }
    }

    fn lower_struct_field(&mut self, f: &cst::StructFieldInit) -> HirStructFieldInit {
        HirStructFieldInit {
            span: f.span,
            name: self.intern_ident(f.name),
            value: f.value.as_ref().map(|v| self.lower_expr(v)),
        }
    }

    const fn lower_binop(b: cst::BinOp) -> HirBinOp {
        match b {
            cst::BinOp::Add => HirBinOp::Add,
            cst::BinOp::Sub => HirBinOp::Sub,
            cst::BinOp::Mul => HirBinOp::Mul,
            cst::BinOp::Div => HirBinOp::Div,
            cst::BinOp::Rem => HirBinOp::Rem,
            cst::BinOp::Eq => HirBinOp::Eq,
            cst::BinOp::Ne => HirBinOp::Ne,
            cst::BinOp::Lt => HirBinOp::Lt,
            cst::BinOp::Le => HirBinOp::Le,
            cst::BinOp::Gt => HirBinOp::Gt,
            cst::BinOp::Ge => HirBinOp::Ge,
            cst::BinOp::And => HirBinOp::And,
            cst::BinOp::Or => HirBinOp::Or,
            cst::BinOp::BitAnd => HirBinOp::BitAnd,
            cst::BinOp::BitOr => HirBinOp::BitOr,
            cst::BinOp::BitXor => HirBinOp::BitXor,
            cst::BinOp::Shl => HirBinOp::Shl,
            cst::BinOp::Shr => HirBinOp::Shr,
            cst::BinOp::Implies => HirBinOp::Implies,
            cst::BinOp::Entails => HirBinOp::Entails,
        }
    }

    const fn lower_unop(u: cst::UnOp) -> HirUnOp {
        match u {
            cst::UnOp::Not => HirUnOp::Not,
            cst::UnOp::Neg => HirUnOp::Neg,
            cst::UnOp::BitNot => HirUnOp::BitNot,
            cst::UnOp::Ref => HirUnOp::Ref,
            cst::UnOp::RefMut => HirUnOp::RefMut,
            cst::UnOp::Deref => HirUnOp::Deref,
        }
    }

    const fn lower_compound_op(c: cst::CompoundOp) -> HirCompoundOp {
        match c {
            cst::CompoundOp::Tp => HirCompoundOp::Tp,
            cst::CompoundOp::Dv => HirCompoundOp::Dv,
            cst::CompoundOp::Kd => HirCompoundOp::Kd,
            cst::CompoundOp::Bv => HirCompoundOp::Bv,
            cst::CompoundOp::Av => HirCompoundOp::Av,
        }
    }

    fn lower_literal(l: &cst::Literal) -> HirLiteral {
        let kind = match l.kind {
            cst::LiteralKind::Int => HirLiteralKind::Int,
            cst::LiteralKind::Float => HirLiteralKind::Float,
            cst::LiteralKind::Str => HirLiteralKind::Str,
            cst::LiteralKind::Char => HirLiteralKind::Char,
            cst::LiteralKind::Bool(b) => HirLiteralKind::Bool(b),
            cst::LiteralKind::Unit => HirLiteralKind::Unit,
        };
        HirLiteral { span: l.span, kind }
    }

    // ─ Blocks / statements ──────────────────────────────────────────────────

    fn lower_block(&mut self, b: &cst::Block) -> HirBlock {
        let stmts = b.stmts.iter().map(|s| self.lower_stmt(s)).collect();
        let trailing = b.trailing.as_ref().map(|e| Box::new(self.lower_expr(e)));
        HirBlock {
            span: b.span,
            id: self.hir_id(),
            stmts,
            trailing,
        }
    }

    fn lower_stmt(&mut self, s: &cst::Stmt) -> HirStmt {
        let kind = match &s.kind {
            cst::StmtKind::Let {
                attrs,
                pat,
                ty,
                value,
            } => HirStmtKind::Let {
                attrs: self.lower_attrs(attrs),
                pat: self.lower_pattern(pat),
                ty: ty.as_ref().map(|t| self.lower_type(t)),
                value: value.as_ref().map(|v| self.lower_expr(v)),
            },
            cst::StmtKind::Expr(e) => HirStmtKind::Expr(self.lower_expr(e)),
            cst::StmtKind::Item(i) => HirStmtKind::Item(Box::new(self.lower_item(i))),
        };
        HirStmt {
            span: s.span,
            id: self.hir_id(),
            kind,
        }
    }

    // ─ Items ────────────────────────────────────────────────────────────────

    fn lower_fn_param(&mut self, p: &cst::Param) -> HirFnParam {
        HirFnParam {
            span: p.span,
            id: self.hir_id(),
            attrs: self.lower_attrs(&p.attrs),
            pat: self.lower_pattern(&p.pat),
            ty: self.lower_type(&p.ty),
            default: p.default.as_ref().map(|e| self.lower_expr(e)),
        }
    }

    fn lower_fn(&mut self, f: &cst::FnItem) -> HirFn {
        HirFn {
            span: f.span,
            def: self.def_id_for(DefKind::Fn, f.span, f.name),
            visibility: Self::lower_visibility(f.visibility),
            attrs: self.lower_attrs(&f.attrs),
            name: self.intern_ident(f.name),
            generics: self.lower_generics(&f.generics),
            params: f.params.iter().map(|p| self.lower_fn_param(p)).collect(),
            return_ty: f.return_ty.as_ref().map(|t| self.lower_type(t)),
            effect_row: f.effect_row.as_ref().map(|r| self.lower_effect_row(r)),
            where_clauses: self.lower_where_clauses(&f.where_clauses),
            body: f.body.as_ref().map(|b| self.lower_block(b)),
        }
    }

    fn lower_struct_body(&mut self, b: &cst::StructBody) -> HirStructBody {
        match b {
            cst::StructBody::Unit => HirStructBody::Unit,
            cst::StructBody::Tuple(fs) => {
                HirStructBody::Tuple(fs.iter().map(|f| self.lower_field(f)).collect())
            }
            cst::StructBody::Named(fs) => {
                HirStructBody::Named(fs.iter().map(|f| self.lower_field(f)).collect())
            }
        }
    }

    fn lower_field(&mut self, f: &cst::FieldDecl) -> HirFieldDecl {
        HirFieldDecl {
            span: f.span,
            id: self.hir_id(),
            attrs: self.lower_attrs(&f.attrs),
            visibility: Self::lower_visibility(f.visibility),
            name: f.name.map(|i| self.intern_ident(i)),
            ty: self.lower_type(&f.ty),
        }
    }

    fn lower_struct(&mut self, s: &cst::StructItem) -> HirStruct {
        HirStruct {
            span: s.span,
            def: self.def_id_for(DefKind::Struct, s.span, s.name),
            visibility: Self::lower_visibility(s.visibility),
            attrs: self.lower_attrs(&s.attrs),
            name: self.intern_ident(s.name),
            generics: self.lower_generics(&s.generics),
            body: self.lower_struct_body(&s.body),
        }
    }

    fn lower_enum(&mut self, e: &cst::EnumItem) -> HirEnum {
        let def = self.def_id_for(DefKind::Enum, e.span, e.name);
        HirEnum {
            span: e.span,
            def,
            visibility: Self::lower_visibility(e.visibility),
            attrs: self.lower_attrs(&e.attrs),
            name: self.intern_ident(e.name),
            generics: self.lower_generics(&e.generics),
            variants: e
                .variants
                .iter()
                .map(|v| HirEnumVariant {
                    span: v.span,
                    def: self.def_id_for(DefKind::Variant, v.span, v.name),
                    attrs: self.lower_attrs(&v.attrs),
                    name: self.intern_ident(v.name),
                    body: self.lower_struct_body(&v.body),
                })
                .collect(),
        }
    }

    fn lower_interface(&mut self, i: &cst::InterfaceItem) -> HirInterface {
        let def = self.def_id_for(DefKind::Interface, i.span, i.name);
        let mut fns = Vec::new();
        let mut assoc_types = Vec::new();
        let mut consts = Vec::new();
        for item in &i.items {
            match item {
                cst::InterfaceAssocItem::Fn(f) => fns.push(self.lower_fn(f)),
                cst::InterfaceAssocItem::AssociatedType(a) => assoc_types.push(HirAssocTypeDecl {
                    span: a.span,
                    def: self.def_id_for(DefKind::AssocTypeDecl, a.span, a.name),
                    attrs: self.lower_attrs(&a.attrs),
                    name: self.intern_ident(a.name),
                    bounds: a.bounds.iter().map(|b| self.lower_type(b)).collect(),
                    default: a.default.as_ref().map(|t| self.lower_type(t)),
                }),
                cst::InterfaceAssocItem::Const(c) => consts.push(self.lower_const(c)),
            }
        }
        HirInterface {
            span: i.span,
            def,
            visibility: Self::lower_visibility(i.visibility),
            attrs: self.lower_attrs(&i.attrs),
            name: self.intern_ident(i.name),
            generics: self.lower_generics(&i.generics),
            super_bounds: i.super_bounds.iter().map(|t| self.lower_type(t)).collect(),
            fns,
            assoc_types,
            consts,
        }
    }

    fn lower_impl(&mut self, i: &cst::ImplItem) -> HirImpl {
        let mut fns = Vec::new();
        let mut assoc_types = Vec::new();
        let mut consts = Vec::new();
        for item in &i.items {
            match item {
                cst::ImplAssocItem::Fn(f) => fns.push(self.lower_fn(f)),
                cst::ImplAssocItem::AssociatedType(a) => assoc_types.push(HirAssocTypeDef {
                    span: a.span,
                    def: self.def_id_for(DefKind::AssocTypeDef, a.span, a.name),
                    attrs: self.lower_attrs(&a.attrs),
                    name: self.intern_ident(a.name),
                    ty: self.lower_type(&a.ty),
                }),
                cst::ImplAssocItem::Const(c) => consts.push(self.lower_const(c)),
            }
        }
        HirImpl {
            span: i.span,
            attrs: self.lower_attrs(&i.attrs),
            generics: self.lower_generics(&i.generics),
            trait_: i.trait_.as_ref().map(|t| self.lower_type(t)),
            self_ty: self.lower_type(&i.self_ty),
            where_clauses: self.lower_where_clauses(&i.where_clauses),
            fns,
            assoc_types,
            consts,
        }
    }

    fn lower_effect_item(&mut self, e: &cst::EffectItem) -> HirEffect {
        HirEffect {
            span: e.span,
            def: self.def_id_for(DefKind::Effect, e.span, e.name),
            visibility: Self::lower_visibility(e.visibility),
            attrs: self.lower_attrs(&e.attrs),
            name: self.intern_ident(e.name),
            generics: self.lower_generics(&e.generics),
            ops: e.ops.iter().map(|f| self.lower_fn(f)).collect(),
        }
    }

    fn lower_handler(&mut self, h: &cst::HandlerItem) -> HirHandler {
        HirHandler {
            span: h.span,
            def: self.def_id_for(DefKind::Handler, h.span, h.name),
            visibility: Self::lower_visibility(h.visibility),
            attrs: self.lower_attrs(&h.attrs),
            name: self.intern_ident(h.name),
            generics: self.lower_generics(&h.generics),
            params: h.params.iter().map(|p| self.lower_fn_param(p)).collect(),
            handled_effect: self.lower_type(&h.handled_effect),
            return_ty: h.return_ty.as_ref().map(|t| self.lower_type(t)),
            ops: h.ops.iter().map(|f| self.lower_fn(f)).collect(),
            return_clause: h.return_clause.as_ref().map(|b| self.lower_block(b)),
        }
    }

    fn lower_type_alias(&mut self, t: &cst::TypeAliasItem) -> HirTypeAlias {
        HirTypeAlias {
            span: t.span,
            def: self.def_id_for(DefKind::TypeAlias, t.span, t.name),
            visibility: Self::lower_visibility(t.visibility),
            attrs: self.lower_attrs(&t.attrs),
            name: self.intern_ident(t.name),
            generics: self.lower_generics(&t.generics),
            ty: self.lower_type(&t.ty),
        }
    }

    fn lower_use(&mut self, u: &cst::UseItem) -> HirUse {
        let mut bindings = Vec::new();
        self.flatten_use_tree(&u.tree, &mut Vec::new(), &mut bindings);
        HirUse {
            span: u.span,
            visibility: Self::lower_visibility(u.visibility),
            attrs: self.lower_attrs(&u.attrs),
            bindings,
        }
    }

    fn flatten_use_tree(
        &mut self,
        t: &cst::UseTree,
        prefix: &mut Vec<Symbol>,
        out: &mut Vec<HirUseBinding>,
    ) {
        match t {
            cst::UseTree::Path { path, alias } => {
                let mut full = prefix.clone();
                full.extend(path.segments.iter().map(|s| self.intern_ident(*s)));
                out.push(HirUseBinding {
                    span: path.span,
                    path: full,
                    alias: alias.map(|a| self.intern_ident(a)),
                    is_glob: false,
                    def: None,
                });
            }
            cst::UseTree::Glob { path } => {
                let mut full = prefix.clone();
                full.extend(path.segments.iter().map(|s| self.intern_ident(*s)));
                out.push(HirUseBinding {
                    span: path.span,
                    path: full,
                    alias: None,
                    is_glob: true,
                    def: None,
                });
            }
            cst::UseTree::Group { prefix: p, trees } => {
                let prior_len = prefix.len();
                prefix.extend(p.segments.iter().map(|s| self.intern_ident(*s)));
                for tree in trees {
                    self.flatten_use_tree(tree, prefix, out);
                }
                prefix.truncate(prior_len);
            }
        }
    }

    fn lower_const(&mut self, c: &cst::ConstItem) -> HirConst {
        HirConst {
            span: c.span,
            def: self.def_id_for(DefKind::Const, c.span, c.name),
            visibility: Self::lower_visibility(c.visibility),
            attrs: self.lower_attrs(&c.attrs),
            name: self.intern_ident(c.name),
            ty: self.lower_type(&c.ty),
            value: self.lower_expr(&c.value),
        }
    }

    fn lower_nested_module(&mut self, m: &cst::ModuleItem) -> HirNestedModule {
        HirNestedModule {
            span: m.span,
            def: self.def_id_for(DefKind::Module, m.span, m.name),
            visibility: Self::lower_visibility(m.visibility),
            attrs: self.lower_attrs(&m.attrs),
            name: self.intern_ident(m.name),
            items: m
                .items
                .as_ref()
                .map(|is| is.iter().map(|i| self.lower_item(i)).collect()),
        }
    }

    fn lower_item(&mut self, i: &cst::Item) -> HirItem {
        match i {
            cst::Item::Fn(f) => HirItem::Fn(self.lower_fn(f)),
            cst::Item::Struct(s) => HirItem::Struct(self.lower_struct(s)),
            cst::Item::Enum(e) => HirItem::Enum(self.lower_enum(e)),
            cst::Item::Interface(i) => HirItem::Interface(self.lower_interface(i)),
            cst::Item::Impl(i) => HirItem::Impl(self.lower_impl(i)),
            cst::Item::Effect(e) => HirItem::Effect(self.lower_effect_item(e)),
            cst::Item::Handler(h) => HirItem::Handler(self.lower_handler(h)),
            cst::Item::TypeAlias(t) => HirItem::TypeAlias(self.lower_type_alias(t)),
            cst::Item::Use(u) => HirItem::Use(self.lower_use(u)),
            cst::Item::Const(c) => HirItem::Const(self.lower_const(c)),
            cst::Item::Module(m) => HirItem::Module(self.lower_nested_module(m)),
        }
    }
}

/// Lower a CST module into a HIR module, with the interner + diagnostic-bag returned
/// separately so downstream passes can access them.
#[must_use]
pub fn lower_module(
    source: &SourceFile,
    module: &CstModule,
) -> (HirModule, Interner, DiagnosticBag) {
    let mut ctx = LowerCtx::new(source);
    let inner_attrs = ctx.lower_attrs(&module.inner_attrs);
    let module_path = module.path.as_ref().map(|p| ctx.intern_path(p));
    let items = module.items.iter().map(|i| ctx.lower_item(i)).collect();
    let arena = core::mem::take(&mut ctx.arena);
    let hir = HirModule {
        span: module.span,
        arena,
        inner_attrs,
        module_path,
        items,
    };
    // Run the resolve pass to populate module-level scope + path references.
    let mut hir = hir;
    resolve_module(&mut hir);
    (hir, ctx.interner, ctx.diagnostics)
}

/// Walk a HIR module and resolve path references that match a top-level item.
///
/// § SCOPE (T3.3) : single-file, single-segment resolution. Multi-segment paths
/// (`foo::bar::Baz`) only resolve when the first segment matches a top-level item ;
/// deeper resolution is T3.4 work when module-tree traversal lands.
pub fn resolve_module(module: &mut HirModule) {
    let scope = build_module_scope(module);
    // Fill in def references on HIR nodes using `scope`.
    for item in &mut module.items {
        resolve_item_refs(item, &scope);
    }
}

fn build_module_scope(module: &HirModule) -> ScopeMap {
    let mut scope = ScopeMap::new();
    for item in &module.items {
        if let (Some(def), Some(name)) = (item.def_id(), item.name()) {
            scope.insert_module(name, def);
        }
        if let HirItem::Enum(e) = item {
            for variant in &e.variants {
                scope.insert_module(variant.name, variant.def);
            }
        }
    }
    scope
}

fn resolve_item_refs(item: &mut HirItem, scope: &ScopeMap) {
    match item {
        HirItem::Fn(f) => resolve_fn_refs(f, scope),
        HirItem::Struct(s) => {
            resolve_struct_body(&mut s.body, scope);
        }
        HirItem::Enum(e) => {
            for v in &mut e.variants {
                resolve_struct_body(&mut v.body, scope);
            }
        }
        HirItem::Interface(i) => {
            for f in &mut i.fns {
                resolve_fn_refs(f, scope);
            }
        }
        HirItem::Impl(i) => {
            resolve_type(&mut i.self_ty, scope);
            if let Some(t) = &mut i.trait_ {
                resolve_type(t, scope);
            }
            for f in &mut i.fns {
                resolve_fn_refs(f, scope);
            }
        }
        HirItem::Effect(e) => {
            for f in &mut e.ops {
                resolve_fn_refs(f, scope);
            }
        }
        HirItem::Handler(h) => {
            resolve_type(&mut h.handled_effect, scope);
            for f in &mut h.ops {
                resolve_fn_refs(f, scope);
            }
        }
        HirItem::TypeAlias(t) => resolve_type(&mut t.ty, scope),
        HirItem::Use(u) => {
            for b in &mut u.bindings {
                if let Some(&first) = b.path.first() {
                    if b.path.len() == 1 {
                        b.def = scope.lookup(first);
                    }
                }
            }
        }
        HirItem::Const(c) => {
            resolve_type(&mut c.ty, scope);
            resolve_expr(&mut c.value, scope);
        }
        HirItem::Module(m) => {
            if let Some(is) = m.items.as_mut() {
                for sub in is {
                    resolve_item_refs(sub, scope);
                }
            }
        }
    }
}

fn resolve_fn_refs(f: &mut HirFn, scope: &ScopeMap) {
    for p in &mut f.params {
        resolve_type(&mut p.ty, scope);
    }
    if let Some(rt) = &mut f.return_ty {
        resolve_type(rt, scope);
    }
    if let Some(body) = &mut f.body {
        resolve_block(body, scope);
    }
}

fn resolve_block(b: &mut HirBlock, scope: &ScopeMap) {
    for s in &mut b.stmts {
        match &mut s.kind {
            HirStmtKind::Let { ty, value, .. } => {
                if let Some(t) = ty {
                    resolve_type(t, scope);
                }
                if let Some(v) = value {
                    resolve_expr(v, scope);
                }
            }
            HirStmtKind::Expr(e) => resolve_expr(e, scope),
            HirStmtKind::Item(_i) => {
                // Nested-block items are resolved at their own scope level — deferred to T3.4.
            }
        }
    }
    if let Some(t) = &mut b.trailing {
        resolve_expr(t, scope);
    }
}

fn resolve_expr(e: &mut HirExpr, scope: &ScopeMap) {
    match &mut e.kind {
        HirExprKind::Path { segments, def } => {
            if let Some(&first) = segments.first() {
                if segments.len() == 1 {
                    *def = scope.lookup(first);
                }
            }
        }
        HirExprKind::Call { callee, args, .. } => {
            resolve_expr(callee, scope);
            for a in args {
                match a {
                    HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => {
                        resolve_expr(e, scope);
                    }
                }
            }
        }
        HirExprKind::Field { obj, .. } => resolve_expr(obj, scope),
        HirExprKind::Index { obj, index } => {
            resolve_expr(obj, scope);
            resolve_expr(index, scope);
        }
        HirExprKind::Binary { lhs, rhs, .. }
        | HirExprKind::Assign { lhs, rhs, .. }
        | HirExprKind::Pipeline { lhs, rhs }
        | HirExprKind::Compound { lhs, rhs, .. } => {
            resolve_expr(lhs, scope);
            resolve_expr(rhs, scope);
        }
        HirExprKind::Unary { operand, .. } => resolve_expr(operand, scope),
        HirExprKind::Block(b)
        | HirExprKind::Region { body: b, .. }
        | HirExprKind::With { body: b, .. } => resolve_block(b, scope),
        HirExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            resolve_expr(cond, scope);
            resolve_block(then_branch, scope);
            if let Some(e) = else_branch {
                resolve_expr(e, scope);
            }
        }
        HirExprKind::Match { scrutinee, arms } => {
            resolve_expr(scrutinee, scope);
            for a in arms {
                if let Some(g) = &mut a.guard {
                    resolve_expr(g, scope);
                }
                resolve_expr(&mut a.body, scope);
            }
        }
        HirExprKind::For { iter, body, .. } => {
            resolve_expr(iter, scope);
            resolve_block(body, scope);
        }
        HirExprKind::While { cond, body } => {
            resolve_expr(cond, scope);
            resolve_block(body, scope);
        }
        HirExprKind::Loop { body } => resolve_block(body, scope),
        HirExprKind::Return { value } | HirExprKind::Break { value, .. } => {
            if let Some(v) = value {
                resolve_expr(v, scope);
            }
        }
        HirExprKind::Cast { expr, ty } => {
            resolve_expr(expr, scope);
            resolve_type(ty, scope);
        }
        HirExprKind::Range { lo, hi, .. } => {
            if let Some(e) = lo {
                resolve_expr(e, scope);
            }
            if let Some(e) = hi {
                resolve_expr(e, scope);
            }
        }
        HirExprKind::TryDefault { expr, default } => {
            resolve_expr(expr, scope);
            resolve_expr(default, scope);
        }
        HirExprKind::Try { expr } | HirExprKind::Run { expr } | HirExprKind::Paren(expr) => {
            resolve_expr(expr, scope);
        }
        HirExprKind::Perform { path, def, args } => {
            if let Some(&first) = path.first() {
                if path.len() == 1 {
                    *def = scope.lookup(first);
                }
            }
            for a in args {
                match a {
                    HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => {
                        resolve_expr(e, scope);
                    }
                }
            }
        }
        HirExprKind::Tuple(es) => {
            for e in es {
                resolve_expr(e, scope);
            }
        }
        HirExprKind::Array(a) => match a {
            HirArrayExpr::List(es) => {
                for e in es {
                    resolve_expr(e, scope);
                }
            }
            HirArrayExpr::Repeat { elem, len } => {
                resolve_expr(elem, scope);
                resolve_expr(len, scope);
            }
        },
        HirExprKind::Struct {
            path,
            def,
            fields,
            spread,
        } => {
            if let Some(&first) = path.first() {
                if path.len() == 1 {
                    *def = scope.lookup(first);
                }
            }
            for f in fields {
                if let Some(v) = &mut f.value {
                    resolve_expr(v, scope);
                }
            }
            if let Some(s) = spread {
                resolve_expr(s, scope);
            }
        }
        HirExprKind::Lambda { body, .. } => resolve_expr(body, scope),
        HirExprKind::Literal(_)
        | HirExprKind::Continue { .. }
        | HirExprKind::SectionRef { .. }
        | HirExprKind::Error => {}
    }
}

fn resolve_type(t: &mut HirType, scope: &ScopeMap) {
    match &mut t.kind {
        HirTypeKind::Path {
            path,
            def,
            type_args,
        } => {
            if let Some(&first) = path.first() {
                if path.len() == 1 {
                    *def = scope.lookup(first);
                }
            }
            for a in type_args {
                resolve_type(a, scope);
            }
        }
        HirTypeKind::Tuple { elems } => {
            for e in elems {
                resolve_type(e, scope);
            }
        }
        HirTypeKind::Array { elem, len } => {
            resolve_type(elem, scope);
            resolve_expr(len, scope);
        }
        HirTypeKind::Slice { elem } => resolve_type(elem, scope),
        HirTypeKind::Reference { inner, .. } | HirTypeKind::Capability { inner, .. } => {
            resolve_type(inner, scope);
        }
        HirTypeKind::Function {
            params,
            return_ty,
            effect_row: _,
        } => {
            for p in params {
                resolve_type(p, scope);
            }
            resolve_type(return_ty, scope);
        }
        HirTypeKind::Refined { base, kind } => {
            resolve_type(base, scope);
            match kind {
                HirRefinementKind::Tag { .. } => {}
                HirRefinementKind::Predicate { predicate, .. } => resolve_expr(predicate, scope),
                HirRefinementKind::Lipschitz { bound } => resolve_expr(bound, scope),
            }
        }
        HirTypeKind::Infer | HirTypeKind::Error => {}
    }
}

fn resolve_struct_body(b: &mut HirStructBody, scope: &ScopeMap) {
    match b {
        HirStructBody::Unit => {}
        HirStructBody::Tuple(fs) | HirStructBody::Named(fs) => {
            for f in fs {
                resolve_type(&mut f.ty, scope);
            }
        }
    }
}

/// Hide the empty Span+SourceFile re-export so the `Span` import doesn't show as unused.
#[allow(dead_code)]
const fn _span_referenced(_: Span) {}

#[cfg(test)]
mod tests {
    use super::lower_module;
    use cssl_ast::{SourceFile, SourceId, Surface};

    fn lex_parse_lower(src: &str) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (m, _bag) = cssl_parse::parse(&f, &toks);
        let (_hir, _interner, _diag) = lower_module(&f, &m);
    }

    #[test]
    fn empty_module_lowers() {
        lex_parse_lower("");
    }

    #[test]
    fn single_fn_lowers() {
        lex_parse_lower("fn f(x : i32) -> i32 { x + 1 }");
    }

    #[test]
    fn struct_and_enum_lower() {
        lex_parse_lower("struct Point { x : f32, y : f32 } enum Opt<T> { Some(T), None }");
    }

    #[test]
    fn use_and_const_lower() {
        lex_parse_lower("use foo::bar as baz ; const FOO : i32 = 42 ;");
    }

    #[test]
    fn fn_with_attrs_and_effects() {
        lex_parse_lower(
            r"
            @differentiable
            @lipschitz(k = 1.0)
            fn sdf(p : vec3, r : f32) -> f32 / {GPU, NoAlloc} {
                p - r
            }
            ",
        );
    }

    // § T11-D39 : turbofish type_args survive through HIR lowering.

    /// Run lex → parse → lower and return the HIR module.
    fn lex_parse_lower_get_hir(src: &str) -> crate::item::HirModule {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (m, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, _interner, _diag) = lower_module(&f, &m);
        hir
    }

    /// Walk the HIR module and return the type_args of the first Call
    /// expression found inside any fn body (depth-first).
    fn first_call_type_args(hir: &crate::item::HirModule) -> Option<Vec<crate::ty::HirType>> {
        fn walk_expr(e: &crate::expr::HirExpr) -> Option<Vec<crate::ty::HirType>> {
            use crate::expr::HirExprKind;
            match &e.kind {
                HirExprKind::Call { type_args, .. } => Some(type_args.clone()),
                HirExprKind::Binary { lhs, rhs, .. } => walk_expr(lhs).or_else(|| walk_expr(rhs)),
                HirExprKind::Block(b) => {
                    for s in &b.stmts {
                        if let crate::stmt::HirStmtKind::Expr(e) = &s.kind {
                            if let Some(v) = walk_expr(e) {
                                return Some(v);
                            }
                        }
                    }
                    b.trailing.as_ref().and_then(|t| walk_expr(t))
                }
                _ => None,
            }
        }
        for item in &hir.items {
            if let crate::item::HirItem::Fn(f) = item {
                if let Some(body) = &f.body {
                    for s in &body.stmts {
                        if let crate::stmt::HirStmtKind::Expr(e) = &s.kind {
                            if let Some(v) = walk_expr(e) {
                                return Some(v);
                            }
                        }
                    }
                    if let Some(t) = &body.trailing {
                        if let Some(v) = walk_expr(t) {
                            return Some(v);
                        }
                    }
                }
            }
        }
        None
    }

    #[test]
    fn hir_call_type_args_empty_when_no_turbofish() {
        let hir = lex_parse_lower_get_hir("fn wrapper() -> i32 { f(5) }");
        let ta = first_call_type_args(&hir).expect("call found");
        assert!(ta.is_empty(), "plain call must have empty type_args in HIR");
    }

    #[test]
    fn hir_call_type_args_populated_from_turbofish() {
        let hir = lex_parse_lower_get_hir("fn wrapper() -> i32 { id::<i32>(5) }");
        let ta = first_call_type_args(&hir).expect("call found");
        assert_eq!(ta.len(), 1, "turbofish produces 1 HIR type-arg");
    }

    #[test]
    fn hir_call_type_args_multi_turbofish() {
        let hir = lex_parse_lower_get_hir("fn wrapper() -> i32 { pair::<i32, f32>(1, 2.0) }");
        let ta = first_call_type_args(&hir).expect("call found");
        assert_eq!(ta.len(), 2, "two turbofish args produce two HIR type-args");
    }

    // ─ § T11-D287 (W-E5-4) attribution-stability integration tests ───────

    /// § same-source-stable-fingerprint — lowering the same source twice yields
    /// the same `module_fingerprint`. Guards against any HashMap-iteration leak
    /// affecting `DefId`-attribution.
    #[test]
    fn lower_same_source_stable_fingerprint() {
        let src = "fn alpha(x : i32) -> i32 { x } \
                   struct Beta { y : f32 } \
                   enum Gamma { One, Two(i32) }";
        let h1 = lex_parse_lower_get_hir(src);
        let h2 = lex_parse_lower_get_hir(src);
        assert_eq!(
            h1.arena.attribution().module_fingerprint(),
            h2.arena.attribution().module_fingerprint(),
            "same source must produce identical attribution-fingerprint"
        );
    }

    /// § attribution-records-source-position — every item DefId carries an
    /// `AttributionKey` whose `span_start` matches the item's source span.
    #[test]
    fn attribution_records_source_position() {
        let src = "fn first() -> i32 { 1 } fn second() -> i32 { 2 }";
        let hir = lex_parse_lower_get_hir(src);
        for item in &hir.items {
            if let crate::item::HirItem::Fn(f) = item {
                let key = hir
                    .arena
                    .attribution()
                    .get(f.def)
                    .expect("attribution recorded for fn");
                assert_eq!(
                    key.span_start, f.span.start,
                    "attribution span_start must match item span"
                );
                assert_eq!(key.kind, crate::arena::DefKind::Fn);
            }
        }
    }

    /// § cross-run-determinism-real-source — lowering two byte-identical source
    /// strings produces the same DefId → AttributionKey mapping for every
    /// definition (not just the aggregate fingerprint).
    #[test]
    fn cross_run_determinism_real_source() {
        let src = "fn a() {} fn b() {} struct C {}";
        let h1 = lex_parse_lower_get_hir(src);
        let h2 = lex_parse_lower_get_hir(src);

        let attr1: Vec<_> = h1.arena.attribution().iter().collect();
        let attr2: Vec<_> = h2.arena.attribution().iter().collect();
        assert_eq!(attr1, attr2, "DefId attribution must match across runs");
    }

    /// § different-source-different-fingerprint — modifying source SHAPE
    /// (kind, position, or identifier-length) changes the fingerprint.
    ///
    /// § INTENTIONAL CONTRACT : the fingerprint is byte-position-based, NOT
    /// name-text-based — two items at the same span with same-length names
    /// produce the same fingerprint by design. That is acceptable for the
    /// fixed-point gate because the gate ALSO compares the rest of the HIR
    /// blob, which carries the resolved name-spans for downstream emission.
    /// This test guards SHAPE-CHANGE detection : if the user adds a new fn,
    /// changes a struct to an enum, or extends an identifier, the fingerprint
    /// must change.
    #[test]
    fn different_source_different_fingerprint() {
        // Different ITEM COUNT → different fingerprint (extra fn appended).
        let src1 = "fn foo() {}";
        let src2 = "fn foo() {} fn extra() {}";
        let h1 = lex_parse_lower_get_hir(src1);
        let h2 = lex_parse_lower_get_hir(src2);
        assert_ne!(
            h1.arena.attribution().module_fingerprint(),
            h2.arena.attribution().module_fingerprint(),
            "extra item must produce different fingerprint"
        );

        // Different ITEM KIND at same position → different fingerprint
        // (struct vs fn) — guards against a kind-misclassification regression.
        let src_fn = "fn x() -> i32 { 0 }";
        let src_struct = "struct x { y : i32 }";
        let hf = lex_parse_lower_get_hir(src_fn);
        let hs = lex_parse_lower_get_hir(src_struct);
        assert_ne!(
            hf.arena.attribution().module_fingerprint(),
            hs.arena.attribution().module_fingerprint(),
            "different item-kind must produce different fingerprint"
        );

        // Different IDENTIFIER LENGTH at same position → different fingerprint.
        let src_short = "fn ab() {}";
        let src_long = "fn abcdef() {}";
        let hsh = lex_parse_lower_get_hir(src_short);
        let hln = lex_parse_lower_get_hir(src_long);
        assert_ne!(
            hsh.arena.attribution().module_fingerprint(),
            hln.arena.attribution().module_fingerprint(),
            "different identifier-length must produce different fingerprint"
        );
    }

    /// § sorted-canonical-view-matches-source-order — items lowered in source
    /// order yield a sorted-canonical view that matches the allocation iter.
    #[test]
    fn sorted_canonical_view_matches_source_order() {
        let src = "fn aaa() {} fn bbb() {} fn ccc() {}";
        let hir = lex_parse_lower_get_hir(src);
        let alloc_order: Vec<_> = hir.arena.attribution().iter().collect();
        let canonical = hir.arena.attribution().sorted_by_source_position();
        assert_eq!(
            alloc_order, canonical,
            "in-source-order lowering matches canonical view"
        );
    }
}
