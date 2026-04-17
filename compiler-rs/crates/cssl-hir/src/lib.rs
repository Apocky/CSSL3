//! CSSLv3 stage0 — High-level IR : CST elaboration + name resolution + inference.
//!
//! § PIPELINE POSITION
//!   ```text
//!     source → lex → parse → CST (cssl-ast::cst::Module)
//!                              ↓  (this crate : cssl-hir)
//!                            HIR  (typed, interned, resolved)
//!                              ↓  (cssl-mir)
//!                            MIR  (MLIR cssl-dialect)
//!                              ↓  (cssl-lir)
//!                            LIR  (target emission)
//!   ```
//!
//! § SPEC SOURCES
//!   - `specs/02_IR.csl` § HIR : contents + type-shape + checks-here + size-target ≤25K LOC
//!   - `specs/03_TYPES.csl` : refinement + effect-row + capability + generics
//!   - `specs/04_EFFECTS.csl` : effect-row operational semantics + evidence-passing
//!   - `specs/11_IFC.csl` : label lattice + non-interference + declassification
//!   - `specs/12_CAPS.csl` : Pony-6 capabilities + gen-refs
//!   - `specs/13_GRAMMAR_SELF.csl` (CSLv3) : morpheme decomposition + slot-template
//!   - `specs/20_SMT.csl` : refinement-obligation discharge via solver-plugin
//!
//! § DECISIONS (see `DECISIONS.md`)
//!   - T3-D2 : string interning lives here (not in CST) via [`lasso`].
//!   - T3-D3 : morpheme-stacking + compound-formation surface at CST ; elaborator here
//!             extracts the morpheme tree from `cst::Expr::Compound`.
//!   - T3-D4 : HIR is modular-split (one file per concern : item / expr / ty / stmt / pat / attr).
//!
//! § T3.3 SCOPE (this commit)
//!   - `symbol`  : [`Symbol`] + [`Interner`] wrapping `lasso::ThreadedRodeo`.
//!   - `arena`   : `HirId` + `DefId` newtypes + simple arenas.
//!   - `item` + `expr` + `ty` + `stmt` + `pat` + `attr` : HIR node types mirroring CST
//!     but with `Symbol` references and slots for later inference results.
//!   - `lower`   : CST → HIR structural transform (no inference, no IFC, no cap).
//!   - `resolve` : basic name-resolution — build scope-tree + resolve paths to defs.
//!
//! § T3.4 SCOPE (next)
//!   - Bidirectional type inference (Hindley-Milner + row-polymorphism for effects).
//!   - Capability inference (Pony-6 per §§ 12).
//!   - IFC-label propagation (Jif-DLM per §§ 11).
//!   - Refinement-obligation generation → SMT queue (§§ 20).
//!   - AD-legality + `@staged` stage-arg comptime-check + macro hygiene-mark.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// § Style allowances for the inference pass — the large match-heavy synth/unify walks
// benefit from `_`-fallthrough + explicit same-body-arms for readability. Tightening
// these lints is T3.4-phase-2 cleanup after the inference API stabilizes.
#![allow(clippy::match_same_arms)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::similar_names)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::unnested_or_patterns)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::len_zero)]
#![allow(clippy::needless_late_init)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::or_fun_call)]

pub mod ad_legality;
pub mod arena;
pub mod attr;
pub mod cap_check;
pub mod env;
pub mod expr;
pub mod ifc;
pub mod infer;
pub mod item;
pub mod lower;
pub mod pat;
pub mod refinement;
pub mod resolve;
pub mod stmt;
pub mod symbol;
pub mod ty;
pub mod typing;
pub mod unify;

pub use ad_legality::{
    check_ad_legality, is_pure_diff_primitive, AdLegalityDiagnostic, AdLegalityReport,
};
pub use arena::{DefId, HirArena, HirId};
pub use attr::{HirAttr, HirAttrArg, HirAttrKind};
pub use cap_check::{
    check_capabilities, hir_cap_to_semantic, param_subtype_check, top_cap, CapMap,
};
pub use env::{TypeScope, TypingEnv};
pub use expr::{
    HirArrayExpr, HirBinOp, HirBlock, HirCallArg, HirCompoundOp, HirExpr, HirExprKind,
    HirLambdaParam, HirLiteral, HirLiteralKind, HirMatchArm, HirStructFieldInit, HirUnOp,
};
pub use ifc::{
    builtin_principals, check_ifc, label_for_secret, resolve_builtin_principal, IfcDiagnostic,
    IfcLabel, IfcLabelRegistry, IfcReport,
};
pub use infer::{check_module, InferCtx};
pub use item::{
    HirConst, HirEffect, HirEnum, HirEnumVariant, HirFn, HirFnParam, HirGenerics, HirHandler,
    HirImpl, HirInterface, HirItem, HirModule, HirNestedModule, HirStruct, HirStructBody,
    HirTypeAlias, HirUse, HirVisibility, HirWhereClause,
};
pub use lower::{lower_module, LowerCtx};
pub use pat::{HirPattern, HirPatternField, HirPatternKind};
pub use refinement::{
    collect_refinement_obligations, ObligationBag, ObligationId, ObligationKind,
    RefinementObligation,
};
pub use resolve::{Scope, ScopeMap};
pub use stmt::{HirStmt, HirStmtKind};
pub use symbol::{Interner, Symbol};
pub use ty::{
    HirCapKind, HirEffectAnnotation, HirEffectArg, HirEffectRow, HirRefinementKind, HirType,
    HirTypeKind,
};
pub use typing::{ArrayLen, EffectInstance, Row, RowVar, Subst, Ty, TyCtx, TyVar, TypeMap};
pub use unify::{unify as unify_types, unify_rows, UnifyError};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
