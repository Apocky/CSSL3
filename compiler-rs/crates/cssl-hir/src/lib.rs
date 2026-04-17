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

pub mod arena;
pub mod attr;
pub mod expr;
pub mod item;
pub mod lower;
pub mod pat;
pub mod resolve;
pub mod stmt;
pub mod symbol;
pub mod ty;

pub use arena::{DefId, HirArena, HirId};
pub use attr::{HirAttr, HirAttrArg, HirAttrKind};
pub use expr::{
    HirBinOp, HirBlock, HirCompoundOp, HirExpr, HirExprKind, HirLiteral, HirLiteralKind,
    HirMatchArm, HirUnOp,
};
pub use item::{
    HirConst, HirEffect, HirEnum, HirEnumVariant, HirFn, HirFnParam, HirGenerics, HirHandler,
    HirImpl, HirInterface, HirItem, HirModule, HirNestedModule, HirStruct, HirStructBody,
    HirTypeAlias, HirUse, HirVisibility, HirWhereClause,
};
pub use lower::{lower_module, LowerCtx};
pub use pat::{HirPattern, HirPatternField, HirPatternKind};
pub use resolve::{Scope, ScopeMap};
pub use stmt::{HirStmt, HirStmtKind};
pub use symbol::{Interner, Symbol};
pub use ty::{
    HirCapKind, HirEffectAnnotation, HirEffectArg, HirEffectRow, HirRefinementKind, HirType,
    HirTypeKind,
};

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
