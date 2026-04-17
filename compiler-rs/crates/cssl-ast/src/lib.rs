//! CSSLv3 stage0 — concrete syntax tree + source-preserving forms.
//!
//! Authoritative design : `specs/02_IR.csl` + `specs/03_TYPES.csl` + `specs/09_SYNTAX.csl`
//!                      + `specs/16_DUAL_SURFACE.csl`.
//! Cross-crate decisions : `DECISIONS.md` (T1-D2, T3-D1..D4).
//!
//! § STATUS : T3 in-progress
//!   - `source` / `span` / `diagnostic` : foundation primitives (landed in T2)
//!   - `cst` : concrete-syntax-tree node types shared by both surfaces (landed in T3)
//!
//! § SURFACE-AGNOSTIC
//!   Both Rust-hybrid and CSLv3-native parsers target the same `cst::Module`. Downstream
//!   elaboration in `cssl-hir` interns strings + threads type inference + IFC-labels.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

pub mod cst;
pub mod diagnostic;
pub mod source;
pub mod span;

pub use cst::{
    ArrayExpr, AssocTypeDecl, AssocTypeDef, Attr, AttrArg, AttrKind, BinOp, Block, CallArg,
    CapKind, CompoundOp, ConstItem, EffectAnnotation, EffectArg, EffectItem, EffectRow, EnumItem,
    EnumVariant, Expr, ExprKind, FieldDecl, FnItem, GenericParam, GenericParamKind, Generics,
    HandlerItem, Ident, ImplAssocItem, ImplItem, InterfaceAssocItem, InterfaceItem, Item, Literal,
    LiteralKind, MatchArm, Module, ModuleItem, ModulePath, Param, Pattern, PatternField,
    PatternKind, RefinementKind, Stmt, StmtKind, StructBody, StructFieldInit, StructItem, Type,
    TypeAliasItem, TypeKind, UnOp, UseItem, UseTree, Visibility, VisibilityKind, WhereClause,
};
pub use diagnostic::{Diagnostic, DiagnosticBag, Severity};
pub use source::{SourceFile, SourceId, SourceLocation, Surface};
pub use span::Span;

/// Crate version, exposes `CARGO_PKG_VERSION`.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
