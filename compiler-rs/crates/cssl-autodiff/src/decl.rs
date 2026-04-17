//! `@differentiable` declaration collection from HIR.

use cssl_hir::{HirAttr, HirFn, HirItem, HirModule, Interner, Symbol};

/// A single `@differentiable` function declaration with its attribute metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffDecl {
    /// Fn name symbol.
    pub name: Symbol,
    /// `DefId` of the fn.
    pub def: cssl_hir::DefId,
    /// Parameter count.
    pub param_count: usize,
    /// Whether `@NoDiff` is also present (overrides `@differentiable`).
    pub no_diff: bool,
    /// Lipschitz bound (if `@lipschitz(k = N)` is attached).
    pub lipschitz_bound: Option<String>,
    /// Whether `@checkpoint` is set.
    pub checkpoint: bool,
}

impl DiffDecl {
    /// Build a decl from a fn + its attribute list (pre-flattened by caller).
    #[must_use]
    pub fn from_fn(f: &HirFn, interner: &Interner) -> Option<Self> {
        let has_differentiable = f
            .attrs
            .iter()
            .any(|a| attr_matches(a, interner, "differentiable"));
        if !has_differentiable {
            return None;
        }
        let no_diff = f.attrs.iter().any(|a| attr_matches(a, interner, "NoDiff"));
        let checkpoint = f
            .attrs
            .iter()
            .any(|a| attr_matches(a, interner, "checkpoint"));
        let lipschitz_bound = f
            .attrs
            .iter()
            .find(|a| attr_matches(a, interner, "lipschitz"))
            .map(|_| "k".to_string()); // Stage-0 placeholder ; full arg-extraction @ T7-phase-2.
        Some(Self {
            name: f.name,
            def: f.def,
            param_count: f.params.len(),
            no_diff,
            lipschitz_bound,
            checkpoint,
        })
    }
}

fn attr_matches(attr: &HirAttr, interner: &Interner, expected: &str) -> bool {
    // Single-segment attribute paths match by interned name comparison. Multi-segment
    // paths (e.g., `@cssl.diff.primal`) are not recognized as `@differentiable` etc.
    if attr.path.len() != 1 {
        return false;
    }
    interner.resolve(attr.path[0]) == expected
}

/// Walk a HIR module and collect every `@differentiable` fn into a `Vec<DiffDecl>`.
#[must_use]
pub fn collect_differentiable_fns(module: &HirModule, interner: &Interner) -> Vec<DiffDecl> {
    let mut out = Vec::new();
    for item in &module.items {
        collect_item(item, interner, &mut out);
    }
    out
}

fn collect_item(item: &HirItem, interner: &Interner, out: &mut Vec<DiffDecl>) {
    match item {
        HirItem::Fn(f) => {
            if let Some(d) = DiffDecl::from_fn(f, interner) {
                out.push(d);
            }
        }
        HirItem::Impl(i) => {
            for f in &i.fns {
                if let Some(d) = DiffDecl::from_fn(f, interner) {
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

#[cfg(test)]
mod tests {
    use super::collect_differentiable_fns;
    use cssl_ast::{SourceFile, SourceId, Surface};

    fn prep(src: &str) -> (cssl_hir::HirModule, cssl_hir::Interner) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _lower_bag) = cssl_hir::lower_module(&f, &cst);
        (hir, interner)
    }

    #[test]
    fn empty_module_yields_no_decls() {
        let (hir, interner) = prep("");
        let decls = collect_differentiable_fns(&hir, &interner);
        assert!(decls.is_empty());
    }

    #[test]
    fn differentiable_fn_is_collected() {
        let src = r"
            @differentiable
            fn sphere_sdf(p : f32, r : f32) -> f32 {
                p - r
            }
        ";
        let (hir, interner) = prep(src);
        let decls = collect_differentiable_fns(&hir, &interner);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].param_count, 2);
    }

    #[test]
    fn non_differentiable_fn_is_skipped() {
        let src = "fn plain(x : i32) -> i32 { x }";
        let (hir, interner) = prep(src);
        let decls = collect_differentiable_fns(&hir, &interner);
        assert!(decls.is_empty());
    }

    #[test]
    fn multiple_differentiable_fns_collected_in_order() {
        let src = r"
            @differentiable fn a(x : f32) -> f32 { x }
            @differentiable fn b(x : f32) -> f32 { x }
            fn c(x : f32) -> f32 { x }
            @differentiable fn d(x : f32) -> f32 { x }
        ";
        let (hir, interner) = prep(src);
        let decls = collect_differentiable_fns(&hir, &interner);
        assert_eq!(decls.len(), 3);
    }
}
