//! Refinement-obligation generator (T3.4-phase-2 slice feeding T9 SMT).
//!
//! § SPEC : `specs/03_TYPES.csl` § REFINEMENT-TYPES + `specs/20_SMT.csl`.
//!
//! § WHAT THIS PRODUCES
//!   Walks a `HirModule` and collects every `{v : T | P(v)}` predicate (from
//!   `HirTypeKind::Refined { kind: Predicate }`) plus `T'tag` sugar (lexical tag
//!   references — left for T9 to resolve via a refinement dictionary) into a list
//!   of [`RefinementObligation`]s. Each obligation is a structured query ready for
//!   SMT-LIB emission by `cssl-smt`.
//!
//! § NOT-YET
//!   - `T'tag` tag-name resolution via a refinement dictionary (needs project-wide
//!     context ; T9 consumer).
//!   - Lipschitz bound (`SDF'L<k>`) obligation generation.
//!   - Obligation simplification / caching (per-obligation-hash dedup).
//!   - Full HIR expression → SMT term translation (phase-2.5 work).

use std::collections::BTreeMap;

use cssl_ast::Span;

use crate::arena::{DefId, HirId};
use crate::expr::{HirExpr, HirExprKind};
use crate::item::{HirFn, HirItem, HirModule};
use crate::symbol::Symbol;
use crate::ty::{HirRefinementKind, HirType, HirTypeKind};

/// Kind of refinement obligation. Stage-0 recognizes two shapes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObligationKind {
    /// `{v : T | P(v)}` predicate-form — SMT query is `∀v : T. P(v) → ⊥` (check
    /// unsatisfiability to validate the refinement).
    Predicate {
        /// The binder symbol (`v` in `{v : T | P(v)}`).
        binder: Symbol,
        /// Pretty-printed predicate expression at stage-0 (actual SMT translation
        /// is T9 work ; we record textual form for now).
        predicate_text: String,
    },
    /// `T'tag` sugar form — the `tag` identifier refers to a refinement dictionary
    /// entry that T9 resolves. Stage-0 just records the tag name.
    Tag { name: Symbol },
    /// `SDF'L<k>` Lipschitz bound — SMT query encodes "fn has Lipschitz constant ≤ k".
    Lipschitz {
        /// Textual bound expression.
        bound_text: String,
    },
}

/// One refinement obligation : a specific refinement-type occurrence in the HIR
/// that must be discharged by SMT before code generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefinementObligation {
    /// Unique id for this obligation (monotonic across a module).
    pub id: ObligationId,
    /// HIR node where the refinement appears.
    pub origin: HirId,
    /// Source span for diagnostics.
    pub span: Span,
    /// The `DefId` of the enclosing fn / const / item (for error context).
    pub enclosing_def: Option<DefId>,
    /// Kind of obligation.
    pub kind: ObligationKind,
    /// Base type the refinement is over (pretty-printed at stage-0).
    pub base_type_text: String,
}

/// Monotonic obligation identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct ObligationId(pub u32);

/// Collection of obligations produced by a single walk.
#[derive(Debug, Default, Clone)]
pub struct ObligationBag {
    obligations: BTreeMap<u32, RefinementObligation>,
    next_id: u32,
}

impl ObligationBag {
    /// Empty bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an obligation ; returns its assigned id.
    pub fn push(&mut self, mut o: RefinementObligation) -> ObligationId {
        let id = ObligationId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        o.id = id;
        self.obligations.insert(id.0, o);
        id
    }

    /// Lookup by id.
    #[must_use]
    pub fn get(&self, id: ObligationId) -> Option<&RefinementObligation> {
        self.obligations.get(&id.0)
    }

    /// Iterate in id-insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &RefinementObligation> {
        self.obligations.values()
    }

    /// Number of obligations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.obligations.len()
    }

    /// `true` iff empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.obligations.is_empty()
    }
}

/// Walk a HIR module and collect every refinement obligation.
#[must_use]
pub fn collect_refinement_obligations(
    module: &HirModule,
    interner: &crate::symbol::Interner,
) -> ObligationBag {
    let mut ctx = ObligationCtx {
        bag: ObligationBag::new(),
        interner,
        enclosing: None,
    };
    for item in &module.items {
        ctx.walk_item(item);
    }
    ctx.bag
}

struct ObligationCtx<'a> {
    bag: ObligationBag,
    interner: &'a crate::symbol::Interner,
    enclosing: Option<DefId>,
}

impl<'a> ObligationCtx<'a> {
    fn walk_item(&mut self, item: &HirItem) {
        match item {
            HirItem::Fn(f) => self.walk_fn(f),
            HirItem::Const(c) => {
                let prev = self.enclosing.replace(c.def);
                self.walk_type(&c.ty);
                self.walk_expr(&c.value);
                self.enclosing = prev;
            }
            HirItem::Impl(i) => {
                for f in &i.fns {
                    self.walk_fn(f);
                }
            }
            HirItem::Interface(i) => {
                for f in &i.fns {
                    self.walk_fn(f);
                }
            }
            HirItem::Effect(e) => {
                for f in &e.ops {
                    self.walk_fn(f);
                }
            }
            HirItem::Handler(h) => {
                for f in &h.ops {
                    self.walk_fn(f);
                }
            }
            HirItem::Module(m) => {
                if let Some(sub) = &m.items {
                    for s in sub {
                        self.walk_item(s);
                    }
                }
            }
            _ => {}
        }
    }

    fn walk_fn(&mut self, f: &HirFn) {
        let prev = self.enclosing.replace(f.def);
        for p in &f.params {
            self.walk_type(&p.ty);
        }
        if let Some(rt) = &f.return_ty {
            self.walk_type(rt);
        }
        if let Some(body) = &f.body {
            for stmt in &body.stmts {
                match &stmt.kind {
                    crate::stmt::HirStmtKind::Let { ty, value, .. } => {
                        if let Some(t) = ty {
                            self.walk_type(t);
                        }
                        if let Some(v) = value {
                            self.walk_expr(v);
                        }
                    }
                    crate::stmt::HirStmtKind::Expr(e) => self.walk_expr(e),
                    crate::stmt::HirStmtKind::Item(i) => self.walk_item(i),
                }
            }
            if let Some(t) = &body.trailing {
                self.walk_expr(t);
            }
        }
        self.enclosing = prev;
    }

    fn walk_type(&mut self, t: &HirType) {
        if let HirTypeKind::Refined { base, kind } = &t.kind {
            let base_text = self.pretty_type(base);
            let obligation_kind = match kind {
                HirRefinementKind::Predicate { binder, predicate } => {
                    let text = self.pretty_expr(predicate);
                    ObligationKind::Predicate {
                        binder: *binder,
                        predicate_text: text,
                    }
                }
                HirRefinementKind::Tag { name } => ObligationKind::Tag { name: *name },
                HirRefinementKind::Lipschitz { bound } => {
                    let text = self.pretty_expr(bound);
                    ObligationKind::Lipschitz { bound_text: text }
                }
            };
            self.bag.push(RefinementObligation {
                id: ObligationId(u32::MAX), // reassigned by push
                origin: t.id,
                span: t.span,
                enclosing_def: self.enclosing,
                kind: obligation_kind,
                base_type_text: base_text,
            });
            // Recurse into the base (nested refinements) too.
            self.walk_type(base);
        } else {
            // Walk structural children.
            match &t.kind {
                HirTypeKind::Tuple { elems } => {
                    for e in elems {
                        self.walk_type(e);
                    }
                }
                HirTypeKind::Array { elem, .. } | HirTypeKind::Slice { elem } => {
                    self.walk_type(elem);
                }
                HirTypeKind::Reference { inner, .. } | HirTypeKind::Capability { inner, .. } => {
                    self.walk_type(inner);
                }
                HirTypeKind::Function {
                    params, return_ty, ..
                } => {
                    for p in params {
                        self.walk_type(p);
                    }
                    self.walk_type(return_ty);
                }
                HirTypeKind::Path { type_args, .. } => {
                    for a in type_args {
                        self.walk_type(a);
                    }
                }
                _ => {}
            }
        }
    }

    fn walk_expr(&mut self, e: &HirExpr) {
        // Stage-0 only walks expressions that can contain types-that-need-refinement
        // (Cast, Lambda.return_ty, Struct constructor). Full expr-walk coverage
        // lands with T3.4-phase-2.5 ; here we keep the walk focused.
        match &e.kind {
            HirExprKind::Cast { expr, ty } => {
                self.walk_type(ty);
                self.walk_expr(expr);
            }
            HirExprKind::Lambda {
                return_ty, body, ..
            } => {
                if let Some(t) = return_ty {
                    self.walk_type(t);
                }
                self.walk_expr(body);
            }
            HirExprKind::Block(b) => {
                for stmt in &b.stmts {
                    match &stmt.kind {
                        crate::stmt::HirStmtKind::Let { ty, value, .. } => {
                            if let Some(t) = ty {
                                self.walk_type(t);
                            }
                            if let Some(v) = value {
                                self.walk_expr(v);
                            }
                        }
                        crate::stmt::HirStmtKind::Expr(ex) => self.walk_expr(ex),
                        crate::stmt::HirStmtKind::Item(i) => self.walk_item(i),
                    }
                }
                if let Some(t) = &b.trailing {
                    self.walk_expr(t);
                }
            }
            HirExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.walk_expr(cond);
                for stmt in &then_branch.stmts {
                    if let crate::stmt::HirStmtKind::Expr(ex) = &stmt.kind {
                        self.walk_expr(ex);
                    }
                }
                if let Some(e) = else_branch {
                    self.walk_expr(e);
                }
            }
            _ => {}
        }
    }

    fn pretty_type(&self, t: &HirType) -> String {
        match &t.kind {
            HirTypeKind::Path { path, .. } => path
                .iter()
                .map(|s| self.interner.resolve(*s))
                .collect::<Vec<_>>()
                .join("."),
            HirTypeKind::Tuple { elems } => {
                format!(
                    "({})",
                    elems
                        .iter()
                        .map(|e| self.pretty_type(e))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            HirTypeKind::Refined { base, .. } => self.pretty_type(base),
            HirTypeKind::Reference { mutable, inner } => {
                let prefix = if *mutable { "&mut " } else { "&" };
                format!("{}{}", prefix, self.pretty_type(inner))
            }
            HirTypeKind::Capability { inner, .. } => self.pretty_type(inner),
            HirTypeKind::Infer => "_".into(),
            HirTypeKind::Error => "!error".into(),
            _ => "<opaque>".into(),
        }
    }

    #[allow(clippy::unused_self)]
    fn pretty_expr(&self, e: &HirExpr) -> String {
        // Stage-0 : pretty-print via debug form. Full expression pretty-print
        // requires walking HirExprKind with interner lookups ; lands at T9-phase-2.
        format!("{:?}", e.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        collect_refinement_obligations, ObligationBag, ObligationId, ObligationKind,
        RefinementObligation,
    };
    use crate::arena::HirId;
    use cssl_ast::{SourceFile, SourceId, Span, Surface};

    fn prep(src: &str) -> (crate::HirModule, crate::Interner) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _lower_bag) = crate::lower_module(&f, &cst);
        (hir, interner)
    }

    #[test]
    fn empty_module_yields_no_obligations() {
        let (hir, interner) = prep("");
        let bag = collect_refinement_obligations(&hir, &interner);
        assert!(bag.is_empty());
    }

    #[test]
    fn bag_push_assigns_monotonic_ids() {
        let interner = crate::Interner::new();
        let mut bag = ObligationBag::new();
        let o = RefinementObligation {
            id: ObligationId(u32::MAX),
            origin: HirId::DUMMY,
            span: Span::new(SourceId::first(), 0, 1),
            enclosing_def: None,
            kind: ObligationKind::Tag {
                name: interner.intern("pos"),
            },
            base_type_text: "f32".into(),
        };
        let id = bag.push(o.clone());
        assert_eq!(id, ObligationId(0));
        let id2 = bag.push(o);
        assert_eq!(id2, ObligationId(1));
        assert_eq!(bag.len(), 2);
    }

    #[test]
    fn predicate_refinement_collected() {
        // `{v : f32 | v > 0}` predicate form.
        let src = r"fn f(x : { v : f32 | v > 0 }) -> f32 { x }";
        let (hir, interner) = prep(src);
        let bag = collect_refinement_obligations(&hir, &interner);
        assert!(bag.len() >= 1);
        let any_predicate = bag
            .iter()
            .any(|o| matches!(o.kind, ObligationKind::Predicate { .. }));
        assert!(any_predicate);
    }

    #[test]
    fn tag_refinement_collected() {
        // `T'tag` sugar — `f32'pos` is lexer-decomposed to Ident + Apostrophe + Ident
        // after T2-D8 ; the parser's refinement-tag path (T3.2) wraps it as
        // `HirRefinementKind::Tag`.
        let src = r"fn f(x : f32'pos) -> f32 { x }";
        let (hir, interner) = prep(src);
        let bag = collect_refinement_obligations(&hir, &interner);
        assert!(bag.len() >= 1);
        let any_tag = bag
            .iter()
            .any(|o| matches!(o.kind, ObligationKind::Tag { .. }));
        assert!(any_tag);
    }

    #[test]
    fn obligation_records_enclosing_def_for_fn_sig() {
        let src = r"fn f(x : { v : f32 | v > 0 }) -> f32 { x }";
        let (hir, interner) = prep(src);
        let bag = collect_refinement_obligations(&hir, &interner);
        let o = bag
            .iter()
            .find(|o| matches!(o.kind, ObligationKind::Predicate { .. }))
            .unwrap();
        assert!(o.enclosing_def.is_some());
    }

    #[test]
    fn obligation_bag_iter_visits_every() {
        let src = r"
            fn a(x : { v : f32 | v > 0 }) -> f32 { x }
            fn b(x : { v : f32 | v > 0 }) -> f32 { x }
        ";
        let (hir, interner) = prep(src);
        let bag = collect_refinement_obligations(&hir, &interner);
        let count = bag.iter().count();
        assert_eq!(count, bag.len());
        assert!(bag.len() >= 2);
    }

    #[test]
    fn obligation_bag_get_roundtrips() {
        let interner = crate::Interner::new();
        let mut bag = ObligationBag::new();
        let o = RefinementObligation {
            id: ObligationId(u32::MAX),
            origin: HirId::DUMMY,
            span: Span::new(SourceId::first(), 0, 1),
            enclosing_def: None,
            kind: ObligationKind::Tag {
                name: interner.intern("pos"),
            },
            base_type_text: "f32".into(),
        };
        let id = bag.push(o);
        assert!(bag.get(id).is_some());
    }
}
