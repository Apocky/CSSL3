//! HIR → HIR-with-diff-variants transform (skeleton).
//!
//! § SCOPE (T7-phase-1)
//!   For every `@differentiable` fn, produce a [`DiffVariants`] record containing
//!   the primal + fwd + bwd variant-names. The actual expanded fn bodies are
//!   generated in T7-phase-2 by walking `HirExpr` + applying rules from
//!   `rules::DiffRuleTable`. Phase-1 establishes the data model + naming
//!   convention that downstream crates (cssl-jets, cssl-mir) can consume.

use std::collections::BTreeMap;

use cssl_hir::{HirModule, Interner, Symbol};

use crate::decl::{collect_differentiable_fns, DiffDecl};
use crate::rules::{DiffMode, DiffRuleTable};

/// The three variant-names generated for a single `@differentiable` fn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffVariants {
    /// Original fn name symbol.
    pub primal: Symbol,
    /// `<name>_fwd` variant (symbolic — generated as a new symbol by T7-phase-2 walk).
    pub fwd_name: String,
    /// `<name>_bwd` variant.
    pub bwd_name: String,
    /// `DefId` of the primal fn.
    pub primal_def: cssl_hir::DefId,
    /// The `@differentiable` decl metadata this variant-set was built from.
    pub decl: DiffDecl,
}

impl DiffVariants {
    /// Build variants from a decl + interner (to resolve the primal name string).
    #[must_use]
    pub fn from_decl(decl: DiffDecl, interner: &Interner) -> Self {
        let primal_name = interner.resolve(decl.name);
        let fwd_name = format!("{}{}", primal_name, DiffMode::Fwd.suffix());
        let bwd_name = format!("{}{}", primal_name, DiffMode::Bwd.suffix());
        Self {
            primal: decl.name,
            primal_def: decl.def,
            fwd_name,
            bwd_name,
            decl,
        }
    }
}

/// AD transform context — holds the rules table + variant-map + interner ref.
#[derive(Debug)]
pub struct DiffTransform<'a> {
    pub interner: &'a Interner,
    pub rules: DiffRuleTable,
    pub variants: BTreeMap<u32, DiffVariants>,
}

impl<'a> DiffTransform<'a> {
    /// Build a transform with the canonical rules table.
    #[must_use]
    pub fn new(interner: &'a Interner) -> Self {
        Self {
            interner,
            rules: DiffRuleTable::canonical(),
            variants: BTreeMap::new(),
        }
    }

    /// Register all `@differentiable` fns in `module` as variants.
    pub fn register_module(&mut self, module: &HirModule) {
        let decls = collect_differentiable_fns(module, self.interner);
        for d in decls {
            if d.no_diff {
                // `@NoDiff` explicitly opts-out.
                continue;
            }
            let v = DiffVariants::from_decl(d, self.interner);
            self.variants.insert(v.primal_def.0, v);
        }
    }

    /// Lookup variants for a primal `DefId`.
    #[must_use]
    pub fn get(&self, def: cssl_hir::DefId) -> Option<&DiffVariants> {
        self.variants.get(&def.0)
    }

    /// Number of registered differentiable fns.
    #[must_use]
    pub fn len(&self) -> usize {
        self.variants.len()
    }

    /// `true` iff no variants are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.variants.is_empty()
    }

    /// Iterate over registered variants.
    pub fn iter(&self) -> impl Iterator<Item = &DiffVariants> {
        self.variants.values()
    }
}

#[cfg(test)]
mod tests {
    use super::{DiffTransform, DiffVariants};
    use crate::rules::DiffMode;
    use cssl_ast::{SourceFile, SourceId, Surface};

    fn prep(src: &str) -> (cssl_hir::HirModule, cssl_hir::Interner) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _lower_bag) = cssl_hir::lower_module(&f, &cst);
        (hir, interner)
    }

    #[test]
    fn empty_module_registers_no_variants() {
        let (hir, interner) = prep("");
        let mut tx = DiffTransform::new(&interner);
        tx.register_module(&hir);
        assert!(tx.is_empty());
    }

    #[test]
    fn differentiable_fn_gets_fwd_bwd_names() {
        let src = r"@differentiable fn sdf(p : f32) -> f32 { p }";
        let (hir, interner) = prep(src);
        let mut tx = DiffTransform::new(&interner);
        tx.register_module(&hir);
        assert_eq!(tx.len(), 1);
        let v = tx.iter().next().unwrap();
        assert_eq!(v.fwd_name, format!("sdf{}", DiffMode::Fwd.suffix()));
        assert_eq!(v.bwd_name, format!("sdf{}", DiffMode::Bwd.suffix()));
    }

    #[test]
    fn multiple_differentiable_fns_all_registered() {
        let src = r"
            @differentiable fn a(x : f32) -> f32 { x }
            @differentiable fn b(x : f32) -> f32 { x }
        ";
        let (hir, interner) = prep(src);
        let mut tx = DiffTransform::new(&interner);
        tx.register_module(&hir);
        assert_eq!(tx.len(), 2);
    }

    #[test]
    fn rules_table_pre_populated() {
        let interner = cssl_hir::Interner::new();
        let tx = DiffTransform::new(&interner);
        // 19 primitives × 2 modes (Fwd + Bwd) = 38 rules.
        assert_eq!(tx.rules.len(), 38);
    }

    #[test]
    fn variants_from_decl_roundtrips_primal_name() {
        use crate::decl::DiffDecl;
        let interner = cssl_hir::Interner::new();
        let name = interner.intern("my_fn");
        let decl = DiffDecl {
            name,
            def: cssl_hir::DefId(7),
            param_count: 2,
            no_diff: false,
            lipschitz_bound: None,
            checkpoint: false,
        };
        let v = DiffVariants::from_decl(decl, &interner);
        assert_eq!(v.fwd_name, "my_fn_fwd");
        assert_eq!(v.bwd_name, "my_fn_bwd");
        assert_eq!(v.primal_def, cssl_hir::DefId(7));
    }
}
