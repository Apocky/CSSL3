//! CSSLv3 staging — `@staged` specializer + `#run` comptime evaluator (F4).
//!
//! § SPEC : `specs/06_STAGING.csl` + `specs/19_FUTAMURA3.csl`.
//!
//! § SCOPE (T8-phase-1 / this commit)
//!   - [`StageArg`] + [`StageArgKind`] : classification of which fn-arguments are
//!     known at compile-time vs runtime.
//!   - [`StagedDecl`] : extracted metadata for every `@staged` fn in a HirModule.
//!   - [`collect_staged_fns`] : walk HIR + return all `@staged` fns.
//!   - [`RunMarker`] : `#run expr` site identification (maps `HirExprKind::Run` →
//!     a comptime-eval queue).
//!   - [`Specializer`] skeleton : per-call-site specialization manifest.
//!
//! § T8-phase-2 DEFERRED
//!   - Actual specialization walk (clone fn + const-propagate stage-args).
//!   - Native comptime-eval (compile-native ; avoid Zig 20× interpreter cost per R14).
//!   - `@type_info` / `@fn_info` / `@module_info` reflection API.
//!   - Transform-dialect pass-schedule emission (`specs/15` § TRANSFORM-DIALECT).
//!   - Futamura-P1 baseline + P2 specializer-reference + P3 self-bootstrap (separate crate `cssl-futamura`).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::semicolon_if_nothing_returned)]
#![allow(clippy::redundant_clone)]

use thiserror::Error;

use cssl_hir::{HirAttr, HirFn, HirItem, HirModule, Interner, Symbol};

/// Classification of a function argument : comptime-known / runtime-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StageArgKind {
    /// Argument is known at compile-time (const-literal, const-fn result, or
    /// already-specialized generic).
    CompTime,
    /// Argument is runtime-only.
    Runtime,
    /// Argument is stage-polymorphic (implementation-specific, currently treated as runtime).
    Polymorphic,
}

/// Stage classification for a single fn argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageArg {
    /// Parameter index (0-based) in the fn signature.
    pub index: usize,
    /// Parameter name (from the HIR pattern, if available).
    pub name: Option<Symbol>,
    /// Whether this arg is comptime-known at the specialization-site.
    pub kind: StageArgKind,
}

/// Declaration metadata for a `@staged` fn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedDecl {
    /// Fn name symbol.
    pub name: Symbol,
    /// Fn `DefId`.
    pub def: cssl_hir::DefId,
    /// Per-argument stage classification (all runtime by default).
    pub args: Vec<StageArg>,
    /// Number of `#run` expressions inside the body — a coarse comptime-eval demand signal.
    pub run_sites: u32,
}

impl StagedDecl {
    /// Build a decl from a fn + interner. Returns `None` if the fn is not `@staged`.
    #[must_use]
    pub fn from_fn(f: &HirFn, interner: &Interner) -> Option<Self> {
        if !f.attrs.iter().any(|a| attr_matches(a, interner, "staged")) {
            return None;
        }
        let args: Vec<StageArg> = f
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| StageArg {
                index: i,
                name: binding_name(&p.pat),
                kind: StageArgKind::Runtime,
            })
            .collect();
        let run_sites = count_run_sites(f);
        Some(Self {
            name: f.name,
            def: f.def,
            args,
            run_sites,
        })
    }
}

fn attr_matches(attr: &HirAttr, interner: &Interner, expected: &str) -> bool {
    if attr.path.len() != 1 {
        return false;
    }
    interner.resolve(attr.path[0]) == expected
}

fn binding_name(pat: &cssl_hir::HirPattern) -> Option<Symbol> {
    if let cssl_hir::HirPatternKind::Binding { name, .. } = &pat.kind {
        Some(*name)
    } else {
        None
    }
}

fn count_run_sites(f: &HirFn) -> u32 {
    let mut count = 0u32;
    if let Some(body) = &f.body {
        count_block(body, &mut count);
    }
    count
}

fn count_block(b: &cssl_hir::HirBlock, n: &mut u32) {
    for stmt in &b.stmts {
        match &stmt.kind {
            cssl_hir::HirStmtKind::Let { value: Some(v), .. } | cssl_hir::HirStmtKind::Expr(v) => {
                count_expr(v, n)
            }
            cssl_hir::HirStmtKind::Let { value: None, .. } | cssl_hir::HirStmtKind::Item(_) => {}
        }
    }
    if let Some(t) = &b.trailing {
        count_expr(t, n);
    }
}

fn count_expr(e: &cssl_hir::HirExpr, n: &mut u32) {
    use cssl_hir::HirExprKind;
    match &e.kind {
        HirExprKind::Run { expr } => {
            *n += 1;
            count_expr(expr, n);
        }
        HirExprKind::Block(b) => count_block(b, n),
        HirExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            count_expr(cond, n);
            count_block(then_branch, n);
            if let Some(e) = else_branch {
                count_expr(e, n);
            }
        }
        HirExprKind::For { iter, body, .. } => {
            count_expr(iter, n);
            count_block(body, n);
        }
        HirExprKind::While { cond, body } => {
            count_expr(cond, n);
            count_block(body, n);
        }
        HirExprKind::Loop { body } => count_block(body, n),
        HirExprKind::Match { scrutinee, arms } => {
            count_expr(scrutinee, n);
            for a in arms {
                if let Some(g) = &a.guard {
                    count_expr(g, n);
                }
                count_expr(&a.body, n);
            }
        }
        HirExprKind::Call { callee, args } => {
            count_expr(callee, n);
            for a in args {
                match a {
                    cssl_hir::HirCallArg::Positional(e)
                    | cssl_hir::HirCallArg::Named { value: e, .. } => count_expr(e, n),
                }
            }
        }
        HirExprKind::Binary { lhs, rhs, .. }
        | HirExprKind::Assign { lhs, rhs, .. }
        | HirExprKind::Pipeline { lhs, rhs }
        | HirExprKind::Compound { lhs, rhs, .. } => {
            count_expr(lhs, n);
            count_expr(rhs, n);
        }
        HirExprKind::Unary { operand, .. }
        | HirExprKind::Field { obj: operand, .. }
        | HirExprKind::Try { expr: operand }
        | HirExprKind::Paren(operand)
        | HirExprKind::Cast { expr: operand, .. } => count_expr(operand, n),
        HirExprKind::Index { obj, index } => {
            count_expr(obj, n);
            count_expr(index, n);
        }
        HirExprKind::Return { value } | HirExprKind::Break { value, .. } => {
            if let Some(v) = value {
                count_expr(v, n);
            }
        }
        HirExprKind::Tuple(es) => {
            for e in es {
                count_expr(e, n);
            }
        }
        HirExprKind::Lambda { body, .. } => count_expr(body, n),
        HirExprKind::With { handler, body } => {
            count_expr(handler, n);
            count_block(body, n);
        }
        HirExprKind::Region { body, .. } => count_block(body, n),
        HirExprKind::TryDefault { expr, default } => {
            count_expr(expr, n);
            count_expr(default, n);
        }
        HirExprKind::Range { lo, hi, .. } => {
            if let Some(e) = lo {
                count_expr(e, n);
            }
            if let Some(e) = hi {
                count_expr(e, n);
            }
        }
        HirExprKind::Array(arr) => match arr {
            cssl_hir::HirArrayExpr::List(items) => {
                for i in items {
                    count_expr(i, n);
                }
            }
            cssl_hir::HirArrayExpr::Repeat { elem, len } => {
                count_expr(elem, n);
                count_expr(len, n);
            }
        },
        HirExprKind::Struct { fields, spread, .. } => {
            for f in fields {
                if let Some(v) = &f.value {
                    count_expr(v, n);
                }
            }
            if let Some(s) = spread {
                count_expr(s, n);
            }
        }
        HirExprKind::Perform { args, .. } => {
            for a in args {
                match a {
                    cssl_hir::HirCallArg::Positional(e)
                    | cssl_hir::HirCallArg::Named { value: e, .. } => count_expr(e, n),
                }
            }
        }
        HirExprKind::Literal(_)
        | HirExprKind::Path { .. }
        | HirExprKind::Continue { .. }
        | HirExprKind::SectionRef { .. }
        | HirExprKind::Error => {}
    }
}

/// Walk a HIR module and collect every `@staged` fn.
#[must_use]
pub fn collect_staged_fns(module: &HirModule, interner: &Interner) -> Vec<StagedDecl> {
    let mut out = Vec::new();
    for item in &module.items {
        collect_item(item, interner, &mut out);
    }
    out
}

fn collect_item(item: &HirItem, interner: &Interner, out: &mut Vec<StagedDecl>) {
    match item {
        HirItem::Fn(f) => {
            if let Some(d) = StagedDecl::from_fn(f, interner) {
                out.push(d);
            }
        }
        HirItem::Impl(i) => {
            for f in &i.fns {
                if let Some(d) = StagedDecl::from_fn(f, interner) {
                    out.push(d);
                }
            }
        }
        HirItem::Module(m) => {
            if let Some(sub) = &m.items {
                for s in sub {
                    collect_item(s, interner, out);
                }
            }
        }
        _ => {}
    }
}

/// A single `#run` marker site in a body, used by the comptime-eval queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunMarker {
    pub hir_id: cssl_hir::HirId,
}

/// A specialization manifest : call-site → stage-arg-values. Stage-0 is empty — we
/// just establish the data shape so downstream passes (transform-dialect emission)
/// can populate it at T8-phase-2.
#[derive(Debug, Default, Clone)]
pub struct Specializer {
    pub sites: Vec<SpecializationSite>,
}

/// One specialization request : `(caller_def, callee_def, args)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecializationSite {
    pub caller: cssl_hir::DefId,
    pub callee: cssl_hir::DefId,
    pub args: Vec<StageArg>,
}

/// Failure modes for staging (placeholder — populated at T8-phase-2).
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum StagingError {
    /// `#run` site contains side-effects incompatible with comptime eval.
    #[error("#run site has runtime-only side-effect : {msg}")]
    RuntimeSideEffect { msg: String },
    /// `@staged` fn has no stage-args marked comptime — specialization cannot progress.
    #[error("@staged fn {name:?} has no comptime stage-args ; specialization trivial")]
    NoStageArgs { name: Symbol },
}

impl Specializer {
    /// Empty specializer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of specialization sites.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sites.len()
    }

    /// `true` iff empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }
}

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::{
        collect_staged_fns, SpecializationSite, Specializer, StageArg, StageArgKind, StagedDecl,
        STAGE0_SCAFFOLD,
    };
    use cssl_ast::{SourceFile, SourceId, Surface};

    fn prep(src: &str) -> (cssl_hir::HirModule, cssl_hir::Interner) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _lower_bag) = cssl_hir::lower_module(&f, &cst);
        (hir, interner)
    }

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn empty_module_yields_no_staged_decls() {
        let (hir, interner) = prep("");
        assert!(collect_staged_fns(&hir, &interner).is_empty());
    }

    #[test]
    fn staged_fn_is_collected() {
        let src = r"@staged fn render<S>(cam : i32) -> i32 { cam }";
        let (hir, interner) = prep(src);
        let decls = collect_staged_fns(&hir, &interner);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].args.len(), 1);
    }

    #[test]
    fn non_staged_fn_is_skipped() {
        let src = r"fn plain(x : i32) -> i32 { x }";
        let (hir, interner) = prep(src);
        assert!(collect_staged_fns(&hir, &interner).is_empty());
    }

    #[test]
    fn run_site_count_tracks_hash_run_exprs() {
        let src = r"@staged fn f(x : i32) -> i32 { #run x }";
        let (hir, interner) = prep(src);
        let decls = collect_staged_fns(&hir, &interner);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].run_sites, 1);
    }

    #[test]
    fn stage_arg_kind_defaults_to_runtime() {
        let src = r"@staged fn f(a : i32, b : i32) -> i32 { a + b }";
        let (hir, interner) = prep(src);
        let decls = collect_staged_fns(&hir, &interner);
        for arg in &decls[0].args {
            assert_eq!(arg.kind, StageArgKind::Runtime);
        }
    }

    #[test]
    fn specializer_starts_empty() {
        let s = Specializer::new();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn stage_arg_indices_increment() {
        let src = r"@staged fn f(a : i32, b : i32, c : i32) -> i32 { a + b + c }";
        let (hir, interner) = prep(src);
        let decls = collect_staged_fns(&hir, &interner);
        assert_eq!(decls[0].args[0].index, 0);
        assert_eq!(decls[0].args[1].index, 1);
        assert_eq!(decls[0].args[2].index, 2);
    }

    #[test]
    fn specialization_site_constructs() {
        let _s = SpecializationSite {
            caller: cssl_hir::DefId(0),
            callee: cssl_hir::DefId(1),
            args: vec![StageArg {
                index: 0,
                name: None,
                kind: StageArgKind::CompTime,
            }],
        };
    }

    #[test]
    fn staged_decl_equality() {
        let interner = cssl_hir::Interner::new();
        let name = interner.intern("f");
        let d1 = StagedDecl {
            name,
            def: cssl_hir::DefId(0),
            args: Vec::new(),
            run_sites: 0,
        };
        let d2 = d1.clone();
        assert_eq!(d1, d2);
    }
}
